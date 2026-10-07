import type { CommitInfo } from "./api";

export interface GraphCommit extends CommitInfo {
  lane: number;
  parent_lanes: number[];
}

/**
 * 简化 lane 分配算法。
 * 遍历提交（已按 git log 顺序），为每个提交分配一个 lane，
 * 并记录它的父提交所在的 lane。
 */
export function assignLanes(commits: CommitInfo[], maxLanes = 10): GraphCommit[] {
  const lanes: (string | null)[] = [];
  const result: GraphCommit[] = [];

  for (const commit of commits) {
    let lane = lanes.indexOf(commit.sha);
    if (lane === -1) {
      lane = lanes.indexOf(null);
      if (lane === -1) {
        if (lanes.length < maxLanes) {
          lane = lanes.length;
          lanes.push(null);
        } else {
          lane = 0;
        }
      }
    }
    lanes[lane] = null;

    const parent_lanes: number[] = [];
    for (let i = 0; i < commit.parents.length; i++) {
      const parent = commit.parents[i];
      if (i === 0) {
        lanes[lane] = parent;
        parent_lanes.push(lane);
      } else {
        let pl = lanes.indexOf(parent);
        if (pl === -1) {
          pl = lanes.indexOf(null);
          if (pl === -1) {
            pl = lanes.length;
            lanes.push(parent);
          } else {
            lanes[pl] = parent;
          }
        }
        parent_lanes.push(pl);
      }
    }

    result.push({ ...commit, lane, parent_lanes });

    while (lanes.length > 0 && lanes[lanes.length - 1] === null) {
      lanes.pop();
    }
  }

  return result;
}

export const LANE_COLORS = [
  "#60a5fa",
  "#f87171",
  "#34d399",
  "#fbbf24",
  "#a78bfa",
  "#f472b6",
  "#2dd4bf",
  "#fb923c",
  "#818cf8",
  "#4ade80",
];

export function laneColor(lane: number): string {
  return LANE_COLORS[lane % LANE_COLORS.length];
}

export function maxLaneOf(commits: GraphCommit[]): number {
  let m = 0;
  for (const c of commits) {
    m = Math.max(m, c.lane);
    for (const pl of c.parent_lanes) m = Math.max(m, pl);
  }
  return m;
}
