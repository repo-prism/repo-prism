# TASK-005: 前端基础层与侧边栏、变更面板

## 目标
- 建立与 Rust 侧对齐的 TypeScript 类型
- 封装 Tauri invoke
- 实现 App 骨架、RepoHeader、BranchList、ChangesPanel

## 验收标准
- [ ] 输入仓库路径后，侧边栏显示分支、标签、HEAD
- [ ] 变更面板正确分组 conflict / staged / unstaged
- [ ] `pnpm typecheck` 通过
- [ ] `pnpm lint` 通过