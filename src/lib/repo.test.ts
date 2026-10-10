import { describe, expect, it } from "vitest";
import { nextActiveAfterClose, repoDisplayName } from "./repo";

describe("repoDisplayName", () => {
  it("取路径的最后一段", () => {
    expect(repoDisplayName("/Users/me/code/repo-prism")).toBe("repo-prism");
    expect(repoDisplayName("/a/b")).toBe("b");
  });

  it("忽略结尾的分隔符", () => {
    // `/a/b/` 的最后一段是 b，不是空字符串
    expect(repoDisplayName("/a/b/")).toBe("b");
    expect(repoDisplayName("/a/b///")).toBe("b");
  });

  it("Windows 的反斜杠同样算分隔符", () => {
    expect(repoDisplayName("C:\\Users\\me\\repo")).toBe("repo");
    expect(repoDisplayName("C:\\repo\\")).toBe("repo");
  });

  it("没有段可取的极端输入原样返回，不抛错", () => {
    expect(repoDisplayName("/")).toBe("/");
    expect(repoDisplayName("///")).toBe("///");
  });

  it("相对路径也按同一规则处理", () => {
    expect(repoDisplayName(".")).toBe(".");
    expect(repoDisplayName("../sibling")).toBe("sibling");
  });

  it("含空格与中文的目录名不会被切开", () => {
    expect(repoDisplayName("/Users/me/我的 仓库/app")).toBe("app");
  });
});

describe("nextActiveAfterClose", () => {
  it("关掉中间那个时选它后面那一个", () => {
    expect(nextActiveAfterClose(["a", "b", "c"], "b")).toBe("c");
  });

  it("关掉第一个时选它后面那一个", () => {
    expect(nextActiveAfterClose(["a", "b"], "a")).toBe("b");
  });

  it("关掉最后一个时退回它前面那一个", () => {
    expect(nextActiveAfterClose(["a", "b", "c"], "c")).toBe("b");
  });

  it("关掉唯一一个时没有下一个，界面该回欢迎页", () => {
    expect(nextActiveAfterClose(["a"], "a")).toBeNull();
  });

  it("关掉的仓库不在列表里时不崩，退回第一个", () => {
    expect(nextActiveAfterClose(["a", "b"], "zzz")).toBe("a");
  });

  it("空列表返回 null", () => {
    expect(nextActiveAfterClose([], "a")).toBeNull();
  });

  it("连续关闭时焦点一直往右走，不来回跳", () => {
    // 关掉 b 后回到 a，再关掉 a 应到 c —— 而不是又跳回某个已关掉的
    const afterFirst = nextActiveAfterClose(["a", "b", "c"], "b");
    expect(afterFirst).toBe("c");
    const afterSecond = nextActiveAfterClose(["a", "c"], "c");
    expect(afterSecond).toBe("a");
  });
});
