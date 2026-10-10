# P-10: blob 只读预览（US-3 剩余：图片对比 / 字节预览）

**状态**：已完成（2026-10-10）
**卡号说明**：同 [P-09](P-09-worktree-stash-state.md) —— 「图片对比 / 媒体预览 / 字节预览」
写在 US-3 里但**从未被分配卡号**（019 = 多仓库、020 = PR 视图），属原需求没做完的部分，
故编入 patch 序列，不占产品血统号。

## 目标

US-3 里最后一条 `[待实现]`。把「看到了哪些文件变更」推进到「看到它们是什么」：

1. **图片对比与预览**：PNG / JPEG / GIF / WebP / BMP / SVG。新增 / 删除看单张，修改看**旧 vs 新并排**
2. **字节预览**：非图片的二进制以**十六进制转储**呈现（前 512 字节 + 真实大小）
3. **按版本查看任意文件的文本内容**（不只看 diff）
4. **LFS 指针识别**：认出指针文本，只展示 object id 与 size，**不下载**

**明确不做**：音视频播放。理由见下方「关键决策 5」。

## 上下文

- 这是 SPEC 优先级表里**最后一条 P0 缺口**（做完之后 v0.3 剩下的都是 P2）。
- 项目此前**从不读取 blob 内容** —— `FileStat.binary` 只标记、不读（SECURITY 威胁 2）。
  所以本卡要新开一条**读取边界**，而不是复用既有的 diff 路径。
- 「审计口径一致」的好处：这条边界一旦立起来，`US-3` 的三件事共享同一份安全论证。

## 关键决策

### 1. 先看大小，再决定读不读

这是本卡与既有 diff 路径**最不相同**的一点：diff 是先读后截断，blob 必须先量再读。

理由很实际：`-s` 对 5 MiB blob 实测 30ms 内返回，而**先读内容**会把一个 2 GiB 的 blob
整个读进内存才轮到「要不要截断」这类判断。所以：

| 调用 | 子进程 | 行为 |
|------|--------|------|
| `blob_size()` | **1** | 只 `cat-file -s`，永远不读内容 |
| `blob_preview()` | **2** | `-s` 后再 `blob`；超过上限就**不读内容**，只报真实大小 |

代价是用户一次点击要起两个进程 —— 可接受，因为这是**异步且用户主动**的操作，
不在首屏路径上。

### 2. 用 `cat-file`，且**不写** `--textconv` / `--filters`

`cat-file` 是 plumbing。2026-10-10 实测了三条安全前提（这些不是推理）：

| 前提 | 实测 |
|------|------|
| 不触发 smudge filter | ✅ 配置 `filter.pf.smudge` 后，**工作区是 SMUDGED、`cat-file blob` 读到原始 REAL** |
| 不触发 hook | ✅ 放了可执行 `post-index-change` hook，`cat-file` 没触发它 |
| 不过滤 / 不联网 | ✅ plumb 层不出来 LFS 下载 |

而 `--textconv` / `--filters` 这两个开关在 `cat-file` 里**真实存在**（`git cat-file --help`
的用法第三行就是它们），会让 cat-file 反过来执行仓库自定义转换器。
**它们已在 `read-only-guard.sh` 的危险选项黑名单里** —— 写上去扫描器直接拦下。
这是既有门禁第一次对新功能提供正向保护，不需要给它新增规则。

### 3. `-s` 的成功不代表那是文件

实测：`cat-file -s HEAD:dir`（一个目录）返回 **29** 且 exit 0；只有 `cat-file blob HEAD:dir`
才失败（exit 128，`bad file`）。

所以那个数字**只能用于判断「值不值得读」**，不能当作「这是一个文件」的证明。
第二次调用失败时必须**显式报错**，绝不能把 29 显示成文件大小。

### 4. 输入界面不留「以 `-` 开头」的路径

`<rev>` 若以 `-` 开头会被 git 当成选项：实测 `-weird:f.txt` → `unknown switch 'w'`。
两条防御都要有：

- 命令一律写 `--`（实测 `cat-file -s -- HEAD:f.txt` 正常）
- 调用方再校验 `rev` / `path` 非空且不以 `-` 开头（纯函数，可断言）

另外：命令经 `Command::args` 传递、**不经 shell**，所以 `;` / `$()` 之类字符是惰性的 ——
实测注入串 `HEAD:a.pc;touch evil` 与 `$(touch evil):a.pc` **没有产生任何副作用文件**。

