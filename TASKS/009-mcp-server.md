# TASK-009: MCP Server

**状态**：已完成（2026-10-09）

> **后续变更**：工具集已由 [TASK-015](015-mcp-extension.md) 从 3 个扩展到 5 个
> （新增 `repoprism_analyze` / `repoprism_remote`）。本卡记录的是当时的形态。

## 目标

实现 stdio 传输的 MCP Server，暴露三个只读工具。

## 协议

- JSON-RPC 2.0，newline-delimited
- 支持 `initialize`、`initialized`、`tools/list`、`tools/call`
- 无 `id` 的消息按通知处理，不返回响应

## 工具

- `repoprism_inspect(path)` → RepoSnapshot
- `repoprism_commits(path, limit, skip)` → CommitInfo[]
- `repoprism_detail(path, sha)` → CommitDetail

## 约束

- **本 crate 不直接调用 Git**，只调用 `repo-prism-core`（AGENTS.md 目录职责表）
- 不暴露 `git` 透传入口：工具名与参数都是白名单固定的
- 读到 EOF 即正常退出；单条消息解析失败只跳过该条，不终止服务

## 验收标准

- [x] 与 Claude Desktop / Cursor 配置后能列出工具
- [x] `tools/call` 返回值放在 `content[0].text`，且 `text` 是**字符串**
- [x] 通知（无 `id`）不产生响应
- [x] 坏输入被跳过，服务继续可用
- [x] 无写操作，CI 只读扫描通过

## 实现

- 重写 `crates/repo-prism-mcp/src/main.rs`（原为 3 行占位）
- `crates/repo-prism-mcp/Cargo.toml`：新增 `serde`、`serde_json`
- 新增 `crates/repo-prism-mcp/README.md`（Claude Desktop / Cursor 配置指引）
- 新增协议测试 `crates/repo-prism-mcp/tests/protocol.rs`（10 个用例，全部走真实子进程）

## 偏离说明

1. **错误码用标准值**：原规划对未知方法统一返回 `-32603`（内部错误）。
   这里改用 `-32601`（method not found）与 `-32602`（invalid params），
   让客户端能按码分支；「不是 Git 仓库」也归为参数问题而非服务端错误。
2. **`text` 字段是字符串**：原规划用 `to_string_pretty` 再放进 `text`，方向正确。
   本轮第一次实现时曾误把 JSON 对象直接塞进 `text`，被协议测试当场抓住——
   严格实现的客户端会拒收，故在源码注释里写明了这条约定。
3. **新增 `ping`**：MCP 的可选方法，零成本，便于客户端探活。

## 禁止事项

- 不得在 MCP 层直接调用 Git
- 不得接受任意命令透传（那是「只读白名单」的破口）
