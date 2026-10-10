# 发布流程

> 面向维护者。产品定位与需求见 `SPEC.md`，只读约束见 `SECURITY.md`。

## 0. 前置条件

| 事项 | 状态 |
|------|------|
| CI 在待发布的提交上全绿（`main`） | 必须 |
| 三处版本号一致 | `pnpm release:dry` 校验 |
| 管理员有仓库 `contents: write` 权限 | 打 tag 需要 |

## 1. 版本 bump

版本号在本项目里有**三处**独立声明，必须同步改：

| 文件 | 字段 |
|------|------|
| `package.json` | `version` |
| `Cargo.toml` | `[workspace.package] version` |
| `src-tauri/tauri.conf.json` | `version` |

各 crate 自己的 `Cargo.toml` 写的是 `version.workspace = true`，自动继承，**不要**改成硬编码。

改完先校验一遍：

```bash
pnpm release:dry
# 期望输出：next version: <当前版本号>
```

`scripts/check-versions.mjs` 一共查五件事，任何一条不过就以退出码 1 失败并逐条列出原因：

| # | 检查 | 为什么 |
|---|------|--------|
| 1 | `package.json` / workspace `Cargo.toml` / `tauri.conf.json` 三处一致 | 三处都能被单独改 |
| 2 | 每个 workspace 成员是**继承**版本而非硬编码 | v0.2.0 出过：`src-tauri/Cargo.toml` 硬编码 `0.1.0`，只有它没跟着走（P-06） |
| 3 | **`Cargo.lock` 里各成员版本与当前版本号一致** | 漏提交锁文件时 CI 会顺手改掉它（CI 不带 `--locked`）而**显示绿**，只有 `release.yml` 的 `--locked` 构建会炸 —— 即打 tag 那一刻（P-06） |
| 4 | `--expect` / `--expect-ref` 指定的版本号相符 | tag 与版本号对不上是最常见的发版事故 |
| 5 | 成员清单来自根 `Cargo.toml` 的 `[workspace] members` | 不再依赖目录结构假设 |

改了版本号但**忘了 `Cargo.lock`** 时，第 3 条会拦住你 —— 这是 v0.2.0 的真实教训。

## 2. 更新 CHANGELOG

在 `CHANGELOG.md` 顶部把 `## [Unreleased]` 的内容整理成新版本条目，
并补上日期（`## [0.3.0] - YYYY-MM-DD`），同时留下一个空的 `## [Unreleased]` 段落
（Keep a Changelog 要求这一节始终存在）。

## 3. 提交并打 tag

```bash
git add package.json Cargo.toml Cargo.lock src-tauri/tauri.conf.json CHANGELOG.md
git commit -m "chore(release): v0.3.0"
git push origin main

git tag -a v0.3.0 -m "RepoPrism v0.3.0"   # 必须是 vX.Y.Z，release workflow 只认这个形状
git push origin v0.3.0
```

> **顺序不能反**：先 push 提交、确认 `main` 上的 CI 全绿，**再**打 tag。
> tag 触发的 `release.yml` 只读扫描与版本号校验，不会替你跑 Rust / 前端门禁 ——
> 那些由 `ci.yml` 在**提交**上把关。反过来先把 tag 推出去，
> 就可能给一个 CI 还没绿的提交产出安装包。

> **`Cargo.lock` 必须在改动清单里**：提版本号会让锁文件里四个成员的版本一起变，
> 而 `release.yml` 用 `--locked` 构建 CLI / MCP，锁文件过期会直接失败。
> `pnpm release:dry` 现在会检查这一点（P-06）。

> tag 必须带 `v` 前缀。`release.yml` 里 `--expect-ref` 会自动剥掉 `v` 再比对
> `package.json` 里的版本号。

## 4. CI 自动构建

`.github/workflows/release.yml` 由 tag 触发，分四个阶段：

1. **verify** —— 校验 tag 与三处版本号一致，并跑一次只读扫描（自检 + 真实扫描）
2. **desktop**（needs: verify）—— `tauri-apps/tauri-action` 在 ubuntu / macos / windows
   三平台打包，并把安装包挂到 draft release；**draft release 由这一步创建**
3. **cli**（needs: verify, desktop）—— 用 `softprops/action-gh-release` 挂四个 CLI 二进制：
   - `repoprism-linux-x86_64`
   - `repoprism-macos-aarch64`
   - `repoprism-macos-x86_64`
   - `repoprism-windows-x86_64.exe`
4. **mcp-binaries**（needs: verify, desktop）—— 同样四个目标的 MCP Server 二进制
   `repoprism-mcp-<平台>-<架构>[.exe]`

`cli` 与 `mcp-binaries` 都排在 `desktop` 之后不是偶然：draft release 由 `tauri-action`
创建，两个 job 同时抢建同一个 release 会撞 422。这两个 job 之间可以并列 ——
它们只是往已存在的 release 追加文件，上传互不冲突。

