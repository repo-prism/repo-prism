/**
 * TypeScript 侧对着 `contracts/*.json` 校验自己（补丁 P-13）。
 *
 * 这些模型的 JSON 由 Rust 的 serde 决定，前端这份类型此前是手抄的 ——
 * 两边没有任何编译期联系。这份测试把「手抄」变成「对着事实来源比对」：
 *
 * | 改了什么 | 谁红 |
 * |---|---|
 * | Rust 的 `rename_all` / 字段名 | Rust 的 `wire_contract.rs` 先红（要重新生成），重生成后**这里**红 |
 * | Rust 加了 / 删了字段 | 同上 |
 * | Rust 把 `String` 改成 `Option<String>` | 同上（可空性进 contract；运行时会拒，这里比对类型清单） |
 * | 枚举多了一个取值 | 这里（Rust 侧的枚举清单要人同步，见 wire_contract.rs 的说明） |
 * | TS 的键表或类型改了 | 编译期就红（`KeyMap` 要求每个字段都在）或这里红 |
 *
 * # 一条容易写成永真的断言
 *
 * 「TS 的清单 == contract 的清单」只有在**两边来源不同**时才成立。
 * 若把 TS 的清单写成从 contract 派生，这条断言就永远为真 —— 探针 F7 在
 * P-12 上就是这么把一条断言绕过去的。所以这里比对的键表全部来自
 * `fieldsOf(...)`（编译器检查过的手抄清单），形状才来自 contract。
 *
 * # 「破坏用例」为什么遍历每一层形状
 *
 * 校验逻辑是递归的，但**只在顶层破坏**就证明不了递归层真的在校验
 * （WorkspaceInfo 顶层恰好没有字符串字段，只在顶层破坏会直接空转）。
 * 所以下面按形状路径逐层构造破坏，并断言「至少真的破坏了一次」——
 * 一条没被执行过的断言等于没有断言。
 */

import { describe, expect, it } from "vitest";
import analysisContract from "../../contracts/analysis.json";
import blobContract from "../../contracts/blob-preview.json";
import snapshotContract from "../../contracts/snapshot.json";
import workspaceContract from "../../contracts/workspace.json";
import { ANALYSIS_ENUMS, ANALYSIS_KEYS, parseChangeAnalysis } from "./analysis";
import {
  BLOB_KIND_TAGS,
  BLOB_KIND_VARIANTS,
  BLOB_PREVIEW_FIELDS,
  parseBlobPreview,
} from "./blobPreview";
import {
  CHANGE_KINDS,
  parseRepoSnapshot,
  REPO_STATE_VARIANTS,
  SNAPSHOT_ENUMS,
  SNAPSHOT_KEYS,
} from "./snapshot";
import { shapeAt, type VariantSpec, WireContractError, type WireShape } from "./wireContract";
import { parseWorkspaceInfo, WORKSPACE_KEYS } from "./workspaceInfo";

interface Sample {
  name: string;
  expect_tag?: string;
  value: unknown;
}

interface ContractFile {
  shape: WireShape;
  union_tag_path?: string;
  samples: Sample[];
}

interface ManagedModel {
  name: string;
  contract: ContractFile;
  parse: (value: unknown) => unknown;
  /** 形状路径 -> 该形状的字段清单（**手抄**，由编译器检查过）。 */
  keys: Record<string, readonly string[]>;
  /** `形状路径.字段名` -> 允许的取值。变体**内部**的枚举不在这里。 */
  enums?: Record<string, readonly string[]>;
  /** 形状路径 -> { 变体名 -> 该变体的登记 }。 */
  unions?: Record<string, Record<string, VariantSpec>>;
}

// 直接 import 而不是 `node:fs` 读：项目没装 `@types/node`（也不打算为一个 contract
// 文件去装），而 `resolveJsonModule` 已经开着。代价是要绕过 TS 从 JSON 推断出的
// 字面量类型 —— 那些推断出来的东西（尤其 `samples[].value`）过于精确，
// 我们要的是「一条不知道形状的 JSON」。
const asContract = (json: unknown) => json as unknown as ContractFile;

