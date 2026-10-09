# ADR-002: Git 后端演进 —— 是否把系统 Git 换成 gix

- **状态**：已接受（决策已执行，见 `TASKS/017-git-read-perf.md`）
- **日期**：2026-10-09
- **决策者**：项目主导
- **关联**：`ADR/001`（Git 读取层技术选型，本卡是其 P1 部分的落实）、`TASK-017`、`SPEC.md` 非功能需求「大仓库首屏 < 3s」

## 摘要

`ADR-001` 把「P1 引入 `gix` 作为备选后端」写进了计划，`TASK-017` 承接了它。
本 ADR 实测之后**否决了这次替换**，理由是三条：

1. **瓶颈不在仓库规模，而在进程数**。提交数从 1 万涨到 4 万，`snapshot()` 耗时
   **没有系统性变化**。所以「减少子进程」是唯一有意义的动作方向 ——
   但用不着换库：把可以合并的子进程合并掉即可，且**改动量小、风险低、零新增依赖**。
2. **换 gix 会让项目的核心安全门禁变成空转，而且它还会打印 `passed`。**
   见第三节第 1 条，这是本次否决里最重的一条。
3. **gix 缺的正是本任务要解决的那个能力**。大仓库加速器（bitmap / commit-graph /
   fsmonitor / split·sparse index）在 gix 里全部未实现，而 `TASK-017` 的目标恰恰是
   「大仓库首屏 < 1s」。

**决策：保留系统 Git 后端；执行子进程合并；把 gix 转为条件触发，触发条件写死在下文。**

## 一、问题

`TASK-017` 的目标是「大仓库首屏 < 1s」。当前实现走系统 Git 子进程（`ADR-001`），
而 `SPEC.md` 的非功能指标只说「首屏 < 3s」——**没有人在真实大仓库上量过**，
所以「大仓库到底慢不慢、慢在哪」一直是猜测。本 ADR 的第一件事就是把它测出来。

## 二、实测

测量工具：`crates/repo-prism-core/tests/scale.rs`（`#[ignore]`，不进 CI 常跑）。
复现：

```bash
cargo test -p repo-prism-core --test scale -- --ignored --nocapture
```

仓库用 `git fast-import` 构造，无网络、可复现。

### 2.1 规模曲线：耗时**不随提交数增长**

| 提交数 | `snapshot()`（轮1 / 轮2 / 轮3） | `commits(200,0)`（轮1 / 轮2 / 轮3） |
|-------:|-------------------------------:|------------------------------------:|
| 1,000 | 416 / 229 / 320 | 132 / 71 / 85 |
| 10,000 | 200 / 140 / 155 | 92 / 74 / 72 |
| 40,000 | 213 / 158 / 151 | 98 / 75 / 70 |

**判读（按可靠性排序）**：

- `commits(200,0)` 三轮都稳定在 70–98ms，**与提交数完全无关** —— 它是 `-n200` 的限量查询。
- `snapshot()` 在 1 万与 4 万之间**没有系统性差异**（200/140/155 对 213/158/151，
  第 2、3 轮里 4 万甚至比 1 万更快）。
- 1,000 那一行三轮都偏高，是它作为第一次迭代吃到冷启动；**它不构成规模趋势**
  —— 若真与规模相关，最小的仓库应当最快。
- 改动后又跑了一轮（轮4），数值更小但抖动更大（10,000 行异常到 189ms/211ms），
  因此**不作为趋势依据**。

结论：**「大仓库慢」这个前提在本项目的读取路径上没有被复现。** 瓶颈另有其因。

### 2.2 拆解：每个子进程的固定成本吃掉了一切

在 4 万提交上逐个测（多轮代表值）：

| 命令 | 耗时 |
|------|-----:|
| `git --version`（**完全不读仓库**） | 26–46ms |
| `rev-parse --show-toplevel` | 28–54ms |
| `status --porcelain=v2 --branch -z` | 30–62ms |
| `symbolic-ref HEAD` | 27–51ms |
| `rev-parse HEAD` | 29–73ms |
| `for-each-ref refs/heads` | 28–81ms |
| `for-each-ref refs/tags` | 26–38ms |
| `for-each-ref`（三条引用合并成一次） | 27–61ms |
| `log --all -n200` | 38–95ms |

