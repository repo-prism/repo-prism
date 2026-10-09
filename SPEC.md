# SPEC.md — RepoPrism 规格说明 v0.2

> 本文档是 RepoPrism 的**唯一需求来源**。任何功能变更必须先改本文档，再改代码。
>
> **编号权威**：US 编号与原规划（`RepoPrism-仓库棱镜-001`）一致，只增不改。
> 新增需求往后续号，**不得重排既有编号的语义**——编号一漂移，历史文档就自相矛盾。
>
> **状态标记**：每条要求都标 `[已实现]` 或 `[待实现]`。未实现的条目**不得**在 README
> 或其他文档中描述为「已提供」——文档承诺与代码能力的差距曾经真实存在过。
>
> 文档版本与产品版本无关；产品版本见 `Cargo.toml`（当前 `0.1.0`）。

## 一句话定位

只读仓库智能工具，为人类和 Agent 提供代码库的多视图洞察。

## 核心原则

1. **只读**：零写操作，不修改被观察仓库
2. **本地优先**：默认不上传任何代码到云端
3. **AI 协同**：为 Agent 提供结构化数据，而非仅人类可读的界面
4. **编排而非重造**：集成 GitDiagram、GitIngest、DeepWiki，不重复实现

---

## 用户故事

### US-1：查看仓库状态（P0）

作为开发者，打开 RepoPrism 后能立即看到：

- `[已实现]` 当前分支、HEAD 位置、标签
- `[已实现]` 提交图（父子关系）——lane 分配与分支配色，ref 附着到对应提交，
  相对时间显示；单页渲染上限 300 条、lane 上限 10（简化算法，不做完整 Git 图复杂度）
- `[已实现]` 本地分支列表
- `[已实现]` 上游跟踪计数（ahead / behind）
  - 数据取自 `git status --porcelain=v2 --branch` 的 `# branch.upstream` 与 `# branch.ab`，
    与工作区状态**共用同一次子进程调用**，不额外起 `rev-list --count`。
  - 分支**未设置上游**时必须返回 `null`，不得退化为 `ahead=0, behind=0`
    ——「与上游同步」和「没有上游」是两种不同状态。
- `[待实现]` worktree、stash、合并/变基进行中状态

### US-2：查看变更（P0）

`[已实现]` 按以下分组查看文件变更：

- 合并冲突（`conflicts`）
- 已暂存改动（`staged`）
- 工作区改动（`unstaged`，含未跟踪文件）

### US-3：查看提交详情（P0）

`[已实现]` 点击任意提交后看到：

- 变更文件列表（含 `ChangeKind` 与增删行数）
- diff：**并排 / 统一**两种布局，含 Git 原始行号（`old_no` / `new_no` 各自独立推进）
- 作者、日期、父提交
- 原始 unified diff 正文（`patch`），供 Agent 直接消费
- 合并提交：`patch` 为空、显示「合并提交或空 diff」提示，而非报错

`[待实现]` 图片对比、媒体预览、字节预览

#### 安全约束（不可协商，见 SECURITY.md）

- 必须显式传 `--no-textconv`：否则会触发仓库自定义的 textconv 转换器
- 绝不调用 `git lfs`：LFS 只按文本读取指针
- diff 内容一律以**文本节点**渲染，禁止 `dangerouslySetInnerHTML`
- 二进制文件只标记 `binary: true` 与字节计数，**不读取内容**
- 路径取自 `--name-status -z`（原字节），不解析 patch 头部（会被转义 / 追加制表符）
- 截断上限：原始 patch 文本 **2 MiB**、结构化解析 **5000 行**；
  超出必须**显式标记** `truncated`，不得静默丢弃（按字节截断必须落在字符边界上）

### US-4：CLI 只读快照（P0）

```bash
repoprism inspect . --json       # [已实现] 输出 JSON 快照（信封见下）
repoprism commits . --json       # [已实现] 输出提交历史，--limit 上限 2000
repoprism detail . --sha <sha> --json  # [已实现] 输出提交详情与原始 diff
repoprism skill --path           # [已实现] 展开 Skill 到缓存目录并打印路径
repoprism skill --print          # [已实现] 直接打印 Skill 内容
repoprism open . --view changes  # [待实现] 打开桌面应用并定位到指定视图
```

所有 `--json` 输出共用同一信封，Agent 只需实现一次解析：

```json
{ "schema_version": "1", "tool": "repoprism", "tool_version": "0.1.0", "data": { } }
```

`schema_version` 变化时客户端需重新适配；信封字段只增不减，不做静默改名。

### US-5：Agent Skill（P1）

`[已实现]` Agent 可安装并调用 RepoPrism Skill 获取仓库状态。

