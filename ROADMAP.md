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
| v0.3 | [TASK-018](TASKS/018-incremental-cache.md) | 增量加载与本地缓存：引用映射跨调用复用（blame 未做，理由见卡片） | ✅ 2026-10-09 |
| v0.3 | P-07 / P-08 / P-09 / P-10 | 工程补丁：本地模型出网边界的真实覆盖、worktree / stash / 进行中状态、blob 只读预览 | ✅ 2026-10-10 |
| v0.3 | — | 发布 v0.3.0（tag `v0.3.0`，打在 `cfc20a9` → release run 38017284575） | ✅ 2026-10-10（draft 待人工点发布） |
| v0.3 | TASK-019–020 | 多仓库 / PR 只读视图 | ⬜ 未开始 |

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
| [P-09](TASKS/patch/P-09-worktree-stash-state.md) | 补全 US-1 缺口：worktree / stash 列表 + 进行中操作状态（**零子进程**探测） | ✅ 2026-10-09 |
| [P-10](TASKS/patch/P-10-blob-preview.md) | 补全 US-3 缺口：blob 只读预览（图片对比 / 十六进制转储 / LFS 指针识别） | ✅ 2026-10-10 |

---

## 四、当前门禁（2026-10-09 实测）

| 检查项 | 命令 | 结果 |
|--------|------|------|
| 只读扫描自检 | `bash scripts/read-only-guard.sh --self-test` | ✅ 26/26（14 正例 + 12 反例） |
| 只读扫描 | `bash scripts/read-only-guard.sh` | ✅ passed |
| Rust 格式 | `cargo fmt --all -- --check` | ✅ 干净 |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 无警告 |
| Rust 测试 | `cargo test --workspace` | ✅ **193 passed**（core 165 / CLI 14 / MCP 14）。⚠️ `cargo test --workspace` 在本机沙箱会被 OOM kill（exit 137），故按包跑后加总 |
| 子进程次数（`ADR-002`） | `cargo test -p repo-prism-core --test perf` | ✅ `spawn_counts_are_pinned`：`snapshot()` 2 次、`snapshot()+commits()` 4 次（原 5 / 7）。**该门禁在 TASK-018 与 P-09 里都一字未改** —— 缓存是 opt-in、状态探测是文件探测，两条都不给快照加子进程，见下 |
| 引用缓存失效（`TASK-018`） | `cargo test -p repo-prism-core --test cache` | ✅ **12 passed**：1 条「缓存确实命中」+ 7 条「引用一变必须失效」+ 4 条边界（工作区状态从不缓存 / 不开启时行为不变 / 链接工作树 / 分支改名）。失败路径由 6 个探针反证 |
| 工作区状态与列表（P-09） | `cargo test -p repo-prism-core --test workspace` | ✅ **11 passed**：真实 git 造出冲突合并 / 变基（含进度）/ 摘取 / 回退 / 二分 / 链接工作树 / stash，逐项断言。另含 lib 内 14 条纯函数单测（`state_from_markers` / `parse_worktrees` / `parse_stashes`）。Rust 侧 9 条 + 前端侧 4 条失败路径探针全部反证通过 |
| 性能门禁 | `cargo test -p repo-prism-core --test perf` | ✅ snapshot 224ms / 500ms、commits 121ms / 800ms |
| 前端 lint | `biome check src` | ✅ 34 files |
| 前端类型 | `tsc --noEmit` | ✅ 无错误 |
| 前端测试 | `vitest run` | ✅ 71 passed（7 files；新增 `workspace.test.ts` 14 条、`preview.test.ts` 16 条） |
| 前端构建 | `vite build` | ✅ `dist/assets/index-BRbENeDd.js` 251.04 kB（gzip 78.39 kB） |
| 版本一致性 | `pnpm release:dry` | ✅ `next version: 0.3.0`（v0.3.0 发版前）。`--expect 0.2.0` 会**真的失败**（退出码 1），已当场复验 |
| Workflow YAML | `yaml.safe_load` 解析 `ci.yml` / `release.yml` | ✅ 可解析，依赖顺序符合预期 |
| 远端 CI（真实执行） | GitHub Actions 的 `CI` workflow | ✅ **最近一次**：`f8e9e59`（P-10）→ run **38015706569**，**6 腿全 success**：Read-only Guard（自检 + 扫描）/ Frontend（lint / typecheck / test / build / release:dry）/ Rust×3（fmt / clippy / test 在三平台全 success）/ Performance。`test` 步骤在 macos 02:07:21、ubuntu 02:07:11、windows 02:08:04 都是 success —— **P-10 新增的 `tests/preview.rs` 因此在三个平台上都真跑过**（`cargo test --workspace` 不筛目标），也就是说那 13 项「真的 `git add` 二进制再读回来」的用例不是只在本机绿。<br>上一次：`49c7a87`（P-09）→ run 37942003269，**6 腿全 success**；`test` 步骤在 macos / ubuntu / windows 三条腿都是 success —— **P-09 新增的 `tests/workspace.rs` 因此在三个平台上都真跑过**（`cargo test --workspace` 不筛目标）。上一版 `9564c67`（TASK-018 文档）→ run 37935852725 亦全绿。<br>**新的 `tests/cache.rs` 确实在 CI 上跑过**：`cargo test --workspace` 不筛目标，本地已确认它把 `tests/cache.rs` 编成可执行文件（`Executable tests/cache.rs`）；若该目标失败，这一步会红。CI 日志需 admin 才能读（匿名 403），故这是**基于构建产物的验证**，不是日志级验证。<br>**修复后连续 9 次全绿**：`99478ea`（run 37912380547）起，至 `eb570c3`（run 37928197958），每次 6 腿全 success。<br>**截至 `eb570c3`**：仓库累计 **14 次**运行 = 修复前 **5 次全失败**（最早 `cad2d96`）+ 修复后 **9 次全成功**。<br>这三个数**锚定在 `eb570c3` 这个提交上**，不是「当前值」——后续每次提交都会让它增长，所以不写「截至目前共 N 次」这种会漂的说法。重算：`curl -s "https://api.github.com/repos/repo-prism/repo-prism/actions/runs?per_page=30&event=push"` 后按 `name == "CI"` 过滤。<br>（本行此前写的「四次」「五次」是**少算**——手数时漏掉了两次文档提交的运行。） |
| 远端发布（真实执行） | GitHub Actions 的 `Release` workflow | ✅ **最近一次**：tag `v0.3.0`（打在 `cfc20a9`）→ run **38017284575**，**Status Success，8m 51s**，**12 个 job 腿全部 success**（verify 10s / desktop 3-of-3 / cli 4-of-4 / mcp-binaries 4-of-4）。cli 与 mcp-binaries 的 8 条腿全部 `cargo build --release --locked` 通过 ⇒ **`Cargo.lock` 与 0.3.0 同步**。产物是 **draft**：匿名 API 查 `/releases/tags/v0.3.0` 拿不到 `name`、附件数为 0（与 v0.2.0 同样），**11 个附件是否齐全只能登录后核验**。新观察到一条 notice：`ubuntu-latest` 将于 2026-10-19 迁移到 Ubuntu 26，之后首次发布需重验 Linux 安装包。<br>上一次：tag `v0.2.0` → run 37914120667，**Status Success，9m 2s**（12 腿全 success）。<br>**两个 draft 都还没被人工点发布**（v0.2.0 自 2026-10-09、v0.3.0 自 2026-10-10） |

