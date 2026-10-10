//! MCP Server 协议集成测试（对应 TASK-009 验收标准）。
//!
//! 用真实子进程 + 真实 stdin/stdout 跑 JSON-RPC：MCP 的全部风险都在
//! 「协议边界」上（通知不该回包、坏输入不该崩、错误码要对），
//! 直接调用处理函数测不到这些。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 临时仓库夹具。仅作用于临时目录，不触碰任何真实仓库。
struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX epoch")
            .as_nanos();
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("repoprism-mcp-{tag}-{nanos}-{seq}"));
        fs::create_dir_all(&path).expect("failed to create temp dir");

        let repo = Self { path };
        repo.git(&["init"]);
        repo.git(&["config", "user.name", "Test User"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo
    }

    fn path_str(&self) -> String {
        self.path.display().to_string()
    }

    fn write(&self, rel: &str, content: &str) {
        fs::write(self.path.join(rel), content).expect("failed to write file");
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", message]);
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .args(["-c", "init.defaultBranch=main", "-C"])
            .arg(&self.path)
            .args(args)
            .output()
            .expect("failed to run git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 把整段输入喂给服务端，返回逐行的响应。
///
/// 写完即关闭 stdin（drop），服务端读到 EOF 后正常退出——这本身也是验收点之一。
fn serve(input: &str) -> Vec<serde_json::Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_repoprism-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn repoprism-mcp");

    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(input.as_bytes())
        .expect("failed to write to stdin");

    let out = child.wait_with_output().expect("failed to wait for server");
    assert!(
        out.status.success(),
        "读到 EOF 后服务端应正常退出，stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("每一行响应都必须是合法 JSON：{line}（{e}）"))
        })
        .collect()
}

fn request(id: u64, method: &str, params: serde_json::Value) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
        .to_string()
}

fn call_tool(id: u64, name: &str, args: serde_json::Value) -> String {
    request(
        id,
        "tools/call",
        serde_json::json!({ "name": name, "arguments": args }),
    )
}

/// 取出 `content[0].text` 并把其中的 JSON 解析出来。
fn content_json(resp: &serde_json::Value) -> serde_json::Value {
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("content[0].text 应为字符串，实际 {resp}"));
    serde_json::from_str(text).unwrap_or_else(|e| panic!("text 应为合法 JSON：{e}"))
}

#[test]
fn initialize_reports_protocol_and_capabilities() {
    let responses = serve(&format!(
        "{}\n",
        request(1, "initialize", serde_json::json!({}))
    ));

    assert_eq!(responses.len(), 1, "initialize 必须有且只有一条响应");
    let resp = &responses[0];
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);
    assert!(resp["result"]["protocolVersion"].is_string());
    assert_eq!(resp["result"]["serverInfo"]["name"], "repoprism-mcp");
    assert!(
        resp["result"]["capabilities"]["tools"].is_object(),
        "必须声明 tools 能力，否则客户端不会列工具"
    );
}

#[test]
fn tools_list_exposes_exactly_six_readonly_tools() {
    let responses = serve(&format!(
        "{}\n",
        request(1, "tools/list", serde_json::json!({}))
    ));

    let tools = responses[0]["result"]["tools"]
        .as_array()
        .expect("tools 应为数组");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(
        names,
        vec![
            "repoprism_inspect",
            "repoprism_commits",
            "repoprism_detail",
            "repoprism_analyze",
            "repoprism_remote",
            "repoprism_blob",
        ],
        "工具集变化必须是有意的：多一个工具就是多一处 Agent 能触发的只读面"
    );

    // 每个工具都必须声明 inputSchema 与 path 参数（客户端据此校验调用）
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert_eq!(tool["inputSchema"]["required"][0], "path");
        assert!(
            tool["description"].as_str().is_some_and(|d| !d.is_empty()),
            "工具必须有描述，Agent 靠它选工具"
        );
    }
}

#[test]
fn inspect_tool_returns_snapshot() {
    let repo = TempRepo::new("inspect");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_inspect",
            serde_json::json!({ "path": repo.path_str() })
        )
    ));

    assert_eq!(responses[0]["result"]["isError"], false);
    let data = content_json(&responses[0]);
    assert_eq!(data["head"]["branch"], "main");
    assert_eq!(data["head"]["detached"], false);
    assert_eq!(data["head"]["upstream"], serde_json::Value::Null);
    assert_eq!(data["branches"][0]["name"], "main");
}

#[test]
fn commits_tool_honours_limit() {
    let repo = TempRepo::new("commits");
    for i in 1..=3 {
        repo.write("log.txt", &format!("{i}\n"));
        repo.commit(&format!("c{i}"));
    }

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_commits",
            serde_json::json!({ "path": repo.path_str(), "limit": 2 })
        )
    ));

    let data = content_json(&responses[0]);
    let commits = data.as_array().expect("commits 应为数组");
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0]["subject"], "c3");
}

