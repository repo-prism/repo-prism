use crate::diffparse::{parse_name_status, parse_numstat, parse_patch};
use crate::model::*;
use anyhow::{Context, Result};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// `git log` 的字段格式：`\x1f` 分隔字段、`\x1e` 分隔记录，
/// 避免提交信息中的换行、制表符、`|` 等字符造成解析歧义。
const COMMIT_FORMAT: &str =
    "%H\x1f%h\x1f%P\x1f%an\x1f%ae\x1f%aI\x1f%cn\x1f%ce\x1f%cI\x1f%s\x1f%b\x1e";

/// 单次 Diff 解析的行数上限。超出即截断并**显式标注**，不静默丢弃。
const MAX_DIFF_LINES: usize = 5_000;

/// 单次提交原始 diff 文本的上限（2 MiB）。超出即截断并**显式标注**。
const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;

/// `for-each-ref` 的统一格式（ADR-002）：一次调用同时喂给分支、标签与提交的 ref 标注。
///
/// 字段顺序：`objectname` \t `refname:short` \t `refname` \t `HEAD`。
/// 带上完整的 `refname` 才能按前缀区分 heads / tags / remotes；
/// 只靠 `refname:short` 无法区分（分支与标签可能同名）。
const REF_FORMAT: &str = "--format=%(objectname)%09%(refname:short)%09%(refname)%09%(HEAD)";

/// 缓存引用映射时，参与指纹的引用命名空间（TASK-018）。
///
/// 与 `refs()` 实际传给 `for-each-ref` 的三个前缀一致：多算会让缓存无谓失效，
/// 少算会让缓存**静默过期**。
const REF_PREFIXES: [&str; 3] = ["refs/heads", "refs/tags", "refs/remotes"];

/// 引用指纹的字节预算（4 MiB）。超出即放弃缓存、每次都重读 ——
/// 宁可多起一个子进程，也不要让「算缓存键」比它省下的那次调用更贵。
const REF_FINGERPRINT_BUDGET: usize = 4 * 1024 * 1024;

/// `git stash list` 的字段格式（US-1，补丁 P-09）。
///
/// 与 [`COMMIT_FORMAT`] 同款：`\x1f` 分隔字段、`\x1e` 分隔记录。
/// stash 的说明文字里可能含冒号、空格甚至换行，用默认的
/// `stash@{0}: WIP on …` 一行格式去切分会把它切坏。
///
/// 三个字段依次是：reflog 选择子（`stash@{0}`）、stash 提交、reflog 主题。
const STASH_FORMAT: &str = "--format=%gd\x1f%H\x1f%gs\x1e";

/// 一次 `for-each-ref` 的三种视图。
///
/// 合并的理由见 `ADR/002`：`snapshot()` 原先为分支、标签、ref 映射各起一次
/// `for-each-ref`，而三者读的是同一份引用表 —— 实测每起一次子进程约 30ms，
/// 三次合并成一次是把 `snapshot()` 从 5 次 spawn 降到 2 次的主要来源。
#[derive(Clone)]
struct Refs {
    branches: Vec<BranchInfo>,
    tags: Vec<TagInfo>,
    /// sha → 指向它的引用短名，用于给提交列表打 ref 标签。
    by_sha: HashMap<String, Vec<String>>,
}

/// 引用映射的缓存条目（TASK-018）。
///
/// `fingerprint` 与 [`ref_fingerprint`] 同源：只有它逐位相同才认为缓存仍有效。
struct CachedRefs {
    fingerprint: u64,
    refs: Refs,
}

/// 从 `status --porcelain=v2 --branch` 的头部行里拿到的分支与 HEAD。
///
/// 这两项原先各要一次 `symbolic-ref` / `rev-parse HEAD`；porcelain v2 的
/// `# branch.head` 与 `# branch.oid` 已经给了，合并掉可以省两次子进程。
struct HeadMeta {
    branch: Option<String>,
    commit: String,
    detached: bool,
}

/// Git 只读读取器。
///
/// **安全约束**：只允许白名单内的只读命令。
/// CI 会静态扫描本文件的 `Command::new("git")` 调用。
///
/// # 关于引用映射缓存（TASK-018）
///
/// 缓存**不是默认行为**：只有 [`Git::open_cached`] 构造的实例才带缓存，
/// [`Git::open`] 的每次调用都重新读一遍引用，行为与逐次成本与 TASK-018 之前完全一致。
/// 这样做的理由有两条：
///
/// 1. 现有的 spawn 次数门禁（`perf.rs::spawn_counts_are_pinned`）钉的是
///    「每次调用起几个进程」这种**与状态无关**的量。缓存一旦成为默认行为，
///    同一个方法就有了冷/热两个数字，门禁会退化成「看测试跑的顺序」。
/// 2. 缓存的有效性靠 [`ref_fingerprint`] 保证，这是一个需要单独成立的性质，
///    值得有自己的一组测试，而不是混进别处的断言里。
pub struct Git {
    repo: PathBuf,
    /// 绝对 git 目录。`HEAD` 在这里，且**链接工作树下每个工作树各有一份**。
    git_dir: PathBuf,
    /// 绝对公共 git 目录。`refs/` 与 `packed-refs` 在这里（链接工作树共享）。
    common_dir: PathBuf,
    /// 本实例自 `open()` 起发起的 `git` 子进程次数。
    ///
    /// 存在的理由（`ADR/002`）：实测表明**耗时几乎完全由子进程数决定**，
    /// 与仓库大小无关（4 万提交仓库上的 `for-each-ref` 与 `git --version`
    /// 耗时相同）。因此「spawn 次数」是一个**与机器无关**的性能契约，
    /// 比毫秒阈值更适合做回归门禁 —— 毫秒数会随 runner 抖动，spawn 数不会。
    spawns: AtomicUsize,
    /// 引用映射缓存。`None` 表示不缓存（[`Git::open`]）。
    refs_cache: Option<Mutex<Option<CachedRefs>>>,
}

