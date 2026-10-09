import { useMemo, useState } from "react";
import type { Diff, DiffFile, DiffLine, DiffLineKind } from "../lib/api";
import { type SideBySideRow, toSideBySide } from "../lib/diff";
import { kindLabel, kindText } from "../lib/kinds";

const MARKER: Record<DiffLineKind, string> = {
  context: " ",
  add: "+",
  del: "-",
};

type Layout = "unified" | "split";

interface Props {
  diff: Diff;
  /** 原始 patch 文本超过 2 MiB 上限而被截断（与结构化解析上限是两道独立的闸）。 */
  byteTruncated?: boolean;
}

/**
 * Diff 视图：统一 / 并排两种布局。
 *
 * **渲染安全**：所有内容都以文本节点写入（`code` / `span` 的 children），
 * 不使用 `dangerouslySetInnerHTML`，也不执行任何来自仓库的内容
 * ——见 SECURITY.md 威胁 3。
 */
export function DiffView({ diff, byteTruncated = false }: Props) {
  const [layout, setLayout] = useState<Layout>("unified");

  if (diff.files.length === 0) {
    // 合并提交的 `git show` 正文本就为空，与「空提交」无法区分，故合并成一句提示。
    return <div className="empty">合并提交或空 diff，无直接变更内容</div>;
  }

  const truncated = diff.truncated || byteTruncated;

  return (
    <div className="diff-view">
      <div className="diff-toolbar">
        <span className="muted">
          {diff.files.length} 个文件 · {layout === "unified" ? "统一视图" : "并排视图"}
        </span>
        <div className="segmented">
          <button
            type="button"
            className={layout === "unified" ? "active" : ""}
            aria-pressed={layout === "unified"}
            onClick={() => setLayout("unified")}
          >
            统一
          </button>
          <button
            type="button"
            className={layout === "split" ? "active" : ""}
            aria-pressed={layout === "split"}
            onClick={() => setLayout("split")}
          >
            并排
          </button>
        </div>
      </div>

      {truncated && (
        <div className="diff-notice">
          Diff 超出单次上限（原始文本 2 MiB / 解析 5000 行），后续内容已截断
        </div>
      )}

      {diff.files.map((file) => (
        <DiffFileBlock key={file.path} file={file} layout={layout} />
      ))}
    </div>
  );
}

function DiffFileBlock({ file, layout }: { file: DiffFile; layout: Layout }) {
  return (
    <div className="diff-file">
      <div className="diff-file-header">
        <span className={`kind kind-${file.kind}`} title={kindText(file.kind)}>
          {kindLabel(file.kind)}
        </span>
        <span className="path">{file.path}</span>
        {file.old_path && <span className="muted">← {file.old_path}</span>}
        {file.binary && <span className="tag">二进制</span>}
        <span className="diff-stat">
          <span className="add-count">+{file.additions}</span>
          <span className="del-count">-{file.deletions}</span>
        </span>
      </div>

      {file.binary && (
        <div className="diff-empty muted">二进制文件，仅标记类型与行数，不读取内容</div>
      )}
      {file.truncated && <div className="diff-notice">本文件 diff 已截断</div>}

      {file.hunks.map((hunk) => (
        <div className="hunk" key={`${hunk.old_start}-${hunk.new_start}-${hunk.header}`}>
          <div className="hunk-header">{hunk.header}</div>
          {layout === "unified" ? (
            <UnifiedHunk lines={hunk.lines} />
          ) : (
            <SplitHunk lines={hunk.lines} />
          )}
        </div>
      ))}
    </div>
  );
}

/** hunk 内 `(旧行号, 新行号)` 必然互不重复，可安全用作 React key。 */
function lineKey(line: DiffLine): string {
  return `${line.kind}:${line.old_no ?? "-"}:${line.new_no ?? "-"}`;
}

function UnifiedHunk({ lines }: { lines: DiffLine[] }) {
  return (
    <div className="diff-rows">
      {lines.map((line) => (
        <div className={`diff-row diff-${line.kind}`} key={lineKey(line)}>
          <span className="line-no">{line.old_no ?? ""}</span>
          <span className="line-no">{line.new_no ?? ""}</span>
          <span className="line-marker">{MARKER[line.kind]}</span>
          <code className="line-content">{line.content}</code>
        </div>
      ))}
    </div>
  );
}

function SplitHunk({ lines }: { lines: DiffLine[] }) {
  const rows = useMemo(() => toSideBySide(lines), [lines]);
  return (
    <div className="diff-rows split">
      {rows.map((row) => (
        <div className="diff-row split-row" key={rowKey(row)}>
          <SplitCell line={row.left} side="left" />
          <SplitCell line={row.right} side="right" />
        </div>
      ))}
    </div>
  );
}

function rowKey(row: SideBySideRow): string {
  const left = row.left ? lineKey(row.left) : "-";
  const right = row.right ? lineKey(row.right) : "-";
  return `${left}|${right}`;
}

function SplitCell({ line, side }: { line: DiffLine | null; side: "left" | "right" }) {
  if (!line) {
    return <div className="split-cell empty-side" aria-hidden="true" />;
  }
  const no = side === "left" ? line.old_no : line.new_no;
  return (
    <div className={`split-cell diff-${line.kind}`}>
      <span className="line-no">{no ?? ""}</span>
      <code className="line-content">{line.content}</code>
    </div>
  );
}
