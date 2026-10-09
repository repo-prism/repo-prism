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
| `005` | TASK-013 | 本地 AI 摘要层（Ollama） | ⬜ 未开始 |
| `005` | TASK-014 | 提交级 AI 分析 | ⬜ 未开始 |
| `005` | TASK-015 | MCP 工具扩展 | ⬜ 未开始 |
| `005` | TASK-016 | 前端虚拟滚动 | ⬜ 未开始 |
| `006` | — | 发布 v0.2.0 | ⬜ 未开始 |
| `005` 预告 | TASK-017–020 | `gix` 后端 / 增量缓存 / 多仓库 / PR 只读视图（v0.3） | ⬜ 未开始 |

### 已知偏差（在 TASK-003 批次前后补做的工程补丁）

| 卡 | 主题 | 状态 |
|----|------|------|
| [P-01](TASKS/patch/P-01-upstream-tracking.md) | 补全上游跟踪 ahead/behind（US-1 缺口） | ✅ |
| [P-02](TASKS/patch/P-02-readonly-guard-hardening.md) | 只读白名单加固到参数级 | ✅ |
| [P-03](TASKS/patch/P-03-deps-ci-hygiene.md) | 依赖与 CI 卫生（pnpm / build / 性能门禁） | ✅ |
| [P-04](TASKS/patch/P-04-docs-consistency.md) | SPEC 补全 + SECURITY 对齐 | ⚠️ 已被取代 |
| [P-05](TASKS/patch/P-05-guard-scope-and-write-verbs.md) | 只读扫描器补调用门槛与写动词黑名单 | ✅ |

---

## 四、当前门禁（2026-10-09 实测）

| 检查项 | 命令 | 结果 |
|--------|------|------|
| 只读扫描自检 | `bash scripts/read-only-guard.sh --self-test` | ✅ 26/26（14 正例 + 12 反例） |
| 只读扫描 | `bash scripts/read-only-guard.sh` | ✅ passed |
| Rust 格式 | `cargo fmt --all -- --check` | ✅ 干净 |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 无警告 |
| Rust 测试 | `cargo test --workspace` | ✅ 99 passed（core 75 / CLI 14 / MCP 10） |
| 性能门禁 | `cargo test -p repo-prism-core --test perf` | ✅ snapshot 164ms / 500ms、commits 78ms / 800ms |
| 前端 lint | `biome check src` | ✅ 23 files |
| 前端类型 | `tsc --noEmit` | ✅ 无错误 |
| 前端测试 | `vitest run` | ✅ 31 passed |
| 前端构建 | `vite build` | ✅ built |
| 版本一致性 | `pnpm release:dry` | ✅ `next version: 0.1.0` |
| Workflow YAML | `yaml.safe_load` 解析 `ci.yml` / `release.yml` | ✅ 可解析，依赖顺序符合预期 |

**性能门禁的采样方式**：预热一次 + 采样 3 次取**最小值**。
单次采样会把调度抖动算成代码性能——同一份代码实测出现过 208ms / 470ms / 576ms
（576ms 那次直接超预算、测试变红）。绝对值随机器负载浮动，**只应看是否超预算**，
不要跨机器比较。（上表 164ms 是负载较轻时的实测值；同一批代码在重负载下测到过 244ms。）

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
| US-6 MCP Server（3 个只读工具） | ✅ 已实现（工具扩展见 TASK-015） |
| US-7 AI 变更摘要 | 🟡 本地启发式部分已实现（10 条规则）；LLM 层待 TASK-013 |
| US-8 一键跳转集成 | ✅ 已实现 |
| US-9 多仓库工作区 | ❌ 未实现（TASK-019） |
| US-10 PR/MR 只读视图 | ❌ 未实现（TASK-020） |

---

## 六、下一步

`004` 批次已完成。按原规划进入 `005` 批次：

1. **TASK-013 本地 AI 摘要层（Ollama）** —— 实现 `Summarizer` trait，
   只允许 localhost endpoint，**默认关闭**、显式启用。`analysis.rs` 已留好接口。
2. **TASK-014 提交级 AI 分析** —— 把分析从「工作区」扩到「单个提交」
3. **TASK-015 MCP 工具扩展** —— 现在 MCP 只有 3 个工具；
   `ChangeAnalysis` 与 `RemoteInfo` 已就绪，可加 `repoprism_analyze` 与 `repoprism_remote`
4. **TASK-016 前端虚拟滚动** —— 提交图当前单页 300 条上限
5. **`006` 发布 v0.2.0** —— 走 `docs/RELEASE.md` 的流程

### 独立待办（不属 `005`，需先决条件）

| 事项 | 阻塞原因 |
|------|---------|
| 代码签名与 macOS 公证 | 缺 Apple Developer 证书与 Windows 代码签名证书 |
| 产品命名统一 | `src-tauri/tauri.conf.json` 的 `productName` 仍是 `repoprism-app`，与 `RepoPrism` 不一致；`AGENTS.md` 规定命名由人类主导，未擅自改 |
| 图片 / 字节预览 | 需先定「只读取 blob 字节」的边界（`git cat-file`），并给 diff 定性能阈值 |

### 未解决的风险（如实说明）

- **UI 接线没有自动化测试**：`App ↔ invoke` 一层需要 Tauri 运行时，单测覆盖不到；
  纯逻辑（graph / diff / risk / integrations / kinds / format）都有测试，
  但「点提交 → 出 Diff」「点按钮 → 开浏览器」需人工 `pnpm tauri dev` 确认。
  TASK-011 已给 `openUrl` 失败加了一层降级显示，但未改变「无端到端测试」这一事实。
- **CI 尚未验证**：全部门禁都是本机跑通的；`ci.yml` 与 `release.yml` 都还没在 GitHub 上执行过。
  `release.yml` 尤其需要人工盯首次运行。
- **MCP 未与真实客户端联调**：协议行为由 10 个真实子进程测试覆盖，
  但尚未在 Claude Desktop / Cursor 中实机连接过。
- **发布产物未签名**：安装时会有系统警告，见 `CHANGELOG.md` 的 Known limitations。
