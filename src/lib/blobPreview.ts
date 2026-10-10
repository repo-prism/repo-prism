/**
 * `BlobPreview` 的 TypeScript 侧类型，以及**跨语言线格式契约**（补丁 P-12）。
 *
 * # 这些类型为什么搬出 `api.ts`
 *
 * 它们此前只是「接口塑料」：`BlobPreview` 的 JSON 由 Rust 的 serde 决定，
 * 前端这份是手抄的，两边没有任何联系。改一个 `rename_all` 或字段名会
 * Rust 测试全绿、UI 静默变空白 —— 而 UI 接线层正好是全项目唯一没有自动化
 * 测试的地方。
 *
 * 现在 `contracts/blob-preview.json` 是唯一事实来源：它由 Rust 侧生成
 * （`cargo test -p repo-prism-core --test wire_contract -- --ignored`），
 * 由 `blobPreview.test.ts` 与下面这些清单逐项比对。
 *
 * # 下面这些常量不是注释，是运行时判据
 *
 * `BLOB_PREVIEW_FIELDS` / `BLOB_KIND_PROPS` / `IMAGE_FORMATS` 被
 * `parseBlobPreview` 真拿来校验每一条进来的 JSON，所以清单与解析器
 * 不可能各自漂移 —— 写在这里一份，两端都用。
 */

export type ImageFormat = "png" | "jpeg" | "gif" | "webp" | "bmp" | "svg";

/** `ImageFormat` 在线上的取值。新增一种格式时 Rust 侧会先让 contract 文件变化。 */
export const IMAGE_FORMATS = ["png", "jpeg", "gif", "webp", "bmp", "svg"] as const;

/** kind 的判别式在线上的取值。`rename_all = "snake_case"` 决定了它是蛇形。 */
export const BLOB_KIND_TAGS = ["image", "text", "binary", "lfs_pointer", "too_large"] as const;

export type BlobKindTag = (typeof BLOB_KIND_TAGS)[number];

/**
 * 顶层字段。**与 Rust 的声明顺序无关**（`serde_json::Value::Object` 是字母序），
 * 这里也按字典序排，比对时集合相等即可。
 */
export const BLOB_PREVIEW_FIELDS = [
  "content",
  "hex",
  "kind",
  "size",
  "text",
  "too_large",
  "truncated",
] as const;

/** 每种 kind 在线上的全部字段，含判别式 `kind` 本身。 */
export const BLOB_KIND_PROPS: Record<BlobKindTag, readonly string[]> = {
  image: ["format", "kind"],
  text: ["kind"],
  binary: ["kind"],
  lfs_pointer: ["kind", "oid", "size"],
  too_large: ["kind"],
};

/**
 * 读取上限。**必须与 Rust 的 `MAX_PREVIEW_BYTES` 一致** ——
 * 界面上那句「超过 X MiB 的预览上限」用的是它。
 * 这条一致性由 `blobPreview.test.ts` 对着 contract 文件断言，不靠人记着改两处。
 */
export const MAX_PREVIEW_BYTES = 4 * 1024 * 1024;

/** 用可辨识联合，`switch` 漏掉一类会在类型层面报错。 */
export type BlobKind =
  | { kind: "image"; format: ImageFormat }
  | { kind: "text" }
  | { kind: "binary" }
  | { kind: "lfs_pointer"; oid: string; size: number }
  | { kind: "too_large" };

export interface BlobPreview {
  /** 仓库里该文件的**真实**字节数。永远给出，即使内容一个字节都没读。 */
  size: number;
  kind: BlobKind;
  /** base64 编码的原始字节（图片）。 */
  content: string | null;
  /** 文本预览（文本）。 */
  text: string | null;
  /** 十六进制转储（未知二进制）。 */
  hex: string | null;
  /** 读了，但呈现时截断。 */
  truncated: boolean;
  /** 超过上限，**一个字节都没读**。 */
  too_large: boolean;
}

/**
 * 后端返回的东西与本文件声明的类型不一致。
 *
 * 这是**契约破了**，不是「读取失败」：把它当成一次普通的 IO 错误重试没有意义，
 * 所以单独一个类型。`getBlobPreview` 的调用方会把它的文案显示给用户 ——
 * 与其静默显示一块空白，不如直说哪里不对。
 */
export class WireContractError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "WireContractError";
  }
}

function typeName(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "数组";
  return typeof value;
}

