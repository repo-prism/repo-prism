import { type RefObject, useEffect, useRef, useState } from "react";
import { computeRange, DEFAULT_OVERSCAN, type VirtualRange } from "./virtual";

interface VirtualList {
  /** 当前需要渲染的区间。 */
  range: VirtualRange;
  /** 挂到滚动容器上。 */
  containerRef: RefObject<HTMLDivElement | null>;
  /** 撑开滚动条用的总高度 = 行数 × 行高。 */
  totalHeight: number;
  /** 渲染窗口的 `translateY`，等于 `range.start × 行高`。 */
  offsetY: number;
}

/**
 * 定高列表的虚拟滚动。
 *
 * 只做两件事：监听滚动与视口尺寸，把区间交给 `computeRange` 算。
 * 滚动事件用 `requestAnimationFrame` 合并 —— 一次快速滚动会触发几十个
 * scroll 事件，逐个 `setState` 反而比不做虚拟化更卡。
 *
 * 不引第三方虚拟化库：这里的需求窄到只有「定高 + overscan」，
 * 自研一个 40 行的 hook 比多背一个依赖更好审。
 */
export function useVirtualList(
  itemCount: number,
  rowHeight: number,
  overscan: number = DEFAULT_OVERSCAN,
): VirtualList {
  const containerRef = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState<VirtualRange>(() =>
    computeRange(0, 0, rowHeight, itemCount, overscan),
  );

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    let frame = 0;
    const measure = () => {
      const next = computeRange(el.scrollTop, el.clientHeight, rowHeight, itemCount, overscan);
      // 区间没变就不 setState：这是「滚动不掉帧」的关键一步。
      setRange((prev) => (prev.start === next.start && prev.end === next.end ? prev : next));
    };

    const onScroll = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measure);
    };

    el.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", measure);
    // 挂载后先量一次：首屏的 clientHeight 只有在 DOM 就位后才是真的。
    measure();

    return () => {
      el.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", measure);
      cancelAnimationFrame(frame);
    };
  }, [itemCount, rowHeight, overscan]);

  return {
    range,
    containerRef,
    totalHeight: Math.max(0, itemCount) * rowHeight,
    offsetY: range.start * rowHeight,
  };
}
