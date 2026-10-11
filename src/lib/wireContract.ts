/**
 * 跨语言线格式的**通用**解析器（补丁 P-13）。
 *
 * # 这份文件解决什么
 *
 * 这些模型的 JSON 形状由 Rust 的 serde 决定，前端这份类型是手抄的，两边没有任何
 * 编译期联系。P-12 给 `BlobPreview` 建了一条闸门（`contracts/blob-preview.json`），
 * 但它的解析器是**为那一个模型手写**的。推广到四个模型时再抄三遍，抄错的
 * 概率远高于第一遍 —— 而抄错的地方正是闸门本身。
 *
 * 所以这里是通用的：形状从 `contracts/*.json` 读进来，校验逻辑只有一份。
 *
 * # 契约的形状从哪来
 *
 * 直接 import contract 文件。这不是偷懒，是**故意的**：
 *
 * - 形状已经有一个唯一事实来源，再手抄一份就会回到两边分叉的老路；
 * - 手抄的那份需要在测试里跟 contract 比对，而通用解析器本身就吃 contract，
 *   于是「运行时判据」与「比对对象」是同一个东西 —— 清单不可能只是注释。
 *
 * 那 TypeScript 的 `interface` 还剩什么用？它是**编译期**的类型，靠
 * `KeyMap` / `fieldsOf`（下面）强制它跟 contract 的字段集合一致：
 * 漏一个字段或多一个字段都是编译错误。
 *
 * # 校验到什么程度
 *
 * - 字段集合**精确**相等（少了是后端删了，多了是后端加了前端还没跟上）
 * - 每个字段的 JSON 类型与可空性
 * - 枚举字段的取值必须落在清单里
 * - tagged union 的判别式取值，以及该变体的字段集合
 *
 * 没覆盖的：**字段的 TypeScript 类型**（`string` 还是 `number`）仍是手抄的。
 * 这是当前机制的残余缺口，见 `TASKS/patch/P-13-wire-contract-rollout.md`。
 */

/** 一个值在**非空**时的 JSON 类型。与 Rust 侧 `json_type()` 的取值一一对应。 */
export type JsonType = "string" | "number" | "boolean" | "array" | "object";

export interface WireUnion {
  /** 判别式的字段名。本项目两个 tagged enum 都用 `kind`。 */
  tag: string;
  /** 判别式取值 -> 该变体的形状。 */
  variants: Record<string, WireShape>;
}

export interface WireShape {
  /** 字段集合。**排序后**的，与 TypeScript 侧的键表比对时两边都要排。 */
  fields: readonly string[];
  types: Record<string, JsonType>;
  /** 线上确实会出现 `null` 的字段。由 Rust 侧的反序列化探测得出。 */
  nullable: readonly string[];
  /** 嵌套对象的形状；数组字段放的是**元素**的形状。 */
  shapes: Record<string, WireShape>;
  /** 字段 -> 允许的取值（unit enum 的全部变体）。 */
  enums: Record<string, readonly string[]>;
  /** 字段 -> tagged union 的描述。 */
  unions: Record<string, WireUnion>;
}

/**
 * 后端返回的东西与本文件声明的类型不一致。
 *
 * 这是**契约破了**，不是「读取失败」：把它当成一次普通的 IO 错误重试没有意义，
 * 所以单独一个类型。调用方会把它的文案显示给用户 —— 与其静默渲染出一块空白，
 * 不如直说哪里不对。
 */
export class WireContractError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "WireContractError";
  }
}

// ---------------------------------------------------------------------------
// TypeScript 侧的键表
//
// `interface` 是编译期的，运行时拿不到它的键。下面这两个东西把键带到运行时，
// 同时**让编译器反过来检查这份键表**：漏一个字段、多一个字段都是编译错误。
// ---------------------------------------------------------------------------

/**
 * 要求 `T` 的**每一个**键都在，且只多不少。
 *
 * 分两种：`T` 是字符串字面量联合（枚举）时按取值要求；是 `interface` 时按
 * 字段要求。写成一个类型是为了让 `fieldsOf` 只有一份签名 ——
 * 分成两个函数就会有人调错那个。
 */
export type KeyMap<T> = [T] extends [string]
  ? { [K in T & string]-?: true }
  : { [K in keyof T]-?: true };

/** 从键表取出排序后的字段名。 */
export function fieldsOf<T>(map: KeyMap<T>): string[] {
  return Object.keys(map).sort();
}

