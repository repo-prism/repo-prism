# TASK-012: 发布硬化

**状态**：已完成（2026-10-09）
**血统**：批次 `004`（原规划 `RepoPrism-仓库棱镜-004`）

## 目标

1. CI 跨平台矩阵构建
2. 版本 bump 脚本
3. 发布 workflow
4. 首次发布 v0.1.0

## 验收标准

- [x] `pnpm release:dry` 输出待发布版本
- [x] GitHub Actions 在 tag push 时构建多平台安装包
- [x] Release 自动生成（**draft**，需人工点发布）

## 实现

| 文件 | 内容 |
|------|------|
| `.github/workflows/release.yml`（新增） | verify → desktop（三平台）→ cli（四目标）三阶段 |
| `scripts/check-versions.mjs`（新增） | 三处版本号一致性校验 |
| `package.json` | 新增 `release:dry` / `release:verify` 两个脚本 |
| `CHANGELOG.md`（新增） | Keep a Changelog 格式，登记 0.1.0 |
| `docs/RELEASE.md`（新增） | 发布流程、产物清单、人工验收步骤、已知缺口 |
| `.github/workflows/ci.yml` | **只加** `Swatinem/rust-cache@v2` 与 `pnpm release:dry` 两步，其余不动 |
| `.gitignore` | 修正一条过期注释（引用了并不存在的 `packageManager` 字段） |

## 关键点一：ci.yml 是合并而不是覆盖

原规划里 `ci.yml` 写的是「覆盖」，但仓库现状**已经比归档强**：

| 归档版 | 仓库现状 |
|--------|---------|
| 无 guard 自检 | 有（`Self-test the guard`） |
| 无性能门禁 | 有（`perf` job 断言而非打印） |
| 无 `pnpm build` | 有 |
| 无跨平台缓存 | 本次补 `rust-cache` |

照抄归档会**拆掉已有门禁**。因此只做加法。

## 关键点二：版本校验脚本不是 `echo`

归档的 `release:dry` 是 `echo "next version: $(node -p ...)"` ——
那是**假 dry**，只念一遍 `package.json` 的版本，不检查另外两处。

版本号在本项目里有三处独立声明：

| 文件 | 字段 |
|------|------|
| `package.json` | `version` |
| `Cargo.toml` | `[workspace.package] version` |
| `src-tauri/tauri.conf.json` | `version` |

`scripts/check-versions.mjs` 把三处绑成一个断言，另外确认各 crate 走的是
`version.workspace = true` 继承而非硬编码。输出 `next version: 0.1.0`
（格式被 `docs/RELEASE.md` 引用）。

三种用法：

```bash
node scripts/check-versions.mjs                      # 只校验三处一致
node scripts/check-versions.mjs --expect 0.2.0       # 还要求等于指定版本
node scripts/check-versions.mjs --expect-ref refs/tags/v0.2.0   # 自动剥 v 前缀
```

同一个脚本已接进 `ci.yml` 的 frontend job：版本漂移在 PR 阶段就红，
而不是等到打 tag 那一刻 —— 那时安装包已经生成了。

## 关键点三：release.yml 的三个设计决定

### 1. `verify` 卡在 tag 与版本号之间

发布事故里最常见的一类是「tag 打了 `v0.2.0`、安装包元数据还是 `0.1.0`」，
只有用户装完才发现。`verify` 用 `--expect-ref` 卡死这一条。

`workflow_dispatch` 从分支触发时没有可比的 tag，则退化为只校验三处一致
（直接拿分支名去比会得到一句莫名其妙的「版本号 0.1.0 与期望的 main 不一致」）。

`verify` 里还补跑一次只读扫描自检 —— 发版这一刻再确认「只读宪法」没被破坏，
成本极低。Rust / 前端的门禁由 `ci.yml` 在 `main` 上把关。

### 2. CLI 二进制用 arm64 runner 交叉编译，**不依赖 macos-13**

归档的 matrix 隐含假设 `macos-latest` 是 x86_64。实际上 GitHub 的
`macos-latest` 已迁到 arm64。`x86_64-apple-darwin` 的产物在 arm64 runner 上
交叉编译即可（rustup 装目标 + 系统 clang 带 `-arch x86_64` + SDK 双切片）。
**刻意不用 `macos-13`** —— 那是最后一代 Intel runner，已在退役路径上。

桌面安装包则只按 host 打包（三平台各打各的），Tauri bundler 不做交叉。

### 3. `cli` 排在 `desktop` 之后

draft release 由 `tauri-action` 创建；两个 job 同时抢着创建同一个 tag 的 release
会撞 422。串行化消除竞态，代价是发布慢一点。

## 签名：本卡明确不做

原规划的本卡只有 matrix + release workflow + CHANGELOG，**没有签名**；
ROADMAP 里「发布硬化（跨平台矩阵 / 签名 / 分发）」的「签名」是我早前的措辞。

macOS 公证需要 Apple Developer 证书、Windows 需要代码签名证书 —— 两者都不具备。
因此本卡只出 **draft release（未签名）**，签名与分发另立独立任务。
`docs/RELEASE.md` 与 `CHANGELOG.md` 都如实标注了这一点，避免用户把
「无法验证开发者」的警告当成产物损坏。

## 未做的两件事（如实记录）

1. **release workflow 从未在 GitHub 上执行过。** 本机没有可用的 Actions 环境，
   只能验证 YAML 可解析（三 job 依赖关系符合预期）、`release:dry` 本地可跑。
   首次发布必须人工盯一遍日志。
2. **`productName` 未改。** 当前是 `repoprism-app`，与产品名 `RepoPrism` 不一致，
   安装包会叫 `repoprism-app_0.1.0_x64.dmg`。但 `AGENTS.md` 明确
   「人类主导架构决策、**命名**、发布」，因此不在本卡擅自更名，留给维护者决定。

## 验证记录（本机 macOS，2026-10-09）

- `pnpm release:dry` → `next version: 0.1.0`；`--expect 9.9.9` 正确以退出码 1 失败；
  `--expect-ref refs/tags/v0.1.0` → 通过
- `python3 -c "yaml.safe_load(...)"` 解析 `ci.yml` / `release.yml` 均 OK，
  job 依赖为 `verify → desktop → cli`（`desktop.needs=verify`、`cli.needs=[verify, desktop]`）
- `cargo test --workspace`：99 passed；`cargo clippy --workspace --all-targets -D warnings` 干净
- 前端：`biome check src` 干净、`tsc --noEmit` 干净、`vitest run` 31 passed、`vite build` 成功
- `bash scripts/read-only-guard.sh`：passed；`--self-test` 26/26