#[test]
fn detail_tool_returns_raw_patch() {
    let repo = TempRepo::new("detail");
    repo.write("a.txt", "one\ntwo\n");
    repo.commit("first");
    repo.write("a.txt", "one\nTWO\n");
    repo.commit("second");
    let sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_detail",
            serde_json::json!({ "path": repo.path_str(), "sha": sha })
        )
    ));

    let data = content_json(&responses[0]);
    assert_eq!(data["info"]["subject"], "second");
    assert!(
        data["patch"]
            .as_str()
            .is_some_and(|p| p.contains("diff --git a/a.txt b/a.txt")),
        "detail 必须带原始 diff 正文，实际 {}",
        data["patch"]
    );
    assert_eq!(data["truncated"], false);
}

#[test]
fn analyze_tool_reports_local_risks() {
    let repo = TempRepo::new("analyze");
    repo.write("keep.rs", "fn keep() {}\n");
    repo.commit("baseline");
    // 已暂存的疑似密钥 —— 应命中 critical 级规则
    repo.write(".env", "TOKEN=1\n");
    repo.git(&["add", ".env"]);

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_analyze",
            serde_json::json!({ "path": repo.path_str() })
        )
    ));

    assert_eq!(responses[0]["result"]["isError"], false);
    let data = content_json(&responses[0]);
    assert_eq!(data["total_files"], 1);
    assert_eq!(data["by_level"]["critical"], 1);

    let risks = data["risks"].as_array().expect("risks 应为数组");
    assert_eq!(risks.len(), 1);
    assert_eq!(risks[0]["path"], ".env");
    assert_eq!(risks[0]["rule_id"], "env-or-secret");
    assert!(
        data["summary"].as_str().is_some_and(|s| !s.is_empty()),
        "分析必须带一句话摘要，Agent 不该自己拼"
    );
}

#[test]
fn analyze_tool_is_clean_on_an_untouched_repository() {
    let repo = TempRepo::new("analyze-clean");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_analyze",
            serde_json::json!({ "path": repo.path_str() })
        )
    ));

    let data = content_json(&responses[0]);
    assert_eq!(data["total_files"], 0);
    assert_eq!(data["risks"].as_array().map(Vec::len), Some(0));
}

#[test]
fn remote_tool_returns_null_without_an_origin() {
    let repo = TempRepo::new("remote-none");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_remote",
            serde_json::json!({ "path": repo.path_str() })
        )
    ));

    assert_eq!(responses[0]["result"]["isError"], false);
    assert_eq!(
        content_json(&responses[0]),
        serde_json::Value::Null,
        "没有 origin 是正常状态，不是错误"
    );
}

#[test]
fn remote_tool_parses_a_github_url() {
    let repo = TempRepo::new("remote-github");
    repo.write("a.txt", "1\n");
    repo.commit("c1");
    repo.git(&[
        "remote",
        "add",
        "origin",
        "git@github.com:repo-prism/repo-prism.git",
    ]);

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_remote",
            serde_json::json!({ "path": repo.path_str() })
        )
    ));

    let data = content_json(&responses[0]);
    assert_eq!(data["host"], "github.com");
    assert_eq!(data["owner"], "repo-prism");
    assert_eq!(data["repo"], "repo-prism");
}

// --- blob 工具（P-11）-------------------------------------------------------
// 前五个工具都只读「元数据」，blob 是唯一会读**文件内容**的。
// 因此这里每条断言都同时盯两件事：读到的对不对，以及「该不读的是不是真的没读」。

#[test]
fn blob_tool_returns_a_text_preview() {
    let repo = TempRepo::new("blob-text");
    repo.write("a.txt", "hello blob\n");
    repo.commit("c1");

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_blob",
            serde_json::json!({ "path": repo.path_str(), "file": "a.txt" })
        )
    ));

    assert_eq!(responses[0]["result"]["isError"], false);
    let data = content_json(&responses[0]);
    // `kind` 是带 tag 的枚举，线上形状是 `kind.kind`（另见 core 侧 preview_json 测试）
    assert_eq!(data["kind"]["kind"], "text");
    assert_eq!(data["text"], "hello blob\n");
    assert_eq!(data["size"], 11, "size 必须是仓库里的真实字节数");
    assert_eq!(data["too_large"], false);
    assert_eq!(data["truncated"], false);
}