### 5. 明确不做媒体播放（`<video>` / `<audio>`）

`<video>` / `<audio>` 要求把**整段** blob 交给浏览器解码，这与决策 1「先看大小再决定读不读」
直接冲突：媒体文件几乎必然超过 4 MiB。支持它就得给媒体单独放宽上限，
那等于把威胁 6「资源耗尽」的口子重新打开。这条因此保持 `[待实现]`，理由已写进 SPEC。

### 6. 渲染安全：SVG 只能 `<img>`

inline SVG 会执行其中的脚本与事件处理器。因此 SVG 走 `data:image/svg+xml;base64,...`
塞进 `<img>`（浏览器在 `<img>` 上下文里禁止脚本与外部引用），**不进 innerHTML**。
沿用威胁 3 的既有规则。

### 7. 不在 diff 里自动预览

只有用户点击某个文件才读取内容。自动预览会让「打开一个含大图的提交」的成本
变得不可预测 —— 与 `snapshot()` 保持 2 次子进程是同一类取舍：**不要让成本随内容变化**。

## 实际改动

| 文件 | 改动 |
|------|------|
| `Cargo.toml` / `crates/repo-prism-core/Cargo.toml` | 新增 `base64 = "0.22"`。**复用依赖树里已在编译的 0.22.1**（ureq / wry 等传递依赖带来），lock 里的 base64 版本数仍是 3，没有引入第四个 |
| `crates/repo-prism-core/src/model.rs` | 新增 `BlobPreview` / `BlobKind` / `ImageFormat`；`ImageFormat::media_type()` 给 `<img>` 用 |
| `crates/repo-prism-core/src/git.rs` | 新增 `blob_size()`（1 次子进程）、`blob_preview()`（2 次，超限则 1 次）、`run_bytes()`（第二个子进程出口）；纯函数 `blob_spec` / `classify_blob` / `classify_image` / `looks_like_svg` / `parse_lfs_pointer` / `bytes_to_text` / `cap_text_preview` / `cap_hex_bytes` / `hex_dump` / `encode_base64` |
| `crates/repo-prism-core/tests/preview.rs` | 新增 13 项：真的 `git add` 二进制文件再读回来断言 |
| `crates/repo-prism-core/tests/common/mod.rs` | 夹具加 `write_bytes`（`write` 走 `&str`，写不了非法 UTF-8 的字节） |
| `crates/repo-prism-core/tests/perf.rs` | 钉住 `blob_size()` = 1、`blob_preview()` = 2 |
| `src-tauri/src/lib.rs` | 新增 `get_blob_preview` 命令（复用 TASK-018 的会话） |
| `src/lib/api.ts` | 对齐类型 + `getBlobPreview` |
| `src/lib/preview.ts` + `preview.test.ts` | MIME / data URL / 文案 / 「取哪几个版本」的纯函数 + 16 条断言 |
| `src/components/BlobPreviewPanel.tsx` | 预览面板：图片旧新并排、文本、十六进制转储、LFS 指针卡片 |
| `src/components/CommitDetail.tsx` | 文件列表可点开（**只有点开才读内容**） |
| `src/App.css` | 预览面板与可点击文件路径的样式 |
| `SECURITY.md` | 威胁 2、威胁 6 补上预览路径的三道闸与「`-s` 成功不代表是文件」 |
| `SPEC.md` | US-3 的三条 `[待实现]` → `[已实现]`（媒体播放仍 `[待实现]`），新增「blob 只读预览契约」整节 |
| `CHANGELOG.md` / `README.md` / `ROADMAP.md` | 新能力、新 Fixed（`-s` 成功不代表是文件）、已知边界（音视频未做 / 按魔法字节判类型） |

## 验收标准

- [x] PNG / JPEG / GIF / WebP / BMP / SVG 各自被识别为 `image`，且带有正确的 `format`
- [x] 识别依据是**魔法字节**而非扩展名：一个内容是 PNG、名字是 `.txt` 的文件必须判为图片；
      反之内容为文本、名字是 `.png` 的必须判为文本