/**
 * 一个 tagged union 变体在 TypeScript 侧的登记。
 *
 * `enums` 是**变体内部**的枚举字段（例如 `BlobKind::Image.format`）——
 * 它不在顶层形状里，没法用「形状路径.字段名」表达，所以挂在变体上。
 */
export interface VariantSpec {
  fields: readonly string[];
  enums?: Record<string, readonly string[]>;
}

/** 按点分路径在形状里找子形状。数组字段的子形状就是它的元素形状。 */
export function shapeAt(root: WireShape, path: string): WireShape | undefined {
  if (path === "") return root;
  let current: WireShape | undefined = root;
  for (const segment of path.split(".")) {
    current = current?.shapes[segment];
    if (current === undefined) return undefined;
  }
  return current;
}

// ---------------------------------------------------------------------------
// 校验
// ---------------------------------------------------------------------------

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
  const extra = Object.keys(obj).filter((key) => !allowed.includes(key));
  if (extra.length > 0) {
    throw new WireContractError(
      `${where} 出现未知字段：${extra.join(" / ")} —— 后端加了前端还不认识的字段，` +
        `多半是 contract 文件还没同步（见 src/lib/wireContract.ts 顶部说明）`,
    );
  }
}

function checkScalar(value: unknown, type: JsonType, where: string): void {
  switch (type) {
    case "string":
      if (typeof value !== "string") {
        throw new WireContractError(`${where} 应是字符串，实际是 ${typeName(value)}`);
      }
      return;
    case "number":
      if (typeof value !== "number" || !Number.isFinite(value)) {
        throw new WireContractError(`${where} 应是数字，实际是 ${typeName(value)}`);
      }
      return;
    case "boolean":
      if (typeof value !== "boolean") {
        throw new WireContractError(`${where} 应是布尔值，实际是 ${typeName(value)}`);
      }
      return;
    default:
      // "array" / "object" 在调用方处理
      return;
  }
}

function checkUnion(union: WireUnion, value: unknown, where: string): void {
  const obj = asObject(value, where);
  const tag = obj[union.tag];
  if (typeof tag !== "string") {
    throw new WireContractError(
      `${where}.${union.tag} 应是字符串形式的判别式，实际是 ${typeName(tag)}`,
    );
  }
  const variant = union.variants[tag];
  if (variant === undefined) {
    throw new WireContractError(
      `${where}.${union.tag} 的取值 ${tag} 不在契约清单里（已知：${Object.keys(union.variants).join(" / ")}）`,
    );
  }
  checkShape(variant, value, `${where}（${tag}）`);
}

function checkShape(shape: WireShape, value: unknown, where: string): void {
  const obj = asObject(value, where);
  requireExactShape(obj, shape.fields, where);

  for (const field of shape.fields) {
    const at = `${where}.${field}`;
    const raw = obj[field];

    // 可空性先判：union 字段本身也可能整体是 null（`RepoSnapshot.state` 就是）。
    if (raw === null) {
      if (!shape.nullable.includes(field)) {
        throw new WireContractError(`${at} 不该是 null —— 契约里这个字段不可为空`);
      }
      continue;
    }

    const union = shape.unions[field];
    if (union !== undefined) {
      checkUnion(union, raw, at);
      continue;
    }

    const allowed = shape.enums[field];
    if (allowed !== undefined) {
      if (typeof raw !== "string" || !allowed.includes(raw)) {
        throw new WireContractError(
          `${at} 的取值 ${String(raw)} 不在契约清单里（已知：${allowed.join(" / ")}）`,
        );
      }
      continue;
    }

    const type = shape.types[field];
    if (type === "array") {
      if (!Array.isArray(raw)) {
        throw new WireContractError(`${at} 应是数组，实际是 ${typeName(raw)}`);
      }
      const element = shape.shapes[field];
      if (element !== undefined) {
        raw.forEach((item, index) => {
          checkShape(element, item, `${at}[${index}]`);
        });
      }
      continue;
    }
    if (type === "object") {
      const nested = shape.shapes[field];
      if (nested !== undefined) {
        checkShape(nested, raw, at);
        continue;
      }
    }
    checkScalar(raw, type, at);
  }
}

/**
 * 把后端返回的 JSON 按 contract 里的形状**严格**校验一遍。
 *
 * 放在每个 `getXxx` 的必经路径上，而不是只在测试里用：那样形状就只是注释，
 * 而「注释会过时」正是这条闸门要解决的问题。
 *
 * 契约破了会抛 `WireContractError`；三个调用点都有 `catch`，会把文案显示出来。
 */
export function parseWire<T>(shape: WireShape, value: unknown, label: string): T {
  checkShape(shape, value, label);
  return value as T;
}
