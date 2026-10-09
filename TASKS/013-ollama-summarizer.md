# TASK-013: 本地 AI 摘要层（Ollama）

**状态**：已完成（2026-10-09）
**血统**：批次 `005`（原规划 `RepoPrism-仓库棱镜-005`）

## 目标

1. `OllamaSummarizer` 实现 `Summarizer` trait
2. **只允许回环 endpoint**（安全边界）
3. Tauri 设置命令 + 前端设置面板
4. 变更面板新增「AI 摘要」按钮

## 验收标准

- [x] `OllamaSummarizer::new` 拒绝非回环 endpoint
- [x] 前端设置面板可配置 endpoint / model / 启用开关
- [x] 未启用时「AI 摘要」按钮置灰
- [ ] 启用且 Ollama 在线时，点击后展示摘要 —— **本机无 Ollama，未做端到端验证**（见下）

## 实现

| 文件 | 内容 |
|------|------|
| `Cargo.toml`（workspace） | 新增 `ureq = { version = "2", default-features = false, features = ["json"] }` |
| `crates/repo-prism-core/Cargo.toml` | `ureq.workspace = true` |
| `crates/repo-prism-core/src/summarizer.rs`（新增） | `OllamaConfig` / `validate_endpoint` / `OllamaSummarizer` / `build_changes_prompt` / `build_commit_prompt`；13 个单测 |
| `crates/repo-prism-core/src/lib.rs` | 追加 `pub mod summarizer` 与 re-export（**保留 `mod diffparse`**） |
| `src-tauri/Cargo.toml` | 新增 `dirs = "5"`（定位配置目录） |
| `src-tauri/src/lib.rs` | `AppState` + `get_ai_settings` / `set_ai_settings` / `test_ai_connection` / `summarize_changes` / `summarize_commit`（**保留 `get_diff` / `get_commit_diff` 等既有七个命令**） |
| `src/lib/api.ts` | `AiSettings` + 五个 invoke 包装 |
| `src/components/AiSettingsPanel.tsx`（新增） | endpoint / model / 启用开关 + 测试连接 + 保存 |

## 安全边界

- endpoint 必须是 `http://` + `localhost` / `127.0.0.1` / `[::1]`
- 默认关闭：`enabled` 为 false 时根本不会构造摘要器
- **只送相对路径与规则结果**：不送仓库绝对路径、不送 remote URL、不送 diff 正文、不送文件内容
- 生成超时 60s；失败一律 `None`，由界面决定怎么提示，不把「模型没开」升级成错误
- `set_ai_settings` **先校验再落盘再入内存** —— 允许存下非法 endpoint 等于允许绕开唯一那道出网闸门

## 与归档规划的三处偏离（均写进本卡，不埋在代码里）

### 1. host 判定改为真解析，不能比前缀（这条是安全缺陷，不是风格）

归档原稿：

```rust
let ok = ep.starts_with("http://localhost") || ep.starts_with("http://127.0.0.1") || ...
```

这会放行两类**把代码送出机器**的写法：

| 输入 | 为什么骗过了前缀判定 |
|------|---------------------|
| `http://localhost.evil.com` | 前缀命中，真实 host 是 `localhost.evil.com` |
| `http://localhost@evil.com` | 前缀命中，URL 语义里 `localhost` 是 userinfo，host 是 `evil.com` |

现改为：拆出 authority → 拒绝含 `@` → 拆出 host 与 port → 精确比对 host。
`rejects_hosts_that_only_look_local` 用 8 条输入把这两类连同
`http://[::1]evil.com`、`http://localhostevil.com` 一起钉住。

### 2. 关掉 TLS，并把 scheme 收窄到 http

归档在 core 引入 `ureq` 默认特性（含 TLS），又在校验里放行 `https://localhost`。
回环地址上的 https 没有实际用途（Ollama 是明文 HTTP），却会带进整棵
`rustls` / `ring` / `webpki` 依赖树。

改为 `default-features = false`，并让校验只接受 `http`：
**放行一个实际跑不通的 scheme，比直接拒绝它更糟。**

### 3. `timeout_secs` 纳入校验；设置落盘持久化

- 归档让 `timeout_secs` 原样进 `Duration`，`0` 会在库里变成「立即超时」这种难定位的失败。
  现限制到 `1..=600`。
- 归档把设置只放在内存里（`Mutex<AiSettings>`），重启即丢 —— 一个每次开机都要重开的
  「启用」开关不算落地。现落盘到 `<系统配置目录>/RepoPrism/ai-settings.json`，
  读取失败一律回默认值（默认关闭）。**这不是对被观察仓库的写操作**，只读宪法不受影响。

### 顺带：「测试连接」只测不存

归档的 `test_ai_connection` 从 `AppState` 读设置，于是前端必须先
`setAiSettings(settings)` 才能测 —— 用户点「测试连接」会被悄悄写盘，点取消也回不去。
现改为入参是面板里当前填的值，测完不留痕。

### 顺带：`analyze_changes` 取行数失败时降级而非失败

原实现 `let stats = g.working_tree_stats()?` 会让行数统计失败（如 stat 缓存异常）
拖垮整个分析。现改为 `unwrap_or_else(|_| LineStats::new())`：
少命中一条 `mass-deletion` 是可接受的降级，整个面板报错不是。

## 未验证项（如实记录）

- **本机没有 Ollama**（`11434` 拒绝连接，`command -v ollama` 为空），
  因此「启用 + 在线时能出摘要」只做了代码路径与单元测试验证，**没有真实调用过模型**。
- 单元测试覆盖的是校验与 prompt 构造，**不覆盖 HTTP**。要真正验证需要
  `ollama serve` + `ollama pull <model>`，或一个返回固定 JSON 的本地 stub。
- 设置面板的界面行为需要 `pnpm tauri dev` 手工确认（见 ROADMAP 的「UI 接线无自动化测试」）。

## 验证记录（本机 macOS，2026-10-09）

- `cargo test -p repo-prism-core --lib summarizer`：13 passed
- `bash scripts/read-only-guard.sh`：passed；`--self-test` 26/26
  （新文件里全是小写词字面量，规则 0 的调用门槛正好挡住这一整类误报）
- `cargo clippy --workspace --all-targets -D warnings`：零 warning
