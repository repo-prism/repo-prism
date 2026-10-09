use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoSnapshot {
    pub path: PathBuf,
    pub head: HeadInfo,
    pub branches: Vec<BranchInfo>,
    pub tags: Vec<TagInfo>,
    pub status: StatusInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeadInfo {
    pub branch: Option<String>,
    pub commit: String,
    pub detached: bool,
    pub upstream: Option<UpstreamInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpstreamInfo {
    pub name: String,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub commit: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagInfo {
    pub name: String,
    pub commit: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatusInfo {
    pub conflicts: Vec<FileChange>,
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
    Unknown,
}

impl ChangeKind {
    /// 由 Git 状态码字符（`status` 的 XY 位、`--name-status` 的首字符）映射而来。
    pub(crate) fn from_status_code(code: char) -> Self {
        match code {
            'A' => ChangeKind::Added,
            'M' => ChangeKind::Modified,
            'D' => ChangeKind::Deleted,
            'R' => ChangeKind::Renamed,
            'C' => ChangeKind::Copied,
            'T' => ChangeKind::TypeChanged,
            'U' => ChangeKind::Unmerged,
            _ => ChangeKind::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub author_date: String,
    pub committer_name: String,
    pub committer_email: String,
    pub committer_date: String,
    pub subject: String,
    pub body: Option<String>,
    pub refs: Vec<String>,
}

// ---------------------------------------------------------------------------
// 提交详情与 Diff（US-3）
// ---------------------------------------------------------------------------

/// 单个提交的详情：提交元信息 + 变更文件清单。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitDetail {
    pub commit: CommitInfo,
    pub files: Vec<FileStat>,
}

/// 变更文件的一行摘要。行数取自 diff 的 `+` / `-` 行计数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileStat {
    pub path: String,
    /// 重命名 / 复制时的原路径。
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    pub additions: u32,
    pub deletions: u32,
    /// 二进制文件。**不读取其内容**，只标记（SECURITY.md 威胁 2）。
    pub binary: bool,
}

/// 一次 Diff 的完整结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diff {
    pub files: Vec<DiffFile>,
    /// 是否因超出行数上限而被截断。截断是显式的，不静默丢弃。
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffFile {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    pub binary: bool,
    /// 本文件的 diff 是否被截断（超上限后其后的文件也会被标记）。
    pub truncated: bool,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffHunk {
    /// 原始 hunk 头，例如 `@@ -1,3 +1,4 @@ fn main()`。
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// 旧文件行号（新增行为 `None`）。
    pub old_no: Option<u32>,
    /// 新文件行号（删除行为 `None`）。
    pub new_no: Option<u32>,
    /// 行内容，**不含**前导的 `+` / `-` / 空格。
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}
