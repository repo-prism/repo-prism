# RepoPrism 仓库棱镜

> Read-only repo intelligence for humans and agents.
> 只读仓库智能工具，为人类和 Agent 提供代码库的多视图洞察。

## 是什么

RepoPrism 把同一份代码仓库，折射成多种视图：

- 🌳 **提交图**：分支、标签、HEAD、父子关系 ✅
- 📝 **变更分组**：冲突 / 已暂存 / 工作区 ✅
- 🔍 **Diff**：统一 / 并排视图，含 Git 原始行号 ✅
- 📄 **文本化仓库**：一键导出 LLM 友好纯文本 —— 计划中
- 🏗️ **架构图**：集成 GitDiagram —— 计划中
- 📚 **AI 文档**：集成 DeepWiki —— 计划中
- 🤖 **Agent 协同**：CLI ✅ / Skill 计划中 / MCP Server 计划中

功能状态以 [`SPEC.md`](SPEC.md) 为唯一事实来源；上表未标 ✅ 的能力**尚未实现**。

## 不是什么

- ❌ 不是 Git 客户端（不做写操作）
- ❌ 不是代码编辑器
- ❌ 不是 CI/CD 平台
- ❌ 不是代码托管

## 核心原则

1. **只读**：零写操作，不修改被观察仓库
2. **本地优先**：默认不上传代码
3. **AI 协同**：为 Agent 提供结构化数据
4. **编排而非重造**：集成优秀工具，不重复实现

## 快速开始

```bash
# CLI：输出只读 JSON 快照
repoprism inspect . --json

# 桌面应用（开发模式）
pnpm tauri dev
```

## 开发

```bash
# Rust：格式 / lint / 测试
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# 前端
pnpm install --frozen-lockfile
pnpm lint && pnpm typecheck && pnpm test && pnpm build

# 只读安全扫描（含扫描器自检）
bash scripts/read-only-guard.sh --self-test
bash scripts/read-only-guard.sh
```

包管理器统一为 **pnpm**（版本以 `.github/workflows/ci.yml` 为准）。
仓库不保留 `package-lock.json`，避免双锁文件让 `--frozen-lockfile` 的判定失去意义。

## 文档地图

| 文件 | 作用 |
|------|------|
| `SPEC.md` | 唯一需求来源，含每条能力的实现状态 |
| `SECURITY.md` | 只读安全模型、威胁模型、CI 扫描说明 |
| `AGENTS.md` | 与 AI Agent 的协作协议与只读宪法 |
| `ROADMAP.md` | 现状盘点与阶段计划 |
| `TASKS/` | 任务卡（每张含目标 / 约束 / 验收标准） |
| `ADR/` | 架构决策记录 |