- Skill 正文位于 `skill/SKILL.md`，以 `include_str!` 内联进 CLI 二进制，随版本分发
- `repoprism skill --path` 把 Skill 展开到**本工具自己的**缓存目录（幂等，内容相同不重写）
- Skill 绝不写入被观察仓库

### US-6：MCP Server（P1）

`[已实现]` Claude Desktop / Cursor 通过 MCP 协议读取仓库状态。

- stdio 传输，JSON-RPC 2.0，newline-delimited
- 三个只读工具：`repoprism_inspect` / `repoprism_commits` / `repoprism_detail`
- `tools/call` 结果放在 `content[0].text`，且 `text` **必须是字符串**
- 无 `id` 的消息按通知处理，不返回响应
- 不暴露 `git` 透传入口；本 crate 不直接调用 Git，只调用 `repo-prism-core`

### US-7：AI 变更摘要（P1）

`[待实现]` 对未提交改动生成人类可读摘要与风险标记（只读，不修改）。
默认纯本地，LLM 摘要层需显式启用且只允许 localhost endpoint。

### US-8：一键跳转集成（P1）

`[待实现]` 从 RepoPrism 一键跳转到 GitDiagram / GitIngest / DeepWiki / GitHub.dev。

### US-9：多仓库工作区（P2）

`[待实现]` 同时打开多个仓库，统一界面切换。

### US-10：PR/MR 只读视图（P2）

`[待实现]` 通过 `gh` CLI 或 API 只读展示 PR 状态、评审、CI 结果。

---

## 非功能需求

| 维度   | 指标                          |
| ------ | ----------------------------- |
| 只读   | 零写操作，CI 静态扫描         |
| 性能   | 大仓库首屏 < 3s，内存 < 300MB |
| 跨平台 | macOS / Windows / Linux       |
| 安全   | 不运行外部 filter，不下载 LFS |
| 隐私   | 默认本地优先，云功能需显式开启 |

### 只读安全

`[已实现]` CI 静态扫描 `crates/repo-prism-core/src`，实现为 `scripts/read-only-guard.sh`：

- 三层校验：动词白名单 + 危险选项黑名单 + 条件动词必须带只读标志
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

---

## 视图清单

| 视图       | 数据来源                    | 优先级 | 状态       |
| ---------- | --------------------------- | ------ | ---------- |
| 提交图     | 系统 git（ADR-001）         | P0     | 已实现     |
| 变更分组   | `git status --porcelain=v2` | P0     | 已实现     |
| Diff       | `git show` / `git diff`     | P0     | 已实现     |
| 分支列表   | `for-each-ref`              | P0     | 已实现     |
| 提交详情   | `git show --stat`           | P0     | 已实现     |
| CLI JSON   | core 直接输出               | P0     | 已实现     |
| Skill      | CLI 包装                    | P1     | 已实现     |
| MCP Server | core 直接输出               | P1     | 已实现     |
| 图片 / 字节预览 | `git cat-file`         | P0     | 待实现     |
| worktree / stash 状态 | `worktree list` / `stash list` | P0 | 待实现 |
| AI 摘要    | LLM（本地或云可选）         | P1     | 待实现     |
| 外部跳转   | URL 拼接                    | P1     | 待实现     |
| 文件热度   | `git log --numstat`         | P2     | 待实现     |
| PR/MR      | `gh` CLI / API              | P2     | 待实现     |
| 多仓库     | 本地配置                    | P2     | 待实现     |

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
| `CommitDetail` | `info` + `files: FileStat[]` + `patch`（原始 diff 正文）+ `truncated` |
| `FileStat` | `path` / `old_path` / `kind` / `additions` / `deletions` / `binary` |
| `Diff` | `files: DiffFile[]` + `truncated` |
| `DiffFile` | 路径 / 类型 / `binary` / `truncated` / 增删计数 / `hunks` |
| `DiffHunk` | 原始 `header` + 新旧起始行与行数 + `lines` |
| `DiffLine` | `kind`（context/add/del）+ `old_no` / `new_no`（缺侧为 `null`）+ `content` |

**尚未落地**：`RepoSnapshot` 目前不含 `worktrees` / `stashes`（US-1 待实现部分），
模型里也没有对应结构——不留「先声明后实现」的空壳字段。

## 边界与不做的事

- ❌ 不做任何写操作（commit / push / merge / rebase / checkout / stage）
- ❌ 不做完整 Git 客户端（不替代 GitKraken / Fork / Tower）
- ❌ 不做代码编辑器（集成 GitHub.dev 而非自研编辑器）
- ❌ 不做 CI/CD 平台
- ❌ 不做代码托管
