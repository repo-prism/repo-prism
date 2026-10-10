# repoprism-mcp

RepoPrism 的 MCP Server，通过 stdio 提供只读 Git 工具。

它是 `repo-prism-core` 的**一层薄包装**：所有 Git 读取都走 core，
因此与桌面端、CLI 共用同一套只读白名单（见 `SECURITY.md`）。

## 构建

```bash
cargo build -p repo-prism-mcp --release
# 产物：target/release/repoprism-mcp
```

## 在 Claude Desktop 中配置

编辑 `~/Library/Application Support/Claude/claude_desktop_config.json`（macOS）
或 `%APPDATA%\Claude\claude_desktop_config.json`（Windows）：

```json
{
  "mcpServers": {
    "repoprism": {
      "command": "/absolute/path/to/repoprism-mcp"
    }
  }
}
```

## 在 Cursor 中配置

`Settings → MCP → Add Server`：

```json
{
  "repoprism": {
    "command": "/absolute/path/to/repoprism-mcp"
  }
}
```

## 可用工具

| 工具 | 说明 |
|------|------|
| `repoprism_inspect(path)` | 仓库快照（HEAD、分支、标签、变更分组） |
| `repoprism_commits(path, limit, skip)` | 提交历史（`limit` 上限 2000） |
| `repoprism_detail(path, sha)` | 提交详情与原始 diff |
| `repoprism_analyze(path)` | 未提交改动的本地风险分析（纯本地规则，不调模型、不联网） |
| `repoprism_remote(path)` | `origin` 的结构化 host / owner / repo / url；无 remote 时为 `null` |
| `repoprism_blob(path, file, rev?, size_only?)` | 某个版本下单个文件的内容预览；`size_only` 为真时只报大小、不读内容 |

`tools/call` 的返回值放在 `content[0].text` 里，是一个 JSON 值。

`repoprism_blob` 返回的 `kind` 是带 tag 的枚举，**线上形状是嵌套的**
（`kind.kind` 才是 `"text"` / `"image"` / …）。图片类给 `content`（base64），
文本类给 `text`，认不出的二进制给 `hex`，LFS 指针只给 `oid` 与真实大小
（**内容从未下载**）。注意 `size` 有两个：外层是**仓库里那个文件**的字节数，
LFS 指针里那个是**它指向的对象**的大小 —— 两者不是一回事。

`repoprism_analyze` 返回的 `risks[]` 每条含 `rule_id` / `level` / `message` / `path`，
`by_level` 是三个等级的计数 —— Agent 可据此决定先看哪个文件。

## 协议

- JSON-RPC 2.0，newline-delimited（每条消息一行）
- 支持 `initialize` / `notifications/initialized` / `ping` / `tools/list` / `tools/call`
- 无 `id` 的消息按**通知**处理，不返回响应

## 安全

所有工具只读，绝不修改仓库。服务端不接收任何写操作参数，
也不暴露 `git` 透传入口——工具名与参数都是白名单固定的。
