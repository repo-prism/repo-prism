# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/) 与 [SemVer](https://semver.org/)。
版本号在三处声明（`package.json` / workspace `Cargo.toml` / `src-tauri/tauri.conf.json`），
由 `pnpm release:dry` 校验一致性。

## [Unreleased]

### Changed

- **首屏读取路径的子进程次数减半**（`ADR-002` / `TASK-017`）

  | | 前 | 后 |
  |---|---|---|
  | `snapshot()` | 5 次 | **2 次** |
  | `snapshot()` + `commits()` | 7 次 | **4 次** |

  - 分支、标签、ref 映射由**三次** `for-each-ref` 合并为**一次**
  - 分支名与 HEAD 改从 `status --porcelain=v2 --branch` 的
    `# branch.head` / `# branch.oid` 头部行取，省掉 `symbolic-ref` 与 `rev-parse HEAD`
  - **行为逐字保持不变**：既有 16 个 `snapshot` 集成测试（含分离头指针、无提交仓库、
    上游 ahead/behind）全部通过
- **本地模型请求显式不使用环境代理**（P-08）：`summarizer.rs` 不再调用
  `ureq::get` / `ureq::post` 这两个顶层便捷函数（它们构造的是默认 agent，
  从调用处**看不出作者有没有考虑过代理**），改为 `local_agent()` 显式构造，
  且**刻意不调用** `AgentBuilder::proxy_from_env()`。
  运行时行为不变（实测 ureq 2.12.1 的默认 agent 本就不改道回环请求），
  但这条安全要求现在写进了代码并被测试钉住，而不是依赖某个库版本的默认值。

### Added

- **机器无关的性能门禁**：`Git::spawns()` 暴露本实例发起的子进程次数，
  `tests/perf.rs::spawn_counts_are_pinned` 逐方法钉住次数。
  实测显示耗时几乎完全由子进程数决定（4 万提交仓库上的 `for-each-ref` 与
  不读仓库的 `git --version` 耗时量级相同），而毫秒阈值会随 runner 抖动。
- **规模诊断基准** `crates/repo-prism-core/tests/scale.rs`（`#[ignore]`，不进 CI 常跑）：
  按 1k / 10k / 40k 提交扫描 `snapshot()` / `commits()`，逐个拆解子命令，
  并用 A/B 序列对比测量合并收益。保留「改动前」的 argv 序列作为可重测的对照。
- **`ADR/002`**：Git 后端演进决策 —— **否决**把系统 Git 换成 `gix`，
  理由与「什么条件下重估」都写进了文档。
- **本地模型 HTTP 路径的真实覆盖**（工程补丁 P-07，
  `crates/repo-prism-core/tests/summarizer_http.rs`）
  - 用 `std::net::TcpListener` 在 127.0.0.1 上起 stub（**不引新依赖**），
    让 `list_models` / `generate` / `summarize` 真实走一遍 TCP → HTTP 解析 → JSON 取值
  - 覆盖四条降级路径：端口无人监听 / 5xx / 非法 JSON / 空白或缺失的 `response`
  - 断言请求的方法、路径、`Content-Type`，并把请求体反序列化后逐字段核对
    （`model` / `stream: false` / `prompt` / `options.temperature`）
  - 「出网 body 只含相对路径」这条不变式改由**读请求体**直接断言
- **与 Ollama 官方 API 文档的字段契约核对**（工程补丁 P-08，
  `crates/repo-prism-core/tests/summarizer_contract.rs`）
  - 输入**逐字取自**官方 `docs/api.md` 的响应示例，而不是我们自己编写 ——
    这是 P-07 无法覆盖的盲区：响应形状自编时，字段名写错会**两边一起错**、测试照样绿
  - 覆盖 `generate` 的 `response` 与非流式全字段（`context` / `total_duration` /
    `eval_count`…）、`list_models` 的 `models[].name` 与嵌套 `details`
  - 含一条反向用例：字段名不是 `response` 时必须取不到正文，**证明上一条断言真的在核对字段名**
  - 多行 NDJSON 必须整体判为失败，而不是截取第一行当摘要
- **本地模型出网边界：环境代理不得改道回环请求**（工程补丁 P-08，
  `crates/repo-prism-core/tests/summarizer_egress.rs`）
  - `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` 会**改写连接目标**：一旦被遵循，
    endpoint 上写着的回环地址就不再是实际连接目标，`validate_endpoint` 的 host 校验形同虚设
  - 测试先把代理指向一个**已确认无人监听**的端口（并清掉 `NO_PROXY` 这条豁免通道），
    再断言请求仍到达回环 stub —— 于是「用了代理」必然导致红，绿与红被彻底分开
  - 同时断言请求行是 origin-form 而非代理用的 absolute-form
- **回环 HTTP stub 抽为共用模块** `crates/repo-prism-core/tests/common/stub.rs`：
  非阻塞 `accept()` 继承 `O_NONBLOCK`、`Connection: close` 这两个坑**只保留一份**，
  由 `summarizer_http` 与 `summarizer_egress` 共用

### Fixed

- `ref_map()` / `branches()` / `tags()` 原先各自发起一次 `for-each-ref`，
  读的是同一份引用表。现已合并为一次（`ADR-002` 实测的直接副产品）。
- `summarizer.rs` 中真正发送请求的两处 `ureq` 调用此前**一次都没被执行过**
  （本机没有 Ollama），而 `SPEC.md` / `SECURITY.md` 把「不把代码送到外部」
  声明为**只由测试兜底**——那条防线是空的。
  这类失效的症状是「AI 摘要静默返回 `None`」：界面上只会少一块内容，没有报错，
  因此可以长期潜伏。现已由上述集成测试覆盖。
- 测试本身也验证过**会失败**：用 5 个探针逐个破坏被测逻辑
  （尾斜杠拼接 / 取值字段名 / 空响应过滤 / 出网 body 注入绝对路径 / 报错文案），
  每一步都有对应用例变红。

### 已知边界（本次未解决）

- **「省了多少毫秒」在本机无法可靠测量**：同一方案在同一轮 A/B 内两次采样差 2.7 倍
  （543ms 与 203ms），**方案间的差异小于方案内的抖动**。干净轮测得 55%、加载轮测得 32%。
  因此本次**不承诺任何百分比**，验收标准只放在确定性的子进程次数上。
  要拿到可信的毫秒收益，需要在有真实大仓库且负载可控的环境里复测。
- **本地 AI 层仍无真实模型验证**：P-07 覆盖的是**我们这一侧**的 HTTP 路径
  （stub 的响应形状是我们自己写的），P-08 的 `summarizer_contract` 已把响应形状换成
  **官方文档逐字示例**，于是字段名写错能被测出来。但**输入来自文档，不是一台真的 Ollama**：
  证明不了某个具体版本的实际行为与文档一致（文档与实现之间仍有差距）。
  `SPEC.md` US-7 该项仍是 `[待实现]` —— 截至本版本**一次真实模型调用都没有发生过**。
- **本机无法安装 Ollama**，故上述缺口只能靠「文档契约 + 回环 stub」逼近，
  无法在本环境内闭合。**没有**改用任何公网模型端点来绕过它：
  那需要放宽 `validate_endpoint` 的回环限制，等于把本版本新增的第 4 层出网缓解作废，
  且会把真实的仓库内容送到未经审核的第三方。

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