**core 165 项的构成**：lib 67（含 `summarizer` 13、`analysis` 16、`git` 的 remote/hash 解析、
`parse_head_meta`，以及 P-09 新增的状态探测与列表解析纯函数 **+14**）/ analysis 5 /
diff 10 / perf 2 / remote 5 / snapshot 16 / `summarizer_http` 8（P-07）/
`summarizer_egress` 1 + `summarizer_contract` 4（P-08）/ `cache` 12（TASK-018）/
**`workspace` 11（P-09）/ `preview` 13（P-10）**，另有 lib 内纯函数 +11（P-10 的类型判据与转储）。

**性能门禁的采样方式**：预热一次 + 采样 3 次取**最小值**。

**为什么还要一条「子进程次数」门禁**（`ADR/002`）：实测显示耗时几乎完全由
「起了几个 `git` 进程」决定 —— 4 万提交仓库上的 `for-each-ref` 与完全不读仓库的
`git --version` 耗时量级相同；提交数 1 万 → 4 万，`snapshot()` 耗时无系统性变化。
而**毫秒阈值会随 runner 抖动**（实测同机连跑三次 208/470/576ms），
**spawn 次数不会**。所以 spawn 次数是比毫秒更硬、且能定位到具体方法的门禁，
`tests/scale.rs`（`#[ignore]`）保留「改动前」的 argv 序列作为可重测的对照。

**TASK-018 的缓存为什么是 opt-in**：如果 `snapshot()` 默认走缓存，同一个方法就有了
冷/热两个 spawn 数字，上面那条门禁会退化成「看测试跑的顺序」—— 它就不再是契约。
所以缓存只在 `Git::open_cached`（桌面端会话）下开启，`Git::open` 的行为与逐次成本
**逐字不变**，门禁一字未改且仍然通过。缓存自己的契约（命中省几次、以及更重要的
**引用一变就必须失效**）在 `tests/cache.rs`，那里数的是真实子进程次数。

