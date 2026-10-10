# 补丁 P-11：把 blob 只读预览接进 CLI 与 MCP（+ 裁决 `open --view`）

- **日期**：2026-10-10
- **状态**：✅ 已完成
- **关联**：`TASKS/patch/P-10-blob-preview.md`（本卡的边界来源）、`ADR/003`（同批产出）、
  `SPEC.md` US-4 / US-10、US-6

## 目标

P-10 建立了「读也要先量再读」这条边界，但它**只建在桌面端**：
`blob_size()` 与 `blob_preview()` 只有 Tauri 在调，`blob_size()` 更是
**只有测试在调**（全仓检索确认：生产代码零调用方）。
本卡把这条边界铺到 CLI 与 MCP，并顺手补上 P-10 漏掉的一类测试。

同一批还裁决了 `repoprism open --view`（归档 `001.md` 第 175 行写着、
`SPEC.md` 标 `[待实现]`、而 CLI 里连 `Open` 子命令都没有）。

## 上下文：为什么现在做

P-10 收尾后，v0.3 的 P0 一条都不剩。剩下三件事里，只有本卡是**零验证缺口**的：

| 待办 | 验证缺口 |
|------|---------|
| **本卡（P-11）** | 无 —— CLI 与 MCP 走的都是真实子进程 / 真实 stdio |
| TASK-020 PR/MR 视图 | 本机没有 `gh`；且是首次非回环出网 |
| 本地 AI 端到端 | 本机没有 Ollama |

按「先做能验的」排序，本卡排第一。

## 关键决策

1. **`--size-only` / `size_only` 单独暴露**。P-10 的「先量后读」里，
   「量」那一半此前没有生产调用方。把它做成显式开关，而不是藏进实现里 ——
   调用方（尤其是 Agent）需要一个「只问大小、绝不读内容」的入口。
2. **人类可读输出不打印 base64**。图片内容以 base64 提供是给程序用的，
   打到终端只会塞满屏幕。人类输出只说清「它是什么、多大、怎么拿内容」。
3. **`MAX_PREVIEW_BYTES` 对外可见**。CLI 与 MCP 都要在输出里报上限，
   各处硬编码一份 `4 MiB` 就会和 core 悄悄分叉。改为 `pub` 并由 lib 重导出。
4. **不动 `BlobPreview` 的模型**。实测发现线格式有两处别扭：
   `kind` 是嵌套的（`kind.kind`），且 LFS 情形下 `size` 出现两次、
   外层是**指针文件**大小、内层是 **LFS 对象**大小。前端已经按这个形状消费，
   改模型会连带改前端，收益不抵风险。**改为把形状钉进测试 + 写进文档**。
5. **`open --view` 不实现，但把前置条件写进 SPEC**，而不是让它继续看起来像遗漏。
   三条前置条件目前一条都不满足（见下）。

## 实际改动

| 文件 | 改动 |
|------|------|
| `crates/repo-prism-core/src/git.rs` | `MAX_PREVIEW_BYTES` 由私有改 `pub`，补注释说明为什么对外可见 |
| `crates/repo-prism-core/src/lib.rs` | 重导出 `MAX_PREVIEW_BYTES` |
| `crates/repo-prism-cli/src/main.rs` | 新增 `blob` 子命令（`--rev` 默认 HEAD / `--file` / `--size-only` / `--json`）、`BlobSize` 输出结构、`print_blob_human` |
| `crates/repo-prism-cli/tests/blob.rs`（新） | 8 项集成测试 |
| `crates/repo-prism-mcp/src/main.rs` | 新增第 6 个工具 `repoprism_blob` |
| `crates/repo-prism-mcp/tests/protocol.rs` | 5 项新用例；`exactly_five` 改 `exactly_six` |
| `crates/repo-prism-core/tests/preview.rs` | 新增 4 项**线格式**测试（本节最要紧的补漏） |
| `ADR/003-pr-view-egress-boundary.md`（新） | US-10 的出网与凭据边界，状态：提案 |
| `SPEC.md` | US-4 加 `blob`、写清 `open --view` 的三条前置条件；US-6 工具数 5→6；US-10 指向 ADR-003 |
| `README.md` / `CHANGELOG.md` / `ROADMAP.md` / `crates/repo-prism-mcp/README.md` / `docs/RELEASE.md` | 同步 |

## 验收标准

- [x] `repoprism blob . --file <p> --json` 输出与其它三条数据命令**同一个信封**
      （`schema_version` / `tool` / `tool_version` / `data`）
- [x] `--size-only` 只报大小，**一个字节都不读**：结果里 `text` / `content` /
      `hex` / `kind` 四个字段**根本不存在**（不是 null、不是空串）
