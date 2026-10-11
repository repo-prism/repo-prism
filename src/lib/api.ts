import { invoke } from "@tauri-apps/api/core";
import type { ChangeAnalysis } from "./analysis";
import { parseChangeAnalysis } from "./analysis";
import type { BlobPreview } from "./blobPreview";
import { parseBlobPreview } from "./blobPreview";
import type { ChangeKind, RepoSnapshot } from "./snapshot";
import { parseRepoSnapshot } from "./snapshot";
import type { WorkspaceInfo } from "./workspaceInfo";
import { parseWorkspaceInfo } from "./workspaceInfo";

// ---------------------------------------------------------------------------
// 受管模型（补丁 P-13）
//
// 下面这些类型的 JSON 形状由 Rust 的 serde 决定，受 `contracts/*.json` 与
// `src/lib/wireContract.ts` 管制，因此住在各自的文件里。这里 re-export 只为
// 让调用方的 import 路径不变 —— **不要**往这个清单里加手抄的类型定义。
// ---------------------------------------------------------------------------

export type {
  ChangeAnalysis,
  Risk,
  RiskCounts,
  RiskLevel,
} from "./analysis";
export type {
  BranchInfo,
  ChangeKind,
  FileChange,
  HeadInfo,
  RepoSnapshot,
  RepoState,
  StatusInfo,
  TagInfo,
  UpstreamInfo,
} from "./snapshot";
export type { StashInfo, WorkspaceInfo, WorktreeInfo } from "./workspaceInfo";

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

/**
 * 工作树与 stash 列表。
 *
 * 与快照分开取：这两项各要一次子进程，而快照的子进程次数是一条被门禁钉住的
 * 契约。因此**只在界面上真正需要时才调它**。
 */
export async function getWorkspace(path: string): Promise<WorkspaceInfo> {
  const wire = await invoke<unknown>("get_workspace", { path });
  return parseWorkspaceInfo(wire);
}

// ---------------------------------------------------------------------------
// blob 只读预览（US-3，补丁 P-10）
//
// 只有用户点开某个文件时才会调用 —— 它需要起 1–2 次子进程，且读的是真实字节，
// 不能进首屏路径（SPEC「不在 diff 里自动预览」）。
//
// 类型与线格式清单住在 `blobPreview.ts`：它们受跨语言契约管制（补丁 P-12），
// 改动要过 `contracts/blob-preview.json` 那道闸门，不适合混在接口清单里。
// ---------------------------------------------------------------------------

export type {
  BlobKind,
  BlobKindTag,
  BlobPreview,
  ImageFormat,
} from "./blobPreview";
export { MAX_PREVIEW_BYTES, parseBlobPreview } from "./blobPreview";
export { WireContractError } from "./wireContract";

/**
 * 取一个文件在某个版本的内容预览。
 *
 * 返回值会过一遍 `parseBlobPreview`：字段漂了在这儿就抛 `WireContractError`，
 * 而不是让 UI 渲染出一块空白。
 */
export async function getBlobPreview(
  path: string,
  rev: string,
  filePath: string,
): Promise<BlobPreview> {
  const wire = await invoke<unknown>("get_blob_preview", { path, rev, filePath });
  return parseBlobPreview(wire);
}

export async function inspectRepo(path: string): Promise<RepoSnapshot> {
  const wire = await invoke<unknown>("inspect_repo", { path });
  return parseRepoSnapshot(wire);
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

/** 未提交改动的本地风险分析。纯本地，不调用任何外部 API。 */
export async function analyzeChanges(path: string): Promise<ChangeAnalysis> {
  const wire = await invoke<unknown>("analyze_changes", { path });
  return parseChangeAnalysis(wire);
}

// ---------------------------------------------------------------------------
// 多仓库工作区（US-9 / TASK-019）
//
// 后端持有一张**按已解析仓库根索引**的会话表（`RepoSet`），这里只做两件事：
// 列出当前打开的仓库根、关掉其中一个。命令本身都还带 `path`，所以切换仓库
// 不需要改任何既有命令的签名。
// ---------------------------------------------------------------------------

/**
 * 当前打开的仓库根，**已排序**（后端保证顺序确定）。
 *
 * 只给路径：分支名之类要看仓库才知道的字段意味着**另一个子进程**，
 * 而切换条只需要一个名字，路径的最后一段就够（见 `repoDisplayName`）。
 */
export async function listOpenRepos(): Promise<string[]> {
  return invoke<string[]>("list_open_repos");
}

/** 关掉一个仓库的会话。返回是否确实关掉了一个（后端按仓库根判定）。 */
export async function closeRepo(path: string): Promise<boolean> {
  return invoke<boolean>("close_repo", { path });
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
