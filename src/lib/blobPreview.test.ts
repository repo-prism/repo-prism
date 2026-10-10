/**
 * TypeScript 侧对着 `contracts/blob-preview.json` 校验自己（补丁 P-12）。
 *
 * `BlobPreview` 的 JSON 由 Rust 的 serde 决定，前端这份类型此前是手抄的 ——
 * 两边没有任何编译期联系。这份测试把「手抄」变成「对着事实来源比对」，错的那一侧会红。
 *
 * 出问题的方向永远不止一个：这些断言要保证**四个**漂移方向都会红 ——
 *
 * | 改了什么 | 谁红 |
 * |---|---|
 * | Rust 的 `rename_all` / 字段名 | Rust 的 `wire_contract.rs` 先红（要重新生成），重生成后**这里**红 |
 * | Rust 加了 / 删了字段 | 同上 |
 * | `MAX_PREVIEW_BYTES` 变了 | 这里（`preview.ts` 的文案跟着走） |
 * | TS 的清单或类型改了 | 这里 |
 *
 * 契约不会因为「两边都改了」而悄悄通过：contract 文件在中间，两边都得对它。
 */

import { describe, expect, it, vi } from "vitest";
import contractJson from "../../contracts/blob-preview.json";
import {
  BLOB_KIND_PROPS,
  BLOB_KIND_TAGS,
  BLOB_PREVIEW_FIELDS,
  IMAGE_FORMATS,
  MAX_PREVIEW_BYTES,
  parseBlobPreview,
  WireContractError,
} from "./blobPreview";
import { formatByteLimit, previewNotice } from "./preview";

interface Sample {
  name: string;
  expect_tag: string;
  value: unknown;
}

interface Contract {
  fields: string[];
  kinds: Record<string, { props: string[] }>;
  image_formats: string[];
  max_preview_bytes: number;
  samples: Sample[];
}

// 直接 import 而不是 `node:fs` 读：项目没装 `@types/node`（也不打算为一个 contract
// 文件去装），而 `resolveJsonModule` 已经开着。代价是要绕过 TS 从 JSON 推断出的
// 字面量类型 —— 那些推断出来的东西（尤其 `samples[].value`）过于精确，
// 我们要的是「一条不知道形状的 JSON」。
const contract = contractJson as unknown as Contract;

const sortAlpha = (xs: readonly string[]): string[] => [...xs].sort();

describe("BlobPreview 的线格式契约", () => {
  it("顶层字段与 Rust 的一字不差", () => {
    expect(sortAlpha(BLOB_PREVIEW_FIELDS)).toEqual(sortAlpha(contract.fields));
  });

  it("kind 的判别式集合与 Rust 的一字不差", () => {
    // 少一个说明 Rust 加了新类型而前端不会处理；
    // 多一个说明前端在为不存在的类型写分支。
    expect(sortAlpha(BLOB_KIND_TAGS)).toEqual(sortAlpha(Object.keys(contract.kinds)));
  });

  it("每种 kind 的字段清单都与 Rust 一致", () => {
    for (const tag of BLOB_KIND_TAGS) {
      expect(sortAlpha(BLOB_KIND_PROPS[tag]), `kind=${tag}`).toEqual(
        sortAlpha(contract.kinds[tag]?.props ?? [`Rust 侧没有这个 tag`]),
      );
    }
  });

  it("图片格式集合与 Rust 一致", () => {
    expect(sortAlpha(IMAGE_FORMATS)).toEqual(sortAlpha(contract.image_formats));
  });

  it("读取上限与 Rust 一致", () => {
    expect(MAX_PREVIEW_BYTES).toBe(contract.max_preview_bytes);
  });

  const tooLargeNotice = () =>
    previewNotice({
      size: contract.max_preview_bytes + 1,
      kind: { kind: "too_large" },
      content: null,
      text: null,
      hex: null,
      truncated: false,
      too_large: true,
    });

  it("界面上那句「预览上限」的数字是从上限值**派生**出来的，不是另抄的一份", async () => {
    // 这里刻意**不**断言「文案里含 4 MiB」：硬编码的 4 MiB 在当前上限恰好是
    // 4 MiB 时也会通过，那样这条断言在值相等的前提下永远不会失败
    // （探针 F7 就是这么把它绕过去的）。
    //
    // 要测的是派生关系 —— 换一个上限值进去，文案必须跟着变。
    vi.resetModules();
    vi.doMock("./blobPreview", async () => ({
      ...(await vi.importActual<Record<string, unknown>>("./blobPreview")),
      MAX_PREVIEW_BYTES: 7 * 1024 * 1024,
    }));
    try {
      const { previewNotice: fresh } = await import("./preview");
      const notice = fresh({
        size: 7 * 1024 * 1024 + 1,
        kind: { kind: "too_large" },
        content: null,
        text: null,
        hex: null,
        truncated: false,
        too_large: true,
      });
      expect(notice).toContain("7 MiB");
      expect(notice).not.toContain("4 MiB");
    } finally {
      vi.doUnmock("./blobPreview");
      vi.resetModules();
    }
  });

  it("同一句话在上限值本身没被改写时仍然成立", () => {
    // 上一条证明了「会被改写就跟着变」，这条证明正常情况下文案也确实说了数。
    // 两者缺一：只有前者会漏掉「压根没说数」，只有后者会漏掉「数是抄的」。
    expect(tooLargeNotice()).toContain(formatByteLimit(contract.max_preview_bytes));
  });

  it("contract 里每个样本都能被 TypeScript 侧解析成它声明的 tag", () => {
    expect(contract.samples.length).toBeGreaterThan(0);

    for (const sample of contract.samples) {
      const parsed = parseBlobPreview(sample.value);
      expect(parsed.kind.kind, sample.name).toBe(sample.expect_tag);
    }
  });

  it("每个 kind 都至少有一个样本 —— 否则上面那条逐样本用例会漏掉某一类", () => {
    const covered = new Set(contract.samples.map((s) => s.expect_tag));
    expect(sortAlpha([...covered])).toEqual(sortAlpha(BLOB_KIND_TAGS));
  });
});

describe("parseBlobPreview 拒不认识的形状", () => {
  const textSample = contract.samples.find((s) => s.name === "text")?.value as Record<
    string,
    unknown
  >;

  it("多一个字段就报错 —— 陌生字段不能被静默丢掉", () => {
    expect(() => parseBlobPreview({ ...textSample, brand_new_field: 1 })).toThrow(
      WireContractError,
    );
  });

  it("少一个字段也报错", () => {
    const { hex: _removed, ...rest } = textSample;
    expect(() => parseBlobPreview(rest)).toThrow(WireContractError);
  });

  it("kind 里出现未知的 tag 会报错，而不是被当作某一类", () => {
    expect(() =>
      parseBlobPreview({
        ...textSample,
        kind: { kind: "something_new" },
      }),
    ).toThrow(WireContractError);
  });

  it("kind 少了该有的字段会报错（LFS 指针没有 oid 就不算指针）", () => {
    expect(() =>
      parseBlobPreview({
        ...textSample,
        kind: { kind: "lfs_pointer", size: 1 },
      }),
    ).toThrow(WireContractError);
  });

  it("该是字符串的地方给数字会报错", () => {
    expect(() => parseBlobPreview({ ...textSample, size: "11" })).toThrow(WireContractError);
  });

  it("null 不算对象", () => {
    expect(() => parseBlobPreview(null)).toThrow(WireContractError);
  });

  it("错误信息里会说明是哪个字段，而不是笼统地说解析失败", () => {
    expect(() => parseBlobPreview({ ...textSample, extra: true })).toThrow(/extra/);
  });
});