**P-10 的 blob 预览是唯一「次数随参数变化」的契约**：`blob_size()` 恒为 1、
`blob_preview()` 对一个普通文件是 2，但对**超过 4 MiB** 的文件是 **1** ——
第二次调用压根没发生。变化的方向恰恰是我们要证明的性质（先看大小再决定读不读），
所以它不是裂缝，它就是契约。这条断言在 `tests/preview.rs`，
把那处早退删掉它立刻红（探针 R10）。

**P-09 为什么把「进行中状态」放进快照、把 worktree / stash 分出去**：同一条理由的
第三次应用 —— 判据是**这个字段值不值得一个子进程**。状态只需看 `<git-dir>` 下的
标志文件（`MERGE_HEAD` / `rebase-merge/` / `CHERRY_PICK_HEAD` / `REVERT_HEAD` /
`BISECT_LOG`），**一个进程都不起**，所以它可以进首屏而 `snapshot()` 仍是 2 次；
worktree 与 stash 各要一次进程，就单独走 `Git::workspace()`（2 次），由前端
**按需调用**（默认折叠，展开才取）。判据不是「要不要这个功能」而是「它值几次进程」。

**`summarizer_*` 三个测试文件各补什么**：`summarizer.rs` 的 13 个单测是纯函数，
而真正把字节送出进程的调用在 P-07 之前**一次都没执行过**（本机无 Ollama）。

| 文件 | 补的是什么 |
|------|-----------|
| `summarizer_http`（P-07，8 项） | 回环 stub 真实往返；连不上 / 5xx / 非法 JSON / 空白 response 四条降级；`//api/tags` 拼接；**直接读请求体**断言「只送相对路径」 |
| `summarizer_contract`（P-08，4 项） | 响应体**逐字取自 Ollama 官方 `docs/api.md`**，于是字段名写错会被测出来（P-07 做不到这一点：它的响应是我们自己写的）；另含「多行 NDJSON 必须整体判失败」 |
| `summarizer_egress`（P-08，1 项） | **环境代理不得改道回环请求**。代理指向一个**已确认无人监听**的死端口，因此「用了代理」必然导致红 |

共用回环 stub 在 `tests/common/stub.rs`（由 P-07 实现抽出，两个坑只保留一份）。
**它们都不替代真实模型验证**——见下方「未解决的风险」。

**P-08 的三个用例在 CI 上跨平台跑过**：`2feef1e` → run 37927868288 的 `test` 步骤
在 macOS / ubuntu / windows 三条腿上都是 success（17s / 11s / 36s）。
即「环境代理不得改道回环请求」这条性质在三个 runner 平台上都成立，
不只是本机成立。
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
| US-1 仓库状态（分支 / HEAD / 标签 / 提交图 / 上游计数 / 进行中操作 / worktree / stash） | ✅ 已实现（P-09 补齐最后一条缺口） |
| US-2 变更分组 + 风险角标 | ✅ 已实现 |
| US-3 提交详情 + Diff（统一 / 并排，含原始行号，`patch` 原文） | ✅ 已实现（含图片对比与字节预览；**音视频播放**仍 `[待实现]`，理由见 SPEC） |
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
2. ~~**TASK-018 增量缓存**~~ —— ✅ 2026-10-09。`ADR/002` 承诺的那一步（共用引用映射，
   `snapshot()+commits()` 4 → 3 次 spawn）已兑现；打开仓库的四条命令由 **13 → 8** 次。
   交付范围与**未做的部分**（blame 是尚未实现的产品能力；提交详情缓存因失效判据
   暂不可证而留待）见 [`TASKS/018`](TASKS/018-incremental-cache.md)。
   缓存刻意做成 **opt-in**（`Git::open_cached`），以保住那条「与状态无关」的 spawn 门禁。
3. **TASK-019 多仓库工作区**（US-9）。当前 `AppState` 只保留一个 `RepoSession`，
   多仓库需要把会话换成按路径索引的表 —— 缓存的失效判据可以原样复用。
4. **TASK-020 PR / MR 只读视图**（US-10）—— 需要网络与凭据，须先定「只读但不本地」的边界

