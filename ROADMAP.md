# ROADMAP.md — RepoPrism 现状盘点与执行计划

> 更新日期：2026-10-09（第二版：阶段一、二已完成，见 §3）
> 依据：`SPEC.md`（唯一需求来源）/ `AGENTS.md` / `SECURITY.md` / `ADR/001` 与仓库实际源码
> 本文只做**规划**。功能变更以 `SPEC.md` 为准，落地以 `TASKS/` 任务卡为单位。

---

## 一、项目目标

**只读**的仓库智能工具：把同一份本地 Git 仓库折射成提交图、变更分组、Diff、架构图、
文本化仓库等多种视图，**同一份结构化数据同时供人类与 AI Agent 消费**
（桌面端 / CLI / Skill / MCP 四条出口）。

四条不可让渡的原则：只读 · 本地优先 · AI 协同 · 编排而非重造。

---

## 二、当前状态（2026-10-09 实测）

### 质量门禁

| 检查项 | 命令 | 结果 |
|--------|------|------|
| 只读扫描自检 | `bash scripts/read-only-guard.sh --self-test` | ✅ 18/18（10 正例 + 8 反例） |
| 只读扫描 | `bash scripts/read-only-guard.sh` | ✅ passed |
| Rust 格式 | `cargo fmt --all -- --check` | ✅ 无输出 |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 无警告 |
| Rust 测试 | `cargo test --workspace` | ✅ 41 passed |
| 性能门禁 | `cargo test -p repo-prism-core --test perf` | ✅ snapshot 208ms / commits 84ms |
| 前端 lint | `biome check src` | ✅ 18 files |
| 前端类型 | `tsc --noEmit` | ✅ 无错误 |
| 前端测试 | `vitest run` | ✅ 17 passed |
| 前端构建 | `vite build` | ✅ built |

### 规格覆盖

| 用户故事 | 状态 |
|---------|------|
| US-1 仓库状态（分支 / HEAD / 标签 / 变更分组 / **上游计数**） | ✅ 已实现（worktree / stash / 合并变基状态待实现） |
| US-2 变更分组（conflict / staged / unstaged） | ✅ 已实现 |
| US-3 提交详情 + **Diff（统一 / 并排，含原始行号）** | ✅ 已实现（图片对比 / 字节预览待实现） |
| US-4 CLI | ✅ `inspect --json`；`open` / `skill --path` 待实现 |
| US-5 提交图 | ✅ 已实现 |
| US-6 Agent 出口（MCP / Skill） | ❌ 未实现 |
| US-7 编排集成（文本化仓库 / 架构图 / AI 文档） | ❌ 未实现 |

---

## 三、已完成（阶段一 · 阶段二）

| 卡 | 主题 | 结论 |
|----|------|------|
| [TASK-007](TASKS/007-core-diff.md) | core 只读 Diff + 前端 Diff 视图 | ✅ 新增 `diffparse` 模块、6 个 core API、DiffView（统一/并排）；路径取自 `--name-status -z`（原字节），不从 patch 头部抠 |
| [TASK-008](TASKS/008-upstream-tracking.md) | 上游 ahead/behind | ✅ 复用已有的 `status` 输出，未新增子进程；无上游 ≠ 同步 |
| [TASK-009](TASKS/009-docs-consistency.md) | SPEC 补全 + SECURITY 对齐 | ✅ SPEC v0.2（含数据模型与实现状态标记）；SECURITY 移除重复白名单并新增威胁 6 |
| [TASK-010](TASKS/010-readonly-guard-hardening.md) | 只读白名单加固到参数级 | ✅ 三层校验；顺带修掉「扫描器崩了却报 passed」的假绿灯（现以退出码 2 失败） |
| [TASK-011](TASKS/011-deps-ci-hygiene.md) | 依赖与 CI 卫生 | ✅ 移除误加的 `pnpm` 运行时依赖；CI 增加 `pnpm build`、`perf` job、clippy `--all-targets` |

已完成的历史卡片：TASK-001 ~ TASK-006（workspace / core 快照 / CLI / 提交图 / 前端骨架）。

---

## 四、待办

### 阶段三：补齐 US-3 的剩余部分（P0）

1. **图片对比 / 媒体预览 / 字节预览**
   需先定「如何用只读方式取 blob 字节」（`git cat-file`）与二进制渲染边界；
   顺带为本阶段定 diff 的性能阈值。
2. **worktree / stash / 合并变基进行中状态**（US-1 剩余）
   `git status --porcelain=v2` 已含部分信息（`# branch.*` 头部已解析），
   `worktree` / `stash` 需新增只读调用；**注意 `stash` 是条件动词**，
   只允许 `stash list`（白名单已支持）。

### 阶段四：Agent 出口（P1）

3. **MCP Server**（`crates/repo-prism-mcp` 当前 3 行占位）
   只读工具：`inspect_repo` / `get_commits` / `get_commit_detail` / `get_diff`，stdio 传输。
4. **Agent Skill 打包**（`repoprism skill --path`）
5. **CLI 补面**：`repoprism open . --view changes`；`inspect` 增加子命令

### 阶段五：编排集成（P1，**必须先改 SPEC**）

6. 文本化仓库（GitIngest 路线）/ 架构图（GitDiagram）/ AI 文档（DeepWiki）

> 接入方式必须先定案「源码是否离开本机」。当前原则 2（本地优先）尚未给出兼容结论，
> 故 SPEC 中这三项保持 `[待实现]`，不得先行实现。

---

## 五、任务卡索引

| 卡 | 标题 | 状态 |
|----|------|------|
| [TASK-007](TASKS/007-core-diff.md) | core 只读 Diff + 前端 Diff 视图 | ✅ 完成 |
| [TASK-008](TASKS/008-upstream-tracking.md) | 上游跟踪 ahead/behind | ✅ 完成 |
| [TASK-009](TASKS/009-docs-consistency.md) | SPEC 补全与 SECURITY 对齐 | ✅ 完成 |
| [TASK-010](TASKS/010-readonly-guard-hardening.md) | 只读白名单加固到参数级 | ✅ 完成 |
| [TASK-011](TASKS/011-deps-ci-hygiene.md) | 依赖与 CI 卫生 | ✅ 完成 |

下一张卡建议从「图片对比 / 字节预览」开始——它是 US-3 的最后一块，
且能顺带把 diff 的性能阈值一并定下来。