- [x] WebP 不会误判：`RIFF` 开头但第 8–12 字节不是 `WEBP` 时**不能**算图片（那可能是 AVI / WAV）
- [x] LFS 指针被判为 `lfs_pointer`，展示其中的 oid 与 size，**且没有发出任何网络请求**
- [x] 超过 4 MiB 的 blob：`too_large = true`，**不读取内容**，但仍给出真实大小
- [x] 文本文件：`text` 有值且超过 256 KiB 时 `truncated = true`（落在字符边界上）
- [x] 未知二进制：`hex` 有值，格式为「偏移 + 十六进制 + ASCII 列」，最多 512 字节
- [x] 目录 / 不存在的路径：显式失败，**不显示** `-s` 返回的那个数字
- [x] `rev` / `path` 为空或以 `-` 开头时被拒绝
- [x] 修改过的图片能同时取到**旧版本与新版本**两个 blob
- [x] `blob_size()` 恰好 1 次子进程、`blob_preview()` 恰好 2 次（perf 门禁）
- [x] 超限时不读内容这一条**真的被断言**（不能只看返回值）
- [x] 只读扫描器通过

## 探针与实测

### 假设探针（写实现之前跑）

探针脚本 `/private/tmp/probe_p10.sh` 与后面的第二轮定向探针。完整结论：

| 假设 | 实测结果 |
|------|----------|
| `cat-file -s` 给出精确字节数 | ✅ `hello world\n` → 12 |
| `cat-file blob` 原样输出、不加换行 | ✅ `abc` → 3 字节 |
| **不触发 smudge filter** | ✅ 工作区 SMUDGED / cat-file REAL |
| 不触发 hook | ✅ 可执行 hook 未被触发 |
| 目录也返回一个 size | ✅ **29**，exit 0（危险，故有决策 3） |
| 目录的第二次调用失败 | ✅ exit 128 `bad file` |
| 缺失路径 | ✅ exit 128 |
| 路径穿越 `HEAD:../../etc/passwd` | ✅ git 自己拒绝：`is outside repository` |
| `-` 开头的 rev 被当选项 | ✅ `unknown switch 'w'`；加 `--` 后按对象名处理 |
| shell 元字符无副作用 | ✅ `;` / `$()` 注入后无副作用文件 |
| LFS 指针形状 | ✅ 130 字节纯文本，首行 `version https://git-lfs.github.com/spec/v1` |
| 符号链接 | ⚠️ `-s` = 4、`blob` = `a.pc`（**链接目标字面量**，不含指向的内容）→ 天然安全，不会穿越 |
| 5 MiB blob 的 `-s` 开销 | ✅ 30ms 内 |
| 魔法字节 | ✅ png `\x89PNG\r\n\x1a\n` / jpeg `\xff\xd8\xff` / gif `GIF8` / webp `RIFF????WEBP` / bmp `BM` |
| 二进制文件在 diff 里的样子 | ✅ `--numstat` 给出 `-  -  big.png` |

> **探针自身也出过 bug**：第一轮的「cat-file 是否走 textconv」这一节，
> 因为夹具在 `checkout`（会把 smudge 后的内容写回工作区）之后又 `git add -A`，
> 把**过滤后的内容提交进了仓库**，于是结论看起来像「cat-file 也走 textconv」。
> 第二轮换了一个**只配 diff驱动、不配 smudge filter** 的干净夹具才把结论拿正。
> 教训：**夹具自身的写操作顺序会污染被测命题**，探针结论要能指出自己是干净的。

### 失败路径探针（写完测试之后跑，反证测试真的会红）

Rust 侧 10 条（脚本 `/private/tmp/probe_p10_failures.sh`）、前端侧 6 条：

| # | 破坏方式 | 被哪条用例抓住 |
|---|----------|----------------|
| R1 | LFS 指针判定整块关掉 | `an_lfs_pointer_shows_the_oid_without_downloading_the_content` |
| R2 | WebP 判据丢掉第 8–12 字节那一半 | `riff_container_without_webp_is_not_an_image` |
| R3 | 图片识别整块关掉 | `image_types_are_recognized_by_magic_bytes` |
| R4 | 指针缺 oid / size 也放行 | `a_pointer_missing_oid_or_size_is_not_a_pointer` |
| R5 | 不再拒绝以连字符开头的 rev / path | `blob_spec_refuses_inputs_git_would_parse_as_options` |
| R6 | 十六进制转储不再限长 | `a_long_hex_dump_says_it_was_cut` |
| R7 | 文本截断不再回退到字符边界 | `text_preview_truncation_lands_on_a_char_boundary` |
| R8 | ASCII 列一律显示成点 | `hex_dump_prints_offset_hex_and_ascii_columns` |
| R9 | 目录不再报错、按空内容返回 | `a_directory_refuses_to_be_shown_as_a_file` |
| R10 | **超过上限也照读不误** | `an_oversized_blob_is_measured_but_never_read` |
| F1 | SVG 的 MIME 少 `+xml` | `mediaType` 两条 |
| F2 | data URL 不再只给图片 | 「非图片即使带着内容也不给 URL」 |
| F3 | 截断提示盖过「太大」 | `previewNotice` |
| F4 | 重命名时两侧都用新路径 | `previewSides` 重命名 |
| F5 | 根提交时拿 `undefined` 当父提交 | `previewSides` 根提交 |
| F6 | 标题里不再带格式名 | `kindLabel` |

