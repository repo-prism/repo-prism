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
  相对时间显示；单页读取上限 300 条、lane 上限 10（简化算法，不做完整 Git 图复杂度）
  - 列表为**虚拟滚动**：DOM 行数只与视口高度有关，与提交总数无关
    （300 条提交下渲染行数 < 80，验收断言见 `src/lib/virtual.test.ts`）
  - 行高固定 48px、overscan 8 行；区间计算抽为纯函数 `computeRange`，可被断言
- `[已实现]` 本地分支列表
- `[已实现]` 上游跟踪计数（ahead / behind）
  - 数据取自 `git status --porcelain=v2 --branch` 的 `# branch.upstream` 与 `# branch.ab`，
    与工作区状态**共用同一次子进程调用**，不额外起 `rev-list --count`。
  - 分支**未设置上游**时必须返回 `null`，不得退化为 `ahead=0, behind=0`
    ——「与上游同步」和「没有上游」是两种不同状态。
- `[已实现]` worktree 列表、stash 列表、**进行中的操作状态**
  （合并 / 变基 / cherry-pick / revert / bisect）
  - 状态探测**不占任何子进程**：直接看 `<git-dir>` 下的标志（`MERGE_HEAD` /
    `rebase-merge/` / `rebase-apply/` / `CHERRY_PICK_HEAD` / `REVERT_HEAD` / `BISECT_LOG`），
    进度取自 `rebase-merge/{msgnum,end}`（或 `rebase-apply/{next,last}`）。
    它因此可以挂在 `snapshot()` 里**而不改变**「恰好 2 次子进程」的契约（见下）。
    `<git-dir>` 是**每个工作树各一份**的那个目录（TASK-018 的 `git_dir`）——
    这些标志也正是在那里。
  - worktree 与 stash 列表需要起进程（`worktree list --porcelain` 与
    `stash list`，各 1 次），因此**不放进快照**，单独由 `Git::workspace()` 取。
    与 TASK-018 把引用缓存做成 opt-in 同一条理由：不让一个方法有随状态变化的成本。
  - 对进行中的操作**只观测、不干预**：不执行 `merge` / `rebase` / `cherry-pick` /
    `commit` / `stash` 任何写子命令。`worktree` / `stash` 是「条件动词」，
    只允许带只读标志的 `list` 形式（`read-only-guard.sh` 第三层）。

### US-2：查看变更（P0）

`[已实现]` 按以下分组查看文件变更：

- 合并冲突（`conflicts`）
- 已暂存改动（`staged`）
- 工作区改动（`unstaged`，含未跟踪文件）

`[已实现]` 每组文件带**风险角标**（红 / 黄 / 蓝对应 critical / warn / info），
面板顶部给出本地摘要条与关键/警告计数。规则清单见 US-7。

`[已实现]` 面板顶部提供「AI 摘要」按钮（US-7 的模型层），**默认置灰**；
未启用本地 AI 或工作区干净时禁用，并给出原因提示。

### US-3：查看提交详情（P0）

`[已实现]` 点击任意提交后看到：

- 变更文件列表（含 `ChangeKind` 与增删行数）
- diff：**并排 / 统一**两种布局，含 Git 原始行号（`old_no` / `new_no` 各自独立推进）
- 作者、日期、父提交
- 原始 unified diff 正文（`patch`），供 Agent 直接消费
- 合并提交：`patch` 为空、显示「合并提交或空 diff」提示，而非报错

- `[已实现]` 图片对比与预览（PNG / JPEG / GIF / WebP / BMP / SVG）：
  新增 / 删除看单张，修改看**旧 vs 新并排**
- `[已实现]` 字节预览：非图片的二进制以**十六进制转储**呈现（前 512 字节 + 真实大小）
- `[已实现]` 按版本查看任意文件的文本内容（含既有 Renamed / Copied 的原路径）
- `[待实现]` 音视频（`<video>` / `<audio>`）播放 —— 见下方「为什么不做媒体播放」

#### blob 只读预览契约（US-3 剩余，补丁 P-10）

**命令形状只有两种**（全部经 `git::Git::run*` 发出）：

