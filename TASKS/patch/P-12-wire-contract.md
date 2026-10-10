# P-12：跨语言线格式闸门（`BlobPreview`）

- **状态**：✅ 完成（2026-10-10，提交见文末）
- **类型**：工程补丁（补 P-11 明确写下的未覆盖项）
- **对应 US**：US-3（内容查看）的边界硬化，横跨 US-4（CLI）/ US-9 / US-6（MCP）三条出口

## 目标

把「改一个 `rename_all` 会 Rust 测试全绿、UI 静默变空白」这类漂移，变成**提交前必须先处理**的事。

## 上下文（为什么之前会漏）

`BlobPreview` 的 JSON 是三处消费者的共同契约：

| 消费者 | 拿到什么 |
|--------|---------|
| 桌面端 | `src/lib/api.ts` 里手写的 `BlobKind` / `BlobPreview` |
| CLI | `repoprism blob --json` 直接打印 serde 输出 |
| MCP | `repoprism_blob` 工具把同一份 JSON 放进 `content[0].text` |

而这份 JSON 的形状完全由 Rust 的 serde derive 决定。**两侧没有任何编译期联系**。

P-11 给 `BlobPreview` 补上了 Rust 侧的 JSON 线格式测试（tag 形状 / 字段集合 /
base64 可解码 / 往返无损），但那四条测的是「Rust 结构序列化成什么」，拦不住
「前端那份手抄的类型已经跟 Rust 分叉了」。当时在任务卡里把它写成了未覆盖项。

## 关键决策

1. **事实来源是文件，不是某一边**：`contracts/blob-preview.json` 由 Rust 侧生成，
   TypeScript 侧对着它校验。任何一侧改了都得让 contract 文件跟着变，
   而让 contract 文件变必须由人明确执行一条 `--ignored` 的命令 ——
   **CI 不会替你更新**（与 `tests/scale.rs` 的诊断基准同一个约定）。
2. **不引入 `specta` / `ts-rs`**：它们能让 TS 类型真正由 Rust 生成，但要加依赖、
   给每个跨语言模型加 derive、并且在 CI 里 diff 生成结果。当前只有一个模型在受管，
   自制方案够用且零依赖。**升级条件写在本文最后一节**。
3. **清单不是注释，是运行时判据**：`BLOB_PREVIEW_FIELDS` / `BLOB_KIND_PROPS` /
   `IMAGE_FORMATS` 被 `parseBlobPreview` 真拿来校验每一条进来的 JSON。
   写成「注释 + 测试各维护一份」会回到两边分叉的老路。
4. **严格解析器接进运行时而非只在测试里用**：`getBlobPreview` 的必经路径。
   否则清单就只是注释（「注释会过时」正是 P-11 那个纯 Rust 测试遇到的同一问题）。
   契约破时抛 `WireContractError`，`BlobPreviewPanel` 的 `catch` 把它显示为一行提示 ——
   比静默渲染出一块空白更容易排查。
5. ** contract 样本不含真实仓库往返**：它管的是「serde 形状」，
   「真的 `git add` 二进制再读回来」已由 `tests/preview.rs` 覆盖，不重复。
6. **发现并顺手修掉一处真实分叉**：`src/lib/preview.ts` 里硬编码了「4 MiB」，
   与 Rust 的 `MAX_PREVIEW_BYTES` 是两个独立的数。现在上限值也在 contract 里受管，
   界面文案由它派生。

## 实际改动

| 文件 | 改动 |
|------|------|
| `contracts/blob-preview.json` | **新增**。字段清单 / 各 kind 的属性清单 / 图片格式清单 / `max_preview_bytes` / 7 个样本 |
| `crates/repo-prism-core/tests/wire_contract.rs` | **新增**。生成 + 比对 contract；`#[ignore]` 的重生成用例 |
| `src/lib/blobPreview.ts` | **新增**。类型 + 清单常量 + 严格解析器 `parseBlobPreview` |
| `src/lib/blobPreview.test.ts` | **新增**，16 项。对着 contract 比对 + 解析器自身的拒绝用例 |
| `src/lib/api.ts` | `BlobPreview` 等类型改为从 `blobPreview.ts` re-export；`getBlobPreview` 走解析器 |
| `src/lib/preview.ts` | 「4 MiB」改由 `formatByteLimit(MAX_PREVIEW_BYTES)` 派生 |
| `CHANGELOG.md` / `ROADMAP.md` / `README.md` | 回填 |

