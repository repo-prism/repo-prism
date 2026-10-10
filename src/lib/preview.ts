import type { BlobKind, BlobPreview, ChangeKind, ImageFormat } from "./api";
import { MAX_PREVIEW_BYTES } from "./blobPreview";

/**
 * 把字节上限显示成人能读的形式：`4194304` → `4 MiB`。
 *
 * 参数而不是写死的字符串：上限值本身与 Rust 的 `MAX_PREVIEW_BYTES`
 * 由 contract 闸门对着管（补丁 P-12），文案要跟着它变。
 */
export function formatByteLimit(bytes: number): string {
  const mib = bytes / (1024 * 1024);
  return `${Number.isInteger(mib) ? mib : mib.toFixed(1)} MiB`;
}

/**
 * `<img>` 需要的 MIME 类型（US-3，补丁 P-10）。
 *
 * 必须与 `ImageFormat` 一一对应：新增一种格式时这个 `switch` 会先报类型错误。
 */
export function mediaType(format: ImageFormat): string {
  switch (format) {
    case "png":
      return "image/png";
    case "jpeg":
      return "image/jpeg";
    case "gif":
      return "image/gif";
    case "webp":
      return "image/webp";
    case "bmp":
      return "image/bmp";
    case "svg":
      return "image/svg+xml";
  }
}

/**
 * 图片的 data URL。**只有图片才有值** —— 其余种类一律返回 `null`，
 * 免得调用方拿一段文本去喂 `<img>`（那会静默显示成一个破图）。
 *
 * SVG 走的是同一条路径，这一点很关键：**只有 `<img>` 上下文会禁用 SVG 里的
 * 脚本与外部引用**。把它 inline 进 DOM 就等于执行仓库作者写的代码
 * （`SECURITY.md` 威胁 3）。
 */
export function blobDataUrl(preview: BlobPreview): string | null {
  if (preview.kind.kind !== "image") return null;
  const content = preview.content;
  if (!content) return null;
  return `data:${mediaType(preview.kind.format)};base64,${content}`;
}

/** 预览面板的标题：这一栏在说什么。 */
export function kindLabel(kind: BlobKind): string {
  switch (kind.kind) {
    case "image":
      return `图片 · ${kind.format.toUpperCase()}`;
    case "text":
      return "文本";
    case "binary":
      return "二进制";
    case "lfs_pointer":
      return "LFS 指针";
    case "too_large":
      return "超出预览上限";
  }
}

/**
 * 需要向用户说明的限制。返回 `null` 表示没什么要解释的。
 *
 * 三条文案各自对应一种「看到的不是全部」的情形，必须分开说：
 * 「没读」与「读了但截断」混在一起，用户会以为看到的就是完整内容。
 */
export function previewNotice(preview: BlobPreview): string | null {
  if (preview.too_large) {
    return `这个文件超过 ${formatByteLimit(MAX_PREVIEW_BYTES)} 的预览上限，内容没有被读取（只取了大小）。`;
  }
  if (preview.kind.kind === "lfs_pointer") {
    return "这是一个 Git LFS 指针：真实内容不在仓库里，也没有被下载。";
  }
  if (preview.truncated) {
    return "内容过长，只显示了前面一部分；下面给出的是它**完整**的大小。";
  }
  return null;
}

/**
 * 一次预览要取哪几个版本（US-3 的「图片对比」）。
 *
 * 旧版本取自**父提交** —— 重命名 / 复制时旧路径与新路径不同，
 * 两边都要用各自的路径，否则会取到一个不存在的 spec。
 * 根提交没有父提交，此时只有新的一侧（也不存在「对比」这回事）。
 */
export function previewSides(
  kind: ChangeKind,
  path: string,
  oldPath: string | null,
  sha: string,
  parents: string[],
): { before: Target | null; after: Target | null } {
  const parent = parents[0] ?? null;
  const beforePath = oldPath ?? path;

  if (kind === "added") {
    return { before: null, after: { rev: sha, path } };
  }
  if (kind === "deleted") {
    return { before: parent ? { rev: parent, path: beforePath } : null, after: null };
  }
  return {
    before: parent ? { rev: parent, path: beforePath } : null,
    after: { rev: sha, path },
  };
}

export interface Target {
  rev: string;
  path: string;
}
