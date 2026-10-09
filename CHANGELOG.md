# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/) 与 [SemVer](https://semver.org/)。
版本号在三处声明（`package.json` / workspace `Cargo.toml` / `src-tauri/tauri.conf.json`），
由 `pnpm release:dry` 校验一致性。

## [Unreleased]

## [0.1.0] - 2026-10-09

首个可发布版本。

### Added

- **核心读取层**（`repo-prism-core`，唯一允许调用 Git 的 crate）
  - `RepoSnapshot`：当前分支、HEAD、标签、分支列表、变更分组
  - 上游跟踪 ahead / behind，与工作区状态共用同一次 `git status` 子进程
  - 提交图数据（父子关系、refs 附着）
  - 提交详情与 Diff：统一 / 并排两种布局，含 Git 原始行号
  - 变更分析与风险标记：10 条纯本地启发式规则 + `Summarizer` trait（默认 Noop）
  - `origin` remote 解析（HTTPS / SSH，GitLab 子组路径）
- **桌面应用**（Tauri 2 + React 19）：提交图、分支标签、变更分组与风险角标、
  提交详情与 Diff、外部工具一键跳转
- **CLI**（`repoprism`）：`inspect` / `commits` / `detail` / `skill`，
  所有 `--json` 输出共用 `{schema_version, tool, tool_version, data}` 信封
- **MCP Server**（`repoprism-mcp`）：stdio + JSON-RPC 2.0，
  三个只读工具 `repoprism_inspect` / `repoprism_commits` / `repoprism_detail`
- **Agent Skill**：内联进 CLI 二进制，`repoprism skill --path` 展开到工具自己的缓存目录
- **外部集成**：GitDiagram / GitIngest / DeepWiki / GitHub.dev / 仓库主页
- **发布链路**：CI 四平台矩阵 + 只读扫描自检 + 性能门禁；tag 触发的 release workflow
- **文档**：`SPEC.md`（US-1~US-10 需求来源）、`SECURITY.md`、`ROADMAP.md`、
  `AGENTS.md`（只读宪法）、`ADR/001`（用系统 Git 而非 libgit2）

### Security

- **只读宪法**：核心层零写操作，CI 用 `scripts/read-only-guard.sh` 静态扫描把关，
  四层校验：Git 调用门槛 + 动词白名单 + 危险选项黑名单（含 `--textconv` / `--filters`）
  + 全局写动词黑名单
- Diff 一律显式传 `--no-textconv`，不触发仓库自定义的转换器
- 不调用 `git lfs`，LFS 只按文本读取指针
- 二进制文件只标记、不读取内容
- Diff 以文本节点渲染，不使用 `dangerouslySetInnerHTML`
- 截断永远显式标记（原始 patch 2 MiB / 结构化 5000 行），不静默丢弃

### Known limitations

- **未做代码签名**：macOS 未公证、Windows 未签名，安装时会有系统警告；
  发布产物为 draft，需人工确认
- **release workflow 尚未在 GitHub 上实际执行过**
- UI 接线层（`App ↔ invoke`）无自动化测试，需人工 `pnpm tauri dev` 确认
- MCP Server 未与真实 Claude Desktop / Cursor 客户端联调
