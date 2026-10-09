# TASK-014: 提交级 AI 分析

**状态**：已完成（2026-10-09）
**血统**：批次 `005`（原规划 `RepoPrism-仓库棱镜-005`）

## 目标

在提交详情面板中添加「AI 分析此提交」按钮。

## 约束

- 复用 `summarize_commit` 命令
- 每次打开新提交时重置摘要
- 未启用 AI 时按钮置灰

## 验收标准

- [x] 详情面板显示 AI 分析按钮
- [x] 点击后展示 1-2 句摘要（**代码路径已验证，未接真实模型**）
- [x] 切换提交时旧摘要清除

## 实现

| 文件 | 内容 |
|------|------|
| `src/components/CommitDetail.tsx` | 「AI 分析此提交」按钮 + 摘要条 + 错误条（**合并式追加**，保留 `DiffView`、`patch` 字节数、父提交跳转等既有结构） |
| `src/components/ChangesPanel.tsx` | 工作区级「AI 摘要」按钮（复用 `summarize_changes`） |
| `src/App.tsx` | 传递 `repoPath` / `aiEnabled`；挂载 `AiSettingsPanel`；设置按钮 |
| `src/App.css` | `.ai-btn` / `.ai-summary` / `.ai-tag` / `.ai-error` / `.detail-ai` |

## 与归档规划的两处偏离

### 1. 状态重置用 `key`，不用 effect

归档在 `CommitDetailPanel` 里用 `useEffect(() => {...}, [repoPath, sha])` 自己管加载与重置。
现把「切换提交即重置」交给调用方的 `key={selectedSha}` 重挂载：

```tsx
{selectedSha && <CommitDetail key={selectedSha} ... />}
```

理由：`useExhaustiveDependencies` 会指出「依赖列表里有 effect 里没用到的项」，
而这类手工重置最容易漏掉某条分支（比如只清了摘要没清错误）。
重挂载由 React 保证「这个提交的状态」和组件实例一一对应，漏不掉。
`ChangesPanel` 同理用 `key={repoPath}`。

### 2. 详情面板改名与结构保留

归档的覆盖版把组件改名为 `CommitDetailPanel` 并改用 `DiffViewer`，
而仓库现役的是 `CommitDetail` + `DiffView`（TASK-007 的产物，并排视图带真实行号）。
**取归档的意图（加 AI 按钮），不覆盖现有结构。**

## 未验证项

与 TASK-013 同一原因：本机无 Ollama，未做真实模型调用。

## 验证记录（本机 macOS，2026-10-09）

- `biome check src`：27 files 干净
- `tsc --noEmit`：干净
- `vitest run`：40 passed（含本卡无新增单测 —— 组件行为需 jsdom 才能测，见 ROADMAP 风险项）
- `pnpm build`：成功，`dist/assets/index-qz-52nYw.js` 244.43 kB
