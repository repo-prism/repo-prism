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
fn tools_list_exposes_exactly_five_readonly_tools() {
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
