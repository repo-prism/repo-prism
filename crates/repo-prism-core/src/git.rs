use crate::model::*;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

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
        Ok(RepoSnapshot {
            path: self.repo.clone(),
            head: self.head()?,
            branches: self.branches()?,
            tags: self.tags()?,
            status: self.status()?,
        })
    }

    /// 读取提交图。
    ///
    /// 使用 `\x1f` 分隔字段、`\x1e` 分隔记录，避免提交信息中的
    /// 换行、制表符、| 等字符造成解析歧义。
    pub fn commits(&self, limit: usize, skip: usize) -> Result<Vec<CommitInfo>> {
        let format = "%H\x1f%h\x1f%P\x1f%an\x1f%ae\x1f%aI\x1f%cn\x1f%ce\x1f%cI\x1f%s\x1f%b\x1e";
        let n = format!("-n{}", limit);
        let s = format!("--skip={}", skip);
        let fmt = format!("--format={}", format);
        let out = self.run(&["log", "--all", "--date=iso-strict", &fmt, &n, &s])?;

        let refs = self.ref_map()?;
        let mut commits = Vec::new();

        if let Some(out) = out {
            for record in out.split('\x1e') {
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
        }

        Ok(commits)
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

    fn head(&self) -> Result<HeadInfo> {
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
            upstream: None,
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
    fn status(&self) -> Result<StatusInfo> {
        let out = self.run(&["status", "--porcelain=v2", "--branch", "-z"])?;
        let mut info = StatusInfo::default();
        if let Some(out) = out {
            for entry in out.split('\0') {
                if entry.is_empty() || entry.starts_with("# ") {
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
        Ok(info)
    }

    fn classify(info: &mut StatusInfo, xy: &str, path: &str) {
        let mut chars = xy.chars();
        let x = chars.next().unwrap_or('.');
        let y = chars.next().unwrap_or('.');
        if x != '.' && x != ' ' {
            info.staged.push(FileChange {
                path: path.to_string(),
                kind: parse_kind(x),
            });
        }
        if y != '.' && y != ' ' {
            info.unstaged.push(FileChange {
                path: path.to_string(),
                kind: parse_kind(y),
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

fn parse_kind(c: char) -> ChangeKind {
    match c {
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
