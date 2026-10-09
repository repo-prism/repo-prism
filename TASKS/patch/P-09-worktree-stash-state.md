# P-09: 补全 worktree / stash / 进行中操作状态

**状态**：已完成（2026-10-09）
**卡号说明**：本卡不在 001–020 的产品血统内，属工程补丁，故编入 patch 序列（见 [README](README.md)）。
判据与 [P-01](P-01-upstream-tracking.md) 一致：**US-1 是原规划里就有的需求**，
这一条只是没做完，所以是「补缺口」而不是「新能力」——不占血统卡号。

## 目标

补齐 US-1「查看仓库状态」里最后一条 `[待实现]`：

1. **进行中的操作状态**：正在合并 / 变基 / cherry-pick / revert / bisect，以及在变基的第几步
2. **worktree 列表**：本仓库连出的所有工作树（路径、检出分支、是否分离 / bare / 锁定 / 主工作树）
3. **stash 列表**：`stash@{n}`、指向的提交、说明文字

## 上下文

- 开工前 SPEC 的视图清单里「worktree / stash 状态」标 **P0 且仍是 `[待实现]`**
  （本卡已改为 `[已实现]`）—— 它是优先级表里仅剩的两条 P0 缺口之一，
  另一条是 US-3 的图片 / 字节预览。US-1 也是四原则里「只读」体现得最直接的一条故事。
- **TASK-018 刚把 `git_dir`（每个工作树各一份）与 `common_dir`（所有工作树共享）分开**，
  而 `MERGE_HEAD` / `rebase-merge/` / `rebase-apply/` / `CHERRY_PICK_HEAD` /
  `REVERT_HEAD` / `BISECT_LOG` **恰好全部住在 `git_dir` 里**。这既让本卡几乎零成本，
  也顺带成为 TASK-018 那处拆分的第一个真实用途。
- 只读扫描器已经把 `worktree` / `stash` 收进**条件动词**表（只允许 `list` 形式），
  自检里已有 `stash list` 正例与 `worktree remove` 反例 —— 无需改扫描器。

## 关键决策

### 1. 进行中状态是**零子进程**的，所以能进快照

状态探测只做文件系统查询（存在性 + 读几个数字文件），一次进程都不起。
因此它可以挂进 `snapshot()`，而 `snapshot()` **仍是恰好 2 次子进程** ——
`tests/perf.rs::spawn_counts_are_pinned` 一字未改，仍然通过。

这条不是巧合，而是**放进快照的依据**：零成本的字段可以进首屏，
要起进程的字段（worktree / stash）另走 `workspace()`。
和 TASK-018 把引用缓存做成 opt-in 是同一条理由 —— 不让一个方法有随状态变化的成本。

### 2. worktree / stash 单独取

`worktree list --porcelain` 与 `stash list` 各要 1 次进程，合起来 `workspace()` 是 2 次。
它们与快照分开，因此：

- 用户不展开这两块时不付这个成本（前端按需调用）
- 快照的 2 次契约与 `snapshot()+commits()` 的 4 次契约都不受影响

### 3. 状态优先级：**最具体的标记优先**

```
rebase-merge  →  rebase-apply  →  cherry-pick  →  revert  →  merge  →  bisect
```

顺序**由真实夹具实测确定**，不靠猜（cherry-pick 冲突时是否同时留下 `MERGE_HEAD`
是本卡实测的第一个问题；实测结论见「探针与实测」）。
取第一个命中的标记，避免同时存在多个标志时报出一个不相关的状态。

### 4. 列表解析走纯函数

`parse_worktrees` / `parse_stashes` / `state_from_markers` 都是纯函数，
输入是命令输出或一组布尔/字符串，**不碰进程**。这样「porcelain 输出格式变了怎么降级」
这类分支能被断言，而不是只能靠真实仓库碰运气。

## 实际改动

| 文件 | 改动 |
|------|------|
| `crates/repo-prism-core/src/model.rs` | 新增 `RepoState` / `WorktreeInfo` / `StashInfo` / `WorkspaceInfo`；`RepoSnapshot` 增加 `state` 字段 |
| `crates/repo-prism-core/src/git.rs` | 新增 `state()`（零子进程）、`worktrees()`、`stashes()`、`workspace()`；纯函数 `state_from_markers` / `parse_worktrees` / `parse_stashes` / `read_progress` |
| `crates/repo-prism-core/tests/workspace.rs` | 新增：真实夹具构造冲突合并 / 变基 / cherry-pick / linked worktree / stash，逐项断言 |
| `crates/repo-prism-core/tests/perf.rs` | 新增 `workspace()` = 2 次；`snapshot()` 仍是 2 次（**未改**） |
| `src-tauri/src/lib.rs` | 新增 `get_workspace` 命令（复用 TASK-018 的会话） |
| `src/lib/api.ts` | 对齐新类型 |
| `src/lib/workspace.ts` + `workspace.test.ts` | 状态文案与进度文本的纯函数 + 断言 |
| `src/components/RepoStateBadge.tsx` / `WorkspacePanel.tsx` | 侧边栏展示 |
| `src/components/RepoHeader.tsx` / `src/App.tsx` / `src/App.css` | 接入 |

