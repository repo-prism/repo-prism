# ROADMAP.md — RepoPrism 血统与执行计划

> 更新日期：2026-10-09
> 依据：`SPEC.md`（唯一需求来源）/ `AGENTS.md` / `SECURITY.md` / `ADR/001` 与实际源码
> 本文只做**规划**。功能变更以 `SPEC.md` 为准，落地以 `TASKS/` 任务卡为单位。

---

## 一、项目目标

**只读**的仓库智能工具：把同一份本地 Git 仓库折射成提交图、变更分组、Diff、架构图、
文本化仓库等多种视图，**同一份结构化数据同时供人类与 AI Agent 消费**
（桌面端 / CLI / Skill / MCP 四条出口）。

四条不可让渡的原则：只读 · 本地优先 · AI 协同 · 编排而非重造。

---

## 二、编号规则

产品血统 `TASKS/001-0xx` 由**原规划**一次性排定（001–016 排到 v0.2，017–020 属 v0.3），
只由产品能力占用。工程补丁（安全加固、依赖卫生、文档对齐等不新增产品能力的事）
走 `TASKS/patch/P-0N` 序列。

**发布批次不占卡号**：`006` 是发布批次（提版本号 + 打 tag），不新增产品能力，
因此没有 `TASK-0xx` 卡片，只记在 `docs/RELEASE.md` 的发布历史与 `CHANGELOG.md` 的版本条目里。

> **为什么有这条规则**：2026-10-09 出过一次真实事故——一轮自发的工程整改把补丁卡
> 编成了 `TASK-008`…`TASK-011`，而血统里 008 = CLI 规范化 + Skill 打包、009 = MCP Server、
> 010 = 变更分析、011 = 外部集成，四个号全被占掉，导致「TASK-008 是什么」有两个
> 互不相容的答案。细节见 `TASKS/patch/README.md`。

---

## 三、产品血统与状态

| 批次 | 卡 | 主题 | 状态 |
|------|----|------|------|
| `001` | TASK-001 | 初始化 Cargo workspace | ✅ |
| `001` | TASK-002 | core 读取 `RepoSnapshot` | ✅ |
| `001` | TASK-003 | CLI `inspect --json` | ✅ |
| `002` | TASK-004 | core 提交图 + Tauri 命令层 | ✅ |
| `002` | TASK-005 | 前端基础层 + 侧边栏 + 变更面板 | ✅ |
| `002` | TASK-006 | 提交图组件 | ✅ |
| `003` | [TASK-007](TASKS/007-core-diff.md) | 提交详情与 diff 视图 | ✅ |
| `003` | [TASK-008](TASKS/008-cli-schema-skill.md) | CLI 规范化 JSON Schema + Skill 打包 | ✅ |
| `003` | [TASK-009](TASKS/009-mcp-server.md) | MCP Server | ✅ |
| `004` | [TASK-010](TASKS/010-analysis.md) | 变更分析与风险标记 | ✅ |
| `004` | [TASK-011](TASKS/011-external-integrations.md) | 外部工具一键集成 | ✅ |
| `004` | [TASK-012](TASKS/012-release-hardening.md) | 发布硬化 | ✅ |
| `005` | [TASK-013](TASKS/013-ollama-summarizer.md) | 本地 AI 摘要层（Ollama） | ✅ |
| `005` | [TASK-014](TASKS/014-commit-ai.md) | 提交级 AI 分析 | ✅ |
| `005` | [TASK-015](TASKS/015-mcp-extension.md) | MCP 工具扩展（3 → 5） | ✅ |
| `005` | [TASK-016](TASKS/016-virtual-scroll.md) | 前端虚拟滚动 | ✅ |
| `006` | — | 发布 v0.2.0（tag `v0.2.0`） | ✅ 2026-10-09 |
| v0.3 | [TASK-017](TASKS/017-git-read-perf.md) | Git 读取层性能：减 spawn（原定「`gix` 后端」，经 [`ADR-002`](ADR/002-git-backend-evolution.md) 改手段） | ✅ 2026-10-09 |
| v0.3 | TASK-018–020 | 增量缓存 / 多仓库 / PR 只读视图 | ⬜ 未开始 |

> **TASK-017 的手段变更**是 v0.3 的第一个决策点。血统里它的主题是「`gix` 后端」，
> `ADR-002` 实测后否决了换后端，改为在同一后端里合并子进程；**目标（大仓库首屏性能）未变**，
> 卡号未变。详见 `ADR/002` 与 `TASKS/017` 的「与归档规划的一处偏离」。

### 已知偏差（在 TASK-003 批次前后补做的工程补丁）

