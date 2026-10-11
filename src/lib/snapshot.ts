/**
 * `RepoSnapshot` 的 TypeScript 侧类型与跨语言契约（补丁 P-13）。
 *
 * # 为什么这些类型从 `api.ts` 搬出来
 *
 * 它们是**受管模型**：JSON 形状由 Rust 的 serde 决定，而前端这份是手抄的。
 * 搬出来是因为它们要跟 `contracts/snapshot.json` 绑定 —— 混在接口清单里
 * 就会有人「顺手加个字段」而不知道那里有一道闸门。
 *
 * # 手抄的部分只有字段名
 *
 * `KeyMap` / `fieldsOf` 让编译器反过来检查这份键表：漏一个字段、多一个字段
 * 都是**编译错误**，再由 `wire.test.ts` 拿它跟 contract 比对。
 */

import contractJson from "../../contracts/snapshot.json";
import { fieldsOf, type KeyMap, parseWire, type VariantSpec, type WireShape } from "./wireContract";

// 直接 import 而不是 `node:fs` 读（项目没装 `@types/node`，而 `resolveJsonModule`
// 已经开着）。代价是要绕过 TS 从 JSON 推断出的字面量类型 —— 那些推断过于精确。
const contract = contractJson as unknown as { shape: WireShape };

/** 变更类型。取值由 Rust 的 `rename_all = "snake_case"` 决定。 */
export type ChangeKind =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "copied"
  | "type_changed"
  | "unmerged"
  | "unknown";

const CHANGE_KIND_MAP: KeyMap<ChangeKind> = {
  added: true,
  modified: true,
  deleted: true,
  renamed: true,
  copied: true,
  type_changed: true,
  unmerged: true,
  unknown: true,
};

export const CHANGE_KINDS: readonly string[] = fieldsOf(CHANGE_KIND_MAP);

export interface UpstreamInfo {
  name: string;
  ahead: number;
  behind: number;
}

export interface HeadInfo {
  branch: string | null;
  commit: string;
  detached: boolean;
  upstream: UpstreamInfo | null;
}

export interface BranchInfo {
  name: string;
  commit: string;
  is_current: boolean;
}

export interface TagInfo {
  name: string;
  commit: string;
}

export interface FileChange {
  path: string;
  kind: ChangeKind;
}

export interface StatusInfo {
  conflicts: FileChange[];
  staged: FileChange[];
  unstaged: FileChange[];
}

/**
 * 仓库正在进行的操作。
 *
 * 没有进行中的操作时是 `null`，**不是**某个表示「干净」的取值 —— 与
 * `head.upstream` 的 `null` 同构。造一个「干净」变体会让「未知」与
 * 「确认无操作」混为一谈。
 */
export type RepoState =
  | { kind: "merge" }
  | { kind: "rebase"; step: number | null; total: number | null }
  | { kind: "cherry_pick" }
  | { kind: "revert" }
  | { kind: "bisect" };

/**
 * 每个变体的字段清单。
 *
 * `Extract<RepoState, { kind: "…" }>` 是为了让编译器检查它：变体加字段而这里
 * 没跟上 ⇒ 编译错误，而不是静默少校验一个字段。
 */
export const REPO_STATE_VARIANTS: Record<string, VariantSpec> = {
  merge: { fields: fieldsOf<Extract<RepoState, { kind: "merge" }>>({ kind: true }) },
  rebase: {
    fields: fieldsOf<Extract<RepoState, { kind: "rebase" }>>({
      kind: true,
      step: true,
      total: true,
    }),
  },
  cherry_pick: {
    fields: fieldsOf<Extract<RepoState, { kind: "cherry_pick" }>>({ kind: true }),
  },
  revert: { fields: fieldsOf<Extract<RepoState, { kind: "revert" }>>({ kind: true }) },
  bisect: { fields: fieldsOf<Extract<RepoState, { kind: "bisect" }>>({ kind: true }) },
};

export interface RepoSnapshot {
  path: string;
  head: HeadInfo;
  branches: BranchInfo[];
  tags: TagInfo[];
  status: StatusInfo;
  state: RepoState | null;
}

/**
 * 每个受管形状的路径 -> 字段清单。
 *
 * 路径是相对 `contract.shape` 的，数组字段的子形状就是它的元素形状
 * （`branches` 是数组，`branches` 这份键表说的是**元素**）。
 */
export const SNAPSHOT_KEYS: Record<string, readonly string[]> = {
  "": fieldsOf<RepoSnapshot>({
    path: true,
    head: true,
    branches: true,
    tags: true,
    status: true,
    state: true,
  }),
  head: fieldsOf<HeadInfo>({ branch: true, commit: true, detached: true, upstream: true }),
  "head.upstream": fieldsOf<UpstreamInfo>({ name: true, ahead: true, behind: true }),
  branches: fieldsOf<BranchInfo>({ name: true, commit: true, is_current: true }),
  tags: fieldsOf<TagInfo>({ name: true, commit: true }),
  status: fieldsOf<StatusInfo>({ conflicts: true, staged: true, unstaged: true }),
  "status.conflicts": fieldsOf<FileChange>({ path: true, kind: true }),
  "status.staged": fieldsOf<FileChange>({ path: true, kind: true }),
  "status.unstaged": fieldsOf<FileChange>({ path: true, kind: true }),
};

/** `shape 路径.字段名` -> 允许的取值。 */
export const SNAPSHOT_ENUMS: Record<string, readonly string[]> = {
  "status.conflicts.kind": CHANGE_KINDS,
  "status.staged.kind": CHANGE_KINDS,
  "status.unstaged.kind": CHANGE_KINDS,
};

/** 严格校验后端返回的快照。形状来自 contract，不是这里手抄的。 */
export function parseRepoSnapshot(value: unknown): RepoSnapshot {
  return parseWire<RepoSnapshot>(contract.shape, value, "RepoSnapshot");
}