## 验收标准

全部 13 条都有可执行断言在 `tests/workspace.rs`（11 条）与 `tests/perf.rs`（2 条）里，
**不是**「打开界面看一眼」。

- [x] 冲突合并中：`state == merge`
- [x] 变基中：`state == rebase`，且 `step` / `total` 与实际进度一致（`msgnum` / `end`）
- [x] cherry-pick 冲突中：`state == cherry_pick`
- [x] revert 冲突中：`state == revert`
- [x] `git bisect start` 之后：`state == bisect`
- [x] 干净仓库：`state == null`（**不是**某个「干净」变体 —— 与 `upstream` 的 `null` 同构）
- [x] linked worktree：`worktrees()` 返回主工作树 + 链接工作树，分支与路径正确
- [x] `bare` / `detached` / 锁定工作树各自被正确标记
- [x] stash：数量、`reference`、`message` 正确；无 stash 时为空数组
- [x] `snapshot()` 的子进程次数**仍是 2**（状态探测没引入 spawn）—— 该断言一字未改
- [x] `workspace()` 恰好 2 次
- [x] 前端：状态角标显示「变基 3/7」这类进度；worktree / stash 列表可展开（`src/lib/workspace.test.ts` 14 条）
- [x] 只读扫描器通过（`worktree` / `stash` 只以 `list` 形式出现）

## 探针与实测

### 1. 假设探针（写实现**之前**跑，验证格式假设，不靠猜）

探针脚本 `/private/tmp/probe_p09.sh`，用真实 git 造出每一种状态后列 `<git-dir>`：

| 假设 | 实测结果 |
|------|----------|
| 变基冲突时 `rebase-merge/` 在场，进度 `msgnum=1` / `end=2` | ✅ 与预期一致 |
| 变基冲突**不**留 `MERGE_HEAD` | ✅ 目录里只有 `AUTO_MERGE` / `COMMIT_EDITMSG` / `ORIG_HEAD` / `REBASE_HEAD` / `rebase-merge` |
| 摘取冲突**不**留 `MERGE_HEAD` | ✅ 只有 `CHERRY_PICK_HEAD` |
| 回退冲突**不**留 `MERGE_HEAD` | ✅ 只有 `REVERT_HEAD` |
| `git bisect start` **当时**就留下 `BISECT_LOG` | ✅ 另有 `BISECT_NAMES` / `BISECT_START` |
| `worktree list --porcelain` 的 `branch` 是**全 ref**（`refs/heads/main`） | ✅ 需削前缀 |
| bare 工作树的记录**没有** `HEAD` 行，只有 `bare` | ✅ 解析器已容错 |
| `stash list --format=%gd%x1f%H%x1f%gs%x1e` 三字段正常 | ✅ 说明文字里的中文与空格未被切坏 |

第一条与第三条**原本是我最不确定的**：如果 cherry-pick 冲突会顺带留下 `MERGE_HEAD`，
那么「最具体的标记优先」这条优先级就必须改成别的形式。实测它不会 —— 优先级才成立。

### 2. 失败路径探针（写完测试**之后**跑，反证测试真的会红）

Rust 侧 9 条、前端侧 4 条，逐条破坏被测逻辑：

| # | 破坏方式 | 结果 |
|---|----------|------|
| R1 | `state_from_markers` 直接返回 `None` | 断言失败（红） |
| R2 | 优先级顺序倒过来（先看 `BISECT_LOG`） | 红 |
| R3 | 进度读取忽略 `rebase-apply` 后端 | 红 |
| R4 | `parse_worktrees` 不削 `refs/heads/` 前缀 | 红 |
| R5 | `parse_worktrees` 忽略 `locked` 标记 | 红 |
| R6 | `parse_stashes` 用 `:` 切分（而非 `\x1f`） | 红 |
| R7 | `snapshot()` 里去掉 `state` 字段 | 红 |
| R8 | `workspace()` 里去掉 `stashes()` | 红 |
| R9 | 状态探测改成起一次子进程 | perf 门禁红 |
| F1 | 进度分子分母互换（3/7 → 7/3） | 红：`expected '7/3' to be '3/7'` |
| F2 | 进度空值判据 `\|\|` 改 `&&`（放行半边进度） | 红：`expected '3/null' to be null` |
| F3 | 工作树名只按正斜杠切（忘掉 Windows 反斜杠） | 红：`expected 'C:\repos\linked' to be 'linked'` |
| F4 | stash 标签恒退回引用名 | 红：`expected 'stash@{0}' to be 'WIP on main'` |

