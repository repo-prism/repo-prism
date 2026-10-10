//! RepoPrism MCP Server。
//!
//! 通过 stdio 提供 JSON-RPC 2.0 服务，暴露只读 Git 工具。
//! 传输格式：newline-delimited JSON（每条消息一行）——MCP 的 stdio 传输约定。
//!
//! **安全边界**：本 crate 不直接调用 Git，只调用 `repo-prism-core`
//! （AGENTS.md 的目录职责表）。因此只读约束与 GUI / CLI 共用同一套实现。

use anyhow::Result;
use repo_prism_core::{ChangeAnalysis, Git, LineStats, RepoSet, MAX_PREVIEW_BYTES};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::sync::{Arc, OnceLock};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "repoprism-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// `commits` 工具的单次上限，与 CLI 的 `--limit` 保持一致。
const MAX_LIMIT: usize = 2000;

/// 跨请求复用的仓库会话表（TASK-019 / US-9）。
///
/// MCP 是被客户端拉起后**长期驻留**的进程，而此前每个请求都 `Git::open`
/// —— 每次都要付一次 `rev-parse`，且引用缓存（TASK-018）在请求结束就随实例丢掉。
///
/// 失效判据与桌面端**同一套**（引用内容指纹），因此不需要调用方声明「仓库变了」：
/// 客户端连着问同一个仓库时省掉重复的 `rev-parse`，仓库在两次调用之间被改动时
/// 缓存会自动失效。表有上限，所以长驻进程的内存不会随「问过的仓库数」无限增长。
fn sessions() -> &'static RepoSet {
    static SET: OnceLock<RepoSet> = OnceLock::new();
    SET.get_or_init(RepoSet::with_default_cap)
}

fn main() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        // 单条消息解析失败不应让服务退出：客户端可能发了非 JSON 的日志行。
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(resp) = handle(&msg) {
            serde_json::to_writer(&mut out, &resp)?;
            writeln!(&mut out)?;
            out.flush()?;
        }
    }
    Ok(())
}

fn handle(msg: &Value) -> Option<Value> {
    let method = msg.get("method")?.as_str()?;

    // 通知（无 id）不返回响应。`notifications/initialized` 走的就是这条路径。
    let id = msg.get("id")?.clone();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(&params),
        other => Err(RpcError::method_not_found(other)),
    };

    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": e.code, "message": e.message }
        }),
    })
}

/// JSON-RPC 错误。用标准错误码，便于客户端按码分支处理。
struct RpcError {
    code: i32,
    message: String,
}

impl RpcError {
    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {method}"),
        }
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "repoprism_inspect",
            "description": "Read a local Git repository's read-only snapshot: HEAD, branches, tags, and change groups (conflicts / staged / unstaged). Never modifies the repository.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to a local Git repository" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "repoprism_commits",
            "description": "Read a repository's commit history in read-only mode.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "limit": { "type": "integer", "default": 100, "minimum": 1, "maximum": 2000 },
                    "skip": { "type": "integer", "default": 0, "minimum": 0 }
                },
                "required": ["path"]
            }
        },
        {
            "name": "repoprism_detail",
            "description": "Read a single commit's detail: author, dates, message, changed files, and unified diff.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "sha": { "type": "string" }
                },
                "required": ["path", "sha"]
            }
        },
        {
            "name": "repoprism_analyze",
            "description": "Analyze a repository's uncommitted changes with a local rule engine. Returns risk findings grouped by severity plus a one-line summary. Fully local: no network calls, no model calls.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to a local Git repository" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "repoprism_remote",
            "description": "Read a repository's origin remote as structured host / owner / repo / url. Returns null when the repository has no origin remote.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to a local Git repository" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "repoprism_blob",
            "description": "Read a single file's content at a given revision, read-only. Returns its real size plus a kind: image (base64 payload), text, binary (hex dump), lfs_pointer (never fetched), or too_large (nothing was read). Set size_only to only measure the size and read no bytes at all.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to a local Git repository" },
                    "rev": { "type": "string", "default": "HEAD", "description": "Revision to read the file from" },
                    "file": { "type": "string", "description": "Path of the file inside the repository" },
                    "size_only": { "type": "boolean", "default": false, "description": "Only report the size; read no content (one subprocess)" }
                },
                "required": ["path", "file"]
            }
        }
    ])
}

