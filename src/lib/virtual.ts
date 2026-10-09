/**
 * 虚拟滚动的**纯计算部分**。
 *
 * 刻意与 React 解耦：`useVirtualList` 里的滚动/尺寸读取在 node 环境下测不到，
 * 但「哪一段该渲染」这件事是纯函数，可以逐条断言。渲染 bug 里最常见的
 * 就是区间算错（滚动到底部白屏、滚动到顶部少一行），把这段抽出来就能测住。
 */

export interface VirtualRange {
  /** 首个要渲染的下标（含）。 */
  start: number;
  /** 最后一个要渲染的下标（**不含**）。 */
  end: number;
}

/** 视口上下各多渲染几行，抵消快速滚动时的白屏。 */
export const DEFAULT_OVERSCAN = 5;

/**
 * 算出当前视口需要渲染的区间 `[start, end)`。
 *
 * 所有入参都当作**可能不可信**：行高来自常量但也可能被改成 0，
 * `clientHeight` 在首次布局前就是 0。任何一项坏掉都不该算出 NaN 区间
 * ——那会让列表「什么都没有」，比多渲染几行难查得多。
 */
export function computeRange(
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  itemCount: number,
  overscan: number = DEFAULT_OVERSCAN,
): VirtualRange {
  const count = Number.isFinite(itemCount) ? Math.max(0, Math.floor(itemCount)) : 0;
  if (count === 0) return { start: 0, end: 0 };

  // 行高坏掉 → 退化为「全渲染」：宁可慢，也不要空
  if (!Number.isFinite(rowHeight) || rowHeight <= 0) {
    return { start: 0, end: count };
  }

  const margin = Number.isFinite(overscan) && overscan > 0 ? Math.floor(overscan) : 0;
  const top = Number.isFinite(scrollTop) ? Math.max(0, scrollTop) : 0;
  const viewport = Number.isFinite(viewportHeight) ? Math.max(0, viewportHeight) : 0;

  const start = Math.max(0, Math.floor(top / rowHeight) - margin);
  const end = Math.min(count, Math.ceil((top + viewport) / rowHeight) + margin);

  return { start, end: Math.max(start, end) };
}