```
git cat-file -s   -- <rev>:<path>     # 大小
git cat-file blob -- <rev>:<path>     # 原始字节
```

- **为什么是 `cat-file`**：它是 plumbing —— 2026-10-10 实测证明三大前提：
  ① 同一文件在配置了 smudge filter 的仓库里，**工作区是过滤后的内容（SMUDGED）、
  `cat-file blob` 读到的是原始字节（REAL）**；② 不触发任何 hook；③ 不联网、不下载 LFS。
- **绝不写 `--textconv` / `--filters`**：这两个开关在 `cat-file` 里真实存在且会执行
  仓库自定义转换器。它们已在 `read-only-guard.sh` 的危险选项黑名单里
  （见 `SECURITY.md` 威胁 1），写上去扫描器会直接拦下。
- `--` 分隔符是**必须**的：`-s` 后面的 `<rev>` 若以 `-` 开头会被 git 当成选项
  （实测 `-weird:f.txt` → `unknown switch`）；加 `--` 后被当作对象名（实测通过）。
- 调用方还必须校验 `rev` / `path` 非空、且不以 `-` 开头（纯函数，可断言）。
  命令一律经 `Command::args` 传递、不经 shell，因此 `;` / `$()` 等字符是惰性的
  （实测注入串没有产生任何副作用文件）。

**先看大小，再决定读不读**（与 diff 路径不同：diff 是先读后截断）：

| 调用 | 子进程 | 说明 |
|------|--------|------|
| `blob_info()` | **1** | 只 `-s`，永远不读内容 |
| `blob_preview()` | **2** | `-s` 后再 `blob`；超过上限就**不读内容** |

- `-s` 的成功**不代表那是文件**：目录也会返回一个数字（实测 29），
  只有 `cat-file blob` 会失败（exit 128）。所以 size 只用来判断「值不值得读」，
  绝不能当作「这是一个文件」的证明；第二次调用失败必须**显式报错**，不能显示那个数字。
- 上限：**读取 4 MiB**（`MAX_PREVIEW_BYTES`）、**文本呈现 256 KiB**、**十六进制 512 字节**。
  超出一律**显式标记** `too_large` / `truncated` 并给出真实大小 —— 不静默丢弃。
- 取舍理由：`-s` 对 5 MiB blob 实测 30ms 内返回；若先读内容，一个 2 GiB 的 blob
  会被整个读进内存。反过来，用户主动点击后才读，所以这两次子进程的代价是可接受的。

**LFS 指针必须被识别而非解析**：以 `version https://git-lfs.github.com/spec/v1`
开头的纯文本是 LFS 指针（实测 130 字节）。此时**只展示其中的 object id 与 size**，
并说明「未下载」—— 不调用 `git lfs`、不发任何网络请求（`SECURITY.md` 威胁 2）。

**渲染安全**：SVG 只能作为 `<img>` 的 data URL 渲染，**不得 inline 到 DOM** ——
inline SVG 会执行其中的脚本与事件处理器。所有文案走文本节点（沿用威胁 3 的规则）。

**不在 diff 里自动预览**：只有用户点击某个文件才读取内容。
自动预览会让打开一个含大图或大二进制的提交时，首屏成本变得不可预测。

**为什么不做媒体播放**：`<video>` / `<audio>` 要求把**整段** blob 交给浏览器解码，
这与上面「先看大小再决定读不读」的最小化原则直接冲突 —— 媒体文件几乎必然超过 4 MiB。
要支持就得给媒体单独放宽上限，那等于把资源耗尽的口子重新打开（`SECURITY.md` 威胁 6）。
在「同时保住预览能力与众进程/内存边界」的方案出现之前，这一条保持 `[待实现]`。

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
- 五个只读工具：
  `repoprism_inspect` / `repoprism_commits` / `repoprism_detail` /
  `repoprism_analyze` / `repoprism_remote`
- `tools/call` 结果放在 `content[0].text`，且 `text` **必须是字符串**
- 错误码分层：未知方法 `-32601`、参数缺失或非法 `-32602`；
  不得把「参数错」与「服务端内部错」压成同一个码
