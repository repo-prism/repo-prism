import { describe, expect, it } from "vitest";
import { computeRange } from "./virtual";

const ROW = 48;
const VIEWPORT = 480; // 正好 10 行

describe("computeRange", () => {
  it("未滚动时从 0 开始，并按 overscan 多渲染几行", () => {
    expect(computeRange(0, VIEWPORT, ROW, 300, 5)).toEqual({ start: 0, end: 15 });
  });

  it("滚动后区间随之下移，上下各留 overscan", () => {
    expect(computeRange(480, VIEWPORT, ROW, 300, 5)).toEqual({ start: 5, end: 25 });
  });

  it("滚动到底部不越过 itemCount —— 越界会让列表渲染出 undefined 行", () => {
    expect(computeRange(480, VIEWPORT, ROW, 20, 5)).toEqual({ start: 5, end: 20 });
  });

  it("内容不足一屏时全部渲染", () => {
    expect(computeRange(0, 5000, ROW, 10, 5)).toEqual({ start: 0, end: 10 });
  });

  it("overscan 为 0 时就是精确的可见区间", () => {
    expect(computeRange(0, VIEWPORT, ROW, 300, 0)).toEqual({ start: 0, end: 10 });
  });

  it("空列表返回空区间", () => {
    expect(computeRange(0, VIEWPORT, ROW, 0, 5)).toEqual({ start: 0, end: 0 });
  });

  it("行高非法时退化为全渲染，而不是算出空区间", () => {
    // 首屏 clientHeight 为 0、行高写成 0，都会走到这条路径；
    // 结果是「列表空掉」还是「稍微慢一点」，这里选择后者。
    expect(computeRange(0, VIEWPORT, 0, 20, 5)).toEqual({ start: 0, end: 20 });
    expect(computeRange(0, VIEWPORT, Number.NaN, 20, 5)).toEqual({ start: 0, end: 20 });
  });

  it("负的 scrollTop（橡皮筋回弹）按 0 处理", () => {
    expect(computeRange(-120, VIEWPORT, ROW, 300, 5)).toEqual({ start: 0, end: 15 });
  });

  it("非有限数不会污染区间", () => {
    expect(computeRange(Number.NaN, Number.NaN, ROW, 300, 5)).toEqual({ start: 0, end: 5 });
  });

  it("300 条提交下渲染行数始终远小于总数（TASK-016 验收：DOM 行数 < 80）", () => {
    const ITEM_COUNT = 300;
    const OVERSCAN = 8;
    // 渲染行数 == end - start（CommitGraph 只 slice 这一段），所以这条断言
    // 与「DOM 里 .commit-row 的数量」是同一件事。
    for (const viewport of [480, 600, 900]) {
      for (let top = 0; top <= ITEM_COUNT * ROW; top += 37) {
        const { start, end } = computeRange(top, viewport, ROW, ITEM_COUNT, OVERSCAN);
        expect(end - start).toBeLessThan(80);
        expect(start).toBeGreaterThanOrEqual(0);
        expect(end).toBeLessThanOrEqual(ITEM_COUNT);
      }
    }
  });
});