function asObject(value: unknown, where: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new WireContractError(`${where} 应是一个 JSON 对象，实际是 ${typeName(value)}`);
  }
  return value as Record<string, unknown>;
}

/**
 * 字段集合必须**精确**相等：少了说明 Rust 那边删了字段，
 * 多了说明 Rust 那边加了字段而前端还没跟上。
 *
 * 两个方向都要管 —— 「多一个」恰恰是最容易静默过去的那一个。
 */
function requireExactShape(
  obj: Record<string, unknown>,
  allowed: readonly string[],
  where: string,
): void {
  const missing = allowed.filter((key) => !(key in obj));
  if (missing.length > 0) {
    throw new WireContractError(`${where} 缺少字段：${missing.join(" / ")}`);
  }
  const extra = Object.keys(obj).filter((key) => !(allowed as readonly string[]).includes(key));
  if (extra.length > 0) {
    throw new WireContractError(
      `${where} 出现未知字段：${extra.join(" / ")} —— 后端加了前端还不认识的字段，` +
        `多半是 contract 文件还没同步（见 src/lib/blobPreview.ts 顶部说明）`,
    );
  }
}

function requireNumber(value: unknown, where: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new WireContractError(`${where} 应是数字，实际是 ${typeName(value)}`);
  }
  return value;
}

function requireBoolean(value: unknown, where: string): boolean {
  if (typeof value !== "boolean") {
    throw new WireContractError(`${where} 应是布尔值，实际是 ${typeName(value)}`);
  }
  return value;
}

function requireNullableString(value: unknown, where: string): string | null {
  if (value === null) return null;
  if (typeof value !== "string") {
    throw new WireContractError(`${where} 应是字符串或 null，实际是 ${typeName(value)}`);
  }
  return value;
}

function isBlobKindTag(value: string): value is BlobKindTag {
  return (BLOB_KIND_TAGS as readonly string[]).includes(value);
}

function isImageFormat(value: string): value is ImageFormat {
  return (IMAGE_FORMATS as readonly string[]).includes(value);
}

/**
 * 把后端返回的 JSON 解析成 `BlobPreview`，**严格校验收到的每一条字段**。
 *
 * 放在 `getBlobPreview` 的必经路径上而不是只在测试里用：那样清单就只是注释，
 * 而「注释会过时」正是 P-11 那份纯 Rust 测试遇到的同一个问题。
 *
 * 契约破了会抛 `WireContractError`，面板把它显示成一行提示；
 * 静默显示一块空白更难排查。
 */
export function parseBlobPreview(value: unknown): BlobPreview {
  const obj = asObject(value, "BlobPreview");
  requireExactShape(obj, BLOB_PREVIEW_FIELDS, "BlobPreview");

  const kindObj = asObject(obj.kind, "BlobPreview.kind");
  const tag = kindObj.kind;
  if (typeof tag !== "string" || !isBlobKindTag(tag)) {
    throw new WireContractError(`未知的 BlobPreview.kind：${String(tag)}`);
  }
  requireExactShape(kindObj, BLOB_KIND_PROPS[tag], `BlobPreview.kind（${tag}）`);

  let kind: BlobKind;
  switch (tag) {
    case "image": {
      const format = kindObj.format;
      if (typeof format !== "string" || !isImageFormat(format)) {
        throw new WireContractError(`未知的图片格式：${String(format)}`);
      }
      kind = { kind: "image", format };
      break;
    }
    case "lfs_pointer":
      kind = {
        kind: "lfs_pointer",
        oid: requireNullableString(kindObj.oid, "BlobPreview.kind.oid") ?? "",
        size: requireNumber(kindObj.size, "BlobPreview.kind.size"),
      };
      break;
    case "text":
      kind = { kind: "text" };
      break;
    case "binary":
      kind = { kind: "binary" };
      break;
    case "too_large":
      kind = { kind: "too_large" };
      break;
  }

  return {
    size: requireNumber(obj.size, "BlobPreview.size"),
    kind,
    content: requireNullableString(obj.content, "BlobPreview.content"),
    text: requireNullableString(obj.text, "BlobPreview.text"),
    hex: requireNullableString(obj.hex, "BlobPreview.hex"),
    truncated: requireBoolean(obj.truncated, "BlobPreview.truncated"),
    too_large: requireBoolean(obj.too_large, "BlobPreview.too_large"),
  };
}
