# SPEC.md — RepoPrism 规格说明 v0.1

> 本文档是 RepoPrism 的**唯一需求来源**。任何功能变更必须先改本文档，再改代码。

## 一句话定位

只读仓库智能工具，为人类和 Agent 提供代码库的多视图洞察。

## 核心原则

1. **只读**：零写操作，不修改被观察仓库
2. **本地优先**：默认不上传任何代码到云端
3. **AI 协同**：为 Agent 提供结构化数据，而非仅人类可读的界面
4. **编排而非重造**：集成 GitDiagram、GitIngest、DeepWiki，不重复实现

## 用户故事

### US-1：查看仓库状态（P0）
作为开发者，打开 RepoPrism 后能立即看到：
- 当前分支、HEAD 位置、标签
- 提交图（父子关系）
- 本地分支列表、上游跟踪计数
- worktree、stash、合并/变基状态

### US-2：查看变更（P0）
按以下分组查看文件变更：
- 合并冲突
- 已暂存改动
- 工作区改动

### US-3：查看提交详情（P0）
点击任意提交后看到：
- 变更文件列表
- diff（并排 / 统一视图，含原始行号）
- 作者、日期、父提交
- 图片对比、媒体预览、字节预览

### US-4：CLI 只读快照（P0）
```bash
repoprism inspect . --json
repoprism open . --view changes
repoprism skill --path