- 无 `id` 的消息按通知处理，不返回响应
- 不暴露 `git` 透传入口；本 crate 不直接调用 Git，只调用 `repo-prism-core`
- `repoprism_analyze` 走**本地规则引擎**，不调用模型、不联网（与桌面端同一实现）

### US-7：AI 变更摘要（P1）

`[已实现]` 对未提交改动生成人类可读摘要与风险标记（只读，不修改）。

**本地启发式层（默认行为，无配置即可用）**

- 10 条启发式规则，覆盖异常处理 / 迁移 / API / 配置 / CI / 依赖 / 测试 / 大量删除
- `summary` 为本地拼装，**不调用任何模型**；`Summarizer` trait 默认实现是 `NoopSummarizer`
- 「大量删除」需要行数：由 `git diff --numstat -z` 提供，读不到时不命中（不误报）

**LLM 摘要层（可选，默认关闭）**

- `OllamaSummarizer` 实现 `Summarizer` trait；由 `set_ai_settings` 显式启用后才可调用
- endpoint **只允许 `http://` + 回环地址**（`localhost` / `127.0.0.1` / `[::1]`）；
  判定必须**解析出 host 后精确比对**，不得比前缀——`http://localhost.evil.com`
  与 `http://localhost@evil.com` 都必须被拒（见 SECURITY.md 威胁 7）
- **不遵循环境里的代理**：`HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` 会**改写连接目标**，
  若被遵循则 endpoint 上写着的回环地址不再是实际连接目标。请求显式使用
  `summarizer::local_agent()`（**刻意不调用** `AgentBuilder::proxy_from_env()`），
  由 P-08 的 `tests/summarizer_egress.rs` 用指向死端口的代理钉住
- 送出内容仅限**文件的相对路径与规则结果**：不送仓库绝对路径、不送 remote URL、
  不送 diff 正文、不送文件内容
- 超时 60s；失败降级为 `None`，不把「模型没启动」升级为错误
- 设置持久化到用户配置目录；读取失败回默认值（关闭）

`[已实现]` 回环 stub 上的 HTTP 往返（工程补丁 P-07，`tests/summarizer_http.rs`）：
用 `std::net::TcpListener` 在 127.0.0.1 起 stub，真实跑通 `list_models` / `generate` / `summarize`，
并覆盖连不上 / 5xx / 非法 JSON / 空白 response 四条降级路径。
「出网 body 只含相对路径」这条不变式由**读请求体**直接断言，不再只靠代码审查。

`[已实现]` 与 Ollama **官方 API 文档**的字段契约核对（P-08，`tests/summarizer_contract.rs`）：
响应体**逐字取自**官方 `docs/api.md` 的示例而非我们自己编写，因此字段名写错会被测出来
（`generate` 的 `response`、`list_models` 的 `models[].name`）。
另覆盖「多行 NDJSON 必须整体判为失败而非截取第一行」。

`[已实现]` 出网边界：环境代理不得改道回环请求（P-08，`tests/summarizer_egress.rs`）。
断言的是**性质**而非机制——究竟是客户端不读代理变量、还是读但豁免回环，本测试不做区分
（两者对本条安全承诺等价）。测试先**确认代理地址是死的**再断言请求仍到达回环 stub，
因此「用了代理」必然导致红，绿与红被彻底分开。

`[待实现]` **真实**模型调用的端到端验证（需本机安装 Ollama）：
契约核对的输入来自官方文档示例，证明不了**某个具体版本的实际行为**与文档一致。
截至本版本，**一次真实的模型调用都没有发生过**。

`[已实现]` 提交级 AI 分析：提交详情面板提供「AI 分析此提交」按钮，复用同一摘要器。
切换提交必须清空上一条摘要（由 `key={sha}` 重挂载保证）。

### US-8：一键跳转集成（P1）

`[已实现]` 从 RepoPrism 一键跳转到 GitDiagram / GitIngest / DeepWiki / GitHub.dev。

- 由 `origin` remote 解析出的 `host` / `owner` / `repo` 拼出 URL，**不访问网络**
- 无 `origin`（或 URL 形式不可解析）时返回 `null`，界面提示而非报错
- 跳转交给系统默认浏览器；RepoPrism 自身不抓取外部内容

