/**
 * `BlobPreview` 里与 contract **来源不同**的那一个量：读取上限（补丁 P-12，
 * P-13 收敛）。
 *
 * 字段集合 / kind 清单 / 样本解析 / 解析器拒绝这些**通用**断言都搬到了
 * `wire.test.ts`（那里对着四份 contract 跑同一套），这里只留 contract
 * 管不到、或管了也不该由它派生的东西。
 *
 * # 为什么只留上限
 *
 * `MAX_PREVIEW_BYTES` 是 TS 侧**独立**的一个数，contract 里也有一个。
 * 两者相等才是对的 —— 若把它改成从 contract 派生，这条相等就变成永真，
 * Rust 改了上限界面文案会跟着悄悄变而测试全绿（探针 F7 就是这么绕过去的）。
 */

import { describe, expect, it, vi } from "vitest";
import contractJson from "../../contracts/blob-preview.json";
import { MAX_PREVIEW_BYTES } from "./blobPreview";
import { formatByteLimit, previewNotice } from "./preview";

const contract = contractJson as unknown as {
  constants: { max_preview_bytes: number };
};

const tooLargeNotice = () =>
  previewNotice({
    size: contract.constants.max_preview_bytes + 1,
    kind: { kind: "too_large" },
    content: null,
    text: null,
    hex: null,
    truncated: false,
    too_large: true,
  });

describe("读取上限", () => {
  it("TypeScript 的上限与 Rust 的是同一个数", () => {
    expect(MAX_PREVIEW_BYTES).toBe(contract.constants.max_preview_bytes);
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
    expect(tooLargeNotice()).toContain(formatByteLimit(contract.constants.max_preview_bytes));
  });
});
