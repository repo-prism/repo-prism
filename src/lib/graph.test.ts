import { describe, expect, it } from "vitest";
import type { CommitInfo } from "./api";
import { assignLanes, LANE_COLORS, laneColor, maxLaneOf } from "./graph";

function commit(sha: string, parents: string[] = [], subject = sha): CommitInfo {
  return {
    sha,
    short_sha: sha.slice(0, 7),
    parents,
    author_name: "Test User",
    author_email: "test@example.com",
    author_date: "2026-01-01T00:00:00+08:00",
    committer_name: "Test User",
    committer_email: "test@example.com",
    committer_date: "2026-01-01T00:00:00+08:00",
    subject,
    body: null,
    refs: [],
  };
}

describe("assignLanes", () => {
  it("线性历史全部落在 lane 0", () => {
    const commits = [commit("c3", ["c2"]), commit("c2", ["c1"]), commit("c1", [])];
    const graph = assignLanes(commits);

    expect(graph).toHaveLength(3);
    for (const c of graph) {
      expect(c.lane).toBe(0);
    }
    // 每个提交的父提交仍在同一条 lane 上
    expect(graph[0].parent_lanes).toEqual([0]);
    expect(graph[1].parent_lanes).toEqual([0]);
    expect(graph[2].parent_lanes).toEqual([]);
  });

  it("合并提交会占用多条 lane", () => {
    // git log 顺序：merge 在最前，两条分支交错
    const commits = [
      commit("merge", ["main1", "feat1"]),
      commit("main1", ["base"]),
      commit("feat1", ["base"]),
      commit("base", []),
    ];
    const graph = assignLanes(commits);

    expect(graph[0].parent_lanes).toHaveLength(2);
    // 第一父提交留在自己的 lane，第二父提交另开一条
    expect(graph[0].parent_lanes[0]).toBe(0);
    expect(graph[0].parent_lanes[1]).not.toBe(0);
    expect(maxLaneOf(graph)).toBeGreaterThanOrEqual(1);
  });

  it("空输入返回空数组", () => {
    expect(assignLanes([])).toEqual([]);
  });

  it("不修改原始提交对象，且保留所有字段", () => {
    const original = commit("aaa", [], "keep me");
    const [first] = assignLanes([original]);

    expect(first.subject).toBe("keep me");
    expect(first.sha).toBe("aaa");
    expect(original).not.toHaveProperty("lane");
  });

  it("lane 数量受 maxLanes 约束", () => {
    const commits: CommitInfo[] = [];
    let previous = "root";
    commits.push(commit("root", []));
    for (let i = 0; i < 20; i++) {
      const sha = `branch-${i}`;
      commits.push(commit(sha, [previous]));
      previous = sha;
    }
    commits.reverse();

    const graph = assignLanes(commits, 4);
    expect(maxLaneOf(graph)).toBeLessThan(4);
  });
});

describe("laneColor", () => {
  it("按 lane 循环取色", () => {
    expect(laneColor(0)).toBe(LANE_COLORS[0]);
    expect(laneColor(LANE_COLORS.length)).toBe(LANE_COLORS[0]);
    expect(laneColor(LANE_COLORS.length + 2)).toBe(LANE_COLORS[2]);
  });

  it("负 lane 不会返回 undefined", () => {
    // lane 恒为非负，但取色函数需对任意输入有定义
    expect(typeof laneColor(-1)).toBe("string");
  });
});

describe("maxLaneOf", () => {
  it("返回出现过的最大 lane，含父 lane", () => {
    const graph = assignLanes([commit("m", ["a", "b"]), commit("a", []), commit("b", [])]);
    let max = 0;
    for (const c of graph) {
      max = Math.max(max, c.lane, ...c.parent_lanes);
    }
    expect(maxLaneOf(graph)).toBe(max);
  });

  it("空列表返回 0", () => {
    expect(maxLaneOf([])).toBe(0);
  });
});
