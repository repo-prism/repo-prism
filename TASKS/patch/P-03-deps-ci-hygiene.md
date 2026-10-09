# P-03: 依赖与 CI 卫生（npm/pnpm、build、性能门禁）

**状态**：已完成（2026-10-09）
**卡号说明**：本卡不在 001–020 的产品血统内，属工程补丁，故编入 patch 序列（见 [README](README.md)）。

## 目标

修掉当前**工作区已经处于脏状态**的依赖问题，并补齐 `AGENTS.md` 要求但 CI 缺失的两道门禁。

## 上下文

### 问题 A：`pnpm` 被误加为运行时依赖（未提交改动）

实测 `git status --porcelain=v2`：

```
1 .M N... package.json      ← 未提交
? package-lock.json          ← 未跟踪
```

`git diff package.json` 内容：

```diff
   "dependencies": {
     "@tauri-apps/api": "^2",
     "@tauri-apps/plugin-opener": "^2",
+    "pnpm": "^12.10.1",
     "react": "^19.1.0",
```

三重问题：

1. 包管理器不是运行时依赖；
2. 仓库同时存在 `pnpm-lock.yaml`（已跟踪）与 `package-lock.json`（未跟踪），**两个锁文件并存**；
3. CI 执行 `pnpm install --frozen-lockfile`，`package.json` 与 `pnpm-lock.yaml` 一旦不一致即失败。

### 问题 B：CI 从不验证前端"能否构建"

`ci.yml` 的 frontend job 只跑 `pnpm lint` / `pnpm typecheck` / `pnpm test`。
`pnpm build`（`tsc && vite build`）从未执行。

### 问题 C：`AGENTS.md` 要求的"性能基准"门禁不存在

`ci.yml` 只有 `read-only-guard` / `rust` / `frontend` 三个 job。各任务卡里的
`< 500ms` / `< 800ms` 指标此前全靠人工自觉。

## 实际改动

| 文件 | 改动 |
|------|------|
| `package.json` | 移除误加的 `"pnpm": "^12.10.1"` 运行时依赖 |
| `.gitignore` | 忽略 `package-lock.json`（附理由注释） |
| （删除）`package-lock.json` | 未跟踪的 npm 锁文件，已删除（可由 `npm install` 重新生成） |
| `.github/workflows/ci.yml` | 新增 `perf` job；frontend job 增加 `pnpm build`；clippy 改为 `--all-targets`；pnpm 版本显式钉为 `12.10.1` |
| `crates/repo-prism-core/tests/perf.rs`（新增） | 性能门禁：真实断言，非"只打印数字" |
| `crates/repo-prism-core/tests/common/mod.rs` | 新增 `git_stdin` / `seed_fast_import`（`git fast-import` 造历史） |

### 与计划的偏离：**未**添加 `packageManager` 字段（有实测依据）

原计划要求在 `package.json` 加 `"packageManager": "pnpm@12.10.1"`。实测（pnpm 12.10.1）发现：

```
× resolve package manager dependencies
╰─▶ Cannot update packageManagerDependencies with "frozen-lockfile" because
    the lockfile is not up to date
```

即：一旦写入该字段，pnpm 要求把**它自己**也记进 `pnpm-lock.yaml`——

- 新增 `packageManagerDependencies` 与 **158 行** `@pnpm/exe.<平台>` 记录；
- `pnpm-lock.yaml` 从 960 行涨到 1118 行（+16%），且变成**双 YAML 文档**
  （两个 `lockfileVersion` / `importers` / `packages`）；
- 在 `.npmrc` 里设 `manage-package-manager-versions=false` **也无法阻止**（同样报错）。

为一行版本声明把锁文件撑大 16%、并让它变成需要解释的格式，代价大于收益。
**改为在 `ci.yml` 里显式钉住 `version: 12.10.1`**，版本来源与本文件内的说明即为事实来源。
该偏离已记录在 `ci.yml` 的注释里，避免后人重新踩坑。

## 验收标准

- [x] `package.json` 中不再存在 `"pnpm"` 依赖项
- [ ] ~~`package.json` 增加 `"packageManager"` 字段~~ → **改为 ci.yml 显式钉版本**，理由见上
- [x] 仓库根目录不再同时存在两个锁文件：`package-lock.json` 已删除**且**加入 `.gitignore`
      （只做 `.gitignore` 只能阻止入库，无法阻止 npm 继续读它，故两者都做）
- [x] 本地 `pnpm install --frozen-lockfile` 通过（隔离目录校验，见下）
- [x] `ci.yml` frontend job 增加 `pnpm build` 步骤
- [x] `ci.yml` 新增性能基准 job，且断言真实失败（不是只打印数字）：
      `snapshot()` < 500ms、`commits(200, 0)` < 800ms
      （采样方式后续修正为「预热 + 3 次取最小值」，理由见文末）
- [x] 工作区 `git status` 无遗留未跟踪的锁文件
- [ ] CI 全绿（含 macOS / Windows / Linux 三平台矩阵）—— 需推送后由 CI 判定

### 实测数据（本机 macOS，2026-10-09）

```
=== frozen 校验（当前 package.json + 已提交 pnpm-lock.yaml）===
FROZEN OK
锁文件未被改写

=== 反证：把 pnpm 依赖加回 package.json ===
EXPECTED FAIL —— 确认这道门禁确实会拦住不一致
```

## 后续修正：性能门禁改为「N 次采样取最小值」

本卡最初用**单次采样**断言阈值，后来发现它不可信。同日实测，**同一份代码**
连跑三次 `snapshot()` 得到：

```
576ms（超预算，测试红） / 268ms / 339ms
```

单次采样测的是「机器当时有多忙」，不是「代码有多快」。CI runner 比开发机更吵，
这种门禁会**随机变红**，进而被当成噪声忽略——那就等于没有门禁。

改为：预热一次 → 采样 3 次 → 取**最小值**断言，并把全部采样打印出来便于区分
「代码退化」与「runner 抖动」。最小值只可能被抖动抬高，不会被抖动压低；
真正的算法退化会让所有采样一起变慢。

修正后连跑三次（本机同时还有其它负载）：

```
snapshot():        431ms / 437ms / 423ms -> best 423ms / budget 500ms
commits(200, 0):   188ms / 188ms / 156ms -> best 155ms / budget 800ms
```

**注意**：这里的绝对值随机器负载浮动（空闲时曾见 208ms），所以门禁只应看
「是否超预算」，不要跨机器、跨时段比较绝对数字。

## 关于性能基准的仓库选择

`AGENTS.md` 原文是「大仓库（Linux 内核级）」。CI 无法下载并长期维护内核级快照，
故改用 `git fast-import` 在测试内构造 **3000 条提交**的代理仓库：
构造 < 1s、不依赖网络、结果确定。

**阈值沿用任务卡原值，未因仓库变小而下调** —— 放松阈值会让门禁失去意义。

## 禁止事项

- 不得把性能基准写成"永远打印数字但从不失败"的形式——必须有断言阈值
- 不得引入需要外网下载大体积数据集的基准方案
