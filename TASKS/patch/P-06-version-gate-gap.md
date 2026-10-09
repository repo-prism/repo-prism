# P-06: 版本一致性闸门补两个洞（成员继承 + Cargo.lock）

**状态**：已完成（2026-10-09）
**卡号说明**：本卡不在 001–020 的产品血统内，属工程补丁，故编入 patch 序列（见 [README](README.md)）。

## 目标

让 `scripts/check-versions.mjs` 真正拦住**提版本号时会漏掉的两件事**。
它此前只校验三处声明的值一致 —— 而 v0.2.0 发版前的人工核对发现，
三处一致的同时，另有两处是错的，脚本全都没报。

## 上下文：v0.2.0 发版前发现的两个洞

### 洞一：`checkInheritance()` 有目录假设，扫不到 `src-tauri` 自己

旧实现遍历 `["crates", "src-tauri"]`，把下一层的 `*/Cargo.toml` 当作成员清单：

```js
for (const dir of ["crates", "src-tauri"]) {
  for (const name of readdirSync(join(root, dir))) {
    const manifest = join(root, dir, name, "Cargo.toml");   // ← 假设「目录下还有一层」
    ...
  }
}
```

这对 `crates/*` 成立，对 `src-tauri` **不成立** —— `src-tauri` 自己就是那个 crate，
不是装 crate 的目录。于是 `src-tauri/Cargo.toml` 从未被读过：

```toml
[package]
name = "repo-prism"
version = "0.1.0"        # ← 硬编码，全仓库唯一一个没继承的
```

后果：`crates/*` 三张 crate 都随 workspace 走到 `0.2.0`，
只有桌面端的 `repo-prism` 包停在 `0.1.0`。安装包版本号取自 `tauri.conf.json`
所以**用户看到的是对的**，但 crate 元数据、崩溃报告、`cargo` 视角全是错的 ——
这类「只有一个地方不对」的漂移恰恰最难被发现。

`docs/RELEASE.md` 当时还写着「各 crate 走 `version.workspace = true`，
`scripts/check-versions.mjs` 会拦下来」——**这句话是假的**，本卡一并改掉。

### 洞二：完全不看 `Cargo.lock`

改版本号会让 `Cargo.lock` 里四个成员的版本一起变。漏提交它会发生什么：

| 环节 | 带不带 `--locked` | 结果 |
|------|------------------|------|
| CI 的 `clippy` / `test` | **不带** | cargo 顺手把锁文件改掉 → **当场变绿**，毫无提示 |
| `release.yml` 编 CLI / MCP | **带** | `the lock file needs to be updated but --locked was passed` → **发布那一刻失败** |

也就是说，「锁文件过期」这个错在 CI 里是隐形的，只在打 tag 时才炸 ——
而那时 tag 已经推上去了。v0.2.0 的第一次提交就漏了 `Cargo.lock`，
是靠提交后核对工作区才发现的（`git status` 里的 `.M Cargo.lock`）。

## 实现

两处校验都改成**以根 `Cargo.toml` 的 `[workspace] members` 为唯一事实来源**，
不再有任何目录结构假设：

| 校验 | 规则 | 失败信息 |
|------|------|---------|
| 4 | 每个成员的 `Cargo.toml` 不得硬编码 `version`（只能是 `version.workspace = true`） | 指名到具体 manifest |
| 5 | 成员在 `Cargo.lock` 里的 `version` 必须与当前版本号一致 | 指名到具体包，并说明「否则 release 的 `--locked` 构建会失败」 |

配套改动：

- `src-tauri/Cargo.toml` 的 `version = "0.1.0"` 改为 `version.workspace = true`
- `Cargo.lock` 里四个成员同步为 `0.2.0`（此前只有三个）
- 顺手把成员名取值收敛到 `[package]` 段内，避免读到 `[lib] name` 之类的同名键

## 验收标准

- [x] `node scripts/check-versions.mjs` 在修正后的仓库上通过（`next version: 0.2.0`）
- [x] **探针 1**：把 `Cargo.lock` 里 `repo-prism-core` 改回 `0.1.0` → 退出码 1，
      报「Cargo.lock 里 repo-prism-core 是 0.1.0，与 0.2.0 不一致」
- [x] **探针 2**：把 `src-tauri/Cargo.toml` 改回硬编码 `version = "0.2.0"` → 退出码 1，
      报「src-tauri/Cargo.toml 硬编码了 version」
- [x] 还原后重新通过（两条探针都是可逆的，探针用的临时改动已全部还原）
- [x] `Cargo.lock` 的 diff 只有 4 行版本号（`4 4`），未引入其他依赖变动
- [x] 两个探针都验证过**失败路径真的会失败** —— 这正是本卡存在的理由：
      一个不会失败的闸门比没有闸门更危险（它让人以为已经检查过了）

## 教训

「三处版本号一致」是**必要不充分**条件。发版闸门要问的不是
「我声明的三处是否一致」，而是「**仓库里所有记录版本号的地方是否一致**」——
后者只能从 workspace 成员清单推导，不能靠人工枚举。
