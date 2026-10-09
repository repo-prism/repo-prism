# P-07: 本地模型 HTTP 路径从未被执行过 —— 用回环 stub 把它变实

**状态**：已完成（2026-10-09）
**卡号说明**：本卡不在 001–020 的产品血统内，不新增产品能力，属工程补丁，
故编入 patch 序列（见 [README](README.md)）。

## 目标

让 `summarizer.rs` 里**真正把字节送出进程的那两处 `ureq` 调用**第一次被真实执行，
并把「送出去的 body 里有什么」钉成断言。

## 上下文：一条从未运行过的防线

`SPEC.md` 与 `SECURITY.md` 都把「不把代码发到外部」这条约束的**唯一防线押在测试上**：

> **必须知道的边界**：CI 的只读扫描器**看不见 HTTP 调用**（它是 Git 动词白名单）。
> 本条约束**只由单元测试兜底**，不是静态扫描兜底 —— 改动 `summarizer.rs`
> 的校验逻辑时，`cargo test -p repo-prism-core --lib summarizer` 是唯一的防线。

而实际上，`summarizer.rs` 的 13 个单测**全部是纯函数**：

| 单测覆盖的 | 单测**没有**覆盖的 |
|-----------|------------------|
| `validate_endpoint` 的 8 类输入 | `list_models()` 的 GET 往返 |
| `OllamaConfig::validate` 的边界 | `generate()` 的 POST 往返与 JSON 取值 |
| `build_changes_prompt` / `build_commit_prompt` 的拼装 | `base()` 的路径拼接 |
| 空 model、越界 timeout | 失败降级（连不上 / 5xx / 非法 JSON） |

`ureq` 那两行**一次都没被执行过** —— 本机没有 Ollama（见 `ROADMAP.md` 的
「本地 AI 端到端验证」待办）。于是下面这些错**不会有任何一个测试变红**：

- `base()` 忘了裁尾斜杠 → 请求打到 `//api/tags` 这个不存在的路由
- 忘了 `"stream": false` → Ollama 返回多行 NDJSON，`into_json()` 直接失败
- 取值字段名从 `response` 打成 `message`（Ollama 的字段就是叫 `response`）
- `read_to_string` 换成 `into_json` 时把 `Content-Length` 算错

全部症状都是「摘要永远返回 `None`」——而 `None` 是被**设计成静默降级**的，
界面上只会少一个按钮的结果，没有任何报错。**这是最容易长期潜伏的一类坏法。**

## 为什么不是「装个 Ollama 测一遍」

那是必要的，但不是充分的，而且它测不了失败路径：

| | 真实 Ollama | 回环 stub |
|---|---|---|
| 证明我们这侧写对了 | ✅ | ✅ |
| 证明字段名与真实响应一致 | ✅ | ❌（stub 的响应形状是我们写的） |
| 5xx / 非法 JSON / 连不上 | 难构造 | ✅ 一行搞定 |
| CI 里能跑 | ❌（runner 上不会有模型） | ✅ |
| 捕获「出网 body 里混入了什么」 | 要抓包 | ✅ 直接读请求体 |

两者互补。本卡做 stub 这一半，真实调用的端到端验证在 SPEC US-7 里**仍是 `[待实现]`**
—— 本卡不改变这个状态，只是把它的前置条件补齐。

## 实现

新增 `crates/repo-prism-core/tests/summarizer_http.rs`，用 `std::net::TcpListener`
在 **127.0.0.1** 上起一个只服务固定次数请求的 stub（**不引新依赖**），
让请求真的走一遍 TCP → HTTP 解析 → JSON 取值。8 个用例分四类：

1. **往返能通**：`list_models` 解析出模型名并跳过无名条目；`generate` 裁掉首尾空白；
   断言方法、路径、`Content-Type`，并把请求体反序列化后逐字段核对
   （`model` / `stream: false` / `prompt` / `options.temperature`）
2. **失败一律降级**：空白 / null / 缺失的 `response` → `None`；5xx → `generate` 给 `None`
   而 `list_models` 给可读错误；非法 JSON → 报错而非静默当成「没有模型」；
   端口无人监听 → 报错里带上地址
3. **URL 拼接**：首尾空白 + 尾斜杠的 endpoint 不得拼出 `//api/tags`
4. **出网 body 的内容**（威胁 7 第 3 层的实测版）：只含相对路径与规则 id，
   不含规则自带的叙述性 `message`、不含 `CARGO_MANIFEST_DIR`、不含 `/Users/`、不含 `file://`

stub 自身的两个坑（都踩过）：

- **`accept()` 出的连接会继承 `O_NONBLOCK`**（macOS 上实测）：监听者设了非阻塞后，
  读请求会随机 `WouldBlock`，客户端只看到「连接被关掉、没有响应」的 `Unexpected EOF`，
  症状随请求数据到达的先后时红时绿。必须对每条连接显式 `set_nonblocking(false)`。
- **解析失败也要回响应**：否则失败会以客户端侧的 EOF 呈现，把排查引向错误的代码。
  解析失败一律回 400 并带上原因。
- 响应带 `Connection: close`：ureq 默认做连接池，不声明关闭可能复用连接，
  于是「第 2 个请求没到 stub」这类假象会污染断言。

## 验收标准

- [x] `cargo test -p repo-prism-core --test summarizer_http` → 8 passed
- [x] 连跑 5 轮全部 8 passed（证明没有时序性红绿 —— 第一版正是因此被推翻重写）
- [x] 全量：`cargo test --workspace` Rust 侧 **123 passed**（原 115 + 本卡 8）
- [x] `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 干净
- [x] **五个失败路径探针全部把测试打红，且各自被对应用例捕获**：

| 探针 | 破坏 | 变红的用例 |
|------|------|-----------|
| M1 | `base()` 不再裁尾斜杠 | `a_sloppy_but_legal_endpoint_still_produces_clean_paths` |
| M2 | 取值字段 `response` → `message` | `generate_posts_...`、`the_bytes_that_leave_the_machine_...` |
| M3 | 去掉空响应的 `filter` | `a_blank_or_missing_response_degrades_to_none` |
| M4 | 出网 body 里塞进主机绝对路径 | `the_bytes_that_leave_the_machine_carry_relative_paths_only` |
| M5 | 连接失败的报错文案变模糊 | `a_server_error_degrades_...`、`nothing_listening_degrades_...` |

  探针脚本逐个破坏 → 跑测试 → 记录 → 还原；跑完 `git diff --stat` 对
  `summarizer.rs` 为空，确认源码干净。

## 教训

**「测试通过」有两个完全不同的问题**：测试跑了没有？测试能不能失败？
第一版 `summarizer_http.rs` 是绿的，但它绿得毫无意义 —— stub 有个时序 bug，
一半用例只是碰巧在请求数据已到达时才读到。所以：

1. **测试要先证明它会红**。本卡的五个探针不是形式，M1 与 M2 这类「静默降级成 `None`」
   的坏法只会让界面少一块内容，靠肉眼永远看不出来。
2. **脚手架自己也会坏，而且坏得和被测代码难以区分**。第一版失败报的是
   `Network Error: Unexpected EOF` —— 看起来像 `ureq` 用错了。实际是我们的 stub
   在非阻塞连接上读不到就放弃。**测试失败的报错本身是要设计的**：
   所以解析失败要回 400，而不是让客户端去猜 EOF。
3. **「只由测试兜底」这句话要有重量**。写进 SPEC/SECURITY 的安全约束，
   如果测试从未运行过那两行代码，那句话就是空的 —— 本卡之前它已经空了两批。
