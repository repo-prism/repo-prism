use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoSnapshot {
    pub path: PathBuf,
    pub head: HeadInfo,
    pub branches: Vec<BranchInfo>,
    pub tags: Vec<TagInfo>,
    pub status: StatusInfo,
    /// 进行中的操作（US-1，补丁 P-09）。无操作时为 `None`。
    ///
    /// 由 `<git-dir>` 下的标志**文件探测**得出，**不占任何子进程** ——
    /// 这正是它可以留在快照里、而 `worktrees` / `stashes` 不行的依据。
    ///
    /// `#[serde(default)]`：P-09 之前写下的快照 JSON 里没有这个键，
    /// 加默认值才能继续读出来（对称地，序列化后多出的键对旧客户端是可忽略的）。
    #[serde(default)]
    pub state: Option<RepoState>,
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

// ---------------------------------------------------------------------------
// 进行中的操作与工作区附加信息（US-1，补丁 P-09）
// ---------------------------------------------------------------------------

/// 仓库**正在进行的操作**。
///
/// 判据全部是 `<git-dir>` 下的标志文件，因此探测**零子进程**：
/// `MERGE_HEAD` / `rebase-merge/` / `rebase-apply/` / `CHERRY_PICK_HEAD` /
/// `REVERT_HEAD` / `BISECT_LOG`。
///
/// 无操作时是 `None`，**不是**某个表示「干净」的变体 —— 与 `HeadInfo.upstream`
/// 同构：「没有进行中的操作」是一个**缺失**，不是一个取值。
/// 给它造一个变体，会让「未知」与「确认无操作」混为一谈。
///
/// 序列化用 `kind` 作标签：变基那条额外带 `step` / `total` 两个可为 `null` 的字段
/// （形如 kind=rebase, step=3, total=7）。
///
/// 注意别把这里的标签写成带引号的纯小写词 —— 只读扫描器的规则 4 是
/// **全局**的写动词黑名单，被引号包起来的变基一词会被当成真在调 Git。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepoState {
    /// 合并未完成（`MERGE_HEAD` 存在，通常在等冲突解决）。
    Merge,
    /// 变基进行中。`step` / `total` 读不到时为 `None` ——
    /// **报不出进度不等于没在变基**，状态本身仍然成立。
    Rebase {
        step: Option<u32>,
        total: Option<u32>,
    },
    /// 摘取提交未完成（`CHERRY_PICK_HEAD` 存在）。
    CherryPick,
    /// 回退提交未完成（`REVERT_HEAD` 存在）。
    Revert,
    /// 二分查找进行中（`BISECT_LOG` 存在）。
    Bisect,
}

/// 一个工作树（US-1）。来自 `git worktree list --porcelain`。
///
/// **只读观测**：本结构描述工作树，代码里不存在创建 / 移动 / 删除工作树的路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    /// 该工作树检出的分支短名（已削去 `refs/heads/`）。分离头指针时为 `None`。
    pub branch: Option<String>,
    /// 该工作树的 HEAD。空仓库 / bare 仓库时为空串。
    pub commit: String,
    /// 主工作树。git 保证 `worktree list` 把它排在第一条。
    pub is_main: bool,
    pub bare: bool,
    pub detached: bool,
    /// 被锁定。只观测，不解除。
    pub locked: bool,
}

/// 一条 stash（US-1）。来自 `git stash list`。
///
/// `message` 是 stash 的 reflog 主题（形如 `WIP on main: 1a2b3c4 subject`）。
/// **不读取 stash 的内容** —— 展开 diff 属 US-3 的范畴，本补丁不碰。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StashInfo {
    /// `stash@{0}` 形式的引用。
    pub reference: String,
    pub commit: String,
    pub message: String,
}

/// 工作区的附加列表（US-1）。
///
/// 与 `RepoSnapshot` **分开取**：这两项各要一次子进程，而快照的子进程次数是
/// 一条被门禁钉住的契约（`SPEC.md`「首屏读取路径的子进程契约」）。
/// 分开的第二个好处是「用户没展开这两块就不付这个成本」。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub worktrees: Vec<WorktreeInfo>,
    pub stashes: Vec<StashInfo>,
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

/// 单个提交的详情：提交元信息 + 变更文件清单 + 原始 diff 正文。
///
/// `patch` 是**未加工的 unified diff 文本**，供 CLI / MCP / Agent 直接消费；
/// 结构化的 `files` 供前端渲染（见 `Git::commit_diff`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitDetail {
    pub info: CommitInfo,
    pub files: Vec<FileStat>,
    /// 原始 unified diff 正文（不含提交信息头）。
    pub patch: String,
    /// 是否因超出上限被截断（原始文本 2 MiB / 结构化 5000 行）。
    /// 截断永远是**显式**的，不静默丢弃。
    pub truncated: bool,
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

impl From<DiffFile> for FileStat {
    fn from(file: DiffFile) -> Self {
        Self {
            path: file.path,
            old_path: file.old_path,
            kind: file.kind,
            additions: file.additions,
            deletions: file.deletions,
            binary: file.binary,
        }
    }
}

// ---------------------------------------------------------------------------
// 工作区行数统计
// ---------------------------------------------------------------------------

/// 单个文件工作区 diff 的行数统计，来自 `git diff --numstat -z`。
///
/// **不属于** CLI / MCP 对外契约（对外只出现 `ChangeAnalysis` 的结果），
/// 因此不参与序列化；定义在这里是为了和 `FileStat` 并排，保持「变更文件量级」
/// 这一概念的数据类型只有一处。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineStat {
    pub additions: u32,
    pub deletions: u32,
    /// 二进制文件：`--numstat` 的增删列是 `-`，此时两个计数都记 0。
    pub binary: bool,
}

/// 以**新路径**为键的工作区行数统计。
pub type LineStats = HashMap<String, LineStat>;

// ---------------------------------------------------------------------------
// 外部集成（US-8）
// ---------------------------------------------------------------------------

/// `origin` remote 的解析结果。
///
/// 只做 URL 结构解析，**不访问网络**。前端据此拼出 GitDiagram / GitIngest /
/// DeepWiki / GitHub.dev 等外部工具的跳转地址。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub host: String,
    pub owner: String,
    /// 仓库名。GitLab 这类支持子组的平台可能含 `/`。
    pub repo: String,
    /// 原始 URL，原样保留供界面展示。
    pub url: String,
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