R9 是本卡最要紧的一条：它证明「状态探测不引入子进程」这条**真的被门禁看着**，
而不只是注释里的一句承诺。

### 3. 端到端实测（真实 git，非 stub）

`tests/workspace.rs` 的夹具**真的跑 git 去制造**每一种状态 —— 真建分支、真造冲突、
真建链接工作树、真 stash，然后断言读出来的东西。11 条全绿。

子进程次数实测（`tests/perf.rs`，`Git::spawns()` 数真实次数）：

| 调用 | 次数 | 说明 |
|------|------|------|
| `snapshot()` | **2** | 与 P-09 之前**完全相同**，状态探测是文件探测 |
| `workspace()` | **2** | `worktree list` + `stash list` |

## 与归档规划的偏离

归档（`RepoPrism-仓库棱镜-001` §US-1）把「worktree、stash、合并/变基状态」写在 US-1 里，
但**从未给它分配卡号** —— 001–016 排到 v0.2、017–020 是 v0.3，其中 019 = 多仓库、
020 = PR 只读视图。因此本卡按 P-01 的先例编入 patch 序列（`P-09`），
**不改动 019 / 020 的语义**。

另一处偏离：归档只写了「合并/变基」，本卡把 **cherry-pick / revert / bisect**
一并做了。理由：它们与「合并/变基」读的是**同一批标志文件**，一起做不增加任何
子进程或依赖；只做一半反而要在下一张卡里回头改同一个函数。

## 禁止事项

- [x] 不执行任何写子命令（`merge` / `rebase` / `cherry-pick` / `commit` / `stash pop` /
      `worktree remove` / `worktree prune`）—— 只观测
      **例外**：`tests/workspace.rs` 的夹具为了**制造**状态而跑写命令。
      扫描器只扫 `crates/repo-prism-core/src`，测试不在视野内；这是有意的 ——
      不真造出冲突就读不到 `MERGE_HEAD`，用桩替代会让「判据来自真实 git」这个性质失效。
- [x] 不使用 `git stash show`（它会展开 stash 的 diff，属 US-3 的范畴，本卡不碰内容）
- [x] 不在 core 之外解析 Git 输出
- [x] 不为「进行中状态」新增任何子进程

## 踩到的坑：规则 4 是**全局**的

只读扫描器的第 4 层（写动词黑名单）不像第 1 层那样有「只在调 Git 的语句里生效」的门槛，
它对**任何** `"..."` 字面量生效。本卡第一次跑扫描被拦下 5 处，全部是我自己写的**注释与提示文案**：

| 位置 | 原文 | 处理 |
|------|------|------|
| `git.rs` 单测 | `temp_dir("clean")` | 改名为 `temp_dir("marker-order")` |
| `git.rs` 单测 | `.expect("rm")` ×3 | 改为 `.expect("unlink marker")` |
| `model.rs` 文档注释 | `{"kind":"rebase",…}` | 去掉引号写成 `kind=rebase, step=3, total=7` |

其中最后一处最讽刺：我为了**提醒别人别这么写**而在注释里写了一个带引号的 `rebase`，
结果自己先被拦了一次。修的时候顺手把「别写成带引号的纯小写词」这条留在注释里了 ——
但这次用的是中文「变基」，不带引号。

## 未覆盖的部分

| 项 | 原因 |
|----|------|
| stash 的**内容**（展开 diff） | 属 US-3「图片/字节预览」的范畴，需要先定 blob 读取边界 |
| rebase 的 `--apply` 后端**在真实仓库里**的进度 | 纯函数层有断言（R3），但夹具只造了 merge 后端的真实状态 |
| `git worktree` 的 `locked` / `prune` 原因文本 | porcelain 只给标记不给原因；本卡只显示「锁」 |
| CLI / MCP 侧暴露 `workspace` | 本卡只接线桌面端；CLI 的 `--json` 快照已自动带上 `state` |
