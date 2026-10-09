import { describe, expect, it } from "vitest";
import type { RepoState, StashInfo, WorktreeInfo } from "./api";
import {
  stashLabel,
  stateLabel,
  stateProgress,
  stateTitle,
  worktreeName,
  worktreeRef,
} from "./workspace";

describe("stateLabel", () => {
  it("五种进行中的操作都有中文标签", () => {
    const cases: Array<[RepoState, string]> = [
      [{ kind: "merge" }, "合并中"],
      [{ kind: "rebase", step: 1, total: 2 }, "变基中"],
      [{ kind: "cherry_pick" }, "摘取中"],
      [{ kind: "revert" }, "回退中"],
      [{ kind: "bisect" }, "二分查找中"],
    ];
    for (const [state, expected] of cases) {
      expect(stateLabel(state)).toBe(expected);
    }
  });
});

describe("stateProgress", () => {
  it("变基的进度渲染成 第几步/共几步", () => {
    expect(stateProgress({ kind: "rebase", step: 3, total: 7 })).toBe("3/7");
  });

  it("只有半边进度时不显示 —— 比显示 3/? 更不容易误导", () => {
    expect(stateProgress({ kind: "rebase", step: 3, total: null })).toBeNull();
    expect(stateProgress({ kind: "rebase", step: null, total: 7 })).toBeNull();
    expect(stateProgress({ kind: "rebase", step: null, total: null })).toBeNull();
  });

  it("变基以外的状态没有进度，即使字段碰巧是数字", () => {
    expect(stateProgress({ kind: "merge" })).toBeNull();
    expect(stateProgress({ kind: "bisect" })).toBeNull();
  });
});

describe("stateTitle", () => {
  it("每种状态都有一句说明，且互相不同", () => {
    const states: RepoState[] = [
      { kind: "merge" },
      { kind: "rebase", step: null, total: null },
      { kind: "cherry_pick" },
      { kind: "revert" },
      { kind: "bisect" },
    ];
    const titles = states.map(stateTitle);
    expect(new Set(titles).size).toBe(states.length);
    for (const title of titles) expect(title.length).toBeGreaterThan(0);
  });
});

function tree(overrides: Partial<WorktreeInfo> = {}): WorktreeInfo {
  return {
    path: "/repos/main",
    branch: "main",
    commit: "1a2b3c4",
    is_main: true,
    bare: false,
    detached: false,
    locked: false,
    ...overrides,
  };
}

describe("worktreeName", () => {
  it("取路径最后一段", () => {
    expect(worktreeName(tree({ path: "/repos/main" }))).toBe("main");
    expect(worktreeName(tree({ path: "/repos/nested/linked" }))).toBe("linked");
  });

  it("同时认反斜杠 —— Windows 上 git 给的就是反斜杠路径", () => {
    expect(worktreeName(tree({ path: "C:\\repos\\linked" }))).toBe("linked");
  });

  it("路径末尾带斜杠时不会取到空串", () => {
    expect(worktreeName(tree({ path: "/repos/linked/" }))).toBe("linked");
  });
});

describe("worktreeRef", () => {
  it("普通工作树显示分支短名", () => {
    expect(worktreeRef(tree({ branch: "feature/x" }))).toBe("feature/x");
  });

  it("分离头指针与 bare 各自有文案，不显示空分支名", () => {
    expect(worktreeRef(tree({ branch: null, detached: true }))).toBe("分离头指针");
    expect(worktreeRef(tree({ branch: null, bare: true }))).toBe("bare 仓库");
  });

  it("bare 优先于分离头指针 —— bare 仓库本来就没有检出任何东西", () => {
    expect(worktreeRef(tree({ branch: null, bare: true, detached: true }))).toBe("bare 仓库");
  });

  it("既没分支也没标记时不吐空串", () => {
    expect(worktreeRef(tree({ branch: null }))).toBe("未知引用");
  });
});

describe("stashLabel", () => {
  const base: StashInfo = { reference: "stash@{0}", commit: "1a2b3c4", message: "WIP on main" };

  it("优先用说明文字", () => {
    expect(stashLabel(base)).toBe("WIP on main");
  });

  it("说明为空（或只有空白）时退回引用名，而不是显示一个空行", () => {
    expect(stashLabel({ ...base, message: "" })).toBe("stash@{0}");
    expect(stashLabel({ ...base, message: "   " })).toBe("stash@{0}");
  });
});
