# TASK-015: MCP 工具扩展

**状态**：已完成（2026-10-09）
**血统**：批次 `005`（原规划 `RepoPrism-仓库棱镜-005`）

## 目标

在 TASK-009 的三个工具之上新增两个只读工具：

- `repoprism_analyze(path)` → `ChangeAnalysis`
- `repoprism_remote(path)` → `RemoteInfo | null`

## 验收标准

- [x] `tools/list` 返回 5 个工具
- [x] `tools/call` 均可正常返回

## 实现

| 文件 | 内容 |
|------|------|
| `crates/repo-prism-mcp/src/main.rs` | `tool_definitions()` 增至 5 项；`call_tool` 新增两条分支 |
| `crates/repo-prism-mcp/README.md` | 工具表由列表改为表格，补两个新工具与 `risks[]` 字段说明 |
| `crates/repo-prism-mcp/tests/protocol.rs` | 三工具用例改为五工具；新增 4 个端到端用例 |

## 与归档规划的一处偏离（不改错误通道分层）

归档的覆盖版把 `handle()` 简化成 `Option<Value>`、错误统一压成 `-32603`，
并丢掉了仓库现役实现里的 `RpcError { code, message }`、`to_json_text`、`ping`
与通知不回包的处理。

**不采用。** 现役分层是有理由的，且已被测试钉住：

| 情形 | 现役错误码 | 归档版 |
|------|-----------|--------|
| 未知方法 | `-32601` | `-32603` |
| 缺参数 / 参数不是仓库 | `-32602` | `-32603` |

MCP 客户端按码分支处理，把「参数错」和「服务端炸了」混成一个码是退步。
本次只在现役实现上**追加**两条工具分支。

## 一处顺带修正：让 `analyze` 也吃到行数统计

归档版用 `ChangeAnalysis::from_status(&snapshot.status)`，这条路径**不带行数**，
于是 `mass-deletion` 规则在 MCP 上永远不命中。

现改为先取 `working_tree_stats()` 再 `from_status_and_stats`，
取不到时 `unwrap_or_else(|_| LineStats::new())` 退化为纯路径规则。
桌面端 `analyze_changes` 同轮一并修正（见 TASK-013 的偏离记录）。

## 验证记录（本机 macOS，2026-10-09）

- `cargo test -p repo-prism-mcp --test protocol`：**14 passed**（原 10 项 + 新增 4 项）
- 新增用例：`analyze_tool_reports_local_risks`、`analyze_tool_is_clean_on_an_untouched_repository`、
  `remote_tool_returns_null_without_an_origin`、`remote_tool_parses_a_github_url`
- 全部走真实子进程 + 真实 stdin/stdout，不 mock 协议
- `cargo clippy --workspace --all-targets -D warnings`：零 warning

## 未验证项

仍未在真实 MCP 客户端（Claude Desktop / Cursor）里联调过。
协议层由测试保证，客户端兼容性未知。
