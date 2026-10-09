use crate::diffparse::{parse_name_status, parse_numstat, parse_patch};
use crate::model::*;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `git log` 的字段格式：`\x1f` 分隔字段、`\x1e` 分隔记录，
/// 避免提交信息中的换行、制表符、`|` 等字符造成解析歧义。
const COMMIT_FORMAT: &str =
    "%H\x1f%h\x1f%P\x1f%an\x1f%ae\x1f%aI\x1f%cn\x1f%ce\x1f%cI\x1f%s\x1f%b\x1e";

/// 单次 Diff 解析的行数上限。超出即截断并**显式标注**，不静默丢弃。
const MAX_DIFF_LINES: usize = 5_000;

/// 单次提交原始 diff 文本的上限（2 MiB）。超出即截断并**显式标注**。
const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;

/// Git 只读读取器。
///
/// **安全约束**：只允许白名单内的只读命令。
/// CI 会静态扫描本文件的 `Command::new("git")` 调用。
pub struct Git {
    repo: PathBuf,
}

impl Git {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .arg("rev-parse")
            .arg("--show-toplevel")
            .output()
            .context("failed to run git rev-parse")?;
        if !out.status.success() {
            anyhow::bail!("not a git repository: {}", path.display());
        }
        let repo = String::from_utf8(out.stdout)?.trim().to_string();
        Ok(Self { repo: repo.into() })
    }

    pub fn path(&self) -> &Path {
        &self.repo
    }

    pub fn snapshot(&self) -> Result<RepoSnapshot> {
        // 上游计数与工作区状态来自同一次 `git status` 调用，不额外起子进程。
        let (status, upstream) = self.status()?;
        Ok(RepoSnapshot {
            path: self.repo.clone(),
            head: self.head(upstream)?,
            branches: self.branches()?,
            tags: self.tags()?,
            status,
        })
    }

    /// 读取提交图。
    pub fn commits(&self, limit: usize, skip: usize) -> Result<Vec<CommitInfo>> {
        let n = format!("-n{}", limit);
        let s = format!("--skip={}", skip);
        let fmt = format!("--format={}", COMMIT_FORMAT);
        let out = self.run(&["log", "--all", "--date=iso-strict", &fmt, &n, &s])?;

        let refs = self.ref_map()?;
        Ok(Self::parse_commits(out.as_deref().unwrap_or(""), &refs))
    }

    /// 读取单个提交的元信息。
    pub fn commit(&self, sha: &str) -> Result<CommitInfo> {
        let fmt = format!("--format={}", COMMIT_FORMAT);
        let out = self.run(&["log", "--max-count=1", "--date=iso-strict", &fmt, sha])?;
        let refs = self.ref_map()?;
        Self::parse_commits(out.as_deref().unwrap_or(""), &refs)
            .into_iter()
            .next()
            .with_context(|| format!("commit not found: {sha}"))
    }

    fn parse_commits(raw: &str, refs: &HashMap<String, Vec<String>>) -> Vec<CommitInfo> {
        let mut commits = Vec::new();
        for record in raw.split('\x1e') {
            let record = record.trim_matches('\n');
            if record.is_empty() {
                continue;
            }
            let parts: Vec<&str> = record.split('\x1f').collect();
            if parts.len() < 11 {
                continue;
            }
            let sha = parts[0].to_string();
            commits.push(CommitInfo {
                short_sha: parts[1].to_string(),
                parents: parts[2].split_whitespace().map(String::from).collect(),
                author_name: parts[3].to_string(),
                author_email: parts[4].to_string(),
                author_date: parts[5].to_string(),
                committer_name: parts[6].to_string(),
                committer_email: parts[7].to_string(),
                committer_date: parts[8].to_string(),
                subject: parts[9].to_string(),
                body: if parts[10].trim().is_empty() {
                    None
                } else {
                    Some(parts[10].trim().to_string())
                },
                refs: refs.get(&sha).cloned().unwrap_or_default(),
                sha,
            });
        }
        commits
    }

    /// 提交详情：元信息 + 变更文件清单 + 原始 diff 正文（US-3）。
    ///
    /// 只跑两次 `git show`：`--name-status -z` 给可靠的路径与类型，
    /// patch 正文既喂结构化解析、也原样交给 `patch` 字段。
    pub fn commit_detail(&self, sha: &str) -> Result<CommitDetail> {
        let info = self.commit(sha)?;
        let names_raw = self.run(&commit_names_args(sha))?.unwrap_or_default();
        let raw = self.run(&commit_patch_args(sha))?.unwrap_or_default();

        let names = parse_name_status(&names_raw);
        let (files, line_truncated) = parse_patch(&raw, &names, MAX_DIFF_LINES);
        let (patch, byte_truncated) = cap_patch(raw);

        Ok(CommitDetail {
            info,
            files: files.into_iter().map(FileStat::from).collect(),
            patch,
            truncated: byte_truncated || line_truncated,
        })
    }

    /// 单个提交的 Diff（US-3）。
    pub fn commit_diff(&self, sha: &str) -> Result<Diff> {
        self.collect_diff(&commit_names_args(sha), &commit_patch_args(sha))
    }

    /// 任意两点之间的 Diff。
    ///
    /// `from` / `to` 都为空时是「工作区 vs 索引」；只给 `from` 是「工作区 vs from」。
    pub fn diff(&self, from: Option<&str>, to: Option<&str>) -> Result<Diff> {
        let mut locate: Vec<&str> = Vec::new();
        if let Some(from) = from {
            locate.push(from);
        }
        if let Some(to) = to {
            locate.push(to);
        }

        let mut names = vec!["diff", "--name-status", "--find-renames", "-z"];
        names.extend_from_slice(&locate);
        let mut patch = vec!["diff", "--find-renames", "--no-textconv", "--unified=3"];
        patch.extend_from_slice(&locate);

        self.collect_diff(&names, &patch)
    }

    /// 解析 `origin` remote 为结构化的 host / owner / repo（US-8）。
    ///
    /// 只读，且**不访问网络** —— 只是把仓库配置里的 URL 字符串拆开。
    /// 没有配置 `origin`（或命令失败）时返回 `None`：让调用方去提示用户，
    /// 而不是把一个「没有远端」硬报成错误。
    pub fn remote_info(&self) -> Result<Option<RemoteInfo>> {
        let Some(url) = self.run(&["remote", "get-url", "origin"])? else {
            return Ok(None);
        };
        Ok(parse_remote(url.trim()))
    }

    /// 工作区（含已暂存）的逐文件行数统计。
    ///
    /// 纯 `status --porcelain=v2` **拿不到**增删行数，而「大量删除」这类风险规则
    /// 需要它。这里的两次 `--numstat` 调用是唯一额外的子进程开销，
    /// 且只被 `analyze_changes` 使用 —— `snapshot()` 不受影响。
    ///
    /// `--no-textconv` 与 `commit_patch_args` 同理：不得触发仓库自定义的转换器。
    /// `--numstat` 只数行数、不产出正文，因此比读 patch 廉价得多。
    ///
    /// 两次调用分别对应「索引 vs HEAD」与「工作区 vs 索引」；同一路径若两处都有，
    /// 后写的（工作区）覆盖前写的 —— 用户更关心还没暂存的那一份。
    pub fn working_tree_stats(&self) -> Result<LineStats> {
        let mut stats = LineStats::new();
        if let Some(raw) = self.run(&["diff", "--cached", "--numstat", "-z", "--no-textconv"])? {
            stats.extend(parse_numstat(&raw));
        }
        if let Some(raw) = self.run(&["diff", "--numstat", "-z", "--no-textconv"])? {
            stats.extend(parse_numstat(&raw));
        }
        Ok(stats)
    }

    /// 两次调用读同一个 diff：`--name-status -z` 给可靠的路径与类型，
    /// patch 正文给 hunk 内容。两者由 Git 按同一顺序生成，按下标对齐。
    ///
    /// `--no-textconv` 是安全要求而非性能优化：不加它，Git 会对声明了
    /// textconv 的文件执行仓库自定义的转换器（SECURITY.md 威胁 1）。
    fn collect_diff(&self, names_args: &[&str], patch_args: &[&str]) -> Result<Diff> {
        let names_raw = self.run(names_args)?.unwrap_or_default();
        let patch = self.run(patch_args)?.unwrap_or_default();

        let names = parse_name_status(&names_raw);
        let (files, truncated) = parse_patch(&patch, &names, MAX_DIFF_LINES);
        Ok(Diff { files, truncated })
    }

    fn ref_map(&self) -> Result<HashMap<String, Vec<String>>> {
        let out = self.run(&[
            "for-each-ref",
            "--format=%(objectname)\t%(refname:short)",
            "refs/heads/",
            "refs/tags/",
            "refs/remotes/",
        ])?;
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(out) = out {
            for line in out.lines() {
                let mut parts = line.split('\t');
                let sha = parts.next().unwrap_or("").to_string();
                let name = parts.next().unwrap_or("").to_string();
                if !sha.is_empty() && !name.is_empty() {
                    map.entry(sha).or_default().push(name);
                }
            }
        }
        Ok(map)
    }

    fn head(&self, upstream: Option<UpstreamInfo>) -> Result<HeadInfo> {
        let symbolic = self.run(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        let (branch, detached) = match symbolic {
            Some(s) if !s.trim().is_empty() => (Some(s.trim().to_string()), false),
            _ => (None, true),
        };
        let commit = self
            .run(&["rev-parse", "HEAD"])?
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        Ok(HeadInfo {
            branch,
            commit,
            detached,
            upstream,
        })
    }

    fn branches(&self) -> Result<Vec<BranchInfo>> {
        let out = self.run(&[
            "for-each-ref",
            "--format=%(refname:short)\t%(objectname)\t%(HEAD)",
            "refs/heads/",
        ])?;
        let mut result = Vec::new();
        if let Some(out) = out {
            for line in out.lines() {
                let mut parts = line.split('\t');
                let name = parts.next().unwrap_or("").to_string();
                let commit = parts.next().unwrap_or("").to_string();
                let is_current = parts.next().unwrap_or("") == "*";
                if !name.is_empty() {
                    result.push(BranchInfo {
                        name,
                        commit,
                        is_current,
                    });
                }
            }
        }
        Ok(result)
    }

    fn tags(&self) -> Result<Vec<TagInfo>> {
        let out = self.run(&[
            "for-each-ref",
            "--format=%(refname:short)\t%(objectname)",
            "refs/tags/",
        ])?;
        let mut result = Vec::new();
        if let Some(out) = out {
            for line in out.lines() {
                let mut parts = line.split('\t');
                let name = parts.next().unwrap_or("").to_string();
                let commit = parts.next().unwrap_or("").to_string();
                if !name.is_empty() {
                    result.push(TagInfo { name, commit });
                }
            }
        }
        Ok(result)
    }

    /// 解析 `git status --porcelain=v2 --branch -z`。
    ///
    /// `-z` 使用 NUL 分隔条目，可安全处理路径中的空格与引号。
    ///
    /// 输出流由三部分组成：一组 `# branch.*` 头部行（`--branch` 开启后才有）、
    /// 工作区条目、以及未跟踪条目。每条记录都以 NUL 结尾，头部行也是——
    /// 因此不需要按换行切分，路径中含换行符也不会破坏解析。
    ///
    /// **上游计数就藏在头部里**（`# branch.upstream <ref>` / `# branch.ab +N -M`），
    /// 所以顺带一并返回，避免再起一个 `rev-list --count` 子进程。
    fn status(&self) -> Result<(StatusInfo, Option<UpstreamInfo>)> {
        let out = self.run(&["status", "--porcelain=v2", "--branch", "-z"])?;
        let mut info = StatusInfo::default();
        let mut upstream_name: Option<String> = None;
        let mut ahead_behind: Option<(u32, u32)> = None;

        if let Some(out) = out {
            for entry in out.split('\0') {
                if entry.is_empty() {
                    continue;
                }
                if let Some(rest) = entry.strip_prefix("# branch.upstream ") {
                    upstream_name = Some(rest.trim().to_string());
                    continue;
                }
                if let Some(rest) = entry.strip_prefix("# branch.ab ") {
                    ahead_behind = parse_ahead_behind(rest);
                    continue;
                }
                if entry.starts_with("# ") {
                    continue;
                }
                match entry.chars().next() {
                    Some('1') => {
                        // "1 XY sub mH mI mW hH hI path"
                        let parts: Vec<&str> = entry.splitn(9, ' ').collect();
                        if parts.len() >= 9 {
                            Self::classify(&mut info, parts[1], parts[8]);
                        }
                    }
                    Some('2') => {
                        // "2 XY sub mH mI mW hH hI Xscore path"
                        let parts: Vec<&str> = entry.splitn(10, ' ').collect();
                        if parts.len() >= 10 {
                            Self::classify(&mut info, parts[1], parts[9]);
                        }
                    }
                    Some('u') => {
                        // "u XY sub m1 m2 m3 mW h1 h2 h3 path"
                        let parts: Vec<&str> = entry.splitn(11, ' ').collect();
                        if parts.len() >= 11 {
                            info.conflicts.push(FileChange {
                                path: parts[10].to_string(),
                                kind: ChangeKind::Unmerged,
                            });
                        }
                    }
                    Some('?') => {
                        // "? path" — untracked
                        let path = entry.get(2..).unwrap_or("").to_string();
                        if !path.is_empty() {
                            info.unstaged.push(FileChange {
                                path,
                                kind: ChangeKind::Added,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        // 未设置上游时 git 根本不输出 `# branch.upstream`，此时必须是 `None`，
        // 而不是「ahead=0, behind=0」——后者会让「本地领先/落后为 0」与
        // 「没有上游可比」这两种完全不同的状态混为一谈。
        //
        // 若用户设置了 `status.aheadBehind=false`，git 只给 upstream 名不给计数，
        // 此时按 0/0 处理（此时计数确实未知，而非同步）。
        let upstream = upstream_name.map(|name| {
            let (ahead, behind) = ahead_behind.unwrap_or((0, 0));
            UpstreamInfo {
                name,
                ahead,
                behind,
            }
        });

        Ok((info, upstream))
    }

    fn classify(info: &mut StatusInfo, xy: &str, path: &str) {
        let mut chars = xy.chars();
        let x = chars.next().unwrap_or('.');
        let y = chars.next().unwrap_or('.');
        if x != '.' && x != ' ' {
            info.staged.push(FileChange {
                path: path.to_string(),
                kind: ChangeKind::from_status_code(x),
            });
        }
        if y != '.' && y != ' ' {
            info.unstaged.push(FileChange {
                path: path.to_string(),
                kind: ChangeKind::from_status_code(y),
            });
        }
    }

    /// 只读执行 Git 命令。返回 `None` 表示命令失败但不致命。
    fn run(&self, args: &[&str]) -> Result<Option<String>> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(args)
            .output()
            .with_context(|| format!("failed to run git {:?}", args))?;
        if !out.status.success() {
            return Ok(None);
        }
        Ok(Some(String::from_utf8(out.stdout)?))
    }
}

/// 把 remote URL 解析成 `host / owner / repo`。
///
/// 支持四种写法：
///
/// ```text
/// git@github.com:owner/repo.git            # SSH 简写（host 与 path 之间是 :）
/// ssh://git@github.com/owner/repo.git      # SSH 显式协议
/// https://github.com/owner/repo.git        # HTTPS
/// https://user@github.com/owner/repo.git   # HTTPS 带用户名
/// ```
///
/// 其余形式（本地路径、`file://`、bundles）一律返回 `None`：
/// **不猜**。猜错会让前端把用户带到别的仓库去。
fn parse_remote(url: &str) -> Option<RemoteInfo> {
    // SSH 简写的分隔符与 URL 形式不同，单独处理
    if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        return build_remote(url, host, path);
    }

    let rest = url
        .strip_prefix("ssh://")
        .or_else(|| url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("http://"))?;

    // 去掉可选的 `user@` / `user:pass@`
    let rest = rest.rsplit('@').next().unwrap_or(rest);
    let (host, path) = rest.split_once('/')?;
    build_remote(url, host, path)
}

fn build_remote(url: &str, host: &str, path: &str) -> Option<RemoteInfo> {
    // 先削尾斜杠再削 `.git`：`owner/repo.git/` 这种也削得干净
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    // `split_once` 而非 `rsplit`：GitLab 子组（owner/sub/repo）拼出的 URL 仍然正确
    let (owner, repo) = path.split_once('/')?;
    if host.is_empty() || owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(RemoteInfo {
        host: host.to_string(),
        owner: owner.to_string(),
        repo: repo.to_string(),
        url: url.to_string(),
    })
}

/// 解析 `# branch.ab` 的值，形如 `+2 -0`。
///
/// `+` 是领先上游的提交数，`-` 是落后上游的提交数。格式不符时返回 `None`，
/// 由调用方决定退化为「计数未知」还是报错。
fn parse_ahead_behind(s: &str) -> Option<(u32, u32)> {
    let mut parts = s.split_whitespace();
    let ahead = parts.next()?.strip_prefix('+')?.parse().ok()?;
    let behind = parts.next()?.strip_prefix('-')?.parse().ok()?;
    Some((ahead, behind))
}

/// `git show` 的 `--name-status -z` 参数：给出可靠的路径与变更类型。
///
/// 路径**只**取自这里。patch 头部的路径不可信：含空格的路径会被追加制表符，
/// 非 ASCII 路径会被 `core.quotePath` 转义。
fn commit_names_args(sha: &str) -> [&str; 6] {
    [
        "show",
        "--format=",
        "--name-status",
        "--find-renames",
        "-z",
        sha,
    ]
}

/// `git show` 的 patch 参数。
///
/// `--no-textconv` 是**安全要求**而非性能优化：不加它，Git 会对声明了 textconv
/// 的文件执行仓库自定义的转换器（SECURITY.md 威胁 1）。
fn commit_patch_args(sha: &str) -> [&str; 6] {
    [
        "show",
        "--format=",
        "--find-renames",
        "--no-textconv",
        "--unified=3",
        sha,
    ]
}

/// 截断原始 patch 文本：超过 [`MAX_DIFF_BYTES`] 时在**字符边界**上截断并标记。
///
/// 按字节截断必须回退到 `char_boundary`，否则会在多字节字符中间切开，
/// 后续的 `String` 构造会 panic。
fn cap_patch(patch: String) -> (String, bool) {
    if patch.len() <= MAX_DIFF_BYTES {
        return (patch, false);
    }
    let mut end = MAX_DIFF_BYTES;
    while end > 0 && !patch.is_char_boundary(end) {
        end -= 1;
    }
    (patch[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::{cap_patch, parse_ahead_behind, parse_remote, MAX_DIFF_BYTES};

    // 注意：本模块位于 core/src 下，会被 read-only guard 扫描，
    // 因此断言里的字面量只使用非「纯小写单词」形式。
    #[test]
    fn parses_branch_ab_counts() {
        assert_eq!(parse_ahead_behind("+2 -0"), Some((2, 0)));
        assert_eq!(parse_ahead_behind("+0 -3"), Some((0, 3)));
        assert_eq!(parse_ahead_behind("+0 -0"), Some((0, 0)));
    }

    #[test]
    fn rejects_malformed_branch_ab() {
        assert_eq!(parse_ahead_behind(""), None);
        assert_eq!(parse_ahead_behind("2 0"), None);
        assert_eq!(parse_ahead_behind("+2"), None);
    }

    #[test]
    fn cap_patch_passes_through_small_text() {
        let small = "diff --git a/a.txt b/a.txt\n".to_string();
        let (out, truncated) = cap_patch(small.clone());
        assert_eq!(out, small);
        assert!(!truncated);
    }

    #[test]
    fn cap_patch_truncates_at_the_byte_limit() {
        // 注意：字面量不能写成「单个小写字母」那种纯小写词，否则会被只读扫描器
        // 当成 Git 子命令 token 报违规 —— 见本模块顶部说明。
        let big = "x\n".repeat(MAX_DIFF_BYTES / 2 + 10);
        let (out, truncated) = cap_patch(big);
        assert!(truncated, "超出上限必须显式标记");
        assert_eq!(out.len(), MAX_DIFF_BYTES);
    }

    #[test]
    fn cap_patch_never_splits_a_multibyte_char() {
        // 2 MiB 不是 3 的倍数，边界必然落在一个汉字中间
        let big = "中".repeat(MAX_DIFF_BYTES / 3 + 2);
        let (out, truncated) = cap_patch(big);
        assert!(truncated);
        assert!(out.len() <= MAX_DIFF_BYTES);
        assert!(
            out.chars().all(|c| c == '中'),
            "截断必须落在字符边界上，否则会 panic"
        );
    }

    #[test]
    fn parses_ssh_shorthand_remote() {
        let info = parse_remote("git@github.com:owner/repo.git").expect("SSH 简写应能解析");
        assert_eq!(info.host, "github.com");
        assert_eq!(info.owner, "owner");
        assert_eq!(info.repo, "repo", ".git 后缀必须削掉");
        assert_eq!(
            info.url, "git@github.com:owner/repo.git",
            "原始 URL 原样保留"
        );
    }

    #[test]
    fn parses_https_remote_with_and_without_user() {
        let plain = parse_remote("https://github.com/owner/repo.git").expect("HTTPS 应能解析");
        assert_eq!(
            (
                plain.host.as_str(),
                plain.owner.as_str(),
                plain.repo.as_str()
            ),
            ("github.com", "owner", "repo")
        );

        // 带口令与不带后缀两种写法都要能吃下
        let with_user =
            parse_remote("https://user@gitlab.com/group/repo").expect("带用户名的 HTTPS 应能解析");
        assert_eq!(with_user.host, "gitlab.com");
        assert_eq!(with_user.owner, "group");
        assert_eq!(with_user.repo, "repo");
    }

    #[test]
    fn parses_explicit_ssh_protocol_remote() {
        let info = parse_remote("ssh://git@github.com/owner/repo.git").expect("ssh:// 应能解析");
        assert_eq!(info.host, "github.com");
        assert_eq!(info.owner, "owner");
        assert_eq!(info.repo, "repo");
    }

    #[test]
    fn trailing_slash_is_trimmed() {
        let info = parse_remote("https://github.com/owner/repo.git/").expect("尾斜杠应能容错");
        assert_eq!(info.repo, "repo", "先削斜杠再削 .git，两种尾巴才都削得干净");
    }

    #[test]
    fn subgroup_path_keeps_the_whole_name() {
        // GitLab 子组：owner 取第一段，其余留给 repo，拼出的 URL 仍然正确
        let info = parse_remote("https://gitlab.com/group/sub/repo.git").expect("子组路径应能解析");
        assert_eq!(info.owner, "group");
        assert_eq!(info.repo, "sub/repo");
    }

    #[test]
    fn rejects_urls_that_are_not_remotes() {
        // 本地路径、file:// 与残缺 URL 一律返回 None —— 不猜，猜错会把用户带到别的仓库
        assert!(parse_remote("/tmp/some/repo").is_none());
        assert!(parse_remote("file:///tmp/some/repo").is_none());
        assert!(
            parse_remote("https://github.com").is_none(),
            "没有 owner/repo"
        );
        assert!(
            parse_remote("git@github.com:repo.git").is_none(),
            "没有 owner"
        );
        assert!(parse_remote("").is_none());
    }
}