`git --version` **不碰仓库**却要 26–46ms，而读 4 万提交仓库的 `for-each-ref`
只要 28–81ms —— 两个区间大幅重叠。**每条命令的大部分时间花在「把 git 进程起起来」，
不是花在仓库上。**

改动前的 spawn 次数（静态可数）：`snapshot()` **5** 次
（`status` 1 + `symbolic-ref` 1 + `rev-parse` 1 + `for-each-ref` 2）、
`commits()` 2 次，合计 **7** 次。

### 2.3 修法：把可合并的子进程合并掉（**验收标准放在次数上，不放在毫秒上**）

合并方式（前提已实测确认，见 2.4）：

1. `ref_map()` / `branches()` / `tags()` 的**三次 `for-each-ref` 合成一次**
2. 分支名与 HEAD 取自 `status --branch` 的头部行，砍掉 `symbolic-ref` 与 `rev-parse HEAD`
3. `commits()` 复用同一份 ref 读取逻辑

结果：`snapshot()` 5 → **2**；`snapshot() + commits()` 7 → **4**。
`commits()` 自己那 2 次（`log` + `for-each-ref`）**省不掉** —— 它本来就只有一次
`for-each-ref`。想把两者共用（降到 3 次）需要**跨调用缓存引用映射**，
那是 `TASK-018 增量缓存` 的范围。

**为什么不用「省了百分之几」当验收标准**：见 2.5 ——
本机噪声带宽大于被测差异，百分比测不准。**spawn 次数是确定性的**，
因此它既是改动目标，也是回归门禁
（`tests/perf.rs::spawn_counts_are_pinned`）。

A/B 实测（示意量级，不作为验收）：

| 方案 | 干净轮 | 加载轮 |
|------|-------:|-------:|
| 现状 7 次 spawn | 205ms / 219ms（省 55–56%） | 203ms `[ 543 / 203 ]`（省 32%） |
| 合并后 4 次 spawn | 90ms / 98ms | 139ms `[ 139 / 222 ]` |

### 2.4 验证「合并可行」而不是假设它

```
# branch.oid eef957d5458177d66ac1ad80fa9382b3309c45f9
# branch.head main
```

`git status --porcelain=v2 --branch -z` 确实吐出 `# branch.oid` 与 `# branch.head`
（另有 `# branch.upstream` / `# branch.ab`，现状代码已在用）。因此分支名与 HEAD sha
**不需要额外子进程**。这一条是实测输出，不是文档推断。

### 2.5 测量自身的可信度（必须写清）

这一节是本 ADR 里**最重要的一节**：它决定了哪些数字可以拿来当结论。

- **本机噪声带宽大于被测差异。** 同一方案在同一轮 A/B 内两次采样可以差 2.7 倍
  （现状：543ms 与 203ms；合并后：139ms 与 222ms）。**方案间的差异（203 对 139）
  小于方案内的抖动。** 因此：
  **「省了百分之几」在本机不可靠，不作为验收标准，也不写进任何承诺。**
- **单次 spawn 的固定成本约 24–38ms**，且**高于**一般 macOS 上的 5–10ms
  （沙箱环境开销）。**绝对毫秒数不可外推到用户机器**；
  但「spawn 次数」是可外推的，因为它与实现相关、与机器无关。
- **「重复执行同一条命令」测出的固定成本不可信**：同一组 5 次 `--version`
  在测试开头测出 356ms（71ms/次）、结尾测出 147ms（29ms/次），差 2.4 倍。
  凡是用「单命令耗时 × 次数」推算出来的数**一律作废**。
- 采样一律取 3 次最小值；A/B 两组各测两轮取最小；A/B 是唯一把两组放在同一进程内
  交替执行的测量，但如上所述，它也只能给方向与量级，不能给精度。

## 三、决策依据

### 1（决定性）换 gix 会让只读扫描器变成**会打印 passed 的空转**

这是本 ADR 里最重的一条。`scripts/read-only-guard.sh` 是项目只读宪法的**唯一事实来源**，
它的**第 0 层门槛**判据是：

> 只有真正在调用 Git 的语句才走下面三条规则。判据是该语句里出现
> `Command::new("git")` 或 `self.run(`。

`gix` 是**库调用**，没有 `Command::new("git")`，也没有 `self.run(`。门槛不再放行任何语句，
于是第 1–4 层（动词白名单 / 危险选项黑名单 / 条件动词 / 写动词黑名单）**全部空转**。
而扫描器的**自检仍然全绿**——它喂的是合成样本，不看仓库里到底有没有 Git 调用。

