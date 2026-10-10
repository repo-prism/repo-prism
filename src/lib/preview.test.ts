import { describe, expect, it } from "vitest";
import type { BlobKind, BlobPreview } from "./api";
import { blobDataUrl, kindLabel, mediaType, previewNotice, previewSides } from "./preview";

function preview(overrides: Partial<BlobPreview> & { kind: BlobKind }): BlobPreview {
  return {
    size: 100,
    content: null,
    text: null,
    hex: null,
    truncated: false,
    too_large: false,
    ...overrides,
  };
}

describe("mediaType", () => {
  it("每种格式都有自己的 MIME，且互不重复", () => {
    const formats = ["png", "jpeg", "gif", "webp", "bmp", "svg"] as const;
    const types = formats.map(mediaType);
    expect(new Set(types).size).toBe(formats.length);
    for (const type of types) expect(type.startsWith("image/")).toBe(true);
  });

  it("SVG 必须是 image/svg+xml —— 只有这个类型能被 <img> 渲染", () => {
    expect(mediaType("svg")).toBe("image/svg+xml");
  });
});

describe("blobDataUrl", () => {
  it("图片给出可直接喂给 <img> 的 data URL", () => {
    const url = blobDataUrl(
      preview({ kind: { kind: "image", format: "png" }, content: "aGVsbG8=" }),
    );
    expect(url).toBe("data:image/png;base64,aGVsbG8=");
  });

  it("SVG 也走 data URL，而不是把标记塞进 DOM", () => {
    const url = blobDataUrl(
      preview({ kind: { kind: "image", format: "svg" }, content: "PHN2Zz48L3N2Zz4=" }),
    );
    expect(url).toBe("data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=");
  });

  it("非图片一律返回 null —— 免得拿一段文本去喂 <img>", () => {
    expect(blobDataUrl(preview({ kind: { kind: "text" }, text: "hello" }))).toBeNull();
    expect(blobDataUrl(preview({ kind: { kind: "binary" }, hex: "00" }))).toBeNull();
    expect(blobDataUrl(preview({ kind: { kind: "too_large" } }))).toBeNull();
  });

  it("图片却没有内容时也不给出半个 URL", () => {
    expect(blobDataUrl(preview({ kind: { kind: "image", format: "png" } }))).toBeNull();
  });

  it("非图片**即使带着内容**也不给 URL —— 上一条被「内容为空」挡住了，这条才测到种类判据本身", () => {
    expect(blobDataUrl(preview({ kind: { kind: "text" }, content: "aGVsbG8=" }))).toBeNull();
  });
});

describe("kindLabel", () => {
  it("每类都有自己的说法", () => {
    const kinds: BlobKind[] = [
      { kind: "image", format: "jpeg" },
      { kind: "text" },
      { kind: "binary" },
      { kind: "lfs_pointer", oid: "abc", size: 1 },
      { kind: "too_large" },
    ];
    const labels = kinds.map(kindLabel);
    expect(new Set(labels).size).toBe(kinds.length);
    expect(labels[0]).toContain("JPEG");
  });
});

describe("previewNotice", () => {
  it("太大、LFS 指针、被截断各有各的说法", () => {
    const tooLarge = previewNotice(preview({ kind: { kind: "too_large" }, too_large: true }));
    const lfs = previewNotice(preview({ kind: { kind: "lfs_pointer", oid: "abc", size: 9 } }));
    const cut = previewNotice(preview({ kind: { kind: "text" }, truncated: true }));

    expect(tooLarge).toContain("没有被读取");
    expect(lfs).toContain("没有");
    expect(cut).toContain("完整");

    // 三种情形的文案必须不同 —— 混在一起用户会以为看到的就是全部
    expect(new Set([tooLarge, lfs, cut]).size).toBe(3);
  });

  it("没限制可说时就闭嘴", () => {
    expect(previewNotice(preview({ kind: { kind: "text" } }))).toBeNull();
  });

  it("「太大」优先于「截断」：一个字节都没读到就谈不上呈现被截断", () => {
    const notice = previewNotice(
      preview({ kind: { kind: "too_large" }, too_large: true, truncated: true }),
    );
    expect(notice).toContain("没有被读取");
  });
});

describe("previewSides", () => {
  const parents = ["parent-sha"];

  it("新增只有新的一侧", () => {
    const sides = previewSides("added", "a.png", null, "sha", parents);
    expect(sides.before).toBeNull();
    expect(sides.after).toEqual({ rev: "sha", path: "a.png" });
  });

  it("删除只有旧的一侧，且取父提交", () => {
    const sides = previewSides("deleted", "a.png", null, "sha", parents);
    expect(sides.before).toEqual({ rev: "parent-sha", path: "a.png" });
    expect(sides.after).toBeNull();
  });

  it("修改两侧都要，旧的一侧用父提交", () => {
    const sides = previewSides("modified", "a.png", null, "sha", parents);
    expect(sides.before).toEqual({ rev: "parent-sha", path: "a.png" });
    expect(sides.after).toEqual({ rev: "sha", path: "a.png" });
  });

  it("重命名时两侧用**各自的**路径 —— 否则会取到一个不存在的版本", () => {
    const sides = previewSides("renamed", "new.png", "old.png", "sha", parents);
    expect(sides.before).toEqual({ rev: "parent-sha", path: "old.png" });
    expect(sides.after).toEqual({ rev: "sha", path: "new.png" });
  });

  it("根提交没有父提交，此时不做对比", () => {
    const sides = previewSides("modified", "a.png", null, "sha", []);
    expect(sides.before).toBeNull();
    expect(sides.after).toEqual({ rev: "sha", path: "a.png" });
  });
});
