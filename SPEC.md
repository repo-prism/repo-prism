# SPEC.md — RepoPrism 规格说明 v0.2

> 本文档是 RepoPrism 的**唯一需求来源**。任何功能变更必须先改本文档，再改代码。
>
> **状态标记**：每条要求都标 `[已实现]` 或 `[待实现]`。未实现的条目**不得**在 README
> 或其他文档中描述为「已提供」——文档承诺与代码能力的差距曾经真实存在过。

## 一句话定位

只读仓库智能工具，为人类和 Agent 提供代码库的多视图洞察。

## 核心原则

1. **只读**：零写操作，不修改被观察仓库
2. **本地优先**：默认不上传任何代码到云端
3. **AI 协同**：为 Agent 提供结构化数据，而非仅人类可读的界面
4. **编排而非重造**：集成 GitDiagram、GitIngest、DeepWiki，不重复实现

## 数据模型

以 `crates/repo-prism-core/src/model.rs` 为唯一事实来源，全部字段以 `snake_case` 序列化，
前端 `src/lib/api.ts` 与之逐字段对齐。

| 结构 | 用途 |
|------|------|
| `RepoSnapshot` | 仓库快照：`path` / `head` / `branches` / `tags` / `status` |
| `HeadInfo` | `branch`（detached 时为 `null`）/ `commit` / `detached` / `upstream` |
| `UpstreamInfo` | `name` / `ahead` / `behind` |
| `BranchInfo` / `TagInfo` | 名称 + 指向的提交 |
| `StatusInfo` | `conflicts` / `staged` / `unstaged` 三组 `FileChange` |
| `FileChange` | `path` + `kind`（`ChangeKind`） |
| `CommitInfo` | sha / 父子关系 / 作者与提交者 / 时间 / 标题 / 正文 / refs |
| `CommitDetail` | `commit` + `files: FileStat[]` |
| `FileStat` | `path` / `old_path` / `kind` / `additions` / `deletions` / `binary` |
| `Diff` | `files: DiffFile[]` + `truncated` |
| `DiffFile` | 路径 / 类型 / `binary` / `truncated` / 增删计数 / `hunks` |
| `DiffHunk` | 原始 `header` + 新旧起始行与行数 + `lines` |
| `DiffLine` | `kind`（context/add/del）+ `old_no` / `new_no`（缺侧为 `null`）+ `content` |

---

## US-1：查看仓库状态（P0）

作为开发者，打开 RepoPrism 后能立即看到：

- `[已实现]` 当前分支、HEAD 位置、标签
- `[已实现]` 提交图（父子关系）
- `[已实现]` 本地分支列表
- `[已实现]` 上游跟踪计数（ahead / behind）
  - 数据取自 `git status --porcelain=v2 --branch` 的 `# branch.upstream` 与 `# branch.ab`，
    与工作区状态**共用同一次子进程调用**，不额外起 `rev-list --count`。
  - 分支**未设置上游**时必须返回 `null`，不得退化为 `ahead=0, behind=0`
    ——「与上游同步」和「没有上游」是两种不同状态。
- `[待实现]` worktree、stash、合并/变基进行中状态

## US-2：查看变更（P0）

`[已实现]` 按以下分组查看文件变更：

- 合并冲突（`conflicts`）
- 已暂存改动（`staged`）
- 工作区改动（`unstaged`，含未跟踪文件）

## US-3：查看提交详情（P0）

`[已实现]` 点击任意提交后看到：

- 变更文件列表（含 `ChangeKind` 与增删行数）
- diff：**并排 / 统一**两种布局，含 Git 原始行号（`old_no` / `new_no` 各自独立推进）
- 作者、日期、父提交

`[待实现]` 图片对比、媒体预览、字节预览

### 安全约束（不可协商，见 SECURITY.md）

- 必须显式传 `--no-textconv`：否则会触发仓库自定义的 textconv 转换器
- 绝不调用 `git lfs`：LFS 只按文本读取指针
- diff 内容一律以**文本节点**渲染，禁止 `dangerouslySetInnerHTML`
- 二进制文件只标记 `binary: true` 与字节计数，**不读取内容**
- 路径取自 `--name-status -z`（原字节），不解析 patch 头部（会被转义 / 追加制表符）
- 单次解析上限 5000 行；超出必须**显式标记** `truncated`，不得静默丢弃

## US-4：CLI 只读快照（P0）

```bash
repoprism inspect . --json       # [已实现] 输出 JSON 快照
repoprism open . --view changes  # [待实现]
repoprism skill --path           # [待实现]
```

## US-5：提交图（P0）

`[已实现]`

- lane 分配与分支配色
- ref（分支 / 标签）附着到对应提交
- 相对时间显示
- 单页渲染上限 300 条；lane 上限 10（简化算法，不做完整 Git 图复杂度）

## US-6：Agent 出口（P1）

- `[待实现]` MCP Server（`crates/repo-prism-mcp`，当前为占位）
- `[待实现]` Agent Skill 打包（`repoprism skill --path`）

## US-7：编排集成（P1）

- `[待实现]` 文本化仓库（GitIngest 路线）
- `[待实现]` 架构图（GitDiagram）
- `[待实现]` AI 文档（DeepWiki）

> 三者接入前必须先在本文档定案「源码是否离开本机」。当前原则 2（本地优先）
> 尚未给出与之兼容的结论，故不得先行实现。

---

## 非功能要求

### 只读安全

`[已实现]` CI 静态扫描 `crates/repo-prism-core/src`，实现为 `scripts/read-only-guard.sh`：

- 先自检扫描器本身（正例不误报、反例能检出），再扫描真实代码
- 扫描器**自身执行失败**（如 awk 报错）必须以非零码退出，不得被当作「通过」
- 详见 SECURITY.md

### 性能

`[已实现]` 由 `crates/repo-prism-core/tests/perf.rs` 断言，CI 的 `perf` job 执行：

- `snapshot()` < 500ms
- `commits(200, 0)` < 800ms

基准仓库由 `git fast-import` 在测试内构造 3000 条提交，不依赖网络；
**阈值不因仓库变小而下调**。

### 跨平台

`[已实现]` CI 在 ubuntu / macos / windows 三平台运行 fmt / clippy / test。

## 与实现的状态对照

| 能力 | 状态 |
|------|------|
| 仓库快照（分支 / HEAD / 标签 / 变更分组 / 上游计数） | 已实现 |
| 提交图 | 已实现 |
| 提交详情 + Diff（统一 / 并排） | 已实现 |
| CLI `inspect --json` | 已实现 |
| 只读扫描（参数级白名单） | 已实现 |
| 性能门禁 | 已实现 |
| worktree / stash / 合并变基状态 | 待实现 |
| 图片对比 / 媒体预览 / 字节预览 | 待实现 |
| CLI `open` / `skill --path` | 待实现 |
| MCP Server | 待实现 |
| 编排集成（文本化仓库 / 架构图 / AI 文档） | 待实现 |