#[test]
fn blob_tool_size_only_reports_the_size_and_no_content() {
    let repo = TempRepo::new("blob-size");
    repo.write("a.txt", "hello blob\n");
    repo.commit("c1");

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_blob",
            serde_json::json!({ "path": repo.path_str(), "file": "a.txt", "size_only": true })
        )
    ));

    let data = content_json(&responses[0]);
    assert_eq!(data["size"], 11);
    assert_eq!(data["preview_cap_bytes"], 4 * 1024 * 1024);
    // 「一个字节都没读」要有可断言的形状：内容字段**根本不存在**，不是 null 也不是空串。
    for field in ["text", "content", "hex", "kind"] {
        assert!(
            data.get(field).is_none(),
            "size_only 的结果里不应出现 {field}，实际 {data}"
        );
    }
}

#[test]
fn blob_tool_reads_a_historic_revision_on_request() {
    let repo = TempRepo::new("blob-rev");
    repo.write("a.txt", "old\n");
    repo.commit("c1");
    repo.write("a.txt", "new\n");
    repo.commit("c2");

    // 不传 rev 时必须是 HEAD；传了就必须真的按那个版本读。
    let responses = serve(&format!(
        "{}\n{}\n",
        call_tool(
            1,
            "repoprism_blob",
            serde_json::json!({ "path": repo.path_str(), "file": "a.txt" })
        ),
        call_tool(
            2,
            "repoprism_blob",
            serde_json::json!({ "path": repo.path_str(), "file": "a.txt", "rev": "HEAD~1" })
        )
    ));

    assert_eq!(
        content_json(&responses[0])["text"],
        "new\n",
        "默认应是 HEAD"
    );
    assert_eq!(content_json(&responses[1])["text"], "old\n");
}

#[test]
fn blob_tool_on_an_lfs_pointer_never_shows_content() {
    let repo = TempRepo::new("blob-lfs");
    let pointer = "version https://git-lfs.github.com/spec/v1\noid sha256:4d7a2146\nsize 12345\n";
    repo.write("big.bin", pointer);
    repo.commit("c1");

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_blob",
            serde_json::json!({ "path": repo.path_str(), "file": "big.bin" })
        )
    ));

    let data = content_json(&responses[0]);
    assert_eq!(data["kind"]["kind"], "lfs_pointer");
    assert_eq!(
        data["kind"]["oid"], "4d7a2146",
        "oid 要能直接读到，便于人工核对"
    );
    assert_eq!(data["kind"]["size"], 12345, "这是 LFS 对象的真实大小");
    assert_eq!(
        data["size"],
        pointer.len() as u64,
        "外层 size 是**指针文件本身**的大小 —— 与上面那个 size 不是一回事"
    );
    assert!(
        data["text"].is_null(),
        "指针的『内容』不在仓库里，不能当成文本呈现，实际 {data}"
    );
}

#[test]
fn blob_tool_on_a_missing_file_is_an_error_not_an_empty_preview() {
    let repo = TempRepo::new("blob-missing");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_blob",
            serde_json::json!({ "path": repo.path_str(), "file": "nope.txt" })
        )
    ));

    assert!(
        responses[0]["result"].is_null() && responses[0]["error"]["code"].is_number(),
        "『这个版本下没有这个文件』必须报错，不能降级成空预览，实际 {}",
        responses[0]
    );
}

#[test]
fn notifications_get_no_response() {
    let responses = serve(&format!(
        "{}\n{}\n",
        // 无 id → 通知，服务端必须不回包
        serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        request(2, "tools/list", serde_json::json!({}))
    ));

    assert_eq!(
        responses.len(),
        1,
        "通知不应产生响应，否则客户端会收到孤立的回包"
    );
    assert_eq!(responses[0]["id"], 2);
}

#[test]
fn malformed_input_is_skipped_without_killing_the_server() {
    let responses = serve(&format!(
        "not json at all\n{}\n{}\n",
        "{ this is broken json",
        request(3, "tools/list", serde_json::json!({}))
    ));

    assert_eq!(responses.len(), 1, "坏输入应被跳过而不是崩溃");
    assert_eq!(responses[0]["id"], 3);
}

#[test]
fn unknown_method_and_tool_return_jsonrpc_errors() {
    let responses = serve(&format!(
        "{}\n{}\n",
        request(1, "repoprism/hack", serde_json::json!({})),
        call_tool(2, "repoprism_delete_everything", serde_json::json!({}))
    ));

    assert_eq!(responses.len(), 2);
    for resp in &responses {
        assert!(
            resp["result"].is_null() && resp["error"]["code"].is_number(),
            "错误必须走 JSON-RPC error 通道，实际 {resp}"
        );
        assert!(resp["error"]["message"].as_str().is_some());
    }
}