fn call_tool(params: &Value) -> Result<Value, RpcError> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RpcError::invalid_params("missing required string field: name"))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let text = match name {
        "repoprism_inspect" => {
            let path = arg_str(&args, "path")?;
            to_json_text(open(&path)?.snapshot().map_err(tool_failure)?)?
        }
        "repoprism_commits" => {
            let path = arg_str(&args, "path")?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize;
            let skip = args.get("skip").and_then(Value::as_u64).unwrap_or(0) as usize;
            to_json_text(
                open(&path)?
                    .commits(limit.clamp(1, MAX_LIMIT), skip)
                    .map_err(tool_failure)?,
            )?
        }
        "repoprism_detail" => {
            let path = arg_str(&args, "path")?;
            let sha = arg_str(&args, "sha")?;
            to_json_text(open(&path)?.commit_detail(&sha).map_err(tool_failure)?)?
        }
        "repoprism_analyze" => {
            let path = arg_str(&args, "path")?;
            let git = open(&path)?;
            let snapshot = git.snapshot().map_err(tool_failure)?;
            // 行数统计取不到就退化为纯路径规则，而不是让整个工具失败 ——
            // 「大量删除」这条规则少命中一次，比工具直接报错可接受得多。
            let stats = git
                .working_tree_stats()
                .unwrap_or_else(|_| LineStats::new());
            to_json_text(ChangeAnalysis::from_status_and_stats(
                &snapshot.status,
                &stats,
            ))?
        }
        "repoprism_remote" => {
            let path = arg_str(&args, "path")?;
            to_json_text(open(&path)?.remote_info().map_err(tool_failure)?)?
        }
        "repoprism_blob" => {
            let path = arg_str(&args, "path")?;
            let rev = args.get("rev").and_then(Value::as_str).unwrap_or("HEAD");
            let file = arg_str(&args, "file")?;
            let size_only = args
                .get("size_only")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let git = open(&path)?;
            if size_only {
                // 「先量后读」里「量」的那一半：一个字节都不读。
                let size = git
                    .blob_size(rev, &file)
                    .map_err(tool_failure)?
                    .ok_or_else(|| {
                        RpcError::invalid_params(format!(
                            "{rev}:{file} 在这个仓库里不存在（版本或路径不对）"
                        ))
                    })?;
                to_json_text(json!({
                    "rev": rev,
                    "path": file,
                    "size": size,
                    "preview_cap_bytes": MAX_PREVIEW_BYTES,
                }))?
            } else {
                to_json_text(git.blob_preview(rev, &file).map_err(tool_failure)?)?
            }
        }
        other => return Err(RpcError::method_not_found(other)),
    };

    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    }))
}

/// 按 MCP 约定，工具结果要放在 `content[].text` 里，而 `text` **必须是字符串**，
/// 不能直接塞 JSON 对象——否则严格实现的客户端会拒收。
fn to_json_text<T: serde::Serialize>(value: T) -> Result<String, RpcError> {
    serde_json::to_string_pretty(&value).map_err(tool_failure)
}

/// 取仓库会话。`path` 不是仓库属参数问题，不是服务端内部错误。
///
/// 返回 `Arc<Git>` 而不是 `Git`：会话表持有同一个实例，请求只是借用它。
/// 借到的句柄在本次请求结束后即释放，但**表里的那份还在** —— 下一次请求
/// 命中同一个仓库时就不用再解析一遍仓库布局。
fn open(path: &str) -> Result<Arc<Git>, RpcError> {
    sessions()
        .open(path)
        .map_err(|e| RpcError::invalid_params(e.to_string()))
}

/// 工具执行失败。MCP 约定：工具级错误仍返回 `result`，但标记 `isError`。
fn tool_failure(e: impl std::fmt::Display) -> RpcError {
    RpcError::invalid_params(e.to_string())
}

fn arg_str(args: &Value, key: &str) -> Result<String, RpcError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| RpcError::invalid_params(format!("missing required string argument: {key}")))
}
