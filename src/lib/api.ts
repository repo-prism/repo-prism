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
  /**
   * 进行中的操作（US-1）。**没有**进行中的操作时是 `null`，
   * 而不是某个表示「干净」的取值 —— 与 `head.upstream` 的 `null` 同构。
   */
  state: RepoState | null;
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
  info: CommitInfo;
  files: FileStat[];
  /** 原始 unified diff 正文，供 CLI / MCP / Agent 直接消费。 */
  patch: string;
  /** 是否因超出上限被截断（原始文本 2 MiB / 结构化 5000 行）。 */
  truncated: boolean;
}

// ---------------------------------------------------------------------------
// 进行中的操作与工作区列表（US-1，补丁 P-09）
//
// `state` 由后端**零子进程**探测（看 <git-dir> 下的标志文件）得出，因此它搭在
// 快照里；`worktrees` / `stashes` 要起进程，所以单独走 getWorkspace()。
// ---------------------------------------------------------------------------

/** 仓库正在进行的操作。判据与降级规则见 SPEC 的 US-1。 */
export type RepoState =
  | { kind: "merge" }
  | { kind: "rebase"; step: number | null; total: number | null }
  | { kind: "cherry_pick" }
  | { kind: "revert" }
  | { kind: "bisect" };

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

/**
 * 工作树与 stash 列表。
 *
 * 与快照分开取：这两项各要一次子进程，而快照的子进程次数是一条被门禁钉住的
 * 契约。因此**只在界面上真正需要时才调它**。
 */
export async function getWorkspace(path: string): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("get_workspace", { path });
}

// ---------------------------------------------------------------------------
// blob 只读预览（US-3，补丁 P-10）
//
// 只有用户点开某个文件时才会调用 —— 它需要起 1–2 次子进程，且读的是真实字节，
// 不能进首屏路径（SPEC「不在 diff 里自动预览」）。
// ---------------------------------------------------------------------------

export type ImageFormat = "png" | "jpeg" | "gif" | "webp" | "bmp" | "svg";

/** 用可辨识联合，`switch` 漏掉一类会在类型层面报错。 */
export type BlobKind =
  | { kind: "image"; format: ImageFormat }
  | { kind: "text" }
  | { kind: "binary" }
  | { kind: "lfs_pointer"; oid: string; size: number }
  | { kind: "too_large" };

export interface BlobPreview {
  /** 仓库里该文件的**真实**字节数。永远给出，即使内容一个字节都没读。 */
  size: number;
  kind: BlobKind;
  /** base64 编码的原始字节（图片）。 */
  content: string | null;
  /** 文本预览（文本）。 */
  text: string | null;
  /** 十六进制转储（未知二进制）。 */
  hex: string | null;
  /** 读了，但呈现时截断。 */
  truncated: boolean;
  /** 超过上限，**一个字节都没读**。 */
  too_large: boolean;
}

export async function getBlobPreview(
  path: string,
  rev: string,
  filePath: string,
): Promise<BlobPreview> {
  return invoke<BlobPreview>("get_blob_preview", { path, rev, filePath });
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

// ---------------------------------------------------------------------------
// 变更分析与风险标记（US-7 的本地部分）
// ---------------------------------------------------------------------------

export type RiskLevel = "info" | "warn" | "critical";

export interface Risk {
  rule_id: string;
  level: RiskLevel;
  message: string;
  path: string;
}

export interface RiskCounts {
  info: number;
  warn: number;
  critical: number;
}

export interface ChangeAnalysis {
  summary: string;
  total_files: number;
  risks: Risk[];
  by_level: RiskCounts;
}

/** 未提交改动的本地风险分析。纯本地，不调用任何外部 API。 */
export async function analyzeChanges(path: string): Promise<ChangeAnalysis> {
  return invoke<ChangeAnalysis>("analyze_changes", { path });
}

// ---------------------------------------------------------------------------
// 外部工具跳转（US-8）
// ---------------------------------------------------------------------------

export interface RemoteInfo {
  host: string;
  owner: string;
  /** 仓库名。GitLab 这类支持子组的平台可能含 `/`。 */
  repo: string;
  url: string;
}

/** 没有配置 `origin` 时返回 `null`，不抛错。 */
export async function getRemoteInfo(path: string): Promise<RemoteInfo | null> {
  return invoke<RemoteInfo | null>("get_remote_info", { path });
}

// ---------------------------------------------------------------------------
// 本地 AI 摘要（TASK-013 / TASK-014，对应 US-7 的模型层）
//
// 全部走用户自己的本地服务（Ollama）。endpoint 由后端 `summarizer.rs` 限制为
// http:// + 回环地址，前端改不了这条边界 —— 它在 Rust 侧，不在输入框里。
// ---------------------------------------------------------------------------

export interface AiSettings {
  enabled: boolean;
  endpoint: string;
  model: string;
}

export async function getAiSettings(): Promise<AiSettings> {
  return invoke<AiSettings>("get_ai_settings");
}

/** 校验失败（非回环 endpoint）时 reject，后端不会把非法配置存下来。 */
export async function setAiSettings(settings: AiSettings): Promise<void> {
  return invoke<void>("set_ai_settings", { settings });
}

/** 探活给定的本地模型服务并返回已安装的模型名列表，**不落盘**。 */
export async function testAiConnection(settings: AiSettings): Promise<string[]> {
  return invoke<string[]>("test_ai_connection", { settings });
}

/** 未启用 AI 时返回 `null`（而非报错），调用方据此不渲染摘要条。 */
export async function summarizeChanges(path: string): Promise<string | null> {
  return invoke<string | null>("summarize_changes", { path });
}

export async function summarizeCommit(path: string, sha: string): Promise<string | null> {
  return invoke<string | null>("summarize_commit", { path, sha });
}