#[test]
fn missing_argument_is_an_invalid_params_error() {
    let responses = serve(&format!(
        "{}\n",
        call_tool(1, "repoprism_detail", serde_json::json!({ "path": "." }))
    ));

    assert_eq!(responses[0]["error"]["code"], -32602);
    assert!(
        responses[0]["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("sha")),
        "错误信息应点名缺了哪个参数，实际 {}",
        responses[0]["error"]
    );
}

#[test]
fn one_process_can_serve_two_repositories() {
    // TASK-019（US-9）：会话表现在是**一张**，同一个长驻进程可以服务多个仓库。
    let left = TempRepo::new("two-left");
    left.write("a.txt", "1\n");
    left.commit("c1");

    let right = TempRepo::new("two-right");
    right.write("b.txt", "2\n");
    right.commit("c1");
    right.git(&["checkout", "-b", "release"]);

    let responses = serve(&format!(
        "{}\n{}\n",
        call_tool(
            1,
            "repoprism_inspect",
            serde_json::json!({ "path": left.path_str() })
        ),
        call_tool(
            2,
            "repoprism_inspect",
            serde_json::json!({ "path": right.path_str() })
        )
    ));

    assert_eq!(responses.len(), 2);
    let left_data = content_json(&responses[0]);
    let right_data = content_json(&responses[1]);

    assert_eq!(left_data["head"]["branch"], "main");
    assert_eq!(right_data["head"]["branch"], "release");
    assert_ne!(
        left_data["path"], right_data["path"],
        "两个仓库的快照不该互相污染"
    );
}

/// 长驻进程的**交互式**会话：写完不关 stdin，于是可以在两次调用之间改动仓库。
///
/// 这是 TASK-019 在 MCP 侧唯一真正的新风险。此前每个请求都新开一个**不缓存**的
/// `Git`，所以「两次调用之间仓库变了」根本不会发生；现在会话被复用，
/// 引用映射留在内存里 —— 它必须能自己失效，否则 Agent 会读到过时的分支列表。
struct Server {
    stdin: std::process::ChildStdin,
    stdout: std::io::BufReader<std::process::ChildStdout>,
    child: std::process::Child,
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_repoprism-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn repoprism-mcp");
        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = std::io::BufReader::new(child.stdout.take().expect("stdout piped"));
        Self {
            stdin,
            stdout,
            child,
        }
    }

    /// 发一条请求，读一条响应。
    ///
    /// 服务端每条响应都是**一行**且在写完后立即 flush，因此 `read_line` 不会
    /// 多读也不会读不满。若服务端崩了，`read_line` 返回 0 字节 —— 那会被断言拦下，
    /// 而不是让测试挂住。
    fn call(&mut self, line: &str) -> serde_json::Value {
        use std::io::{BufRead, Write};
        writeln!(self.stdin, "{line}").expect("write request");
        self.stdin.flush().expect("flush request");
        let mut buf = String::new();
        let read = self.stdout.read_line(&mut buf).expect("read response");
        assert!(read > 0, "服务端没有回包，可能已经退出");
        serde_json::from_str(&buf).unwrap_or_else(|e| panic!("响应必须是合法 JSON：{buf}（{e}）"))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // 关掉 stdin 让服务端自己读到 EOF 退出；万一它卡住，再强杀一次。
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_reused_session_still_sees_a_branch_created_between_calls() {
    let repo = TempRepo::new("stale-refs");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let mut server = Server::start();
    let inspect = call_tool(
        1,
        "repoprism_inspect",
        serde_json::json!({ "path": repo.path_str() }),
    );

    // 第一次：缓存被填上，此时只有 main
    let first = content_json(&server.call(&inspect));
    let names_before: Vec<&str> = first["branches"]
        .as_array()
        .expect("branches 应为数组")
        .iter()
        .filter_map(|b| b["name"].as_str())
        .collect();
    assert_eq!(names_before, vec!["main"]);

    // 两次调用之间，仓库被外部改了：多出一个分支
    repo.git(&["branch", "feature"]);

    // 第二次：复用的会话**必须**看得见它。若引用缓存不会失效，这里会少一个分支。
    let second = content_json(&server.call(&inspect));
    let names_after: Vec<&str> = second["branches"]
        .as_array()
        .expect("branches 应为数组")
        .iter()
        .filter_map(|b| b["name"].as_str())
        .collect();
    assert!(
        names_after.contains(&"feature"),
        "复用的会话读到了过时的引用：{names_after:?}"
    );
    assert_eq!(names_after.len(), 2);
}

#[test]
fn non_repository_path_is_reported_as_invalid_params() {
    let dir = std::env::temp_dir().join(format!("repoprism-mcp-notrepo-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("failed to create temp dir");

    let responses = serve(&format!(
        "{}\n",
        call_tool(
            1,
            "repoprism_inspect",
            serde_json::json!({ "path": dir.display().to_string() })
        )
    ));

    assert_eq!(
        responses[0]["error"]["code"], -32602,
        "不是仓库属于参数问题，不该报成内部错误"
    );

    let _ = fs::remove_dir_all(&dir);
}
