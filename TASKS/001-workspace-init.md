# TASK-001: 初始化 Cargo Workspace 结构

## 目标
将 Tauri 项目与 CLI、MCP、core 合并为 Cargo workspace。

## 上下文
- 仓库已用 `create-tauri-app` 初始化
- 需要共享 Git 读取逻辑

## 约束
- `src-tauri` 保持 Tauri 标准结构
- `crates/repo-prism-core` 是唯一允许调用 Git 的 crate
- workspace resolver = "2"

## 验收标准
- [ ] 根 `Cargo.toml` 定义 workspace，members 包含 4 个 crate
- [ ] `cargo build --workspace` 成功
- [ ] `pnpm tauri dev` 仍能启动
- [ ] `crates/repo-prism-core/src/lib.rs` 存在且导出 `RepoSnapshot`

## 禁止事项
- 不得在 core 之外的 crate 中调用 Git