### US-9：多仓库工作区（P2）

`[已实现]` 同时打开多个仓库，统一界面切换。

**会话表的契约**（`repo_prism_core::RepoSet`）：

| 规则 | 说明 |
|------|------|
| 键是**已解析的仓库根**，不是用户输入的字符串 | 实测：同一个仓库的四种写法（根 / 子目录 / 符号链接 / 符号链接下的子目录）都会被 `rev-parse --show-toplevel` 解析成**同一个绝对路径**，所以拿它当键才是稳定的 |
| 输入别名也命中 | 记住 `输入路径 → 仓库根`，下次用同一写法进来不再起 `rev-parse` |
| **同一仓库的新写法只补一条别名**，不新建会话 | 否则「先经符号链接打开、再用真实路径打开」会变成两个会话，各自缓存一份引用 |
| 命令在**表锁之外**执行 | 表只负责把 `Arc<Git>` 交出去；两条不同仓库的命令因此不互相阻塞 |
| **有上限**（默认 8 个会话），超出按最久未用淘汰 | 无上限的长驻进程（桌面端 / MCP）会随访问过的仓库数无限增长内存。淘汰只丢缓存，**不丢正确性** —— 被淘汰后再打开只是重新读一遍引用 |
| 关闭一个仓库会连它的别名一起清 | 否则关掉之后用原写法再打开，会拿回刚关掉的那个会话 |
| **不持久化** | 应用重启后不恢复上次打开的仓库列表 —— 那需要把用户访问过的路径写进配置文件，与本项目的隐私取向冲突 |
| MCP 长驻进程复用同一张表 | 连续多次调用同一仓库省掉每次的 `rev-parse`；失效判据仍是引用指纹，与桌面端同一套 |

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
| 出网   | 唯一出网点是回环地址上的本地模型服务，默认关闭 |
| 发布   | 三处版本号一致；产物为 draft，人工确认后发布 |

`[已实现]` **首屏读取路径的子进程契约**（`ADR/002`）：

- `snapshot()` 恰好 **2** 次 `git` 子进程（`status` + 一次 `for-each-ref`）
- `snapshot() + commits()` 恰好 **4** 次
- `workspace()` 恰好 **2** 次（`worktree list` + `stash list`）
- 门禁：`tests/perf.rs::spawn_counts_are_pinned`，逐方法钉住次数；
  诊断基准 `tests/scale.rs`（`#[ignore]`）

> P-09 给 `snapshot()` 加了「进行中的操作状态」，**次数仍是 2、门禁未动** ——
> 状态是文件探测，不起进程。这也是它被放进快照的依据：零成本的字段可以进来，
> 要起进程的 worktree / stash 则另走 `workspace()`。

`[已实现]` 分支名与 HEAD 取自 `git status --porcelain=v2 --branch` 的
`# branch.head` / `# branch.oid` 头部行，**不另起** `symbolic-ref` / `rev-parse HEAD`。
`(detached)` / `(initial)` 占位值与头部行缺失时的降级由 `parse_head_meta` 的纯函数单测钉住。

`[已实现]` **引用映射的跨调用契约**（`TASK-018`）：

- 缓存**只在 `Git::open_cached` 下开启**。`Git::open` 的行为与逐次成本**逐字不变** ——
  上面那条「与状态无关」的门禁因此得以保持，不必区分冷/热两个数字。
- 开启缓存后，同一会话内 `snapshot() + commits()` 是 **3** 次子进程
  （`commits()` 复用 `snapshot()` 已读到的引用映射）；桌面端打开仓库的四条命令
  由 **13 → 8** 次。
- **失效判据是内容指纹，不是文件时间戳**：每次读取引用前先哈希 git 用来解析引用的
  那几份数据（`HEAD` + `packed-refs` + `refs/{heads,tags,remotes}/**` 的
  **相对路径与内容**），逐位相同才复用。文件时间戳判不出来：多数文件系统粒度到秒，
  且 `git update-ref` 改写已存在的松引用时连文件大小都不变。
- **任何时候都不允许静默过期**：读失败、或引用数据超出预算，一律**放弃缓存、每次重读**。
  退化的方向永远是「多起一个子进程」，不是「用一份可能过期的数据」。
