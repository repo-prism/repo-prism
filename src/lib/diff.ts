import type { DiffHunk, DiffLine } from "./api";

/**
 * 并排视图的一行：左右两侧各自承载一个 diff 行。
 *
 * 纯函数、不依赖 DOM，便于单测覆盖配对的边界情况。
 */
export interface SideBySideRow {
  left: DiffLine | null;
  right: DiffLine | null;
}

/**
 * 把统一的 diff 行序列配对成并排视图的行。
 *
 * 规则：
 * - 上下文行两侧共用同一行。
 * - 连续的新增（或删除）合并为一段，逐行左右对齐：
 *   删除放左、新增放右，**短的一侧补空行**，这样后面的行不会错位。
 *
 * 之所以要在遇到「上下文」或「段类型切换」时先结算，是因为 Git 在同一处改动里
 * 可能连续输出多组 `-` / `+`，不能假定整个 hunk 只有一次删除段。
 */
export function toSideBySide(lines: DiffLine[]): SideBySideRow[] {
  const rows: SideBySideRow[] = [];
  let dels: DiffLine[] = [];
  let adds: DiffLine[] = [];

  const flush = () => {
    const count = Math.max(dels.length, adds.length);
    for (let i = 0; i < count; i++) {
      rows.push({ left: dels[i] ?? null, right: adds[i] ?? null });
    }
    dels = [];
    adds = [];
  };

  for (const line of lines) {
    if (line.kind === "del") {
      // 已经积了新增，说明上一段结束了（例如 + 出现在 - 之前的情况）
      if (adds.length > 0) flush();
      dels.push(line);
    } else if (line.kind === "add") {
      adds.push(line);
    } else {
      flush();
      rows.push({ left: line, right: line });
    }
  }
  flush();

  return rows;
}

/** hunk 内的行数统计，用于文件摘要。 */
export function countHunkLines(hunk: DiffHunk): { additions: number; deletions: number } {
  let additions = 0;
  let deletions = 0;
  for (const line of hunk.lines) {
    if (line.kind === "add") additions++;
    else if (line.kind === "del") deletions++;
  }
  return { additions, deletions };
}
