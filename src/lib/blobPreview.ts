/**
 * `BlobPreview` 的 TypeScript 侧类型，以及**跨语言线格式契约**（补丁 P-12，
 * P-13 起改用通用解析器）。
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
 * 由 `wire.test.ts` 与下面这些键表逐项比对。
 *
 * # 手抄的只有三样
 *
 * 1. **字段名**（`fieldsOf` —— 漏一个或多一个都是编译错误）
 * 2. **枚举取值**（`ImageFormat` / kind 的 tag —— 编译器检查）
 * 3. **`MAX_PREVIEW_BYTES`**（见下面它自己的注释，那里解释了为什么**故意**
 *    不从 contract 派生）
 *
 * 形状本身（哪个字段是数组、哪个可为空、嵌套长什么样）从 contract 读，
 * 再由 `parseWire` 真拿来校验每一条进来的 JSON。
 */

import contractJson from "../../contracts/blob-preview.json";
import { fieldsOf, type KeyMap, parseWire, type VariantSpec, type WireShape } from "./wireContract";

const contract = contractJson as unknown as {
  shape: WireShape;
  constants: { max_preview_bytes: number };
};

export type ImageFormat = "png" | "jpeg" | "gif" | "webp" | "bmp" | "svg";

const IMAGE_FORMAT_MAP: KeyMap<ImageFormat> = {
  png: true,
  jpeg: true,
  gif: true,
  webp: true,
  bmp: true,
  svg: true,
};

/** `ImageFormat` 在线上的取值。 */
export const IMAGE_FORMATS: readonly string[] = fieldsOf(IMAGE_FORMAT_MAP);

/** 用可辨识联合，`switch` 漏掉一类会在类型层面报错。 */
export type BlobKind =
  | { kind: "image"; format: ImageFormat }
  | { kind: "text" }
  | { kind: "binary" }
  | { kind: "lfs_pointer"; oid: string; size: number }
  | { kind: "too_large" };

/**
 * 每种 kind 的字段清单（含判别式 `kind` 本身）。
 *
 * `Extract<BlobKind, { kind: "…" }>` 让编译器检查它：变体加字段而这里没跟上
 * ⇒ 编译错误。
 */
export const BLOB_KIND_VARIANTS: Record<string, VariantSpec> = {
  image: {
    fields: fieldsOf<Extract<BlobKind, { kind: "image" }>>({ kind: true, format: true }),
    // `format` 只在 image 这个变体里出现，所以挂在变体上而不是顶层形状。
    enums: { format: IMAGE_FORMATS },
  },
  text: { fields: fieldsOf<Extract<BlobKind, { kind: "text" }>>({ kind: true }) },
  binary: { fields: fieldsOf<Extract<BlobKind, { kind: "binary" }>>({ kind: true }) },
  lfs_pointer: {
    fields: fieldsOf<Extract<BlobKind, { kind: "lfs_pointer" }>>({
      kind: true,
      oid: true,
      size: true,
    }),
  },
  too_large: { fields: fieldsOf<Extract<BlobKind, { kind: "too_large" }>>({ kind: true }) },
};

/** kind 的判别式在线上的取值。`rename_all = "snake_case"` 决定了它是蛇形。 */
export const BLOB_KIND_TAGS: readonly string[] = Object.keys(BLOB_KIND_VARIANTS).sort();

export type BlobKindTag = Extract<BlobKind, { kind: string }>["kind"];

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

/** 顶层字段清单。**与 Rust 的声明顺序无关**，两边比对时都排序。 */
export const BLOB_PREVIEW_FIELDS: readonly string[] = fieldsOf<BlobPreview>({
  size: true,
  kind: true,
  content: true,
  text: true,
  hex: true,
  truncated: true,
  too_large: true,
});

/**
 * 读取上限。**故意不从 contract 派生**。
 *
 * 若写成 `contract.constants.max_preview_bytes`，那「TS 的上限 == Rust 的上限」
 * 就变成一个**永真**的断言 —— Rust 改了上限，TS 自动跟着改，测试永远不会红，
 * 界面文案也就悄悄变了（探针 F7 就是这么把原来那条断言绕过去的）。
 *
 * 这里必须是**另一个独立的数**，测试才比得出「两者一致」。
 */
export const MAX_PREVIEW_BYTES = 4 * 1024 * 1024;

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
  return parseWire<BlobPreview>(contract.shape, value, "BlobPreview");
}
