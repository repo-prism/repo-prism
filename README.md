# RepoPrism 仓库棱镜

> Read-only repo intelligence for humans and agents.
> 只读仓库智能工具，为人类和 Agent 提供代码库的多视图洞察。

## 是什么

RepoPrism 把同一份代码仓库，折射成多种视图：

- 🌳 **提交图**：分支、标签、HEAD、父子关系 ✅
- 📝 **变更分组**：冲突 / 已暂存 / 工作区 ✅
- ⚠️ **风险标记**：10 条纯本地启发式规则 + 摘要条 ✅
- 🔍 **Diff**：统一 / 并排视图，含 Git 原始行号 ✅
- 🔗 **一键跳转**：GitDiagram / GitIngest / DeepWiki / GitHub.dev ✅
- 🤖 **Agent 协同**：CLI ✅ / Skill ✅ / MCP Server ✅
- 🖼️ **图片对比 / 媒体预览 / 字节预览** —— 计划中
- 📄 **文本化仓库（就地导出）** —— 计划中，当前走 GitIngest 跳转
- 🏗️ **架构图（就地生成）** —— 计划中，当前走 GitDiagram 跳转
- 📚 **AI 文档（就地问答）** —— 计划中，当前走 DeepWiki 跳转

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
# CLI：只读快照（所有 --json 输出共用同一 schema 信封）
repoprism inspect . --json
repoprism commits . --limit 50 --json
repoprism detail . --sha <sha> --json

# Agent：取出打包好的 Skill
repoprism skill --path      # 打印展开目录
repoprism skill --print     # 直接打印内容

# 桌面应用（开发模式）
pnpm tauri dev
```

MCP Server 见 [`crates/repo-prism-mcp/README.md`](crates/repo-prism-mcp/README.md)，
Agent 使用说明见 [`skill/SKILL.md`](skill/SKILL.md)。

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

# 版本号一致性（三处声明：package.json / workspace Cargo.toml / tauri.conf.json）
pnpm release:dry
```

包管理器统一为 **pnpm**（版本以 `.github/workflows/ci.yml` 为准）。
仓库不保留 `package-lock.json`，避免双锁文件让 `--frozen-lockfile` 的判定失去意义。

## 发布

打 `v*.*.*` tag 会触发 [`.github/workflows/release.yml`](.github/workflows/release.yml)，
产出三平台桌面安装包与四目标 CLI 二进制，**全部是 draft release**，需人工确认后发布。
当前**不做代码签名**（macOS 未公证、Windows 未签名），安装时会有系统警告 —— 这是已知情况。

完整流程见 [`docs/RELEASE.md`](docs/RELEASE.md)，版本历史见 [`CHANGELOG.md`](CHANGELOG.md)。

## 文档地图

| 文件 | 作用 |
|------|------|
| `SPEC.md` | 唯一需求来源，含每条能力的实现状态 |
| `SECURITY.md` | 只读安全模型、威胁模型、CI 扫描说明 |
| `AGENTS.md` | 与 AI Agent 的协作协议与只读宪法 |
| `ROADMAP.md` | 产品血统、编号规则与阶段计划 |
| `CHANGELOG.md` | 版本历史（Keep a Changelog 格式） |
| `docs/RELEASE.md` | 发布流程与人工验收步骤 |
| `TASKS/` | 任务卡（血统编号；工程补丁见 `TASKS/patch/`） |
| `ADR/` | 架构决策记录 |
