import { invoke } from "@tauri-apps/api/core";

export type ChangeKind =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "copied"
  | "type_changed"
  | "unmerged"
  | "unknown";

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
export interface RepoSnapshot {
  path: string;
  head: HeadInfo;
  branches: BranchInfo[];
  tags: TagInfo[];
  status: StatusInfo;
}
export interface CommitInfo {
  sha: string;
  short_sha: string;
  parents: string[];
  author_name: string;
  author_email: string;
  author_date: string;
  committer_name: string;
  committer_email: string;
  committer_date: string;
  subject: string;
  body: string | null;
  refs: string[];
}

// ---------------------------------------------------------------------------
// 提交详情与 Diff（US-3）。字段与 core 的 model.rs 一一对应。
// ---------------------------------------------------------------------------

export type DiffLineKind = "context" | "add" | "del";

export interface DiffLine {
  kind: DiffLineKind;
  /** 旧文件行号；新增行为 null。 */
  old_no: number | null;
  /** 新文件行号；删除行为 null。 */
  new_no: number | null;
  /** 行内容，不含前导的 + / - / 空格。 */
  content: string;
}

export interface DiffHunk {
  header: string;
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  lines: DiffLine[];
}

export interface DiffFile {
  path: string;
  old_path: string | null;
  kind: ChangeKind;
  /** 二进制文件只标记、不展示内容。 */
  binary: boolean;
  truncated: boolean;
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
}

export interface Diff {
  files: DiffFile[];
  truncated: boolean;
}

export interface FileStat {
  path: string;
  old_path: string | null;
  kind: ChangeKind;
  additions: number;
  deletions: number;
  binary: boolean;
}

export interface CommitDetail {
  commit: CommitInfo;
  files: FileStat[];
}

export async function inspectRepo(path: string): Promise<RepoSnapshot> {
  return invoke<RepoSnapshot>("inspect_repo", { path });
}

export async function getCommits(path: string, limit = 200, skip = 0): Promise<CommitInfo[]> {
  return invoke<CommitInfo[]>("get_commits", { path, limit, skip });
}

export async function getCommitDetail(path: string, sha: string): Promise<CommitDetail> {
  return invoke<CommitDetail>("get_commit_detail", { path, sha });
}

export async function getCommitDiff(path: string, sha: string): Promise<Diff> {
  return invoke<Diff>("get_commit_diff", { path, sha });
}

/** 任意两点之间的 Diff。`from` / `to` 皆省略时为「工作区 vs 索引」。 */
export async function getDiff(path: string, from?: string, to?: string): Promise<Diff> {
  return invoke<Diff>("get_diff", { path, from: from ?? null, to: to ?? null });
}
