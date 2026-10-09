//! 本地模型 HTTP 路径的实测覆盖（对应 P-07）。
//!
//! **为什么需要这个文件**：`summarizer.rs` 里原有的 13 个单测全部是纯函数
//! （endpoint 校验、prompt 拼装），而真正**把字节送出进程的两处 `ureq` 调用
//! 一次都没被执行过** —— 本机没有 Ollama。
//!
//! 麻烦在于 SPEC/SECURITY 把「不把代码发到外部」这条约束的**唯一防线押在测试上**
//! （只读扫描器是 Git 动词白名单，看不见 HTTP 调用）。一条从未运行过的防线不是防线：
//! `base()` 拼接写错、`stream` 忘了关、字段名从 `response` 改成 `message`、
//! `Content-Length` 算错——这些都会让「摘要永远返回 None」而**没有任何一个测试变红**。
//!
//! 这里用 `std::net::TcpListener` 在 **127.0.0.1** 上起一个极小的 stub（不引新依赖），
//! 让请求真的走一遍 TCP → HTTP 解析 → JSON 取值，并把「出网 body 里到底有什么」
//! 钉成断言。覆盖四类：
//!
//! 1. 往返能通（`list_models` / `generate` / `summarize`），且方法、路径、请求体正确
//! 2. 失败一律降级（连不上 / 5xx / 非法 JSON / 空 response），不 panic 不升级为错误
//! 3. endpoint 的尾斜杠与首尾空白不污染路径
//! 4. **送出去的 body 只含相对路径**（SECURITY 威胁 7 第 3 层）
//!
//! **本文件不替代真实模型验证**：stub 只能证明「我们这一侧写得对」，
//! 证明不了「真实 Ollama 的响应形状与我们的假设一致」。真实调用的端到端验证
//! 在 SPEC US-7 里仍是 `[待实现]`。

use repo_prism_core::{
    ChangeAnalysis, OllamaConfig, OllamaSummarizer, Risk, RiskCounts, RiskLevel, Summarizer,
    DEFAULT_MODEL,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// stub 在没有新连接时最多再等多久就自行退出。
///
/// 有这个上限，断言失败导致的 panic 才不会把 `join` 挂死——测试要能失败得干脆。
const STUB_IDLE_LIMIT: Duration = Duration::from_secs(3);

// ---------------------------------------------------------------------------
// 一个只认 HTTP/1.1、只服务固定次数请求的 stub 服务
// ---------------------------------------------------------------------------

/// stub 收到的一条请求。留着 `headers` 与 `body` 是为了断言**我们发出去了什么**，
/// 而不只是断言「收到了一个请求」。
#[derive(Debug, Clone)]
struct Recorded {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Recorded {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

struct Stub {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<Recorded>>>,
    handle: JoinHandle<()>,
}

impl Stub {
    /// 在 127.0.0.1 的随机端口上起 stub，服务满 `expect` 个请求即退出。
    ///
    /// `respond` 拿到请求，返回 `(状态码, 响应体)`；不做路由——每个用例只关心一种响应。
    fn start<F>(expect: usize, respond: F) -> Self
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
                        // 坑：监听者设成非阻塞后，macOS 上 accept 出的连接会**继承
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

    fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.addr.port())
    }

    /// 等 stub 收工并取回它记下的请求。
    ///
    /// **必须调用**：不 join 就读 `seen` 会读到半个请求，断言就变成了看时序脸色。
    fn finish(self) -> Vec<Recorded> {
        let seen = Arc::clone(&self.seen);
        let handle = self.handle;
        handle.join().expect("stub 线程不该 panic");
        let drained = seen.lock().expect("stub 互斥锁").clone();
        drained
    }
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
    // `Connection: close` 是必须的：ureq 默认做连接池，不声明关闭它可能复用连接，
    // 于是「第 2 个请求没到达 stub」这类假象会污染断言。
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

// ---------------------------------------------------------------------------
// 用例
// ---------------------------------------------------------------------------

fn config(endpoint: &str) -> OllamaConfig {
    OllamaConfig {
        endpoint: endpoint.to_string(),
        model: DEFAULT_MODEL.to_string(),
        timeout_secs: 30,
    }
}

fn summarizer_for(endpoint: &str) -> OllamaSummarizer {
    OllamaSummarizer::new(config(endpoint)).expect("回环地址上的合法配置必须构造成功")
}

/// 绑一个端口拿到号后立刻释放——得到一个「此刻无人监听」的端口。
fn unused_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑一个临时端口");
    let port = listener.local_addr().expect("取临时端口号").port();
    drop(listener);
    port
}

#[test]
fn list_models_parses_names_out_of_a_real_round_trip() {
    let stub = Stub::start(1, |_| {
        (
            200,
            r#"{"models":[{"name":"llama3.2:latest"},{"name":"qwen2.5:7b"},{"size":3826793677}]}"#
                .to_string(),
        )
    });

    let models = summarizer_for(&stub.endpoint())
        .list_models()
        .expect("stub 返回的是合法 JSON");

    assert_eq!(models, vec!["llama3.2:latest", "qwen2.5:7b"]);

    let requests = stub.finish();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/api/tags");
    assert!(
        requests[0].body.is_empty(),
        "探活不该带请求体，实际带的是：{}",
        requests[0].body
    );
}

#[test]
fn generate_posts_a_non_streaming_request_and_trims_the_reply() {
    let stub = Stub::start(1, |_| {
        (
            200,
            r#"{"response":"  改的是文档，先看 CHANGELOG。  ","done":true}"#.to_string(),
        )
    });

    let text = summarizer_for(&stub.endpoint())
        .generate("hello")
        .expect("stub 返回了非空 response");

    assert_eq!(text, "改的是文档，先看 CHANGELOG。", "首尾空白必须被裁掉");

    let requests = stub.finish();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/api/generate");
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/json"),
        "send_json 应带上 Content-Type"
    );

    let sent: serde_json::Value =
        serde_json::from_str(&requests[0].body).expect("请求体必须是 JSON");
    assert_eq!(sent["model"], DEFAULT_MODEL);
    assert_eq!(
        sent["stream"], false,
        "stream 必须关掉：开着会返回多行 JSON，into_json() 直接失败"
    );
    assert_eq!(sent["prompt"], "hello");
    assert_eq!(
        sent["options"]["temperature"], 0.2,
        "低温是刻意的，摘要不需要发挥"
    );
}