- 工作区状态（`status`）**从不**进入缓存。
- 门禁：`tests/cache.rs`（12 项，其中 7 项断言「引用一变必须失效」并同时验
  「结果正确」与「确实重读了」）；失败路径由 6 个探针反证。

**为什么契约是「子进程次数」而不是毫秒**：实测表明耗时几乎完全由子进程数决定 ——
4 万提交仓库上的 `for-each-ref` 与**完全不读仓库**的 `git --version` 耗时量级相同；
提交数 1 万 → 4 万，`snapshot()` 耗时无系统性变化。
而毫秒阈值会随 runner 抖动（实测同机连跑三次 `snapshot()` 得 208/470/576ms），
足以让门禁随机变红。**子进程次数是确定性的，毫秒不是。**

### 出网（本地模型）

`[已实现]` `repo-prism-core` 是唯一出网的 crate，且出网范围被硬编码收窄：

- 只有 `summarizer::OllamaConfig` 一个入口，`new()` 即校验；构造成功 == 配置合法
- scheme 只允许 `http`，host 只允许 `localhost` / `127.0.0.1` / `[::1]`
- **请求不遵循环境代理**：agent 由 `summarizer::local_agent()` 显式构造，
  刻意不使用 `AgentBuilder::proxy_from_env()`。`HTTP_PROXY` 之类会改写连接目标，
  一旦被遵循，上面那条 host 校验就形同虚设
- 依赖侧同步收窄：`ureq` 关闭默认特性（不带 TLS），避免为一个用不上的
  `https://localhost` 引入整棵 `rustls` / `ring` / `webpki` 依赖树
- **只读扫描器不覆盖出网**：它是字面量级 Git 动词白名单，看不见 HTTP 调用。
  这条边界靠四组测试守住：
  `OllamaConfig::validate` 的单测（`rejects_hosts_that_only_look_local`
  用 8 条输入钉住前缀判定会放行的写法）、P-07 的 `summarizer_http`
  （回环 stub 真实往返 + 读请求体断言只送了相对路径）、
  P-08 的 `summarizer_egress`（代理不得改道回环请求）与
  `summarizer_contract`（响应字段名与厂商文档一致）。
  这是本规格里**唯一靠测试而非静态扫描兜底**的安全约束 ——
  **因此这些测试必须会失败**：P-07 用 5 个探针、P-08 用 4 个探针逐个破坏被测逻辑，
  逐条验证过它们真的会红。

### 只读安全

`[已实现]` CI 静态扫描 `crates/repo-prism-core/src`，实现为 `scripts/read-only-guard.sh`：

- 四层校验：Git 调用门槛 + 动词白名单 + 危险选项黑名单（含 `--textconv` / `--filters`）
  + 全局写动词黑名单
- 先自检扫描器本身（正例不误报、反例能检出），再扫描真实代码
- 扫描器**自身执行失败**（如 awk 报错）必须以非零码退出，不得被当作「通过」
- 详见 SECURITY.md

### 发布

`[已实现]`

- 版本号在三处声明（`package.json` / workspace `Cargo.toml` / `src-tauri/tauri.conf.json`），
  由 `pnpm release:dry` 绑定为一条断言，CI 每次运行都校验
- `.github/workflows/release.yml` 由 `v*.*.*` tag 触发，构建三平台桌面安装包与四目标
  CLI 二进制，**全部以 draft release 产出**