const models: ManagedModel[] = [
  {
    name: "RepoSnapshot",
    contract: asContract(snapshotContract),
    parse: parseRepoSnapshot,
    keys: SNAPSHOT_KEYS,
    enums: SNAPSHOT_ENUMS,
    unions: { state: REPO_STATE_VARIANTS },
  },
  {
    name: "WorkspaceInfo",
    contract: asContract(workspaceContract),
    parse: parseWorkspaceInfo,
    keys: WORKSPACE_KEYS,
  },
  {
    name: "ChangeAnalysis",
    contract: asContract(analysisContract),
    parse: parseChangeAnalysis,
    keys: ANALYSIS_KEYS,
    enums: ANALYSIS_ENUMS,
  },
  {
    name: "BlobPreview",
    contract: asContract(blobContract),
    parse: parseBlobPreview,
    keys: { "": BLOB_PREVIEW_FIELDS },
    unions: { kind: BLOB_KIND_VARIANTS },
  },
];

const sortAlpha = (xs: readonly string[]): string[] => [...xs].sort();

/** 按点分路径取值。用于把「解析出来的 tag」跟 contract 声明的 tag 比。 */
function atPath(value: unknown, path: string): unknown {
  let current: unknown = value;
  for (const segment of path.split(".")) {
    if (typeof current !== "object" || current === null) return undefined;
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

/**
 * 形状路径 -> JSON 路径。
 *
 * 形状里数组字段挂的是**元素**形状，而 JSON 里要写成 `field[0]`。
 */
function toJsonPath(root: WireShape, shapePath: string): string {
  if (shapePath === "") return "";
  const segments: string[] = [];
  let current = root;
  for (const segment of shapePath.split(".")) {
    segments.push(current.types[segment] === "array" ? `${segment}[0]` : segment);
    current = current.shapes[segment];
  }
  return segments.join(".");
}

/** 深拷贝后按 JSON 路径改写一处。路径里的 `[0]` 表示数组首元素。 */
function setAt<T>(value: T, path: string, next: unknown): T {
  const clone = structuredClone(value);
  if (path === "") return next as T;
  const parts = path
    .replace(/\[(\d+)\]/g, ".$1")
    .split(".")
    .filter((part) => part !== "");
  let current = clone as unknown as Record<string, unknown>;
  for (const part of parts.slice(0, -1)) {
    current = current[part] as Record<string, unknown>;
  }
  current[parts[parts.length - 1]] = next;
  return clone;
}

/**
 * union 挂在**哪个形状**上。
 *
 * `state` 是根形状的字段，所以它的 union 描述在根形状的 `unions.state` 里，
 * 而不是在 `shapes.state` —— 形状路径说的是「字段所在的对象」。
 */
function unionOf(
  root: WireShape,
  shapePath: string,
): { union: WireShape["unions"][string]; field: string } | undefined {
  const cut = shapePath.lastIndexOf(".");
  const parentPath = cut < 0 ? "" : shapePath.slice(0, cut);
  const field = cut < 0 ? shapePath : shapePath.slice(cut + 1);
  const parent = shapeAt(root, parentPath);
  const union = parent?.unions[field];
  return union === undefined ? undefined : { union, field };
}

describe("受管模型登记表本身", () => {
  it("四个模型都在表里 —— 表空了不能让下面的循环悄悄变绿", () => {
    expect(models.map((m) => m.name)).toEqual([
      "RepoSnapshot",
      "WorkspaceInfo",
      "ChangeAnalysis",
      "BlobPreview",
    ]);
  });

  it("每个模型都至少登记了顶层字段清单", () => {
    for (const model of models) {
      expect(model.keys[""]?.length ?? 0, model.name).toBeGreaterThan(0);
    }
  });
});

describe("键表与 contract 的字段集合一致", () => {
  for (const model of models) {
    for (const [path, expected] of Object.entries(model.keys)) {
      it(`${model.name} 的 ${path === "" ? "顶层" : path} 字段一字不差`, () => {
        const shape = shapeAt(model.contract.shape, path);
        expect(shape, `contract 里没有形状 ${path}`).toBeDefined();
        // 两边都排序：Rust 侧是字母序，TS 侧是声明顺序。
        expect(sortAlpha(shape?.fields ?? [])).toEqual(sortAlpha(expected));
      });
    }
  }
});

describe("枚举取值与 contract 一致", () => {
  for (const model of models) {
    for (const [dotted, expected] of Object.entries(model.enums ?? {})) {
      it(`${model.name} 的 ${dotted} 取值一字不差`, () => {
        const cut = dotted.lastIndexOf(".");
        const shapePath = dotted.slice(0, cut);
        const field = dotted.slice(cut + 1);
        const actual = shapeAt(model.contract.shape, shapePath)?.enums[field];
        expect(actual, `contract 里没有枚举 ${dotted}`).toBeDefined();
        expect(sortAlpha(actual ?? [])).toEqual(sortAlpha(expected));
      });
    }
  }
});

describe("tagged union 与 contract 一致", () => {
  for (const model of models) {
    for (const [path, variants] of Object.entries(model.unions ?? {})) {
      it(`${model.name} 的 ${path} 变体集合一字不差`, () => {
        const union = unionOf(model.contract.shape, path)?.union;
        expect(union, `contract 里 ${path} 不是 union`).toBeDefined();
        expect(sortAlpha(Object.keys(union?.variants ?? {}))).toEqual(
          sortAlpha(Object.keys(variants)),
        );
      });

      for (const [tag, spec] of Object.entries(variants)) {
        it(`${model.name} 的 ${path}（${tag}）字段一字不差`, () => {
          const variant = unionOf(model.contract.shape, path)?.union.variants[tag];
          expect(variant, `contract 里没有变体 ${tag}`).toBeDefined();
          expect(sortAlpha(variant?.fields ?? [])).toEqual(sortAlpha(spec.fields));
        });

        for (const [field, expected] of Object.entries(spec.enums ?? {})) {
          it(`${model.name} 的 ${path}（${tag}）的 ${field} 取值一字不差`, () => {
            const variant = unionOf(model.contract.shape, path)?.union.variants[tag];
            const actual = variant?.enums[field];
            expect(actual, `contract 里没有 ${tag}.${field}`).toBeDefined();
            expect(sortAlpha(actual ?? [])).toEqual(sortAlpha(expected));
          });
        }
      }
    }
  }
});

describe("contract 里的每个样本都能被 TypeScript 侧解析", () => {
  for (const model of models) {
    it(`${model.name} 的 ${model.contract.samples.length} 个样本全部通过`, () => {
      expect(model.contract.samples.length).toBeGreaterThan(0);
      for (const sample of model.contract.samples) {
        expect(() => model.parse(sample.value), `${model.name}/${sample.name}`).not.toThrow();
      }
    });
  }

  it("声明了 expect_tag 的样本，解析出来的 tag 与 Rust 声明的一致", () => {
    let checked = 0;
    for (const model of models) {
      const tagPath = model.contract.union_tag_path;
      for (const sample of model.contract.samples) {
        if (sample.expect_tag === undefined) continue;
        // 没有 union 的模型不该出现 expect_tag —— 那说明 Rust 侧的样本登记错了
        expect(
          tagPath,
          `${model.name}/${sample.name} 有 expect_tag 却没有 union_tag_path`,
        ).toBeDefined();
        expect(
          atPath(model.parse(sample.value), tagPath ?? ""),
          `${model.name}/${sample.name}`,
        ).toBe(sample.expect_tag);
        checked += 1;
      }
    }
    expect(checked).toBeGreaterThan(0);
  });

  it("每个 union 变体都至少有一个样本 —— 否则上面那条会漏掉某一类", () => {
    let checked = 0;
    for (const model of models) {
      const tags = new Set(
        model.contract.samples.map((s) => s.expect_tag).filter((t): t is string => t !== undefined),
      );
      for (const [path, variants] of Object.entries(model.unions ?? {})) {
        for (const tag of Object.keys(variants)) {
          expect(tags.has(tag), `${model.name} 的 ${path} 变体 ${tag} 没有样本`).toBe(true);
          checked += 1;
        }
      }
    }
    expect(checked).toBeGreaterThan(0);
  });
});

describe("解析器拒不认识的形状", () => {
  for (const model of models) {
    const base = model.contract.samples[0]?.value as Record<string, unknown>;

    it(`${model.name}：多一个字段就报错 —— 陌生字段不能被静默丢掉`, () => {
      expect(() => model.parse({ ...base, brand_new_field: 1 })).toThrow(WireContractError);
    });

    it(`${model.name}：少一个字段也报错`, () => {
      const [first] = model.contract.shape.fields;
      const { [first]: _removed, ...kept } = base;
      expect(() => model.parse(kept)).toThrow(WireContractError);
    });

    it(`${model.name}：null 不算对象`, () => {
      expect(() => model.parse(null)).toThrow(WireContractError);
    });

    it(`${model.name}：每一层形状里，该是字符串的位置收到数字都会报错`, () => {
      let checked = 0;
      for (const path of Object.keys(model.keys)) {
        const shape = shapeAt(model.contract.shape, path);
        const field = shape?.fields.find(
          (f) => shape.types[f] === "string" && shape.enums[f] === undefined,
        );
        if (field === undefined) continue;
        const jsonPath = [toJsonPath(model.contract.shape, path), field]
          .filter((part) => part !== "")
          .join(".");
        expect(() => model.parse(setAt(base, jsonPath, 12345))).toThrow(WireContractError);
        checked += 1;
      }
      // 顶层没有字符串字段的模型（WorkspaceInfo）不能让这条悄悄空转
      expect(checked, `${model.name} 一次都没破坏成功`).toBeGreaterThan(0);
    });

    it(`${model.name}：每一层形状里，不可为空的字段收到 null 都会报错`, () => {
      let checked = 0;
      for (const path of Object.keys(model.keys)) {
        const shape = shapeAt(model.contract.shape, path);
        const field = shape?.fields.find((f) => !shape.nullable.includes(f));
        if (field === undefined) continue;
        const jsonPath = [toJsonPath(model.contract.shape, path), field]
          .filter((part) => part !== "")
          .join(".");
        expect(() => model.parse(setAt(base, jsonPath, null))).toThrow(WireContractError);
        checked += 1;
      }
      expect(checked, `${model.name} 一次都没破坏成功`).toBeGreaterThan(0);
    });

    it(`${model.name}：错误信息里会说明是哪个字段，而不是笼统地说解析失败`, () => {
      expect(() => model.parse({ ...base, brand_new_field: 1 })).toThrow(/brand_new_field/);
    });
  }

  describe("union 特有的拒绝", () => {
    const tagged = models.filter((m) => m.unions !== undefined);

    it("带 union 的模型是 RepoSnapshot 与 BlobPreview —— 表空了不能让下面空转", () => {
      expect(tagged.map((m) => m.name).sort()).toEqual(["BlobPreview", "RepoSnapshot"]);
    });

    for (const model of tagged) {
      for (const path of Object.keys(model.unions ?? {})) {
        it(`${model.name} 的 ${path}：未知 tag 会被拒绝`, () => {
          const base = model.contract.samples[0]?.value as Record<string, unknown>;
          expect(() => model.parse({ ...base, [path]: { kind: "something_new" } })).toThrow(
            WireContractError,
          );
        });

        it(`${model.name} 的 ${path}：每个变体少了该有的字段都会被拒绝`, () => {
          const base = model.contract.samples[0]?.value as Record<string, unknown>;
          const variants = model.unions?.[path] ?? {};
          let checked = 0;

          for (const [tag, spec] of Object.entries(variants)) {
            // 用 contract 里该变体**真实的**样本，而不是拼出来的值：
            // 拼出来的值可能先倒在枚举校验上，那就测不到「少字段」这件事。
            const sample = model.contract.samples.find((s) => s.expect_tag === tag);
            expect(sample, `${model.name} 的 ${tag} 没有样本`).toBeDefined();
            if (sample === undefined) continue; // 上一行已经断言过，这里只为收窄类型
            const intact = (sample.value as Record<string, unknown>)[path];
            expect(() => model.parse({ ...base, [path]: intact })).not.toThrow();

            const dropable = spec.fields.find((f) => f !== "kind");
            if (dropable === undefined) continue; // 只有 tag 的变体没得删
            const broken = { ...(intact as Record<string, unknown>) };
            delete broken[dropable];
            expect(() => model.parse({ ...base, [path]: broken })).toThrow(WireContractError);
            checked += 1;
          }
          expect(checked, `${model.name} 没有可删字段的变体`).toBeGreaterThan(0);
        });
      }
    }
  });
});

describe("几个清单本身", () => {
  it("ChangeKind 的取值与 contract 里的一致（三个数组字段共用同一份）", () => {
    const shape = shapeAt(asContract(snapshotContract).shape, "status.conflicts");
    expect(sortAlpha(shape?.enums.kind ?? [])).toEqual(sortAlpha(CHANGE_KINDS));
  });

  it("BlobPreview 的 kind tag 清单覆盖 5 类", () => {
    expect(sortAlpha(BLOB_KIND_TAGS)).toEqual([
      "binary",
      "image",
      "lfs_pointer",
      "text",
      "too_large",
    ]);
  });
});
