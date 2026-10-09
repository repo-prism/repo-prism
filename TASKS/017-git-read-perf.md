# TASK-017: Git 读取层性能 —— 减 spawn，不换后端

**状态**：已完成（2026-10-09）
**手段变更说明**：血统里本卡的原始手段是「换 `gix` 后端」。`ADR/002` 实测后**否决了换后端**，
改为在同一个后端里合并子进程，目标不变（大仓库首屏性能）。本卡记录这次变更的全部依据。

## 目标

让首屏读取路径（`snapshot()` + `commits()`）尽可能接近它的下限。

**验收标准用「子进程次数」而不是毫秒** —— 理由见 `ADR/002` 第 2.5 节：
本机噪声带宽大于被测差异，毫秒测不准；而 spawn 次数是确定性的、与机器无关的量。

## 上下文：为什么原始手段（换 gix）被否决

见 `ADR/002`，三条主要理由：

1. **瓶颈不是仓库规模，是进程数。** 提交数 1 万 → 4 万，`snapshot()` 耗时无系统性变化；
   而 `git --version`（不读仓库）与读 4 万提交仓库的 `for-each-ref` 耗时量级相同。
2. **换 gix 会让 `scripts/read-only-guard.sh` 变成空转，且仍打印 `passed`。**
   该扫描器的第 0 层门槛判据是「语句里出现 `Command::new("git")` 或 `self.run(`」，
   gix 是库调用，两者都没有 → 四层规则全部不再检查任何语句，而自检仍绿。
   这正是本项目已被烧过两次的那类失效（`P-02` / `P-05` / `P-06`）。
3. **gix 缺的正是本任务的目标能力**：大仓库加速器（bitmap / commit-graph / fsmonitor /
   split·sparse index）在其 `crate-status.md` 里全部为 `[ ]`。

## 约束

- 不改后端：仍然只通过 `Command::new("git")` 调用系统 Git。
- **不得新增任何 `git` 子命令种类**，尤其是不得为了省子进程而引入写命令。
  本次只做「少起几次同样的命令」，命令集合是缩小而非扩大的。
- 行为必须逐字保持：`snapshot()` 的输出对既有 16 个测试必须完全一致
  （含分离头指针、空仓库、上游 ahead/behind）。
- 不引入 `GitReader` trait 抽象（`ADR/002` 第七节已否）。

## 实现

| 改动 | 前 | 后 |
|------|----|----|
| 分支 / 标签 / ref 映射 | 三次 `for-each-ref` | **一次** `for-each-ref`（`REF_FORMAT` 带完整 `refname` 以区分前缀） |
| 分支名 | `symbolic-ref --quiet --short HEAD` | `status --branch` 的 `# branch.head` |
| HEAD 提交 | `rev-parse HEAD` | `status --branch` 的 `# branch.oid` |
| 子进程计数 | 无 | `Git::spawns()`（`AtomicUsize`，`open()` 与 `run()` 各记一次） |

`for-each-ref` 的合并是**行为等价**的：原 `ref_map()` 本来就是一次带三个前缀的调用，
git 按完整 refname 排序（heads → remotes → tags），合并后输出顺序逐字相同。

`parse_head_meta` 抽成纯函数，使「头部行缺失」这条在集成测试里构造不出来的降级路径可测。

## 验收标准

- [x] `snapshot()` 的 spawn 次数 **5 → 2**
- [x] `snapshot() + commits()` 的 spawn 次数 **7 → 4**
- [x] `tests/perf.rs::spawn_counts_are_pinned` 逐方法钉住次数
      （`open` 1 / `snapshot` 2 / `commits` 2 / `commit` 2 / `commit_detail` 4 /
      `working_tree_stats` 2 / `remote_info` 1 / `diff` 2）
- [x] **该门禁的失败路径已实测**：把 `rev-parse HEAD` 加回 `snapshot()` 后，
      门禁报 `left: 3, right: 2` 并给出「有人把分支/标签/HEAD 又拆回了独立子进程」的提示；
      验证后源码已还原（`grep PROBE` 为 0）
- [x] 既有的 16 个 `snapshot` 集成测试全部通过（含分离头指针、无提交仓库、
      upstream ahead/behind/in-sync）—— 这是「合并行为等价」的主要证据
- [x] 新增 3 个 `parse_head_meta` 纯函数单测：正常头部行 / `(detached)` 与 `(initial)`
      占位值 / 头部行缺失时的降级
- [x] 只读扫描器通过（`Read-only guard passed`）——
      新增的断言字面量没有触发误报
- [x] `tests/scale.rs` 保留「改动前」的 argv 序列作为对照，使「省了多少」可被重新测量

## 一处必须说明的真实限制

**「省了多少毫秒」在本机无法可靠测量。** 同一方案在同一轮 A/B 内两次采样差 2.7 倍
（现状 543ms / 203ms），方案间差异（203 对 139）小于方案内抖动。
干净轮测到 55%，加载轮测到 32%。因此：

- 本卡**不承诺任何百分比**；
- 验收标准只放在确定性的 spawn 次数上；
- 这是一个**已知的测量能力缺口**，不是被忽略的问题。
  要拿到可信的毫秒收益，需要在有真实大仓库且负载可控的环境里复测。

## 与归档规划的一处偏离（必须记录）

`005` 文档的预告里，`TASK-017` 的主题写作「`gix` 后端」。本卡把手段改为「减 spawn」，
**目标（大仓库首屏性能）未变**。这不是新造卡号，而是同一张卡的手段替换，
依据是 `ADR/002` 的实测数据。若将来第五节的重估条件成立，`gix` 会作为
**新的决策**重新进入讨论，而不是在这一张卡里「顺便实现」。

## 验证记录（本机 macOS，2026-10-09）

```text
cargo test -p repo-prism-core              → 53 lib + 5 + 10 + 2 + 5 + 16 + 8 passed
cargo test -p repo-prism-core --test perf  → 2 passed（含 spawn_counts_are_pinned）
bash scripts/read-only-guard.sh            → Read-only guard passed
cargo test -p repo-prism-core --test scale -- --ignored --nocapture
                                           → 4 万提交：snapshot 151ms / commits 70ms（干净轮）
```