产物共 **11 个文件**：3 桌面安装包 + 4 CLI + 4 MCP。

## 5. 人工验收（必做）

产物是 **draft**，不会自动发布。发布前请：

1. 下载三个平台的桌面安装包，各装一遍，打开仓库看四个视图是否正常
2. 下载至少一个 CLI 二进制，`repoprism inspect . --json` 与 `repoprism skill --print` 各跑一次
3. 下载一个 MCP 二进制，`echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | ./repoprism-mcp`
   应列出 **6 个**工具名
4. 确认 Release 说明与 `CHANGELOG.md` 一致

**未签名提示**：当前不做代码签名（缺 Apple Developer 证书与 Windows 代码签名证书），
macOS 会报「无法验证开发者」、Windows 会报 SmartScreen 警告。这是已知情况，
不要因为警告就以为产物坏了。

## 6. 发布后

- 手动把 draft release 点成 published
- P1：Homebrew tap / winget manifest / npm 包
- P1：补代码签名与 macOS 公证（需要证书与 secrets，属独立任务卡）

## 版本号约定

遵循 SemVer：

- `v0.x.y`：MVP 阶段，API 可能变动
- `v1.0.0`：API 稳定，只读宪法冻结

**文档版本与产品版本无关**：`SPEC.md` 顶部的文档版本号不随产品版本变化，
它只标记规格本身的修订（见 `SPEC.md` 开头说明）。

## 发布历史

| 版本 | 日期 | tag | 内容 |
|------|------|-----|------|
| 0.3.0 | 2026-10-10 | `v0.3.0` | `TASK-018` + `P-07` / `P-08` / `P-09` / `P-10`：引用缓存、本地模型出网边界的真实覆盖、worktree / stash / 进行中状态、**blob 只读预览**（图片对比 / 字节预览 / 按版本看文本）。**SPEC 优先级表里不再有未实现的 P0**。tag 打在 `cfc20a9`；release run **38017284575**，**Status Success，8m 51s**，**12 个 job 腿全部 success**（verify 10s / desktop 3-of-3 / cli 4-of-4 / mcp-binaries 4-of-4），全部 `--locked` 构建通过 ⇒ `Cargo.lock` 与版本号同步 |
| 0.2.0 | 2026-10-09 | `v0.2.0` | 首次实际发布：`004` + `005` 批次（变更分析、外部集成、发布硬化、本地 AI 摘要层、MCP 扩展、虚拟滚动）。`release.yml` 首跑 **Status Success**（9m 2s，12 个 job 腿全部完成） |
| 0.1.0 | — | — | **从未发布**：`CHANGELOG.md` 保留了这一段的开发记录，但没有打过 tag，也没有产出过安装包 |

> 首个可下载版本是 v0.2.0。0.1.0 的条目仅作历史记录，不要用它去对 Release 页面。
>
> v0.2.0 与 v0.3.0 的 draft release 都需人工点发布才会对外可见；
> **draft 只有有写权限的人能看到**，所以「11 个附件是否齐全」这一步无法由外部核验
> （`v0.3.0` 已实测：匿名 API 查 `/releases/tags/v0.3.0` 拿不到 `name`、附件数为 0，
> 与 v0.2.0 记录的情况一致），必须登录后在 Releases 页面确认。
>
> **v0.2.0 的 draft 至今仍未人工点发布**（截至 2026-10-10）。v0.3.0 会再产生一个 draft，
> 两个都在 Releases 页面等着。

## 已知缺口

| 缺口 | 影响 |
|------|------|
| 无代码签名 | 用户安装时看到系统警告 |
| draft 附件清单无法从外部核验 | draft release 只有有写权限的人可见，仓库外的人（包括匿名 API）看不到附件列表 |
| **`ubuntu-latest` 将于 2026-10-19 迁移到 Ubuntu 26** | `v0.3.0` 的 run 38017284575 报出这条 notice（每个 ubuntu job 一条）。本次发布仍在 Ubuntu 24 上完成；**2026-10-19 之后的第一次发布会换基础镜像**，Linux 安装包（AppImage / deb）需重新人工验一遍。另有 macOS arm64 runner 的排队变长提示，只影响耗时 |
| GitHub Actions 的 Node 20 弃用告警 | `v0.2.0` 与 `v0.3.0` 两次跑都报出 **12 条** warning（同样的几个 action）：`actions/checkout@v4` / `actions/setup-node@v4` / `pnpm/action-setup@v4` / `softprops/action-gh-release@v2` 仍面向 Node 20、被强制跑到 Node 24。**升版前必须先确认各 action 的新版本号存在**，盲升会当场打断发布链路 |
| 无自动更新（updater） | 用户需手动下载新版本 |
| `productName` 仍是 `repoprism-app` | 安装包与窗口标题显示的是这个旧名，与产品名 `RepoPrism` 不一致；`AGENTS.md` 规定命名由人类主导，一直没擅自改 |