**R10 是本卡最要紧的一条**：它证明「先看大小再决定读不读」真的被门禁看着 ——
把早退删掉，`blob_preview()` 的子进程数就从 1 变成 2，断言立刻红。

**F2 抓到了一个真实缺口**：它第一次跑时是**绿的**。原因是我的「非图片返回 null」用例
里那些 preview 的 `content` 本来就是 `null`，于是「内容为空」这条早退先挡住了，
种类判据**根本没被执行到**。补了一条「非图片**即使带着内容**也不给 URL」才补上。
——「一条用例与它声称覆盖的判据之间有缝隙」这件事，又一次只有真去破坏实现才发现。

### 端到端实测

`tests/preview.rs` 的夹具**真的 `git add` 一个二进制文件**再读回来断言，
而不是在内存里构造一个 `Vec<u8>` 就完事 —— 后者证明不了
「一个真实文件经过 git 往返之后还是那些字节」。13 项全绿。

子进程次数（`Git::spawns()` 数真实次数）：

| 调用 | 次数 | 说明 |
|------|------|------|
| `blob_size()` | **1** | 只 `cat-file -s` |
| `blob_preview()`（普通文件） | **2** | `-s` + `blob` |
| `blob_preview()`（超过 4 MiB） | **1** | **第二次调用压根没发生** —— 这就是「没读内容」的证明 |

## 未覆盖的部分

| 项 | 原因 |
|----|------|
| 音视频播放 | 见「关键决策 5」，需与「先看大小再读」的最小化原则重新调和，SPEC 里仍为 `[待实现]` |
| CLI / MCP 侧暴露 blob 预览 | 本卡只接线桌面端。要加的话应与 TASK-019（多仓库）一起定形状 |
| 图片的实际**渲染**是否成功 | core 侧只能验字节往返一致 + 夹具是合法 PNG（有一条专门的断言）；浏览器真把它画出来仍需人工 `pnpm tauri dev` 看一眼 |
| 符号链接 | 实测 `cat-file blob` 返回的是**链接目标的字面量**（`a.pc`）而不是指向的内容，因此天然不会穿越路径；UI 未做特殊区分 |

## 与归档规划的偏离

归档 `RepoPrism-仓库棱镜-001` §US-3 只写了一行「图片对比、媒体预览、字节预览」，
§威胁 2 写了「只读取标准 LFS 指针文件，展示 object ID 和 size，绝不调用 git lfs」。

本卡据此落地，但有两处收敛（都写进了 SPEC）：

1. **媒体播放不做** —— 归档没说清「媒体」指什么，本卡把它定为 `<video>` / `<audio>`，
   并按上述理由留 `[待实现]`。理由本身是新写的，不是归档给的。
2. **先看大小再读内容** —— 归档的应对是「展示并说明限制」，没给具体机制。
   本卡选了「4 MiB 硬上限 + 显式 too_large」，因为它把「说明限制」变成了**可执行断言**。

## 禁止事项

- [ ] 不写出 `--textconv` / `--filters`（写在 `cat-file` 里会让仓库自定义转换器被执行）
- [ ] 不调用 `git lfs`，不发任何网络请求
- [ ] 不在 core 之外解析 `cat-file` 的输出
- [ ] 不用 inline SVG，不用 `dangerouslySetInnerHTML`
- [ ] 不在 `<rev>` / `<path>` 里接受以 `-` 开头的值
- [ ] 不在 `cat-file -s` 成功时就把返回值当成文件大小（目录会返回一个数）
- [x] 不自动预览 —— 只在用户点击后读取
- [ ] 不用扩展名判定类型（只作为 UI 上的辅助信息）
