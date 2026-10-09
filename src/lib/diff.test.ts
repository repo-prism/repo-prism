import { describe, expect, it } from "vitest";
import type { DiffLine } from "./api";
import { countHunkLines, toSideBySide } from "./diff";

function line(
  kind: DiffLine["kind"],
  content: string,
  oldNo: number | null,
  newNo: number | null,
): DiffLine {
  return { kind, content, old_no: oldNo, new_no: newNo };
}

describe("toSideBySide", () => {
  it("上下文行两侧共用同一行", () => {
    const rows = toSideBySide([line("context", "keep", 1, 1)]);
    expect(rows).toHaveLength(1);
    expect(rows[0].left?.content).toBe("keep");
    expect(rows[0].right?.content).toBe("keep");
  });

  it("等量的删除与新增逐行左右配对", () => {
    const rows = toSideBySide([line("del", "old", 1, null), line("add", "new", null, 1)]);
    expect(rows).toHaveLength(1);
    expect(rows[0].left?.content).toBe("old");
    expect(rows[0].right?.content).toBe("new");
  });

  it("新增多于删除时短的一侧补空", () => {
    const rows = toSideBySide([
      line("del", "old", 1, null),
      line("add", "n1", null, 1),
      line("add", "n2", null, 2),
    ]);
    expect(rows).toHaveLength(2);
    expect(rows[0].left?.content).toBe("old");
    expect(rows[1].left).toBeNull();
    expect(rows[1].right?.content).toBe("n2");
  });

  it("删除多于新增时右侧补空", () => {
    const rows = toSideBySide([
      line("del", "o1", 1, null),
      line("del", "o2", 2, null),
      line("add", "n1", null, 1),
    ]);
    expect(rows).toHaveLength(2);
    expect(rows[0].right?.content).toBe("n1");
    expect(rows[1].right).toBeNull();
    expect(rows[1].left?.content).toBe("o2");
  });

  it("上下文会切断段落，不跨段落配对", () => {
    const rows = toSideBySide([
      line("del", "a", 1, null),
      line("add", "b", null, 1),
      line("context", "mid", 2, 2),
      line("del", "c", 3, null),
      line("add", "d", null, 3),
    ]);
    expect(rows).toHaveLength(3);
    expect(rows[0].left?.content).toBe("a");
    expect(rows[1].left?.content).toBe("mid");
    expect(rows[1].right?.content).toBe("mid");
    expect(rows[2].left?.content).toBe("c");
    expect(rows[2].right?.content).toBe("d");
  });

  it("新增先于删除出现时也不串段", () => {
    const rows = toSideBySide([line("add", "n1", null, 1), line("del", "o1", 1, null)]);
    expect(rows).toHaveLength(2);
    expect(rows[0].left).toBeNull();
    expect(rows[0].right?.content).toBe("n1");
    expect(rows[1].left?.content).toBe("o1");
    expect(rows[1].right).toBeNull();
  });

  it("空输入返回空", () => {
    expect(toSideBySide([])).toHaveLength(0);
  });
});

describe("countHunkLines", () => {
  it("分别统计新增与删除", () => {
    const counts = countHunkLines({
      header: "@@ -1,2 +1,2 @@",
      old_start: 1,
      old_lines: 2,
      new_start: 1,
      new_lines: 2,
      lines: [line("context", "a", 1, 1), line("del", "b", 2, null), line("add", "B", null, 2)],
    });
    expect(counts).toEqual({ additions: 1, deletions: 1 });
  });
});