这正是本项目已经被烧过两次的那类失效（见 `TASKS/patch/P-02`、`P-05` 与
`P-06` 的教训「不会失败的闸门比没有闸门更危险」）：**门禁在那里、报告绿灯、
但什么都没检查。** 换后端不是「顺手换个实现」，而是**要把整套安全机制重写**，
并且新机制的核心（一个 API 白名单）远不如「字面量子命令白名单」可审计。

### 2 gix 缺的正是本任务的目标能力

`TASK-017` 要的是「**大**仓库首屏 < 1s」。而 gitoxide 官方的
`crate-status.md` 把「big-repo accelerators」整体列为**未实现**（`[ ]`）：

- `bitmap` 文件 —— `[ ]`
- commit-graph 在 `gix-traverse` 中的支持 —— `[ ]`（仅在 `gix-revwalk` 里声明有加速）
- sparse-index / split-index 的 status 加速 —— `[ ]`
- fsmonitor —— `[ ]`
- untracked-cache —— `[ ]`

这些恰恰是**真实超大仓库上 git 之所以快的原因**。我们在一份平坦的曲线（2.1）
上为了「大仓库性能」去换一个**尚未实现大仓库优化**的库，是方向性的错误。

### 3 gix 目前产不出 Git 兼容的 patch 文本

US-3 要求提交详情里给出 diff 正文。`crate-status.md` 中 `gix-diff` 的
「blobs → patches：text / binary / `git-apply` 兼容」全部为 `[ ]`，
只有「blobs → lines」（基于 `imara-diff` 的行级 diff）是 `[x]`。

也就是说换过去之后，**要么我们自己重写一个补丁格式化器**（去对齐 git 的输出格式，
这是持续的维护负担），**要么 diff 文本仍然走子进程** —— 那就要同时维护两条后端路径，
比现在更复杂。

### 4 依赖面从 5 涨到 34+

本项目当前整个 workspace 只有 **5 个**依赖（`serde` / `serde_json` / `anyhow` /
`thiserror` / `ureq`），并且曾**为砍掉 TLS 特意关闭 `ureq` 的默认特性**，
理由是「不引入整棵 `rustls` / `ring` / `webpki` 依赖树」——这个项目对供应链面是**有意**收窄的。

`gix` 0.89.0 的**强制**（normal + 非 optional）直接依赖是 **34 个**：
`gix-actor`、`gix-commitgraph`、`gix-config`、`gix-date`、`gix-diff`、`gix-discover`、
`gix-error`、`gix-fs`、`gix-glob`、`gix-hash`、`gix-hashtable`、`gix-lock`、`gix-object`、
`gix-odb`、`gix-pack`、`gix-parallel`、`gix-path`、`gix-protocol`、`gix-quote`、`gix-ref`、
`gix-refspec`、`gix-revision`、`gix-revwalk`、`gix-sec`、`gix-shallow`、`gix-tempfile`、
`gix-trace`、`gix-traverse`、`gix-url`、`gix-utils`、`gix-validate`、`gix-zlib`、
`nonempty`、`smallvec`（另有 `gix-status` / `gix-index` / `gix-worktree` / `gix-dir` /
`gix-ignore` 等可选依赖，是我们真正需要的功能所在）。

其中 `gix-protocol`（网络协议栈）是**强制**依赖，`gix-transport` / `gix-credentials` /
`gix-prompt` 属于我们需要打开的可选依赖。这与本项目的「唯一出网点是回环地址上的
本地模型服务」这一约束方向相反：**新增了一整棵具备网络能力的依赖树**。

### 5 版本与成熟度：pre-1.0、月度 breaking、MSRV 持续上抬

- 最新 `gix` 为 **0.89.0（2026-10-08）**。7 周内走了 `0.86 → 0.87 → 0.87.1 → 0.88 → 0.89`。
- 子 crate 全是 0.x 且频繁 major 位递增，例如 `gix-worktree` 已 45 次 breaking release，
  当前 `0.58.0`；`gix-diff` `0.69.0`；`gix-ref` `0.69.0`；`gix-status` `0.36.0`。
