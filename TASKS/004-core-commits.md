# TASK-004: core 读取提交图 + Tauri 命令层

## 目标
1. 扩展 `repo-prism-core`：实现完整的 `status()` 与 `commits()`
2. 在 `src-tauri` 暴露两个 Tauri 命令：`inspect_repo`、`get_commits`

## 上下文
- ADR-001：使用系统 Git 可执行文件
- 命令白名单：`rev-parse` `symbolic-ref` `for-each-ref` `status` `log`
- 输出为稳定 JSON，供前端与未来 MCP 共用

## 约束
- 所有 Git 调用仍集中在 `git.rs`
- `commits()` 使用 `\x1f` (字段) / `\x1e` (记录) 分隔符，避免解析歧义
- 单次最多返回 2000 条，默认 200

## 验收标准
- [ ] `RepoSnapshot.status` 正确分组 conflict / staged / unstaged
- [ ] `commits(200, 0)` 在 Linux 内核仓库上 < 800ms
- [ ] `for-each-ref` 输出的 refs 正确映射到 `CommitInfo.refs`
- [ ] CI 只读扫描通过
- [ ] `cargo test -p repo-prism-core` 通过