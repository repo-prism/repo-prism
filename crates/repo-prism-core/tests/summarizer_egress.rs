//! 本地模型的**出网边界**实测：环境里的代理不得把「回环请求」改道到别处。
//!
//! # 为什么这条比「真实 Ollama 跑通」更要紧
//!
//! 整个 `summarizer.rs` 的安全承诺是一句话：**唯一出网点是到 127.0.0.1**。
//! `validate_endpoint()` 把 endpoint 的 host 卡死在 `localhost / 127.0.0.1 / ::1`，
//! 于是「不把仓库发到外部」看起来是成立的。
//!
//! 但 `summarizer.rs` 用的是 `ureq::get()` / `ureq::post()` 这两个**顶层便捷函数**，
//! 它们内部构造的是**默认 agent**。而 HTTP 客户端普遍遵循 `HTTP_PROXY` / `HTTPS_PROXY` /
//! `ALL_PROXY` 环境变量——**代理是会改写连接目标的**。若默认 agent 遵循它，
//! 那么 endpoint 即便写着 `127.0.0.1`，字节也会先送到代理：
//!
//! - 出网约束在**校验层**看着是绿的，实际连接目标是**环境变量**说了算；
//! - 企业网、CI、容器里 `HTTP_PROXY` 是常态，这不是假想场景
//!   （本仓库的开发机上 `HTTP_PROXY=http://127.0.0.1:58883` 就是设着的）；
//! - 而 `read-only-guard.sh` 是 Git 动词白名单，**看不见 HTTP**，
//!   这条边界没有任何静态扫描兜底。
//!
//! 也就是说：`validate_endpoint` 管得住**写死的 endpoint**，管不住**代理**。
//! 这个测试就是补上那一半。
//!
//! # 测试怎么做到「能失败」
//!
//! 光设一个代理变量然后断言成功是**测不出东西的**：如果客户端根本不认代理，
//! 测试当然绿；如果它认代理但代理恰好能通，测试也会绿。所以本测试先做两件事：
//!
//! 1. **确认那个代理地址是死的**（先连一次，必须连不上）——
//!    于是「用了代理」必然导致调用失败，绿与红被彻底分开；
//! 2. 把 `NO_PROXY` 一并清掉——否则「走直连」可能只是被 NO_PROXY 豁免，
//!    而不是客户端真的不认代理。
//!
//! 断言里同时核对请求行是 **origin-form**（`/api/tags`）而不是 **absolute-form**
//! （`http://host/api/tags`）。后者是代理请求的写法，它出现在 stub 里
//! 就说明连接目标被改写过。
//!
//! # 本文件为什么不放进 `summarizer_http.rs`
//!
//! 环境变量是**进程级**的，而同一个测试二进制内的用例**并行跑**。
//! 在这里 `set_var` 会把同进程内其它用例的请求一起带偏。独立成一个测试二进制
//! （各自一个进程）才是安全的做法。

mod common;

use common::stub::{unused_port, Stub};
use repo_prism_core::{OllamaConfig, OllamaSummarizer, DEFAULT_MODEL};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const PROXY_KEYS: [&str; 6] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];

/// 本文件只有这一个用例，是刻意的：环境变量是进程级的，
/// 多一个用例就多一处互相污染的入口。
#[test]
fn a_proxy_in_the_environment_must_not_hijack_the_loopback_request() {
    // 1. 先拿到 stub 的端口（保持监听，占住它），再取一个「必然空闲」的端口做代理。
    //    顺序不能反：先取空闲端口的话，两次 bind 可能选中同一个号。
    let stub = Stub::start(2, |request| {
        if request.path == "/api/tags" {
            (
                200,
                r#"{"models":[{"name":"llama3.2:latest"}]}"#.to_string(),
            )
        } else {
            (200, r#"{"response":"ok","done":true}"#.to_string())
        }
    });
    let dead_proxy_port = unused_port();

    // 2. 前提一：那个代理地址确实是死的。
    //    没有这一步，测试即使在「客户端认代理且代理恰好能通」时也会绿。
    let addr = ("127.0.0.1", dead_proxy_port)
        .to_socket_addrs()
        .expect("回环地址必须可解析")
        .next()
        .expect("至少有一个地址");
    assert!(
        TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err(),
        "代理端口 {dead_proxy_port} 竟然有人监听——本测试的前提不成立，\
         换一个端口重跑（unused_port 只是「此刻空闲」，不是保留）"
    );

    // 3. 前提二：把环境变量设成「必然失败」的代理，并清掉 NO_PROXY 这条豁免通道。
    let proxy = format!("http://127.0.0.1:{dead_proxy_port}");
    for key in PROXY_KEYS {
        std::env::set_var(key, &proxy);
    }
    for key in ["NO_PROXY", "no_proxy"] {
        std::env::remove_var(key);
    }
    assert_eq!(
        std::env::var("HTTP_PROXY").as_deref(),
        Ok(proxy.as_str()),
        "代理变量没设上，本测试什么也证明不了"
    );

    let endpoint = stub.endpoint();
    let summarizer = OllamaSummarizer::new(OllamaConfig {
        endpoint: endpoint.clone(),
        model: DEFAULT_MODEL.to_string(),
        timeout_secs: 10,
    })
    .expect("回环地址上的合法配置必须构造成功");

    // 4. 关键断言：代理指向死端口，请求却仍然要到达回环 stub。
    let models = summarizer.list_models().unwrap_or_else(|error| {
        panic!(
            "环境里的代理改道了回环请求。\n\
             代理指向 {proxy}（已确认无人监听），endpoint 是 {endpoint}。\n\
             这说明 ureq 的默认 agent 遵循了代理环境变量——\n\
             「唯一出网点是回环地址」这条安全承诺在设了代理的机器上不成立，\n\
             仓库内容会被送到代理。修法：在 summarizer.rs 里构造**显式禁用代理**的\n\
             agent（ureq 2.x 用 AgentBuilder，不要用顶层 ureq::get/post），\n\
             或改用不读环境变量的传输层。\n\
             实际错误：{error}"
        )
    });
    assert_eq!(
        models,
        vec!["llama3.2:latest"],
        "回环 stub 的服务必须被真正取到"
    );

    let text = summarizer
        .generate("hello")
        .expect("POST 路径同样不该被代理改道");
    assert_eq!(text, "ok");

    // 5. 请求行必须是 origin-form。absolute-form 是代理请求的写法，
    //    它出现在这里就等于连接目标被改写过。
    let requests = stub.finish();
    assert_eq!(requests.len(), 2, "一次探活 + 一次生成");
    for request in &requests {
        assert!(
            request.path.starts_with('/'),
            "stub 收到的请求行是 `{} {}`——不是 origin-form。\
             absolute-form（`http://host/path`）说明请求发给了代理而不是直接发给回环地址。",
            request.method,
            request.path
        );
    }
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/api/tags");
    assert_eq!(requests[1].method, "POST");
    assert_eq!(requests[1].path, "/api/generate");
}