- MSRV 在 0.88.0 从 1.85 抬到 **1.88**。本项目 CI 用 `stable`，不锁 MSRV，
  所以这不是硬阻塞，但它意味着**上游会周期性地强制我们升工具链**。
- 官方稳定性分级里，Tier 1 只有 `gix-lock`，Tier 2 只有 `gix-tempfile`；
  `gix-ref` / `gix-config` / `gix-glob` / `gix-actor` / `gix-hash` 属
  「stabilization candidates」，其余（含我们最依赖的 object / odb / pack / index /
  status / diff / revwalk）仍在 **Initial Development**。

### 6 `ADR-001` 选系统 Git 的原始理由，今天仍然成立

> 与用户环境完全一致；行为可预测

这一点在本项目里被反复验证过价值：`--no-textconv` 的安全语义、
`--name-status -z` 的字节级路径、`status --porcelain=v2` 的头部行格式 ——
我们输出的每一个字段都在**对齐用户本地 git 的真实行为**。换成另一套实现，
就多了一条「两套 git 语义可能不一致」的永久性风险。

## 四、决策

1. **保留系统 Git 子进程后端**，不引入 `gix`，**也不引入 `GitReader` trait 抽象**。
   抽象只有在存在第二个实现时才有价值；今天引入它只会多一层间接，
   并诱导「顺手加个 gix 实现」。（`ADR-001` 里那段 trait 草图**作废**。）
2. **`TASK-017` 的手段由「换 gix」改为「减 spawn」**：按 2.3 合并，
   `snapshot()` 5 → 2，`snapshot() + commits()` 7 → 4。**已执行**。
3. **把 spawn 次数固化成门禁**：`Git::spawns()` 暴露计数，
   `tests/perf.rs::spawn_counts_are_pinned` 逐方法钉住次数。
   这是**与机器无关**的性能契约 —— 毫秒阈值会随 runner 抖动（实测同机连跑三次
   208/470/576ms），spawn 次数不会。
4. **把本轮测量固化成可复跑的诊断**（`tests/scale.rs`），
   保留「改动前」的 argv 序列作为对照，使「省了多少」可以被重新测量，
   而不是只能引用一次历史结论。
5. **`gix` 转为条件触发**，见下。

## 五、什么条件下重估 gix

必须**同时**满足，且每条都要有实测证据：

1. 在一个**真实**的大仓库上（用户报告的最慢仓库，而非合成代理仓库）
   首屏实测 **> 1s**，且已经完成了本 ADR 的 spawn 合并；
2. 拆解显示瓶颈**不再是 spawn** —— 即某条单命令在真实仓库上的**自身**耗时
   （减去固定成本后）随规模显著增长；
3. 此时对比「自研缓存（`TASK-018` 增量缓存）」与「换后端」两条路，
   拿出同样的 A/B 数据。

并且，若届时仍要换 gix，**前置条件**是：

- github 官方 `crate-status.md` 中大仓库加速器（bitmap / commit-graph / fsmonitor）
  由 `[ ]` 变为 `[x]`；
- `gix-diff` 能产出与 git 兼容的 patch 文本，或已决定 diff 文本永久走子进程；
- **先设计出 gix 路径的等效只读门禁**并让它能失败 ——
  在这一点解决之前，换后端等于**主动移除项目的核心安全约束**，不予考虑。

## 六、后果

**正面**

- `snapshot()` 的 spawn 次数 5 → 2，`snapshot() + commits()` 7 → 4；**确定性的**减少，
  且由门禁钉住。
- 零新增依赖，供应链面不变。
- 只读扫描器继续有效 —— 它的第 0 层门槛仍然放行真实调用，四层规则仍然在检查。
- 顺带发现 `ref_map()` / `branches()` / `tags()` 三处重复的 `for-each-ref`，
  这是本次实测的直接副产品。

**负面 / 代价**

- 绝对首屏时间仍有约 60–80ms 的进程开销下限（剩下的 4 次 spawn 无法再合并
  —— Git 没有通用的常驻服务模式，只有 `cat-file --batch` 这一条对象读取通道）。
  若要突破这个下限，**只能**换内嵌库，届时应按第五节重估。
- 分支名与 HEAD 改为从 `status --branch` 的头部行取，**依赖 git porcelain v2 的格式稳定性**。
  该格式是 git 官方为脚本消费而定义的，风险低；已用 `parse_head_meta` 的纯函数单测
  钉住 `(detached)` / `(initial)` / 头部行缺失三种降级行为。