| 卡 | 主题 | 状态 |
|----|------|------|
| [P-01](TASKS/patch/P-01-upstream-tracking.md) | 补全上游跟踪 ahead/behind（US-1 缺口） | ✅ |
| [P-02](TASKS/patch/P-02-readonly-guard-hardening.md) | 只读白名单加固到参数级 | ✅ |
| [P-03](TASKS/patch/P-03-deps-ci-hygiene.md) | 依赖与 CI 卫生（pnpm / build / 性能门禁） | ✅ |
| [P-04](TASKS/patch/P-04-docs-consistency.md) | SPEC 补全 + SECURITY 对齐 | ⚠️ 已被取代 |
| [P-05](TASKS/patch/P-05-guard-scope-and-write-verbs.md) | 只读扫描器补调用门槛与写动词黑名单 | ✅ |
| [P-06](TASKS/patch/P-06-version-gate-gap.md) | 版本闸门补两个洞（成员继承 + Cargo.lock） | ✅ |
| [P-07](TASKS/patch/P-07-http-path-coverage.md) | 本地模型 HTTP 路径用回环 stub 变实（原防线从未运行过） | ✅ 2026-10-09 |

---

## 四、当前门禁（2026-10-09 实测）

| 检查项 | 命令 | 结果 |
|--------|------|------|
| 只读扫描自检 | `bash scripts/read-only-guard.sh --self-test` | ✅ 26/26（14 正例 + 12 反例） |
| 只读扫描 | `bash scripts/read-only-guard.sh` | ✅ passed |
| Rust 格式 | `cargo fmt --all -- --check` | ✅ 干净 |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 无警告 |
| Rust 测试 | `cargo test --workspace` | ✅ **127 passed**（core 99 / CLI 14 / MCP 14） |
| 子进程次数（`ADR-002`） | `cargo test -p repo-prism-core --test perf` | ✅ `spawn_counts_are_pinned`：`snapshot()` 2 次、`snapshot()+commits()` 4 次（原 5 / 7） |
| 性能门禁 | `cargo test -p repo-prism-core --test perf` | ✅ snapshot 224ms / 500ms、commits 121ms / 800ms |
| 前端 lint | `biome check src` | ✅ 27 files |
| 前端类型 | `tsc --noEmit` | ✅ 无错误 |
| 前端测试 | `vitest run` | ✅ 41 passed（5 files） |
| 前端构建 | `vite build` | ✅ `dist/assets/index-qz-52nYw.js` 244.43 kB（gzip 76.49 kB） |
| 版本一致性 | `pnpm release:dry` | ✅ `next version: 0.2.0` |
| Workflow YAML | `yaml.safe_load` 解析 `ci.yml` / `release.yml` | ✅ 可解析，依赖顺序符合预期 |
| 远端 CI（真实执行） | GitHub Actions 的 `CI` workflow | ✅ 修复后**四次**全绿：`99478ea` → run 37912380547、`278ceb9` → run 37913817616、`039399f`（P-07）→ run 37920164788、`02060e0`（ADR-002 / TASK-017）→ run 37924123551（6 腿全 success，2m 12s）。修复前（2026-10-07 19:33 起）**当时存在的 5 次运行全部失败** |
| 远端发布（真实执行） | GitHub Actions 的 `Release` workflow | ✅ tag `v0.2.0` → run 37914120667，**Status Success，9m 2s**（verify 9s / desktop 3-of-3 / cli 4-of-4 / mcp-binaries 4-of-4） |

**core 99 项的构成**：lib 53（含 `summarizer` 13、`analysis` 16、`git` 的 remote/hash 解析与
`parse_head_meta` 等）/ analysis 5 / diff 10 / perf 2 / remote 5 / snapshot 16 /
**`summarizer_http` 8（P-07 新增）**。

**性能门禁的采样方式**：预热一次 + 采样 3 次取**最小值**。

**为什么还要一条「子进程次数」门禁**（`ADR/002`）：实测显示耗时几乎完全由
「起了几个 `git` 进程」决定 —— 4 万提交仓库上的 `for-each-ref` 与完全不读仓库的
`git --version` 耗时量级相同；提交数 1 万 → 4 万，`snapshot()` 耗时无系统性变化。
而**毫秒阈值会随 runner 抖动**（实测同机连跑三次 208/470/576ms），
**spawn 次数不会**。所以 spawn 次数是比毫秒更硬、且能定位到具体方法的门禁，
`tests/scale.rs`（`#[ignore]`）保留「改动前」的 argv 序列作为可重测的对照。

**`summarizer_http` 补的是什么**：`summarizer.rs` 那 13 个单测是纯函数，
而真正把字节送出进程的两处 `ureq` 调用在 P-07 之前**一次都没执行过**（本机无 Ollama）。
现在用 `std::net::TcpListener` 在 127.0.0.1 起 stub 跑通往返，并把「出网 body 只含相对路径」
钉成断言。**它不替代真实模型验证**——见下方「未解决的风险」。
单次采样会把调度抖动算成代码性能——同一份代码实测出现过 208ms / 470ms / 576ms
（576ms 那次直接超预算、测试变红）。绝对值随机器负载浮动，**只应看是否超预算**，
不要跨机器比较。（上表 224ms 是负载较轻时的实测值；同一批代码在重负载下测到过 244ms。）

