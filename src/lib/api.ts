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

export async function inspectRepo(path: string): Promise<RepoSnapshot> {
  return invoke<RepoSnapshot>("inspect_repo", { path });
}

export async function getCommits(path: string, limit = 200, skip = 0): Promise<CommitInfo[]> {
  return invoke<CommitInfo[]>("get_commits", { path, limit, skip });
}
