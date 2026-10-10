import { useEffect, useState } from "react";
import { type BlobPreview, getBlobPreview } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { Target } from "../lib/preview";
import { blobDataUrl, kindLabel, previewNotice } from "../lib/preview";

interface Props {
  repoPath: string;
  /** 变更文件的显示路径（用于标题）。 */
  path: string;
  /** 旧版本所在的版本与路径；新增文件为 `null`。 */
  before: Target | null;
  /** 新版本所在的版本与路径；删除文件为 `null`。 */
  after: Target | null;
  onClose: () => void;
}

/**
 * blob 内容预览（US-3，补丁 P-10）。
 *
 * **只在被挂载时才读内容** —— 这是 SPEC 里「不在 diff 里自动预览」的落点：
 * 读取要起 1–2 次子进程，而且读的是真实字节，绝不能跟着首屏一起发生。
 *
 * 两块并排（旧 / 新）是「图片对比」的形态；只有一侧时不做对比。
 */
export function BlobPreviewPanel({ repoPath, path, before, after, onClose }: Props) {
  const [beforeView, setBeforeView] = useState<BlobPreview | null>(null);
  const [afterView, setAfterView] = useState<BlobPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 依赖写成基本类型而不是对象：调用方每次渲染都会新建 `Target`，
  // 直接依赖对象会让这个 effect 无限重跑。
  const beforeRev = before?.rev ?? null;
  const beforePath = before?.path ?? null;
  const afterRev = after?.rev ?? null;
  const afterPath = after?.path ?? null;

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setBeforeView(null);
    setAfterView(null);

    (async () => {
      try {
        const [old, next] = await Promise.all([
          beforeRev && beforePath
            ? getBlobPreview(repoPath, beforeRev, beforePath)
            : Promise.resolve(null),
          afterRev && afterPath
            ? getBlobPreview(repoPath, afterRev, afterPath)
            : Promise.resolve(null),
        ]);
        if (cancelled) return;
        setBeforeView(old);
        setAfterView(next);
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [repoPath, beforeRev, beforePath, afterRev, afterPath]);

  const shown = afterView ?? beforeView;
  const both = beforeView !== null && afterView !== null;

  return (
    <div className="preview-card">
      <div className="preview-header">
        <h3 title={path}>{path}</h3>
        <button type="button" className="ghost" onClick={onClose}>
          关闭
        </button>
      </div>

      {loading && <div className="preview-note">读取内容…</div>}
      {error && <div className="preview-note error">{error}</div>}

      {!loading && !error && shown && (
        <>
          <div className="preview-meta">
            <span className="tag">{kindLabel(shown.kind)}</span>
            <span className="muted">{formatBytes(shown.size)}</span>
            {shown.kind.kind === "lfs_pointer" && (
              <span className="muted" title={shown.kind.oid}>
                指向 {formatBytes(shown.kind.size)}
              </span>
            )}
          </div>

          {previewNotice(shown) && <div className="preview-notice">{previewNotice(shown)}</div>}

          {both ? (
            <div className="preview-pair">
              <BlobFigure label="旧" preview={beforeView} />
              <BlobFigure label="新" preview={afterView} />
            </div>
          ) : (
            <BlobFigure label={beforeView ? "旧" : "新"} preview={shown} />
          )}
        </>
      )}
    </div>
  );
}

/** 一个版本的内容。图片用 `<img>`，文本与转储用 `<pre>`（文本节点，不解析标记）。 */
function BlobFigure({ label, preview }: { label: string; preview: BlobPreview | null }) {
  if (!preview) return null;
  const url = blobDataUrl(preview);

  return (
    <figure className="preview-figure">
      <figcaption>{label}</figcaption>
      {url && <img src={url} alt={`${label} 版本`} />}
      {preview.text !== null && <pre className="preview-text">{preview.text}</pre>}
      {preview.hex !== null && <pre className="preview-hex">{preview.hex}</pre>}
      {!url && preview.text === null && preview.hex === null && (
        <div className="preview-note">这一版没有可显示的内容</div>
      )}
    </figure>
  );
}