**本机沙箱备忘**：`vitest run` 的并行 worker 在本机受限环境下会被 OOM kill
（表现为 `no tests / N errors`，其实是资源问题）；加 `--no-file-parallelism` 即稳定。
CI runner 无此问题，故未改项目配置。

---

## 五、规格覆盖

| 用户故事 | 状态 |
|---------|------|
| US-1 仓库状态（分支 / HEAD / 标签 / 提交图 / 上游计数） | ✅ 已实现（worktree / stash / 合并变基状态待实现） |
| US-2 变更分组 + 风险角标 | ✅ 已实现 |
| US-3 提交详情 + Diff（统一 / 并排，含原始行号，`patch` 原文） | ✅ 已实现（图片对比 / 字节预览待实现） |
| US-4 CLI（`inspect` / `commits` / `detail` / `skill` + schema 信封） | ✅ 已实现（`open --view` 待实现） |
| US-5 Agent Skill | ✅ 已实现 |
| US-6 MCP Server（5 个只读工具） | ✅ 已实现 |
| US-7 AI 变更摘要 | ✅ 已实现（本地 10 条规则 + 可选本地模型层，默认关闭）<br>⚠️ 模型层的真实调用未端到端验证（本机无 Ollama） |
| US-8 一键跳转集成 | ✅ 已实现 |
| US-9 多仓库工作区 | ❌ 未实现（TASK-019） |
| US-10 PR/MR 只读视图 | ❌ 未实现（TASK-020） |

---

## 六、下一步

`006` 已执行：三处版本号提到 `0.2.0`、`CHANGELOG.md` 定版、`release.yml` 增补 MCP 二进制 job，
tag `v0.2.0` 已推送。**v0.1.0 从未打过 tag**，所以 v0.2.0 是第一个真正可下载的版本。

随后补了 [P-07](TASKS/patch/P-07-http-path-coverage.md)：本地模型的 HTTP 路径在
`SPEC`/`SECURITY` 里被声明为「只由测试兜底」，而那条测试**从未运行过那两行代码**。
现在用回环 stub 把它跑通并钉住出网 body 的内容。

接下来按原规划进 v0.3：

1. ~~**TASK-017 `gix` 后端**~~ —— ✅ 2026-10-09，但**手段被 [`ADR-002`](ADR/002-git-backend-evolution.md) 换掉了**：
   实测发现瓶颈不是仓库规模而是子进程数，且换 `gix` 会让只读扫描器空转。
   改为在同一后端里合并子进程：`snapshot()` 5 → 2、`snapshot()+commits()` 7 → 4，
   并把 **spawn 次数**做成机器无关的门禁。`gix` 转为条件触发
   （触发条件写在 `ADR/002` 第五节：真实大仓库上首屏 > 1s 且瓶颈已不是 spawn）。
2. **TASK-018 增量缓存** —— 提交图分页与 diff 结果的本地缓存。
   `ADR/002` 已指出其中一条可量化的收益：把共用的引用映射跨调用缓存下来，
   可以把 4 次 spawn 再降到 3 次
3. **TASK-019 多仓库工作区**（US-9）
4. **TASK-020 PR / MR 只读视图**（US-10）—— 需要网络与凭据，须先定「只读但不本地」的边界

### 发布收尾（v0.2.0 之后立刻要做）

| 事项 | 说明 |
|------|------|
| ~~盯 `release.yml` 首次运行~~ | ✅ 已跑：run 37914120667，Status Success，9m 2s，12 个 job 腿全部完成 |
| 人工核附件清单 | 预期 11 个（3 桌面 + 4 CLI + 4 MCP）。**draft 不对外可见，匿名访问看不到**，需在 Releases 页面确认 |
| 人工验收 draft release | 三平台安装包各装一遍；CLI 与 MCP 各下一个跑通；详见 `docs/RELEASE.md` §5 |
| 点发布 | draft 不会自动对外可见 |

### 独立待办（需先决条件）

