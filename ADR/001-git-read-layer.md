# ADR-001: Git 读取层技术选型

- **状态**：已接受
- **日期**：2026-10-08
- **决策者**：项目主导

## 背景

RepoPrism 需要在 Rust 中读取 Git 仓库状态。有三种主流方案，需选择一种作为 MVP 主后端，并规划长期演进。

## 候选方案

| 方案 | 优势 | 劣势 |
|------|------|------|
| **系统 Git 可执行文件** | 与用户环境完全一致；行为可预测；无需额外依赖 | 依赖 PATH；进程启动有开销；解析输出需稳定 |
| **gitoxide (`gix`)** | 纯 Rust；无 PATH 依赖；性能优秀；天然支持只读 | 部分高级功能仍在开发中；API 演进快 |
| **libgit2** | 成熟稳定；功能完整 | C 依赖；跨平台编译复杂；与系统 Git 行为可能有差异 |

## 决策

**MVP 阶段采用系统 Git 可执行文件**，通过 `std::process::Command` 调用，所有命令限制在只读白名单内。

**P1 阶段引入 `gix` 作为备选后端**，通过 trait 抽象读取层：

```rust
pub trait GitReader {
    fn snapshot(&self, path: &Path) -> Result<RepoSnapshot>;
    fn commit_detail(&self, path: &Path, sha: &str) -> Result<CommitDetail>;
    fn diff(&self, path: &Path, from: &str, to: &str) -> Result<Diff>;
}