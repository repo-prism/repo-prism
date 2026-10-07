# TASK-002: 实现 core 的 RepoSnapshot 读取

## 目标
在 `repo-prism-core` 中实现 `snapshot()`，返回仓库只读快照。

## 上下文
- 使用系统 Git 可执行文件（ADR-001）
- 只读白名单命令：`rev-parse`、`for-each-ref`、`status --porcelain=v2`、`branch`、`tag`

## 约束
- 不触发 hook
- 不运行 external filter
- 不下载 LFS
- 所有 Git 调用集中在 `git.rs`，便于 CI 扫描

## 验收标准
- [ ] `RepoSnapshot` 包含 head / branches / tags / status
- [ ] 单元测试覆盖：正常仓库、detached HEAD、无提交仓库
- [ ] 在 Linux 内核仓库上 `snapshot()` < 500ms
- [ ] CI 只读扫描通过

## 禁止事项
- 不得使用 `checkout`、`commit`、`merge`、`rebase`、`reset`