补丁序列在 TASK-018 之后又补了一张 [P-09](TASKS/patch/P-09-worktree-stash-state.md)：
US-1 里那条「worktree / stash / 合并变基状态」在 SPEC 中标 **P0**，却**从未被分配卡号**
（019 = 多仓库、020 = PR 视图），于是按 P-01 的先例编入 patch 序列。
它顺带成为 **TASK-018 把 `git_dir` 与 `common_dir` 拆开的第一个真实用途** ——
那些进行中操作的标志文件恰好全部住在每个工作树各一份的 `git_dir` 里。

再往后一张是 [P-10](TASKS/patch/P-10-blob-preview.md)，同一个原因：US-3 的
「图片对比 / 媒体预览 / 字节预览」同样标 P0、同样没有卡号，而它是 SPEC 优先级表里
**最后一条未实现的 P0**。它开的是一条**此前不存在**的边界 —— 项目此前从不读取
blob 内容（`FileStat.binary` 只标记不读），所以「只读」这次要从「不执行写命令」
推进到「读也要先量再读」。做完之后，v0.3 剩下的 P0 一条都没有了。

### 发布收尾（v0.2.0 / v0.3.0 两个 draft 都卡在这一步）

| 事项 | 说明 |
|------|------|
| ~~盯 `release.yml` 首次运行~~ | ✅ 已跑两次：v0.2.0 → run 37914120667（9m 2s，12 腿全 success）；v0.3.0 → run 38017284575（8m 51s，12 腿全 success） |
| 人工核附件清单 | 每个 draft 预期 11 个（3 桌面 + 4 CLI + 4 MCP）。**draft 不对外可见**，v0.3.0 已实测匿名 API 只拿到「附件数 0」且没有 `name`，需在 Releases 页面确认 |
| 人工验收 draft release | 三平台安装包各装一遍；CLI 与 MCP 各下一个跑通；详见 `docs/RELEASE.md` §5。v0.3.0 另需看一眼**新能力**：状态角标、worktree / stash 列表、点开变更文件看图片 |
| 点发布 | draft 不会自动对外可见。**两个都还没点**（v0.2.0 / v0.3.0） |

### 独立待办（需先决条件）

| 事项 | 阻塞原因 |
|------|---------|
| 代码签名与 macOS 公证 | 缺 Apple Developer 证书与 Windows 代码签名证书 |
| 本地 AI 端到端验证 | 本机未安装 Ollama；需 `ollama serve` + 拉一个模型。**我们这一侧的 HTTP 路径已由 P-07 的 stub 覆盖**，剩下的是「真实 Ollama 的响应形状与我们的假设一致」这半 |
| 产品命名统一 | `src-tauri/tauri.conf.json` 的 `productName` 仍是 `repoprism-app`，与 `RepoPrism` 不一致；`AGENTS.md` 规定命名由人类主导，未擅自改。**v0.2.0 / v0.3.0 两版的安装包与窗口标题用的都是这个名字** |
| ~~图片 / 字节预览~~ | ✅ P-10 已完成（边界定在 `git cat-file`，先看大小再决定读不读）。剩余「音视频播放」见 SPEC |
| `commit-graph` 虚拟化的布局耦合 | 虚拟滚动依赖 `.main` / `.main-split` / `.commit-graph` 上的 `min-height: 0`；改这几处布局必须回归虚拟滚动 |

### 未解决的风险（如实说明）

- **本地 AI 层仍没有真实模型验证**：P-07 已让**我们这一侧**的 HTTP 路径真实跑起来
  （`tests/summarizer_http.rs`，回环 stub 往返 + 出网 body 内容断言 + 四条降级路径）；
  P-08 又把响应形状的**权威性**往前推了一步 —— `summarizer_contract` 的输入
  **逐字取自 Ollama 官方 `docs/api.md`**，不再是我们自己编的，
  于是「字段名写错」这种坏法第一次能被测出来。
  但**输入来自文档，不是一台真的 Ollama**：它证明不了某个具体版本的实际行为与文档一致。
  真实调用的端到端验证在 SPEC US-7 里**仍是 `[待实现]`** ——
  截至本版本，**一次真实的模型调用都没有发生过**。
- **出网闸门不归 CI 静态扫描管**：`read-only-guard.sh` 是 Git 动词白名单，
  看不见 `ureq` 调用。这条边界现在有四组测试兜底：
  `OllamaConfig::validate` 的单测（8 条「像 localhost」的输入）、P-07 的
  `the_bytes_that_leave_the_machine_carry_relative_paths_only`（直接读请求体）、
  P-08 的 `summarizer_egress`（**代理不得改道回环请求**）与 `summarizer_contract`
  （响应字段名与厂商文档一致）。
  四组都不在扫描器里 —— **改动 `summarizer.rs` 若不跑这四个 `summarizer*` 目标，
  闸门失效不会有任何提示。**
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
