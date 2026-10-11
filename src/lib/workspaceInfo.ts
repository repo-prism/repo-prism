/**
 * `WorkspaceInfo` 的 TypeScript 侧类型与跨语言契约（补丁 P-13）。
 *
 * 与快照**分开**：这两项各要一次子进程，而快照的子进程次数是一条被门禁钉住的
 * 契约。分开的第二个好处是「用户没展开这两块就不付这个成本」。
 */

import contractJson from "../../contracts/workspace.json";
import { fieldsOf, parseWire, type WireShape } from "./wireContract";

const contract = contractJson as unknown as { shape: WireShape };

export interface WorktreeInfo {
  path: string;
  /** 该工作树检出的分支短名；分离头指针时为 `null`。 */
  branch: string | null;
  /** 该工作树的 HEAD。bare / 空仓库时为空串。 */
  commit: string;
  /** 主工作树。git 保证它排在第一条。 */
  is_main: boolean;
  bare: boolean;
  detached: boolean;
  locked: boolean;
}

export interface StashInfo {
  /** `stash@{0}` 形式的引用。 */
  reference: string;
  commit: string;
  message: string;
}

export interface WorkspaceInfo {
  worktrees: WorktreeInfo[];
  stashes: StashInfo[];
}

/** 路径 -> 字段清单。数组字段对应的是**元素**形状。 */
export const WORKSPACE_KEYS: Record<string, readonly string[]> = {
  "": fieldsOf<WorkspaceInfo>({ worktrees: true, stashes: true }),
  worktrees: fieldsOf<WorktreeInfo>({
    path: true,
    branch: true,
    commit: true,
    is_main: true,
    bare: true,
    detached: true,
    locked: true,
  }),
  stashes: fieldsOf<StashInfo>({ reference: true, commit: true, message: true }),
};

/** 严格校验后端返回的工作区列表。 */
export function parseWorkspaceInfo(value: unknown): WorkspaceInfo {
  return parseWire<WorkspaceInfo>(contract.shape, value, "WorkspaceInfo");
}