impl Git {
    /// 打开仓库，**不缓存**引用映射。每次读取都重新起一次 `for-each-ref`。
    ///
    /// 适合一次性使用（CLI 命令、测试）；需要跨调用复用的宿主请用
    /// [`Git::open_cached`]。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_inner(path.as_ref(), false)
    }

    /// 打开仓库，并**跨调用复用引用映射**（TASK-018）。
    ///
    /// 收益来自一个实测事实：单个命令内部只会读一次引用，所以缓存的价值
    /// **完全在多次命令之间**。桌面端打开一个仓库要跑四条命令
    /// （快照 / 提交 / 分析 / 远端），其中三条都要引用映射 ——
    /// 缓存在这里的意义是把「每条命令各读一次」变成「整个会话读一次」。
    ///
    /// 失效由 [`ref_fingerprint`] 自动判定，不需要调用方声明「仓库变了」。
    pub fn open_cached(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_inner(path.as_ref(), true)
    }

    fn open_inner(path: &Path, cache_refs: bool) -> Result<Self> {
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .args([
                "rev-parse",
                // 必须放在被它影响的选项之前：否则 `--git-common-dir` 会相对 cwd 输出
                "--path-format=absolute",
                "--show-toplevel",
                "--absolute-git-dir",
                "--git-common-dir",
            ])
            .output()
            .context("failed to run git rev-parse")?;
        if !out.status.success() {
            anyhow::bail!("not a git repository: {}", path.display());
        }
        let text = String::from_utf8(out.stdout)?;
        let mut lines = text.lines();
        let repo = lines.next().unwrap_or("").trim();
        let git_dir = lines.next().unwrap_or("").trim();
        let common_dir = lines.next().unwrap_or("").trim();
        // 三行缺一不可：缓存要靠 git_dir / common_dir 定位引用数据，
        // 缺了就会去读一个空路径，指纹恒等于「空目录的哈希」——缓存永不失效。
        if repo.is_empty() || git_dir.is_empty() || common_dir.is_empty() {
            anyhow::bail!(
                "git rev-parse did not report the repository layout for {}",
                path.display()
            );
        }
        Ok(Self {
            repo: repo.into(),
            git_dir: git_dir.into(),
            common_dir: common_dir.into(),
            spawns: AtomicUsize::new(1),
            refs_cache: cache_refs.then(|| Mutex::new(None)),
        })
    }

    /// 本实例自 `open()`（含 `open()` 自己那一次）起发起的 `git` 子进程次数。
    ///
    /// 供性能回归断言使用。**开销可忽略**（一次 relaxed 原子读）。
    pub fn spawns(&self) -> usize {
        self.spawns.load(Ordering::Relaxed)
    }

    pub fn path(&self) -> &Path {
        &self.repo
    }

    /// 首屏快照。**恰好 2 次子进程**（`status` + 一次 `for-each-ref`）—— 见 `ADR/002`。
    ///
    /// P-09 之后它多了 `state`（进行中的操作），但**次数没变**：
    /// 状态是 `<git-dir>` 下的文件探测，一次进程都不起。
    pub fn snapshot(&self) -> Result<RepoSnapshot> {
        // 分支名、HEAD、上游计数全部来自同一次 `git status --branch` 的头部行
        // （`# branch.head` / `# branch.oid` / `# branch.upstream` / `# branch.ab`）；
        // 分支列表、标签列表与 ref 映射来自同一次 `for-each-ref`。
        // 这两条合并把 snapshot 从 5 次子进程降到 2 次，是全项目唯一值得优化的地方。
        let (status, upstream, meta) = self.status()?;
        let refs = self.refs()?;
        Ok(RepoSnapshot {
            path: self.repo.clone(),
            head: HeadInfo {
                branch: meta.branch,
                commit: meta.commit,
                detached: meta.detached,
                upstream,
            },
            branches: refs.branches,
            tags: refs.tags,
            status,
            // 零子进程，所以放得进这条 2 次的契约里
            state: self.state(),
        })
    }

    /// 进行中的操作（US-1，补丁 P-09）。**零子进程**。
    ///
    /// 只查 `<git-dir>` 下有没有标志文件，以及变基进度那两个数字文件。
    /// 链接工作树下的变基、摘取、合并，标志都写在该工作树自己的 `git_dir` 里，
    /// 所以这里用 `git_dir` 而不是共享的 `common_dir`。
    pub fn state(&self) -> Option<RepoState> {
        state_from_markers(&self.git_dir)
    }

    /// 工作树列表（US-1，补丁 P-09）。**恰好 1 次子进程**。
    ///
    /// 用 `--porcelain` 而不是默认的表格输出：后者会按终端宽度对齐到列、
    /// 按宽度换行，解析它等于把 git 的排版当契约。
    pub fn worktrees(&self) -> Result<Vec<WorktreeInfo>> {
        let out = self.run(&["worktree", "list", "--porcelain"])?;
        Ok(parse_worktrees(out.as_deref().unwrap_or("")))
    }

    /// stash 列表（US-1，补丁 P-09）。**恰好 1 次子进程**。
    pub fn stashes(&self) -> Result<Vec<StashInfo>> {
        let out = self.run(&["stash", "list", STASH_FORMAT])?;
        Ok(parse_stashes(out.as_deref().unwrap_or("")))
    }

    /// 工作区的附加列表：工作树 + stash。**恰好 2 次子进程**。
    ///
    /// 刻意**不**并进 [`Git::snapshot`]：这两项要起进程，合进去会让快照的次数
    /// 随「要不要这两块」而变化，那条「与状态无关」的门禁就不再是契约。
    /// 与 TASK-018 把引用缓存做成 opt-in 是同一条理由。
    pub fn workspace(&self) -> Result<WorkspaceInfo> {
        Ok(WorkspaceInfo {
            worktrees: self.worktrees()?,
            stashes: self.stashes()?,
        })
    }

    /// 读取提交图。
    ///
    /// **恰好 2 次子进程**：`git log` + 一次 `for-each-ref`（ref 映射）。
    pub fn commits(&self, limit: usize, skip: usize) -> Result<Vec<CommitInfo>> {
        let n = format!("-n{}", limit);
        let s = format!("--skip={}", skip);
        let fmt = format!("--format={}", COMMIT_FORMAT);
        let out = self.run(&["log", "--all", "--date=iso-strict", &fmt, &n, &s])?;

        let refs = self.refs()?;
        Ok(Self::parse_commits(
            out.as_deref().unwrap_or(""),
            &refs.by_sha,
        ))
    }

    /// 读取单个提交的元信息。
    pub fn commit(&self, sha: &str) -> Result<CommitInfo> {
        let fmt = format!("--format={}", COMMIT_FORMAT);
        let out = self.run(&["log", "--max-count=1", "--date=iso-strict", &fmt, sha])?;
        let refs = self.refs()?;
        Self::parse_commits(out.as_deref().unwrap_or(""), &refs.by_sha)
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

    /// 引用映射，带缓存（仅 [`Git::open_cached`] 构造的实例）。
    ///
    /// 命中判据是 [`ref_fingerprint`]：它直接哈希 git 解析引用所用的那几份数据，
    /// 因此「仓库在两次调用之间被改了」会被发现，缓存不会静默过期。
    ///
    /// 两种情况退化为「每次都重读」：实例没开缓存，或指纹算不出来
    /// （读失败 / 引用数据超出预算）。**退化的方向永远是「多起一个子进程」，
    /// 而不是「用一份可能过期的数据」**。
    fn refs(&self) -> Result<Refs> {
        let Some(slot) = &self.refs_cache else {
            return self.read_refs();
        };

        let fingerprint = ref_fingerprint(&self.git_dir, &self.common_dir);
        if let Some(fingerprint) = fingerprint {
            if let Ok(cache) = slot.lock() {
                if let Some(hit) = cache.as_ref().filter(|c| c.fingerprint == fingerprint) {
                    return Ok(hit.refs.clone());
                }
            }
        }

        let refs = self.read_refs()?;
        // 锁中毒时同样只是不缓存，不影响到手的结果。
        if let Some(fingerprint) = fingerprint {
            if let Ok(mut cache) = slot.lock() {
                *cache = Some(CachedRefs {
                    fingerprint,
                    refs: refs.clone(),
                });
            }
        }
        Ok(refs)
    }

    /// 一次 `for-each-ref` 同时给出分支、标签与「sha → 引用短名」映射（`ADR/002`）。
    ///
    /// 原先这是三次独立调用，读的却是同一张引用表。合并后**总输出顺序不变**：
    /// 原来 `ref_map()` 就是一次带三个前缀的调用，git 按完整 refname 排序，
    /// 因此 heads → remotes → tags 的顺序与合并前逐字相同。
    ///
    /// 前缀列表与 [`REF_PREFIXES`] 必须一致 —— 后者是缓存指纹的取样范围。
    fn read_refs(&self) -> Result<Refs> {
        let out = self.run(&[
            "for-each-ref",
            REF_FORMAT,
            "refs/heads/",
            "refs/tags/",
            "refs/remotes/",
        ])?;

        let mut refs = Refs {
            branches: Vec::new(),
            tags: Vec::new(),
            by_sha: HashMap::new(),
        };

        if let Some(out) = out {
            for line in out.lines() {
                let mut parts = line.split('\t');
                let sha = parts.next().unwrap_or("");
                let short = parts.next().unwrap_or("");
                let full = parts.next().unwrap_or("");
                let head_marker = parts.next().unwrap_or("");
                if sha.is_empty() || short.is_empty() {
                    continue;
                }

                refs.by_sha
                    .entry(sha.to_string())
                    .or_default()
                    .push(short.to_string());

                if full.starts_with("refs/heads/") {
                    refs.branches.push(BranchInfo {
                        name: short.to_string(),
                        commit: sha.to_string(),
                        is_current: head_marker == "*",
                    });
                } else if full.starts_with("refs/tags/") {
                    refs.tags.push(TagInfo {
                        name: short.to_string(),
                        commit: sha.to_string(),
                    });
                }
            }
        }

        Ok(refs)
    }

    /// 解析 `git status --porcelain=v2 --branch -z`。
    ///
    /// `-z` 使用 NUL 分隔条目，可安全处理路径中的空格与引号。
    ///
    /// 输出流由四部分组成：`# branch.*` 头部行（`--branch` 开启后才有）、
    /// 工作区条目、未跟踪条目，以及被忽略条目。每条记录都以 NUL 结尾，头部行也是——
    /// 因此不需要按换行切分，路径中含换行符也不会破坏解析。
    ///
    /// **三样东西都藏在这批头部行里**：上游计数（`# branch.upstream` / `# branch.ab`）、
    /// 分支名（`# branch.head`）、HEAD（`# branch.oid`）。原先后两项各要一次
    /// `symbolic-ref` / `rev-parse HEAD` 子进程，现在一并从同一次调用里取
    /// （`ADR/002`：每省一次子进程约省 30ms）。
    ///
    /// 头部行缺失时的降级：分支视为 `None`、`detached` 为 `true`、commit 为空串 ——
    /// 与「`symbolic-ref` 失败 + `rev-parse HEAD` 失败」的旧行为一致。
    /// 正常仓库不会走到这里，`snapshot_http` 之外另有一条测试钉住头部行的存在。
    fn status(&self) -> Result<(StatusInfo, Option<UpstreamInfo>, HeadMeta)> {
        let out = self.run(&["status", "--porcelain=v2", "--branch", "-z"])?;
        let mut info = StatusInfo::default();
        let mut upstream_name: Option<String> = None;
        let mut ahead_behind: Option<(u32, u32)> = None;
        let mut branch_head: Option<String> = None;
        let mut branch_oid: Option<String> = None;

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
                if let Some(rest) = entry.strip_prefix("# branch.head ") {
                    branch_head = Some(rest.trim().to_string());
                    continue;
                }
                if let Some(rest) = entry.strip_prefix("# branch.oid ") {
                    branch_oid = Some(rest.trim().to_string());
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

        // `# branch.head` 在分离头指针时是 `(detached)`，未出生分支上是分支名。
        // 任何以 `(` 开头的都是占位值，不算分支名。
        let meta = parse_head_meta(branch_head.as_deref(), branch_oid.as_deref());

        Ok((info, upstream, meta))
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
    ///
    /// **这里是全项目唯一的子进程出口**（`open()` 那一次除外），
    /// 因此 spawn 计数在这里累加 —— 见 [`Git::spawns`]。
    fn run(&self, args: &[&str]) -> Result<Option<String>> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(args)
            .output()
            .with_context(|| format!("failed to run git {:?}", args))?;
        self.spawns.fetch_add(1, Ordering::Relaxed);
        if !out.status.success() {
            return Ok(None);
        }
        Ok(Some(String::from_utf8(out.stdout)?))
    }
}

/// 从 `status --branch` 的 `# branch.head` / `# branch.oid` 推出分支与 HEAD（`ADR/002`）。
///
/// 抽成纯函数是为了让**降级分支可测**：git 正常输出时这两个头部行一定在，
/// 所以「缺失」这条路径在集成测试里根本构造不出来 —— 只有纯函数能钉住它。
fn parse_head_meta(branch_head: Option<&str>, branch_oid: Option<&str>) -> HeadMeta {
    HeadMeta {
        // `(detached)` / `(unknown)` 这类占位值都不算分支名。
        branch: branch_head
            .filter(|name| !name.starts_with('('))
            .map(str::to_string),
        // `(initial)` 是空仓库（尚无提交）的占位值，此时 HEAD 视为空串 ——
        // 与旧实现里 `rev-parse HEAD` 失败后取默认值的表现一致。
        commit: branch_oid
            .filter(|oid| !oid.starts_with('('))
            .unwrap_or("")
            .to_string(),
        // 分隔：分支名取不到时按分离头指针处理（保守：不谎称在某个分支上）。
        detached: match branch_head {
            Some(name) => name.starts_with('('),
            None => true,
        },
    }
}

/// 由 `<git-dir>` 下的标志推出进行中的操作（US-1，补丁 P-09）。**零子进程**。
///
/// # 为什么按这个顺序
///
/// **最具体的标记优先**，命中第一个即返回：
///
/// ```text
/// rebase-merge → rebase-apply → CHERRY_PICK_HEAD → REVERT_HEAD → MERGE_HEAD → BISECT_LOG
/// ```
///
/// 变基排在合并之前，是因为变基**自己会**在冲突时留下一份合并状态：
/// 若不先看变基目录，正在变基的仓库会被报成「合并中」。
/// 摘取 / 回退同理（它们也停在合并式的冲突上）。
/// 顺序不是猜的 —— 每一条都由 `tests/workspace.rs` 用真实夹具逐个构造验证。
///
/// 抽成纯函数（只碰文件系统、不碰进程）是为了让每条分支都能被断言：
/// 「同时在场的多个标志报哪一个」「进度读不到时怎么办」这类问题，
/// 靠一个真实仓库碰运气是测不全的。
fn state_from_markers(git_dir: &Path) -> Option<RepoState> {
    let marker = |name: &str| git_dir.join(name);

    // 两种变基后端：`rebase-merge`（默认的 merge 后端）与
    // `rebase-apply`（`--apply` / `am` 后端）。进度文件名不同。
    if marker("rebase-merge").is_dir() {
        let dir = marker("rebase-merge");
        let (step, total) = (
            read_number(&dir.join("msgnum")),
            read_number(&dir.join("end")),
        );
        return Some(RepoState::Rebase { step, total });
    }
    if marker("rebase-apply").is_dir() {
        let dir = marker("rebase-apply");
        let (step, total) = (
            read_number(&dir.join("next")),
            read_number(&dir.join("last")),
        );
        return Some(RepoState::Rebase { step, total });
    }
    if marker("CHERRY_PICK_HEAD").exists() {
        return Some(RepoState::CherryPick);
    }
    if marker("REVERT_HEAD").exists() {
        return Some(RepoState::Revert);
    }
    if marker("MERGE_HEAD").exists() {
        return Some(RepoState::Merge);
    }
    if marker("BISECT_LOG").exists() {
        return Some(RepoState::Bisect);
    }
    None
}

/// 读一个单行十进制数文件（变基进度的 `msgnum` / `end` / `next` / `last`）。
///
/// 读不到、不是数字、或超出 `u32` 都返回 `None`。**调用方不得因此改变状态**：
/// 报不出进度不等于没在变基。
fn read_number(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// 解析 `git worktree list --porcelain`（US-1，补丁 P-09）。
///
/// 输出是**空行分隔的记录块**，每块形如（`HEAD` 与分支二选一的那个必有）：
///
/// ```text
/// worktree /path/to/wt          # 必有，块的第一行
/// HEAD 1a2b3c4                  # 必有
/// branch refs/heads/main        # 或 detached，二者互斥
/// bare                          # 可选
/// locked [原因]                 # 可选
/// prunable [原因]               # 可选
/// ```
///
/// `branch` 给的是**完整 ref**，这里削成短名 —— 与 `BranchInfo.name` 保持同一口径。
///
/// 未知键一律忽略而不是报错：git 版本升级时多出新键是常态，
/// 为它失败会让整个面板空掉，而少显示一行信息是可接受的降级。
///
/// 逐行状态机而非按 `\n\n` 切块：Windows 上 git 的输出行尾是 CRLF，
/// 按空行字符串切会一块都切不开（`str::lines` 会连行尾的 `\r` 一起去掉）。
fn parse_worktrees(raw: &str) -> Vec<WorktreeInfo> {
    let mut out: Vec<WorktreeInfo> = Vec::new();
    let mut current: Option<WorktreeInfo> = None;

    for line in raw.lines() {
        if line.trim().is_empty() {
            if let Some(tree) = current.take() {
                out.push(tree);
            }
            continue;
        }

        let mut parts = line.splitn(2, ' ');
        let key = parts.next().unwrap_or("");
        // 只有 `locked` / `prunable` 会带原因，其余行的值就是全部剩余内容
        // （路径可能含空格，因此用 splitn(2) 而不是 split）。
        let value = parts.next().unwrap_or("").trim();

        match key {
            "worktree" => {
                if let Some(tree) = current.take() {
                    out.push(tree);
                }
                current = Some(WorktreeInfo {
                    path: PathBuf::from(value),
                    branch: None,
                    commit: String::new(),
                    // git 保证主工作树是第一条记录
                    is_main: out.is_empty(),
                    bare: false,
                    detached: false,
                    locked: false,
                });
            }
            "HEAD" => {
                if let Some(tree) = current.as_mut() {
                    tree.commit = value.to_string();
                }
            }
            "branch" => {
                if let Some(tree) = current.as_mut() {
                    tree.branch = Some(
                        value
                            .strip_prefix("refs/heads/")
                            .unwrap_or(value)
                            .to_string(),
                    );
                }
            }
            "detached" => {
                if let Some(tree) = current.as_mut() {
                    tree.detached = true;
                }
            }
            "bare" => {
                if let Some(tree) = current.as_mut() {
                    tree.bare = true;
                }
            }
            "locked" => {
                if let Some(tree) = current.as_mut() {
                    tree.locked = true;
                }
            }
            // `prunable` 与未知键：看清了就够了，不为它们建模
            _ => {}
        }
    }

    if let Some(tree) = current.take() {
        out.push(tree);
    }
    out
}

/// 解析 `git stash list` 的 [`STASH_FORMAT`] 输出（US-1，补丁 P-09）。
///
/// 与提交列表同款的 `\x1f` / `\x1e` 分隔。字段数不足 3 的记录整条丢弃 ——
/// 宁可少一条，也不要吐出一个 `commit` 为空、点进去必报错的假条目。
fn parse_stashes(raw: &str) -> Vec<StashInfo> {
    let mut out = Vec::new();
    for record in raw.split('\x1e') {
        let record = record.trim_matches(['\n', '\r']);
        if record.is_empty() {
            continue;
        }
        let parts: Vec<&str> = record.split('\x1f').collect();
        if parts.len() < 3 {
            continue;
        }
        out.push(StashInfo {
            reference: parts[0].to_string(),
            commit: parts[1].to_string(),
            message: parts[2].to_string(),
        });
    }
    out
}

/// 引用映射的内容指纹（TASK-018）。
///
/// # 为什么不用 mtime
///
/// 缓存唯一不可接受的失效是「过期了却没人知道」。文件时间戳不足以判定这件事：
/// 多数文件系统的时间戳粒度到秒，而 `git update-ref` 改写一个已存在的松引用时
/// **连文件大小都不变**（内容恒为 40/64 位十六进制 + 换行）。同一秒内的改写会被漏掉。
///
/// # 改成什么
///
/// 直接哈希 git 用来解析引用的那几份数据本身：
///
/// - `<git-dir>/HEAD` —— 决定当前分支与 `%(HEAD)` 标记；链接工作树下每个工作树各一份
/// - `<common-dir>/packed-refs` —— 多数仓库的引用都在这里
/// - `<common-dir>/refs/{heads,tags,remotes}/**` —— 松引用，递归收集后排序
///
/// **相对路径也参与哈希**，否则「改名但内容相同」不会改变指纹。
/// 排序保证指纹与目录遍历顺序无关。
///
/// 读失败或总量超出 [`REF_FINGERPRINT_BUDGET`] 时返回 `None`，
/// 调用方据此退化为「每次都重读」。这是刻意选的偏向：
/// 宁可多起一个子进程，也不要拿一份可能过期的映射继续用。
fn ref_fingerprint(git_dir: &Path, common_dir: &Path) -> Option<u64> {
    let mut hasher = DefaultHasher::new();
    let mut budget = REF_FINGERPRINT_BUDGET;

    // HEAD 缺失说明这不是一个可用的仓库布局，直接放弃缓存。
    hash_file(&mut hasher, &git_dir.join("HEAD"), &mut budget)?;

    // packed-refs 不存在是正常的（全部为松引用），但存在却读不了则放弃缓存。
    let packed = common_dir.join("packed-refs");
    if packed.exists() {
        hash_file(&mut hasher, &packed, &mut budget)?;
    }

    let mut loose = Vec::new();
    for prefix in REF_PREFIXES {
        collect_files(&common_dir.join(prefix), &mut loose)?;
    }
    loose.sort();
    for file in &loose {
        file.strip_prefix(common_dir)
            .unwrap_or(file)
            .to_string_lossy()
            .hash(&mut hasher);
        hash_file(&mut hasher, file, &mut budget)?;
    }

    Some(hasher.finish())
}

/// 把单个文件的内容并进指纹。读不了或超出预算都返回 `None`（→ 放弃缓存）。
fn hash_file(hasher: &mut DefaultHasher, path: &Path, budget: &mut usize) -> Option<()> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > *budget {
        return None;
    }
    *budget -= bytes.len();
    bytes.hash(hasher);
    Some(())
}

/// 递归收集目录下的所有文件。
///
/// 返回 `None` 表示**指纹不完整，必须放弃缓存**。这里刻意区分两种情况：
///
/// - 目录不存在 → 是该命名空间为空（例如仓库没有标签），指纹照样成立，返回 `Some`
/// - 目录存在却读不了（权限、IO 错误、条目损坏）→ 指纹会少覆盖一块内容，
///   表现为「缓存该失效时没失效」，因此返回 `None`
///
/// 区别对待的理由：前者的「缺失」本身是确定的事实，后者的「缺失」是未知。
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Option<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) if !dir.is_dir() => return Some(()),
        Err(_) => return None,
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return None;
        };
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Some(())
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
    use super::{
        cap_patch, parse_ahead_behind, parse_head_meta, parse_remote, parse_stashes,
        parse_worktrees, read_number, state_from_markers, MAX_DIFF_BYTES,
    };
    use crate::model::RepoState;

    // 注意：本模块位于 core/src 下，会被 read-only guard 扫描。
    // 第四层（写动词黑名单）**全局生效、不看语句是否在调 Git**，所以这里
    // 不出现任何被引号包起来的写动词字面量（连注释里也不行）。

    #[test]
    fn head_meta_reads_porcelain_v2_headers() {
        let meta = parse_head_meta(Some("main"), Some("abc123"));
        assert_eq!(meta.branch.as_deref(), Some("main"));
        assert_eq!(meta.commit, "abc123");
        assert!(!meta.detached, "有分支名就不是分离头指针");
    }

    #[test]
    fn head_meta_rejects_placeholder_values() {
        // 分离头指针：`# branch.head` 是 `(detached)`，但 oid 是真实的。
        let detached = parse_head_meta(Some("(detached)"), Some("abc123"));
        assert_eq!(detached.branch, None, "`(detached)` 不能当分支名透给前端");
        assert!(detached.detached);
        assert_eq!(detached.commit, "abc123", "分离头指针仍有 commit");

        // 空仓库（尚无提交）：分支名是真的，oid 是占位值。
        let initial = parse_head_meta(Some("main"), Some("(initial)"));
        assert_eq!(initial.branch.as_deref(), Some("main"));
        assert_eq!(initial.commit, "", "`(initial)` 不能当成 sha");
        assert!(!initial.detached, "未出生分支仍未分离");
    }

    /// git 正常输出时不会走到这条路径。它钉住的是「万一 porcelain v2 的头部行改名了」
    /// 时的降级：宁可退化成「未知的分离头指针」，也不要把占位字符串当数据送出去。
    #[test]
    fn head_meta_degrades_to_detached_when_headers_are_missing() {
        let meta = parse_head_meta(None, None);
        assert_eq!(meta.branch, None);
        assert!(meta.detached);
        assert_eq!(meta.commit, "");
    }
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

    // -----------------------------------------------------------------------
    // P-09：进行中状态的标志探测（零子进程）
    // -----------------------------------------------------------------------

    /// 建一个进程内唯一的临时目录。
    ///
    /// 刻意**不**复用 `tests/common` 的 `TempRepo` —— 那个夹具会 `git init`，
    /// 而这一组用例测的正是「一个进程都不起」这件事：给一个空目录就够，
    /// 顺便也证明了标志探测根本不需要仓库。
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before UNIX epoch")
            .as_nanos();
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("repoprism-state-{tag}-{nanos}-{seq}"));
        std::fs::create_dir_all(&path).expect("failed to create temp dir");
        path
    }

    fn write_marker(dir: &std::path::Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).expect("failed to write marker");
    }

    #[test]
    fn state_is_none_when_no_marker_is_present() {
        let dir = temp_dir("marker-order");
        // 目录里有正常文件也不该被误判 —— 只有标志文件算数
        write_marker(&dir, "HEAD", "ref: refs/heads/main\n");

        assert_eq!(state_from_markers(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_prefers_the_most_specific_marker() {
        let dir = temp_dir("priority");

        // 变基自己在冲突时会留下一份合并状态，所以变基必须压过合并
        write_marker(&dir, "MERGE_HEAD", "1a2b3c4\n");
        write_marker(&dir, "CHERRY_PICK_HEAD", "1a2b3c4\n");
        std::fs::create_dir_all(dir.join("rebase-merge")).expect("mkdir rebase-merge");
        assert_eq!(
            state_from_markers(&dir),
            Some(RepoState::Rebase {
                step: None,
                total: None
            }),
            "变基目录在场时应报变基，而不是合并"
        );

        // 拿掉变基目录，摘取提示压过合并提示
        std::fs::remove_dir_all(dir.join("rebase-merge")).expect("rmdir marker");
        assert_eq!(state_from_markers(&dir), Some(RepoState::CherryPick));

        // 再拿掉摘取，轮到回退
        std::fs::remove_file(dir.join("CHERRY_PICK_HEAD")).expect("unlink marker");
        write_marker(&dir, "REVERT_HEAD", "1a2b3c4\n");
        assert_eq!(state_from_markers(&dir), Some(RepoState::Revert));

        // 只剩合并
        std::fs::remove_file(dir.join("REVERT_HEAD")).expect("unlink marker");
        assert_eq!(state_from_markers(&dir), Some(RepoState::Merge));

        // 最后才轮到二分查找
        std::fs::remove_file(dir.join("MERGE_HEAD")).expect("unlink marker");
        write_marker(&dir, "BISECT_LOG", "记录\n");
        assert_eq!(state_from_markers(&dir), Some(RepoState::Bisect));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rebase_reads_progress_from_both_backend_layouts() {
        // 默认的合并后端：msgnum / end
        let root = temp_dir("rebase-merge");
        let dir = root.join("rebase-merge");
        std::fs::create_dir_all(&dir).expect("mkdir");
        write_marker(&dir, "msgnum", "3\n");
        write_marker(&dir, "end", "7\n");
        assert_eq!(
            state_from_markers(&root),
            Some(RepoState::Rebase {
                step: Some(3),
                total: Some(7)
            })
        );
        let _ = std::fs::remove_dir_all(&root);

        // 打补丁后端：next / last
        let root = temp_dir("rebase-apply");
        let dir = root.join("rebase-apply");
        std::fs::create_dir_all(&dir).expect("mkdir");
        write_marker(&dir, "next", "1\n");
        write_marker(&dir, "last", "2\n");
        assert_eq!(
            state_from_markers(&root),
            Some(RepoState::Rebase {
                step: Some(1),
                total: Some(2)
            })
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 进度读不到时**只降级进度，不改变状态**：
    /// 「报不出第几步」与「没在变基」是两件事。
    #[test]
    fn rebase_without_readable_progress_is_still_a_rebase() {
        let root = temp_dir("rebase-noprogress");
        let dir = root.join("rebase-merge");
        std::fs::create_dir_all(&dir).expect("mkdir");

        assert_eq!(
            state_from_markers(&root),
            Some(RepoState::Rebase {
                step: None,
                total: None
            }),
            "进度文件缺失时状态仍成立"
        );

        write_marker(&dir, "msgnum", "not-a-number\n");
        write_marker(&dir, "end", "7\n");
        assert_eq!(
            state_from_markers(&root),
            Some(RepoState::Rebase {
                step: None,
                total: Some(7)
            }),
            "进度不是数字时只丢那一侧"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn read_number_accepts_only_a_single_plain_decimal() {
        let dir = temp_dir("numbers");
        let file = dir.join("progress");

        std::fs::write(&file, " 42 \n").expect("write");
        assert_eq!(read_number(&file), Some(42), "两侧空白应被容忍");

        std::fs::write(&file, "42\n7\n").expect("write");
        assert_eq!(read_number(&file), None, "两行不是合法进度");

        std::fs::write(&file, "-1\n").expect("write");
        assert_eq!(read_number(&file), None, "负数不是合法进度");

        assert_eq!(read_number(&dir.join("absent")), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // P-09：worktree / stash 列表解析
    // -----------------------------------------------------------------------

    #[test]
    fn worktrees_parse_main_and_linked_entries() {
        let raw = "worktree /repos/main\n\
HEAD 1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b\n\
branch refs/heads/main\n\n\
worktree /repos/linked\n\
HEAD 5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e\n\
detached\n\n";

        let trees = parse_worktrees(raw);

        assert_eq!(trees.len(), 2);
        assert_eq!(trees[0].path, std::path::PathBuf::from("/repos/main"));
        assert_eq!(
            trees[0].branch.as_deref(),
            Some("main"),
            "refs/heads/ 前缀必须削掉，与分支列表同一口径"
        );
        assert!(trees[0].is_main, "第一条记录是主工作树");
        assert!(!trees[0].detached, "有分支名就不是分离头指针");

        assert_eq!(trees[1].path, std::path::PathBuf::from("/repos/linked"));
        assert_eq!(trees[1].branch, None, "分离头指针的工作树没有分支名");
        assert!(trees[1].detached);
        assert!(!trees[1].is_main);
    }

    #[test]
    fn worktrees_keep_paths_that_contain_spaces() {
        let raw = "worktree /repos/my repo\nHEAD 1a2b3c4\nbranch refs/heads/main\n\n";

        let trees = parse_worktrees(raw);
        assert_eq!(trees[0].path, std::path::PathBuf::from("/repos/my repo"));
    }

    /// Windows 上 git 的输出是 CRLF。按空行字符串切块会**一块也切不开**，
    /// 因此这里用逐行状态机，并钉住 CRLF 能被吃下。
    #[test]
    fn worktrees_tolerate_crlf_and_keys_with_a_reason() {
        let raw = "worktree C:\\repos\\main\r\n\
HEAD 1a2b3c4\r\n\
branch refs/heads/main\r\n\
locked 维护中\r\n\r\n";

        let trees = parse_worktrees(raw);
        assert_eq!(trees.len(), 1);
        assert_eq!(trees[0].branch.as_deref(), Some("main"));
        assert!(trees[0].locked, "带原因的 locked 行也要认出来");
        assert!(!trees[0].bare);
    }

    #[test]
    fn worktrees_mark_bare_entries() {
        let raw = "worktree /repos/bare\nHEAD 1a2b3c4\nbare\n\n";

        let trees = parse_worktrees(raw);
        assert_eq!(trees.len(), 1);
        assert!(trees[0].bare);
        assert_eq!(trees[0].branch, None, "bare 工作树没有检出的分支");
    }

    /// 未知键忽略而不是报错：git 版本升级时加键是常态，
    /// 为它失败会让整个面板空掉，而少显示一行是可接受的降级。
    #[test]
    fn worktrees_ignore_keys_they_do_not_model() {
        let raw = "worktree /repos/main\n\
HEAD 1a2b3c4\n\
branch refs/heads/main\n\
prunable gitdir 文件缺失\n\
some-future-key 值\n\n";

        let trees = parse_worktrees(raw);
        assert_eq!(trees.len(), 1);
        assert_eq!(trees[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn worktrees_of_empty_output_is_an_empty_list() {
        assert!(parse_worktrees("").is_empty());
    }

    #[test]
    fn stashes_parse_every_field() {
        let raw = "stash@{0}\x1f1a2b3c4\x1fWIP on main: 5d6e7f8 second\n\x1e\
stash@{1}\x1f9f8e7d6\x1fWIP on main: 1a2b3c4 first\n\x1e";

        let stashes = parse_stashes(raw);

        assert_eq!(stashes.len(), 2);
        assert_eq!(stashes[0].reference, "stash@{0}");
        assert_eq!(stashes[0].commit, "1a2b3c4");
        assert_eq!(stashes[0].message, "WIP on main: 5d6e7f8 second");
        assert_eq!(stashes[1].reference, "stash@{1}");
        assert_eq!(stashes[1].commit, "9f8e7d6");
    }

    /// 字段不足的记录**整条丢弃**：宁可少一条，也不要吐出一个
    /// `commit` 为空、点进去必报错的假条目。
    #[test]
    fn stashes_drop_records_that_lose_a_field() {
        let raw = "stash@{0}\x1fall-good\x1f完整说明\n\x1estash@{1}\x1f\n\x1e";

        let stashes = parse_stashes(raw);
        assert_eq!(stashes.len(), 1);
        assert_eq!(stashes[0].reference, "stash@{0}");
    }

    #[test]
    fn stashes_of_empty_output_is_an_empty_list() {
        assert!(parse_stashes("").is_empty());
    }
}