## 探测结果（探针必须「会红」才有意义）

**12 条探针，12 条会红**。R 类走两环：contract 文件未同步时 Rust 必须先红，
重生成之后 TS 侧必须也红 —— 只红一环说明链条断了。

| # | 破坏方式 | 结果 |
|---|---------|------|
| R1 | `BlobKind` 的 tag 改 PascalCase | ✓ Rust红 → 重生成 → TS红 |
| R2 | `BlobPreview` 加一个字段 | ✓ 同左 |
| R3 | 改一个字段的 serde 名字 | ✓ 同左 |
| R4 | `ImageFormat` 改一个变体的线上名字 | ✓ 同左 |
| R5 | `MAX_PREVIEW_BYTES` 改值 | ✓ 同左 |
| F1 | 顶层字段清单少一个 | ✓ TS 红 |
| F2 | kind tag 集合少一个 | ✓ TS 红 |
| F3 | `lfs_pointer` 的 props 少一个 | ✓ TS 红 |
| F4 | TS 的 `MAX_PREVIEW_BYTES` 改值 | ✓ TS 红 |
| F5 | 图片格式清单少一个 | ✓ TS 红 |
| F6 | 解析器不再拒绝未知字段 | ✓ TS 红 |
| F7 | 「预览上限」文案改回硬编码 | ✓ TS 红 |

### 探针自己出的三次错（都记进工作了 MEMORY）

1. **R5 锚点命中 2 次**：`git.rs` 里还有一个 `4 * 1024 * 1024`
   （`REF_FINGERPRINT_BUDGET`，TASK-018 的引用指纹预算），用途完全不同。
   锚点必须带变量名。这是**第三次**踩到同类问题，而且每次都是往「报全绿」的方向失败。
2. **R2 一开始跑不到第二环**：给结构体加字段会让 `git.rs` 与 contract 生成器里的
   两处字面量构造编译失败。这不是闸门有缝，是探针没模拟「人会一起改构造点」。
   补齐三处（model.rs / git.rs / wire_contract.rs）后才成立。
   **顺带证明了加字段在 Rust 侧是编译期强制的**，比前端那边还早一步。
3. **F7 第一次「不红」，揭示的是断言真的有缝**：我原本断言
   `expect(notice).toContain(formatByteLimit(contract.max_preview_bytes))`，
   而硬编码的「4 MiB」在当前上限恰好等于 4 MiB 时**也会通过** ——
   这条断言在值相等的前提下永远不会失败。
   改成用 `vi.doMock` 把上限换成另一个值，测**派生关系**而非值相等。

## 未覆盖（明确写下）

- **只管 `BlobPreview` 一个模型**。`RepoSnapshot` / `WorkspaceInfo` / `ChangeAnalysis`
  等同样有 TS 手写类型，同样会漂移。本次先把机制建立起来，推广到更多模型属于下一步。
- **TS 侧的字段名与 Rust 的 serde 名字不必相同也能通过**（只要字段集合一致）。
  要拦住「改名但两边都没同步」需要更进一步的定义。
- **抓不到类型层面的漂移**：如 `size` 由 `u64` 变 `String`。contract 只标形状不标类型。
- **`BlobPreview` 之外，UI 运行时拿到的其他命令返回值仍不做校验**
  （`inspectRepo` / `getCommits` 等），那些依然是「 trust 后端」。

## 与既有文档的关系

- `AGENTS.md` 的「Git 调用只在 core」与本次无关：这里没有新增 Git 调用。
- `scripts/read-only-guard.sh` 不受影响（第 0 层门槛只管 Git 动词）。
- 上一轮新增的 `scripts/check-invoke-names.mjs` 管「命令名」，本闸门管「命令返回值的形状」——
  两者相邻但不同层。

## 什么时候升级到 `specta` / `ts-rs`

满足任一就该换：

1. 受管的跨语言模型超过 3 个，手工维护 contract 文件开始重复
2. 出现过一次「形状对了但类型错了」的事故（凹陷清单的合同 ask）
3. 有第二个前端消费者（比如要在别处再写一份类型）

## 禁止事项

- 不要手改 `contracts/blob-preview.json` —— 它由 Rust 侧生成，手改会被下一条测试撞回来。
- 不要把重生成用例从 `#[ignore]` 里放出来 —— CI 替你更新 contract 就等于没有契约。
- 不要让 `src/lib/api.ts` 再单独定义一份 `BlobKind` —— 它是 re-export，再定义一份就会得到两套各自漂移的类型。