- **本次的「省了多少毫秒」无法在本机可靠测量**（2.5）。这是一个**已知的测量能力缺口**，
  不是被忽略的问题。

## 七、被否方案

| 方案 | 否决理由 |
|------|---------|
| **换 `gix`** | 见第三节六条。核心是会让只读门禁空转（第 1 条）且缺大仓库加速器（第 2 条） |
| **`libgit2`** | `ADR-001` 已否：C 依赖、跨平台编译复杂、与系统 Git 行为可能有差异。本节不重复论证 |
| **常驻 git 子进程（长连接）** | Git 没有通用服务模式，只有 `cat-file --batch` 这一条对象读取通道；而我们的瓶颈在 refs/status，不在对象读取。**不可行** |
| **引入 `GitReader` trait 抽象「为将来换后端做准备」** | 只有一个实现时是纯成本。而且它会诱导「顺手加第二个实现」，绕过本 ADR 的第五节约束 |
| **用 `--porcelain` 之外的自定义格式减少解析成本** | 解析不是瓶颈（2.2：固定成本与读仓库的成本同量级）。增加格式依赖只会引入风险 |
| **降低 `SPEC` 的首屏阈值以「达标」** | 门禁数字不能为了让报告好看而改。且实测显示当前指标本就远在阈值内（4 万提交 snapshot 约 150ms vs 阈值 3s），改阈值毫无必要 |

## 八、附录：本次实测的原始输出

**改动前**（干净轮）：

```text
=== 一、规模曲线（每个规模重建一次临时仓库，采样 3 次取最小）===
     提交数        构造耗时     open(ms)  snapshot(ms)   commits(ms)
    1000        278ms         73ms         320ms          85ms
   10000       1313ms         30ms         155ms          72ms
   40000       4857ms         28ms         151ms          70ms

=== 二、最大规模（40000 提交）上把各子命令拆开：谁在花时间 ===
  （不读仓库）--version                    31ms
  rev-parse --show-toplevel          28ms
  status --porcelain=v2 --branch     32ms
  symbolic-ref HEAD                  30ms
  rev-parse HEAD                     29ms
  for-each-ref refs/heads            29ms
  for-each-ref refs/tags             29ms
  for-each-ref（ref_map 三条）           29ms
  for-each-ref（三条合并成一次）              30ms
  log --all -n200                    38ms

=== 三、`status --branch` 的头部行 ===
  "# branch.oid eef957d5458177d66ac1ad80fa9382b3309c45f9"
  "# branch.head main"

=== 四、A/B 序列对比 ===
  现状 7 次 spawn：   219ms   [ 223 / 219 ]
  提议 3 次 spawn：    98ms   [ 99 / 98 ]      ← 当时误把两次 API 调用当成一次，已修正为 4
  省下 55%，即 121ms

=== 五、固定成本基线 ===
  1 次 spawn  开头 37ms / 结尾 28ms
  5 次 spawn  开头 356ms / 结尾 147ms   → 折合每次 71ms / 29ms
  snapshot()+commits() 现状 7 次 spawn 实测      219ms → 折合每次 31ms
```

**加载轮**（同一天稍后，机器有其他负载 —— 正是这一轮暴露了噪声问题）：

```text
=== 四、A/B 序列对比（修正为 4 次 spawn 之后）===
  现状 7 次 spawn：   203ms   [ 543 / 203 ]     ← 同一方案内 2.7 倍抖动
  提议 4 次 spawn：   139ms   [ 139 / 222 ]
  省下 32%，即 64ms
```

## 九、参考

- `ADR/001-git-read-layer.md` —— 本次决策的上下文与 trait 草图（后者作废）
- `TASKS/017-git-read-perf.md` —— 由本 ADR 重新界定手段的任务卡（已执行）
- `crates/repo-prism-core/tests/scale.rs` —— 本次全部数据的可复跑来源
- `crates/repo-prism-core/tests/perf.rs::spawn_counts_are_pinned` —— 机器无关的回归门禁
- `scripts/read-only-guard.sh` 头部注释 —— 第 0 层门槛的原文
- gitoxide `crate-status.md` —— 能力实现状态（复选框标注）
- crates.io API：`/api/v1/crates/gix/0.89.0/dependencies` —— 34 个强制直接依赖的来源
