# RepoPrism 仓库棱镜

> Read-only repo intelligence for humans and agents.
> 只读仓库智能工具，为人类和 Agent 提供代码库的多视图洞察。

## 是什么

RepoPrism 把同一份代码仓库，折射成多种视图：

- 🌳 **提交图**：分支、标签、HEAD、父子关系
- 📝 **变更分组**：冲突 / 已暂存 / 工作区
- 🔍 **Diff**：并排 / 统一视图，图片对比，媒体预览
- 📄 **文本化仓库**：一键导出 LLM 友好纯文本
- 🏗️ **架构图**：集成 GitDiagram
- 📚 **AI 文档**：集成 DeepWiki
- 🤖 **Agent 协同**：CLI / Skill / MCP Server

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
# CLI
repoprism inspect . --json

# 桌面应用
repoprism open . --view changes

# Agent Skill
repoprism skill --path