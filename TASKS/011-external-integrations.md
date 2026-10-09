# TASK-011: 外部工具一键集成

**状态**：已完成（2026-10-09）
**血统**：批次 `004`（原规划 `RepoPrism-仓库棱镜-004`）

## 目标

从 RepoPrism 一键跳转到 GitDiagram / GitIngest / DeepWiki / GitHub.dev。
从 `origin` remote 自动推断 owner/repo，无 remote 时提示用户输入。

## 验收标准

- [x] `get_remote_info` 返回 owner/repo/host
- [x] 前端工具栏显示跳转按钮
- [x] 点击在系统默认浏览器打开对应 URL
- [x] 无 remote 时按钮不渲染并给出说明

## 实现

| 文件 | 内容 |
|------|------|
| `crates/repo-prism-core/src/model.rs` | `RemoteInfo`（`host` / `owner` / `repo` / `url`） |
| `crates/repo-prism-core/src/git.rs` | `remote_info()`（`git remote get-url origin`）+ `parse_remote()` / `build_remote()`；6 个单测 |
| `crates/repo-prism-core/tests/remote.rs`（新增） | 5 个集成测试，真实仓库 |
| `src-tauri/src/lib.rs` | 新增 `get_remote_info` 命令 |
| `src/lib/api.ts` | `RemoteInfo` + `getRemoteInfo` |
| `src/lib/integrations.ts`（新增） | `Integration` 类型 + 5 个入口 |
| `src/lib/integrations.test.ts`（新增） | 5 个用例 |
| `src/components/IntegrationBar.tsx`（新增） | 集成栏 |
| `src/App.tsx` | 挂载集成栏 + 并行拉取远端信息 |
| `src/App.css` | 集成栏样式 |

## 一处与卡片的差异：不用改扫描器

卡片原文写「`git remote get-url origin` 是只读命令，**加入白名单**」「本次只增加了 `remote`」。
实际上这一步在 [P-02](patch/P-02-readonly-guard-hardening.md) 就做完了：
`remote` 已在动词白名单，`get-url` 已在条件动词的只读标志白名单里。

**但** P-02 的规则 1 门槛过宽，会把 `get-url` 与 `origin` 当子命令报违规 ——
真正需要做的是修扫描器门槛，见 [P-05](patch/P-05-guard-scope-and-write-verbs.md)。
`SECURITY.md` 的白名单表也**无需改动**。

## URL 解析

支持四种写法，其余一律返回 `None`：

```
git@github.com:owner/repo.git            # SSH 简写（分隔符是 :）
ssh://git@github.com/owner/repo.git      # SSH 显式协议
https://github.com/owner/repo.git        # HTTPS
https://user@github.com/owner/repo.git   # HTTPS 带用户名
```

**不猜**是刻意的：本地路径、`file://`、bundles 都返回 `None` 而不是拼一个地址 ——
拼错会把用户带到别的仓库去。集成栏据 `None` 显示「未检测到 origin remote」。

两个细节：

- 先削尾斜杠再削 `.git`，`owner/repo.git/` 这种也削得干净
- 用 `split_once` 而非 `rsplit`：GitLab 子组 `group/sub/repo` 的 `owner=group`、
  `repo=sub/repo`，拼出的 `https://gitlab.com/group/sub/repo` 仍然正确

## 集成清单（5 个）

| id | 地址模板 |
|----|---------|
| `gitdiagram` | `https://gitdiagram.com/{owner}/{repo}` |
| `gitingest` | `https://gitingest.com/{owner}/{repo}` |
| `deepwiki` | `https://deepwiki.com/{owner}/{repo}` |
| `github-dev` | `https://github.dev/{owner}/{repo}` |
| `host` | `https://{host}/{owner}/{repo}` |

卡片验收标准写的是「4 个按钮」，但原规划的清单里就有 5 项，且卡片另外要求
「从 origin remote 自动推断」—— 保留 5 个入口才与清单一致。

第 5 项用 `host` 而非硬编码 `github.com`：硬编码会把 GitLab / Gitea 上的仓库
带到错误的地方。`integrations.test.ts` 有专门用例 (`仓库主页入口用 host 而不是硬编码 github.com`)。

## 两处与归档实现的偏离

1. **归档写 `import { open } from "@tauri-apps/plugin-opener"`，该导出不存在。**
   实测 `@tauri-apps/plugin-opener@2.7.0` 导出的是 `openUrl`。已改用 `openUrl`。
2. **`openUrl` 需要 Tauri 运行时。** 在纯 `pnpm dev`（无 Tauri）里会 reject。
   归档直接 `onClick={() => open(...)}` 会抛出未处理的 Promise。
   现改为失败时把地址显示在栏内让人手动复制，并配一条 `.integration-fallback` 样式。
   这也顺手给「UI 接线无自动化测试」这条已知风险加了一层兜底。

## 安全与只读

- `git remote get-url origin` 只读，**不访问网络**（只读仓库配置里的字符串）
- 跳转一律交给系统默认浏览器（`opener:default` 能力已在 `src-tauri/capabilities/default.json`）
- RepoPrism 自身不抓取外部内容，只负责拼 URL 并交给系统

## 验证记录（本机 macOS，2026-10-09）

- `cargo test -p repo-prism-core --test remote`：5 passed
- `cargo test -p repo-prism-core --lib`：含 `parse_remote` 的 6 个单测
- 前端：`tsc --noEmit` 干净、`vitest run` 含 5 个集成用例
- `bash scripts/read-only-guard.sh`：passed
- **未做**：`pnpm tauri dev` 里实际点一遍按钮（需人工，见 ROADMAP 已知风险）
