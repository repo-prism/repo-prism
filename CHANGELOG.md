# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/) 与 [SemVer](https://semver.org/)。
版本号在三处声明（`package.json` / workspace `Cargo.toml` / `src-tauri/tauri.conf.json`），
由 `pnpm release:dry` 校验一致性。

## [Unreleased]

## [0.2.0] - 2026-10-09

`004` + `005` 两个批次的内容。**这是本仓库的首次实际发布**：`0.1.0` 的记录保留在下方，
但它从未打过 tag，也没有产出过任何安装包——第一个可下载的版本是 v0.2.0。

### Added

- **本地 AI 摘要层**（`crates/repo-prism-core/src/summarizer.rs`）
  - `OllamaSummarizer` 实现 `Summarizer` trait，接入用户自己的本地模型服务
  - **只允许 `http://` + 回环地址**（`localhost` / `127.0.0.1` / `[::1]`）；
    校验解析出 host 后精确比对，拒绝 `http://localhost.evil.com`、
    `http://localhost@evil.com` 这类只在前缀上像 localhost 的写法
  - 只发送文件的**相对路径与规则结果**；不发送仓库路径、remote URL、diff 正文或文件内容
  - 默认关闭；设置持久化到用户配置目录（读取失败回默认值）
- **提交级 AI 分析**：提交详情面板的「AI 分析此提交」按钮，复用同一摘要器
- **AI 设置面板**：endpoint / model / 启用开关 + 测试连接（只测不存）
- **MCP 工具扩展**：新增 `repoprism_analyze` 与 `repoprism_remote`，工具数 3 → 5
- **提交列表虚拟滚动**：行高 48px、overscan 8；300 条提交下渲染行数 < 80
- **`repoprism_analyze` 走带行数统计的规则路径**：桌面端与 MCP 的
  `mass-deletion` 规则现在真正生效（此前 `from_status` 不带行数，该规则恒不命中）
- **发布产物新增 MCP 二进制**：`release.yml` 增补 `mcp-binaries` job，
  与 CLI 一样出四个目标的 `repoprism-mcp-*`（此前只发桌面安装包与 CLI）

### Changed

- `analyze_changes` 在行数统计失败时**降级为纯路径规则**，而不是让整个分析报错
- `ureq` 关闭默认特性（不带 TLS）：回环地址上的 https 无实际用途，
  关掉可整棵移除 `rustls` / `ring` / `webpki` 依赖树
- **`ci.yml` 的 rust job 在 ubuntu 上补装 Tauri Linux 系统依赖**
  （WebKitGTK / GTK / libsoup 等），并显式 `fail-fast: false`

### Fixed

- **CI 在 ubuntu 上一直是红的**（2026-10-07 起 5 次运行全部失败，含 `004` / `005` 两批）。
  失败点是 ubuntu leg 的 `cargo clippy --workspace`（exit 101）：它会连 `src-tauri`
  一起检查，而该 job 从未安装 Linux 系统库。macOS / Windows 的系统 SDK 自带等价物，
  所以只有 ubuntu 红；又因矩阵默认 `fail-fast`，一个腿红掐掉另两个腿，
  日志里只剩 `cancelled`，把问题掩盖了 5 次运行。详见 `ROADMAP.md` 的风险清单

### Security

- 新增威胁 7「代码被发往外部服务」及其三层缓解（真解析 host / 收窄 scheme 与依赖 /
  默认关闭且只送最小内容），见 `SECURITY.md`
- **明确记录**：只读扫描器**看不见 HTTP 调用**，出网闸门只由 `summarizer.rs`
  的单元测试兜底 —— 这是本规格里唯一不靠静态扫描的安全约束

### Known limitations

- **本地 AI 层没有端到端验证**：校验与 prompt 构造有 13 个单测，
  但本机未安装 Ollama，**一次真实的模型调用都没有发生过**（HTTP 路径未经运行验证）
- 虚拟滚动的区间计算有断言覆盖，但真实滚动帧率未测量（需 GUI）
- 其余同 v0.1.0：未签名、UI 接线无自动化测试、MCP 未与真实客户端联调
- **`release.yml` 的第一次实机运行就是 v0.2.0 这次** —— 结果是 **Status Success**
  （9m 2s，12 个 job 腿全部完成）。但产物是 **draft，人工验收前不会对外可见**，
  且 draft 的附件清单无法从仓库外核验（匿名访问看不到），需登录确认「11 个附件」齐全
- **GitHub Actions 的 Node 20 弃用告警**：首跑报出 12 条 warning，多个 action 仍面向
  Node 20、被强制跑到 Node 24。仅告警，但升版前必须逐一确认新版本号存在

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
