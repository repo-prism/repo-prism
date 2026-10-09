//! 回环地址上的极小 HTTP/1.1 stub，供「本地模型出网边界」相关测试共用。
//!
//! 由 `summarizer_http.rs`（P-07）内的实现抽出：`summarizer_egress.rs` 需要同一个东西。
//! 抽出来是因为下面两处坑**只应该有一份**，复制一份就等于让其中一份迟早失传：
//!
//! 1. **非阻塞 listener 的 `accept()` 在 macOS 上会让连接继承 `O_NONBLOCK`**。
//!    读请求时会随机 `WouldBlock`，若此时直接放弃，客户端只看到「连接被关掉」，
//!    报 `Unexpected EOF`——而症状会随请求数据到达的先后时红时绿。
//!    修法是每条 accepted 连接显式设回阻塞模式。
//! 2. **响应必须带 `Connection: close`**。ureq 默认做连接池，不声明关闭它可能复用
//!    连接，于是「第 2 个请求没到达 stub」这类假象会污染断言。
//!
//! 另外：解析失败也要回一个响应（见 `Stub::start` 的 `accept` 循环），
//! 让失败长成**可读的断言失败**而不是客户端侧的 EOF——**测试失败的报错本身是要设计的**。
//!
//! 本模块只监听 `127.0.0.1` 的随机端口、只服务固定次数的请求，不涉及任何外部网络。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// stub 在没有新连接时最多再等多久就自行退出。
///
/// 有这个上限，断言失败导致的 panic 才不会把 `join` 挂死——测试要能失败得干脆。
const STUB_IDLE_LIMIT: Duration = Duration::from_secs(3);

/// stub 收到的一条请求。
///
/// 留着 `headers` 与 `body` 是为了断言**我们发出去了什么**，
/// 而不只是断言「收到了一个请求」。
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

pub struct Stub {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<Recorded>>>,
    handle: JoinHandle<()>,
}

impl Stub {
    /// 在 127.0.0.1 的随机端口上起 stub，服务满 `expect` 个请求即退出。
    ///
    /// `respond` 拿到请求，返回 `(状态码, 响应体)`；不做路由——每个用例只关心一种响应。
    pub fn start<F>(expect: usize, respond: F) -> Self
    where
        F: Fn(&Recorded) -> (u16, String) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("stub 必须能在回环地址上监听");
        listener
            .set_nonblocking(true)
            .expect("stub 监听必须设为非阻塞，否则 accept 会卡死待退出的线程");
        let addr = listener.local_addr().expect("取 stub 监听地址");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);

        let handle = thread::spawn(move || {
            let deadline = Instant::now() + STUB_IDLE_LIMIT;
            let mut served = 0usize;
            while served < expect && Instant::now() < deadline {
                match listener.accept() {
                    Ok((stream, _)) => {
                        // 坑 1：监听者设成非阻塞后，macOS 上 accept 出的连接会**继承
                        // O_NONBLOCK**，读请求时会随机 WouldBlock。若此时直接放弃，
                        // 客户端只会看到「连接被关掉、没有响应」，报 Unexpected EOF——
                        // 而症状会随请求数据到达的先后时红时绿。显式设回阻塞。
                        stream
                            .set_nonblocking(false)
                            .expect("accept 出的连接必须设回阻塞模式");

                        let request = read_request(&stream);
                        let (status, body) = match &request {
                            Some(request) => respond(request),
                            // 解析失败也要回一个响应：让失败长成一个**可读的断言失败**，
                            // 而不是变成客户端侧的 EOF（那种失败会把排查引向错误的代码）。
                            None => (
                                400,
                                r#"{"error":"stub could not parse the request"}"#.to_string(),
                            ),
                        };
                        write_response(&stream, status, &body);
                        if let Some(request) = request {
                            sink.lock().expect("stub 互斥锁").push(request);
                        }
                        served += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });

        Self { addr, seen, handle }
    }

    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.addr.port())
    }

    /// 等 stub 收工并取回它记下的请求。
    ///
    /// **必须调用**：不 join 就读 `seen` 会读到半个请求，断言就变成了看时序脸色。
    pub fn finish(self) -> Vec<Recorded> {
        let seen = Arc::clone(&self.seen);
        let handle = self.handle;
        handle.join().expect("stub 线程不该 panic");
        let drained = seen.lock().expect("stub 互斥锁").clone();
        drained
    }
}

/// 绑一个端口拿到号后立刻释放——得到一个「此刻无人监听」的端口。
///
/// 用于构造「连不上」的用例，也用于把代理指向一个必然失败的位置。
pub fn unused_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑一个临时端口");
    let port = listener.local_addr().expect("取临时端口号").port();
    drop(listener);
    port
}

fn read_request(stream: &TcpStream) -> Option<Recorded> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("设读超时");
    let mut reader = BufReader::new(stream);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line).ok()? == 0 {
        return None;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_string();
            let value = value.trim().to_string();
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).ok()?;
    }

    Some(Recorded {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

fn write_response(mut stream: &TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        500 => "Internal Server Error",
        _ => "Error",
    };
    // 坑 2：`Connection: close` 是必须的，理由见模块文档。
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}
