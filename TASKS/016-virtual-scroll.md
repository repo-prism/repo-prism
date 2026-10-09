# TASK-016: 提交列表虚拟滚动

**状态**：已完成（2026-10-09）
**血统**：批次 `005`（原规划 `RepoPrism-仓库棱镜-005`）

## 目标

将提交列表改为虚拟滚动，300+ 提交下滚动保持流畅。

## 约束

- 不引入第三方虚拟化库（自研，控制体积）
- 行高固定 48px
- overscan 8 行（归档写 5，见下）

## 验收标准

- [x] 300 条提交下 DOM 节点 < 80
- [ ] 滚动流畅，无明显卡顿 —— **需 `pnpm tauri dev` 手工确认**
- [x] 点击选中仍正常（行仍是 `<button>`，`aria-pressed` 保留）

## 实现

| 文件 | 内容 |
|------|------|
| `src/lib/virtual.ts`（新增） | `computeRange` 纯函数 + `VirtualRange` + `DEFAULT_OVERSCAN` |
| `src/lib/virtual.test.ts`（新增） | 10 个用例，含 300 条提交的验收断言 |
| `src/lib/useVirtualList.ts`（新增） | rAF 节流 + resize 监听，复用 `computeRange` |
| `src/components/CommitGraph.tsx` | 加 `commit-list-inner` / `commit-list-window` 两层，只渲染 `graph.slice(start, end)` |
| `src/App.css` | `.commit-list-inner`（`position: relative`）、`.commit-list-window`（绝对定位）；滚动容器的 20px 内边距保持不变 |

## 关键设计：把验收标准变成可执行断言

「300 条提交下 DOM 节点 < 80」这句话原本只能靠打开 DevTools 数。
但 CommitGraph 渲染的行数**恰好等于** `end - start`，于是这条验收标准
可以写成一个循环断言：

```ts
for (const viewport of [480, 600, 900]) {
  for (let top = 0; top <= 300 * ROW; top += 37) {
    const { start, end } = computeRange(top, viewport, ROW, 300, 8);
    expect(end - start).toBeLessThan(80);
  }
}
```

这也是为什么 `computeRange` 被抽成纯函数、与 React 解耦：
`useVirtualList` 里的滚动与尺寸读取在 node 环境下测不到，
但「哪一段该渲染」是纯的。渲染 bug 里最常见的就是区间算错
（滚到底部白屏、滚到顶部少一行），把这段抽出来才测得住。

## 与归档规划的两处偏离

### 1. overscan 由 5 提到 8

48px 行高下一屏约 10 行，overscan 5 在快速滚动时会露出未渲染区域。
8 行的代价是多渲染 6 个 DOM 节点，仍在 `< 80` 的预算内。

### 2. 不用 `useEffect` 手工算，改为「纯函数 + hook」

归档把 rAF 与区间计算揉在一个 hook 里。现拆成 `virtual.ts`（纯）+ `useVirtualList.ts`（副作用）。
理由同上：拆开后验收标准才能变成断言。

## 一处必须说明的真实限制

`useVirtualList` 依赖滚动容器自身的 `clientHeight`。
`.commit-list` 是 `flex: 1` 的子项，父级链上任何一处缺少 `min-height: 0`
都会让它的高度变成「内容高度」，虚拟化随即失效（表现为「没有滚动条」或「全渲染」）。
现役的 `.main` / `.main-split` / `.commit-graph` 都已有 `min-height: 0`（TASK-007 时就位），
但**改动这几处布局时要把虚拟化一起回归**——这是本条实现里最脆的一环。

## 验证记录（本机 macOS，2026-10-09）

- `vitest run`：**41 passed**（`virtual.test.ts` 10 项）
- `biome check src`：干净；`tsc --noEmit`：干净
- `pnpm build`：成功
- 未做：真实 300 提交仓库的滚动帧率测量（需 GUI）