| 事项 | 阻塞原因 |
|------|---------|
| 代码签名与 macOS 公证 | 缺 Apple Developer 证书与 Windows 代码签名证书 |
| 本地 AI 端到端验证 | 本机未安装 Ollama；需 `ollama serve` + 拉一个模型。**我们这一侧的 HTTP 路径已由 P-07 的 stub 覆盖**，剩下的是「真实 Ollama 的响应形状与我们的假设一致」这半 |
| 产品命名统一 | `src-tauri/tauri.conf.json` 的 `productName` 仍是 `repoprism-app`，与 `RepoPrism` 不一致；`AGENTS.md` 规定命名由人类主导，未擅自改。**v0.2.0 的安装包与窗口标题用的就是这个名字** |
| 图片 / 字节预览 | 需先定「只读取 blob 字节」的边界（`git cat-file`），并给 diff 定性能阈值 |
| `commit-graph` 虚拟化的布局耦合 | 虚拟滚动依赖 `.main` / `.main-split` / `.commit-graph` 上的 `min-height: 0`；改这几处布局必须回归虚拟滚动 |

### 未解决的风险（如实说明）

- **本地 AI 层仍没有真实模型验证**：P-07 已让**我们这一侧**的 HTTP 路径真实跑起来
  （`tests/summarizer_http.rs`，回环 stub 往返 + 出网 body 内容断言 + 四条降级路径），
  但 **stub 的响应形状是我们自己写的** —— 它证明不了真实 Ollama 的字段名、
  `stream: false` 的行为、错误码与我们的假设一致。真实调用的端到端验证
  在 SPEC US-7 里**仍是 `[待实现]`**（本机无 Ollama）。
- **出网闸门不归 CI 静态扫描管**：`read-only-guard.sh` 是 Git 动词白名单，
  看不见 `ureq` 调用。这条边界现在有两层测试兜底：
  `OllamaConfig::validate` 的单测（8 条「像 localhost」的输入）+ P-07 的
  `the_bytes_that_leave_the_machine_carry_relative_paths_only`（直接读请求体）。
  两层都不在扫描器里 —— **改动 `summarizer.rs` 若不跑 `summarizer` 与 `summarizer_http`
  两组测试，闸门失效不会有任何提示。**
- **UI 接线没有自动化测试**：`App ↔ invoke` 一层需要 Tauri 运行时，单测覆盖不到；
  纯逻辑（graph / diff / risk / integrations / kinds / format / virtual）都有测试，
  但「点提交 → 出 Diff」「点按钮 → 开浏览器」「滚动列表」需人工 `pnpm tauri dev` 确认。
  TASK-016 把虚拟滚动的**区间计算**变成了可断言的形式，但真实滚动帧率仍未被测。
- **CI 曾在 ubuntu 上一直红**（2026-10-09 发现并修复，修复后已两次验证全绿）：
  本文件此前写的「CI 尚未验证」是**错的**。仓库是公开的，`ci.yml` 在每次 push 到 `main`
  时都真实执行 —— 修复前**当时存在的 5 次运行全部失败**（最早一次是仓库的 `INIT` 提交），
  包括 `004` 与 `005` 两批。
  失败点始终只有一处：**ubuntu leg 的 clippy（exit 101）**。原因不是代码，而是
  `cargo clippy --workspace` 会连 `src-tauri` 一起检查，而 rust job 从未安装
  WebKitGTK / GTK / libsoup 这些 **Linux 系统库**（只有 `release.yml` 的 desktop job 装了）。
  macOS 与 Windows 的系统 SDK 自带等价物，所以那两个平台是绿的。
  又因为矩阵默认 `fail-fast: true`，ubuntu 一红就把另外两个腿掐掉，日志里只剩 `cancelled`
  —— 把「只有一个平台红」这件事掩盖了整整 5 次运行。
  修复：`ci.yml` 的 rust job 在 ubuntu 上补装 Tauri Linux 依赖，并显式关掉 `fail-fast`。
  验证：`99478ea` → run 37912380547（仓库首次全绿）、`278ceb9` → run 37913817616。
  教训：**门禁红过就是红过，不能因为本机绿就写「尚未验证」。**
- **`release.yml` 已实机跑过并被验为绿**（2026-10-09，tag `v0.2.0` → run 37914120667，
  Status Success，9m 2s，12 个 job 腿全部完成）。但**产物是 draft、不对外可见**，
  「11 个附件是否齐全」需要人工在 Releases 页面确认（匿名访问看不到 draft）。
- **GitHub Actions 的 Node 20 弃用告警**：该次运行报出 12 条 warning，
  `actions/checkout@v4` / `actions/setup-node@v4` / `pnpm/action-setup@v4` /
  `softprops/action-gh-release@v2` 都还在 Node 20 上、被强制跑到 Node 24。
  目前只是告警，但需要在**确认各 action 的新版本号之后**再统一升版 ——
  盲升 action 版本号正是会当场炸掉发布链路的那类改动，故未在本批处理。
- **MCP 未与真实客户端联调**：协议行为由 14 个真实子进程测试覆盖，
  但尚未在 Claude Desktop / Cursor 中实机连接过。
- **发布产物未签名**：安装时会有系统警告，见 `CHANGELOG.md` 的 Known limitations。