#[test]
fn a_blank_or_missing_response_degrades_to_none() {
    for payload in [
        r#"{"response":"   "}"#,
        r#"{"response":null}"#,
        r#"{"response":""}"#,
        r#"{}"#,
    ] {
        let stub = Stub::start(1, move |_| (200, payload.to_string()));
        let summarizer = summarizer_for(&stub.endpoint());

        assert!(
            summarizer.generate("hi").is_none(),
            "`{payload}` 应降级为 None，而不是返回空串让界面显示一块空白"
        );
        stub.finish();
    }
}

#[test]
fn a_server_error_degrades_instead_of_escaping_as_a_panic() {
    let stub = Stub::start(1, |_| (500, r#"{"error":"model not found"}"#.to_string()));
    let summarizer = summarizer_for(&stub.endpoint());
    assert!(
        summarizer.generate("hi").is_none(),
        "5xx 时 generate 降级为 None"
    );
    stub.finish();

    let stub = Stub::start(1, |_| (500, r#"{"error":"boom"}"#.to_string()));
    let summarizer = summarizer_for(&stub.endpoint());
    let error = summarizer
        .list_models()
        .expect_err("5xx 时 list_models 必须报错，设置面板要靠它给出原因");
    assert!(
        error.contains("cannot reach"),
        "错误信息要指路（告诉用户去哪儿），实际是：{error}"
    );
    stub.finish();
}

#[test]
fn invalid_json_from_the_server_is_reported_not_swallowed() {
    // 两个请求：list_models 与 generate 各打一次。
    let stub = Stub::start(2, |_| (200, "{not json".to_string()));
    let summarizer = summarizer_for(&stub.endpoint());

    let error = summarizer
        .list_models()
        .expect_err("非法 JSON 必须报错，不能静默当成「没有模型」");
    assert!(error.contains("invalid JSON"), "实际是：{error}");

    assert!(
        summarizer.generate("hi").is_none(),
        "生成侧遇到非法 JSON 降级为 None"
    );
    stub.finish();
}

#[test]
fn nothing_listening_degrades_instead_of_blowing_up() {
    let endpoint = format!("http://127.0.0.1:{}", unused_port());
    let summarizer = summarizer_for(&endpoint);

    let error = summarizer
        .list_models()
        .expect_err("没有服务在听时必须报错");
    assert!(
        error.contains("cannot reach"),
        "错误里要带上地址方便排查，实际是：{error}"
    );
    assert!(
        summarizer.generate("hi").is_none(),
        "模型没启动不该把整个界面变成报错态"
    );
}

#[test]
fn a_sloppy_but_legal_endpoint_still_produces_clean_paths() {
    let stub = Stub::start(1, |_| (200, r#"{"models":[]}"#.to_string()));

    // 首尾空白 + 尾斜杠：校验放行，拼接就必须自行处理干净。
    let padded = format!("  {}/  ", stub.endpoint());
    let models = summarizer_for(&padded)
        .list_models()
        .expect("空 models 数组也是成功的响应");
    assert!(models.is_empty());

    let requests = stub.finish();
    assert_eq!(
        requests[0].path, "/api/tags",
        "尾斜杠不得拼成 `//api/tags`——请求会打到别的路由上"
    );
}

/// SECURITY 威胁 7 第 3 层的实测版：不看文档怎么写，看出网 body 里到底有什么。
#[test]
fn the_bytes_that_leave_the_machine_carry_relative_paths_only() {
    let stub = Stub::start(1, |_| (200, r#"{"response":"ok"}"#.to_string()));
    let summarizer = summarizer_for(&stub.endpoint());

    let analysis = ChangeAnalysis {
        summary: "1 个文件涉及环境变量".to_string(),
        total_files: 1,
        by_level: RiskCounts {
            critical: 1,
            ..RiskCounts::default()
        },
        risks: vec![Risk {
            rule_id: "env-or-secret".to_string(),
            level: RiskLevel::Critical,
            message: "疑似密钥".to_string(),
            path: "config/app.env".to_string(),
        }],
    };

    assert_eq!(summarizer.summarize(&analysis).as_deref(), Some("ok"));

    let requests = stub.finish();
    let sent = &requests[0].body;

    assert!(
        sent.contains("config/app.env"),
        "相对路径是摘要的唯一依据，必须送到：{sent}"
    );
    assert!(
        !sent.contains("疑似密钥"),
        "规则自带的叙述是二次加工，不该进 prompt：{sent}"
    );
    assert!(
        !sent.contains(env!("CARGO_MANIFEST_DIR")),
        "出网 body 里出现了仓库绝对路径：{sent}"
    );
    assert!(!sent.contains("file://"), "不该出现本地文件 URL：{sent}");
    assert!(
        !sent.contains("/Users/") && !sent.contains("file:///") && !sent.contains(r"C:\\"),
        "出网 body 里出现了主机绝对路径：{sent}"
    );
}