- [x] 默认 `--rev HEAD`；传了 `--rev HEAD~1` 就真的按那个版本读
- [x] 文件不存在 → 非零退出 + stderr 说明「不存在」，**不降级成空预览**
- [x] 传目录 → 非零退出 + stderr 说明「不是普通文件」
      （这是 `cat-file -s` 对目录返回 29 那条坑的**第二次**调用路径）
- [x] 空路径被自己的校验挡住，而不是交给 git
- [x] 人类输出给出 `kind` 与 MIME，**不含** base64
- [x] LFS 指针：给出 oid 与真实大小，并写明「未下载」
- [x] MCP 第 6 个工具 `repoprism_blob` 与 CLI 同契约；`tools/list` 断言 6 个
- [x] **线格式**：5 个 tag 的 snake_case 形状、字段集合、`content` 可解码 base64、
      JSON 往返无损、两个 `size` 不是一回事
- [x] 探针 **10/10 会红**（逐条破坏被测逻辑）
- [x] 只读扫描器自检 + 扫描通过；fmt / clippy / 全套测试 / 前端门禁全绿

## 探针结果（10/10）

| # | 破坏点 | 对应用例 |
|---|--------|---------|
| R1 | CLI 忽略 `--size-only` | `blob_size_only_reports_the_size_and_reads_nothing` |
| R2 | 人类输出打印 base64 | `blob_human_output_names_the_kind_without_dumping_base64` |
| R3 | CLI 忽略 `--rev` | `blob_defaults_to_head_and_reads_a_historic_revision_on_request` |
| R4 | 目录早退改成功返回 | `blob_on_a_directory_fails_even_though_the_size_query_succeeds` |
| R5 | `blob_spec` 不再拒绝空路径 | `blob_rejects_an_empty_file_path` |
| R6 | MCP 忽略 `size_only` | `blob_tool_size_only_reports_the_size_and_no_content` |
| R7 | MCP 忽略传入的 `file` | `blob_tool_on_a_missing_file_is_an_error_not_an_empty_preview` |
| R8 | `BlobKind` 的 tag 改 PascalCase | `every_kind_carries_a_snake_case_tag_on_the_wire` |
| R9 | `content` 不再是 base64 | `an_image_preview_carries_base64_under_content` |
| R10 | LFS 指针不再被识别 | `a_preview_round_trips_through_json_without_losing_a_field` |

**R8 第一次报「探针无效」**：锚点 `#[serde(tag = "kind", rename_all = "snake_case")]`
在 `model.rs` 里出现**两次**（另一处是 `OperationState`），命中 2 次 ⇒ 探针拒绝执行。
加上下文后 10/10。**这正说明了为什么探针必须校验命中次数** ——
它若默默改错了地方，报出来的会是全绿。

## `open --view` 的裁决

归档 `001.md` 第 175 行写了 `repoprism open . --view changes`，`SPEC.md` 第 158 行
标 `[待实现]`，而 CLI 里连 `Open` 子命令都没有。**裁决：不实现，但写明为什么。**

| 前置条件 | 现状 |
|---------|------|
| 桌面应用必须真的可安装 | v0.2.0 / v0.3.0 两个 Release 仍是 **draft**，从未发布 |
| 应用要能接受「定位到哪个视图」的参数 | 需单实例 + 参数转发，Tauri 不自带；**应用侧工作量比 CLI 侧大** |
| 应用标识必须定下来 | `productName` 仍是 `repoprism-app`（命名由人类主导，未定） |

三条不齐时写出来的 `open` 只有「应用未安装」这一条路径能被验证 ——
成功路径在本机**永远跑不到**。三条满足后它是一个几十行的子命令。

## 未覆盖（明确写在这里）

- **前端 TS 类型仍是对着手写的**。`src/lib/api.ts` 里那份 `BlobKind` 与 serde 之间
  没有编译期联系。本卡只把 **Rust 侧**的线格式钉住（改 `rename_all` 会让 Rust 红），
  改 TS 侧目前不会有任何测试红 —— 除非再加一道跨语言闸门。
- **真机渲染仍未自动化验证**：`content` 是「可解码的 base64」已被断言，
  但它拼成的 data URL 在浏览器里真的显示出来没有，仍需人工看一眼
  （`v0.3.0` 的 Known limitations 已记）。
- **音视频播放**仍 `[待实现]`（与 P-10 同一条理由，未在本卡范围）。

## 与归档的偏离

无 —— 本卡的卡号不在归档 `001`–`006` 里（归档只到 006），
属按 P-01 先例自行编入的 patch 序列。`open --view` 这条是**裁决不改**而非删需求。

## 禁止事项

- 不在 CLI / MCP 里自己拼 `git cat-file`（只读宪法：Git 只许出现在 core）
- 不把 `--textconv` / `--filters` 加进任何路径
- 不下载 LFS 真实内容
- 不在 diff 里自动预览（必须点击才读）