- `[待实现]` 代码签名与 macOS 公证（缺证书，属独立任务）

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
| 风险标记   | `git status` + `diff --numstat`（纯本地） | P1 | 已实现 |
| 外部跳转   | `remote get-url` + URL 拼接  | P1     | 已实现     |
| Diff       | `git show` / `git diff`     | P0     | 已实现     |
| 分支列表   | `for-each-ref`              | P0     | 已实现     |
| 提交详情   | `git show --stat`           | P0     | 已实现     |
| CLI JSON   | core 直接输出               | P0     | 已实现     |
| Skill      | CLI 包装                    | P1     | 已实现     |
| MCP Server | core 直接输出（5 个工具）     | P1     | 已实现     |
| LLM 摘要   | 本地模型（Ollama，回环地址）  | P1     | 已实现（默认关闭） |
| 虚拟滚动   | 前端渲染层，无新增数据源      | P0     | 已实现     |
| 图片对比与预览 | `cat-file blob` + 魔法字节识别       | P0     | 已实现     |
| 字节预览（十六进制转储） | `cat-file blob`            | P0     | 已实现     |
| 音视频播放 | `<video>` / `<audio>`                    | P1     | 待实现     |
| worktree / stash 列表 | `worktree list --porcelain` / `stash list` | P0 | 已实现 |
| 进行中操作状态 | `<git-dir>` 下的标志文件（零子进程） | P0 | 已实现 |
| 文件热度   | `git log --numstat`         | P2     | 待实现     |
| PR/MR      | `gh` CLI / API              | P2     | 待实现     |
| 多仓库     | 本地配置                    | P2     | 待实现     |

## 数据模型

以 `crates/repo-prism-core/src/model.rs` 为唯一事实来源，全部字段以 `snake_case` 序列化，
前端 `src/lib/api.ts` 与之逐字段对齐。

| 结构 | 用途 |
|------|------|
| `RepoSnapshot` | 仓库快照：`path` / `head` / `branches` / `tags` / `status` / `state` |
| `RepoState` | 进行中的操作：`merge` / `rebase{step,total}` / `cherry_pick` / `revert` / `bisect`；无操作时 `state` 为 `null` |
| `WorkspaceInfo` | `worktrees: WorktreeInfo[]` + `stashes: StashInfo[]`（`Git::workspace()` 产出） |
| `WorktreeInfo` | `path` / `branch`（分离时 `null`）/ `commit` / `bare` / `detached` / `locked` / `is_main` |
| `StashInfo` | `reference`（`stash@{n}`）/ `commit` / `message` |
| `BlobPreview` | blob 预览：`size` / `kind` / `content`（base64）/ `text` / `hex` / `truncated` / `too_large` |
| `BlobKind` | `image` / `text` / `binary` / `lfs_pointer` / `too_large`（`size` 永远给出真实值） |
| `ImageFormat` | `png` / `jpeg` / `gif` / `webp` / `bmp` / `svg`，由魔法字节判定，不靠扩展名 |
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
| `Risk` | `rule_id`（稳定标识）+ `level`（info/warn/critical）+ `message` + `path` |
| `RiskCounts` | `info` / `warn` / `critical` 三个计数 |
| `ChangeAnalysis` | `summary` + `total_files` + `risks: Risk[]` + `by_level: RiskCounts` |
| `RemoteInfo` | `host` / `owner` / `repo` / `url`（`origin` 解析结果） |
| `OllamaConfig` | `endpoint` / `model` / `timeout_secs`（本地模型连接配置，**不属仓库 JSON 契约**） |

**不属于对外契约的内部类型**：`LineStat` / `LineStats`（工作区行数统计，只喂规则引擎，
不进任何 JSON 输出）、`ChangeFacts` / `Rule`（规则引擎内部结构）。

**仅存在于 Tauri 层、不进 core 的类型**：`AiSettings`（设置面板的
`enabled` / `endpoint` / `model`，落盘于用户配置目录）。core 只知道 `OllamaConfig`。

**尚未落地**：没有「先声明后实现」的空壳字段。`RepoSnapshot.state` 与
`WorkspaceInfo` 的每个字段都在当前版本由真实数据填充（P-09）。
`RepoSnapshot` **仍不含** `worktrees` / `stashes` —— 那两项要起进程，刻意留在
`Git::workspace()` 里，以保住快照的子进程契约（见「首屏读取路径的子进程契约」）。

## 边界与不做的事

- ❌ 不做任何写操作（commit / push / merge / rebase / checkout / stage）
- ❌ 不做完整 Git 客户端（不替代 GitKraken / Fork / Tower）
- ❌ 不做代码编辑器（集成 GitHub.dev 而非自研编辑器）
- ❌ 不做 CI/CD 平台
- ❌ 不做代码托管
- ❌ **不把代码或仓库内容发往任何云端服务**；唯一的出网目标是回环地址上的本地模型
