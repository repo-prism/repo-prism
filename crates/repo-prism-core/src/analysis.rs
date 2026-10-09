//! 变更分析与风险标记（US-7 的本地部分）。
//!
//! **纯本地、只读**。不调用任何外部 API、不访问网络。
//! LLM 摘要层通过 [`Summarizer`] trait 抽象，默认使用 [`NoopSummarizer`]，
//! 真正的模型接入（本地 Ollama）留给后续任务卡。
//!
//! # 规则引擎为什么用「数据表 + 函数指针」而不是一串 if
//!
//! 规则是**数据**：每条规则有稳定的 `id`（前端角标与测试都按 id 断言）、
//! 一个等级、一个判定函数、一个措辞函数。加规则 = 往规则表里加一行，
//! 不动引擎逻辑；测试也不必跟着改结构。
//!
//! # 与归档规划的偏离（两处，均为让验收标准真正成立）
//!
//! 1. 归档给出的规则表只有 8 条，但同一条验收标准要求「覆盖异常处理、迁移、
//!    API、配置、CI、依赖、测试、**大量删除**」。**大量删除**必须知道行数，
//!    而 `FileChange` 只有路径与类型 —— 于是引入 [`ChangeFacts`]，把工作区
//!    diff 的行数统计（[`LineStat`]，来自 `git diff --numstat`）作为**可选**
//!    输入带进规则。归档里「结构化解析 diff（hunks、文件、增删行）」这一条
//!    目标也正是为此，本模块现在真的用上了它。
//! 2. 归档的 `public-api` 规则写作 `contains("/src/")`，对 `src/a.rs`
//!    这类顶层路径**恒为假**（没有前导斜杠），且未排除 `.spec.*` 与 `tests/`。
//!    这里改成按扩展名判定并以共享的测试路径判定排除测试。
//!
//! 两条都写进任务卡，不埋在代码里。

use crate::model::{ChangeKind, LineStat, LineStats, StatusInfo};
use serde::{Deserialize, Serialize};

/// 风险等级。前端按 `info < warn < critical` 排序取最高项作角标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Info,
    Warn,
    Critical,
}

/// 一条命中的风险。`rule_id` 稳定，可供前端与测试断言。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Risk {
    pub rule_id: String,
    pub level: RiskLevel,
    pub message: String,
    /// 命中的文件路径。文件名级别的规则也记完整路径，便于前端按行定位。
    pub path: String,
}

/// 按等级的风险计数。前端用于「N 关键 / N 警告」两个 pill。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskCounts {
    pub info: usize,
    pub warn: usize,
    pub critical: usize,
}

/// 一次变更分析的完整结果。这就是 Tauri 命令 `analyze_changes` 的返回值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeAnalysis {
    /// 人类可读的一句话摘要，由规则结果拼出，不做模型调用。
    pub summary: String,
    pub total_files: usize,
    pub risks: Vec<Risk>,
    pub by_level: RiskCounts,
}

impl ChangeAnalysis {
    /// 只用 `status` 分析：规则只看路径、变更类型与分组。
    ///
    /// 「大量删除」这类需要行数的规则在此**不会命中**（`:stats` 为 `None`），
    /// 而不是误报。
    pub fn from_status(status: &StatusInfo) -> Self {
        Self::build(status, &LineStats::new())
    }

    /// 带工作区行数统计的分析。行数覆盖不到的路径退化为纯路径判定。
    pub fn from_status_and_stats(status: &StatusInfo, stats: &LineStats) -> Self {
        Self::build(status, stats)
    }

    fn build(status: &StatusInfo, stats: &LineStats) -> Self {
        let groups = [
            (ChangeGroup::Conflict, &status.conflicts),
            (ChangeGroup::Staged, &status.staged),
            (ChangeGroup::Unstaged, &status.unstaged),
        ];

        let mut risks = Vec::new();
        let mut total_files = 0usize;

        for (group, files) in groups {
            for change in files {
                total_files += 1;
                // `LineStats` 以路径为键；重命名时 status 报的也是新路径，能对上。
                let facts = ChangeFacts {
                    path: &change.path,
                    kind: change.kind,
                    group,
                    stats: stats.get(&change.path).copied(),
                };
                risks.extend(apply_rules(&facts));
            }
        }

        let by_level = count_by_level(&risks);
        let summary = build_summary(total_files, &by_level, &risks);
        Self {
            summary,
            total_files,
            risks,
            by_level,
        }
    }
}

/// 变更所属的分组，与前端「冲突 / 已暂存 / 工作区」三栏一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeGroup {
    Conflict,
    Staged,
    Unstaged,
}

impl ChangeGroup {
    /// 稳定标识，规则与测试按它断言，不用中文显示名。
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeGroup::Conflict => "conflict",
            ChangeGroup::Staged => "staged",
            ChangeGroup::Unstaged => "unstaged",
        }
    }
}

/// 规则引擎的输入。只带规则真正需要的东西，避免规则去翻整个 `StatusInfo`。
#[derive(Debug, Clone, Copy)]
pub struct ChangeFacts<'a> {
    pub path: &'a str,
    pub kind: ChangeKind,
    pub group: ChangeGroup,
    /// 该文件在工作区 diff 中的行数；`status` 路径下为 `None`。
    pub stats: Option<LineStat>,
}

/// 单条规则。规则用数据描述，便于扩展与测试。
struct Rule {
    id: &'static str,
    level: RiskLevel,
    /// 返回 `true` 表示命中。
    matches: for<'a> fn(&ChangeFacts<'a>) -> bool,
    message: for<'a> fn(&ChangeFacts<'a>) -> String,
}

/// 规则表。**加规则只往这里加一行**，引擎逻辑不用改。
///
/// 覆盖验收标准点名的 8 类：异常处理、迁移、API、配置、CI、依赖、测试、大量删除。
/// 「配置」由 `ci-config` + `dependency-manifest` + `env-or-secret` 三条合起来覆盖。
const RULES: &[Rule] = &[
    Rule {
        id: "conflict-in-progress",
        level: RiskLevel::Critical,
        matches: is_conflict,
        message: msg_conflict,
    },
    Rule {
        id: "env-or-secret",
        level: RiskLevel::Critical,
        matches: is_env_or_secret,
        message: msg_env_or_secret,
    },
    Rule {
        id: "migration-file",
        level: RiskLevel::Critical,
        matches: is_migration,
        message: msg_migration,
    },
    Rule {
        id: "ci-config",
        level: RiskLevel::Warn,
        matches: is_ci_config,
        message: msg_ci_config,
    },
    Rule {
        id: "dependency-manifest",
        level: RiskLevel::Warn,
        matches: is_dependency_manifest,
        message: msg_dependency_manifest,
    },
    Rule {
        id: "test-deleted",
        level: RiskLevel::Warn,
        matches: is_deleted_test,
        message: msg_deleted_test,
    },
    Rule {
        id: "mass-deletion",
        level: RiskLevel::Warn,
        matches: is_mass_deletion,
        message: msg_mass_deletion,
    },
    Rule {
        id: "error-handling",
        level: RiskLevel::Info,
        matches: is_error_handling,
        message: msg_error_handling,
    },
    Rule {
        id: "public-api",
        level: RiskLevel::Info,
        matches: is_public_api,
        message: msg_public_api,
    },
    Rule {
        id: "lockfile-only",
        level: RiskLevel::Info,
        matches: is_lockfile,
        message: msg_lockfile,
    },
];

fn apply_rules(facts: &ChangeFacts<'_>) -> Vec<Risk> {
    RULES
        .iter()
        .filter(|rule| (rule.matches)(facts))
        .map(|rule| Risk {
            rule_id: rule.id.to_string(),
            level: rule.level,
            message: (rule.message)(facts),
            path: facts.path.to_string(),
        })
        .collect()
}

fn count_by_level(risks: &[Risk]) -> RiskCounts {
    let mut counts = RiskCounts::default();
    for risk in risks {
        match risk.level {
            RiskLevel::Info => counts.info += 1,
            RiskLevel::Warn => counts.warn += 1,
            RiskLevel::Critical => counts.critical += 1,
        }
    }
    counts
}

// ---------------------------------------------------------------------------
// 判定
// ---------------------------------------------------------------------------

fn is_conflict(facts: &ChangeFacts<'_>) -> bool {
    facts.group == ChangeGroup::Conflict
}

fn is_env_or_secret(facts: &ChangeFacts<'_>) -> bool {
    let name = basename_lower(facts.path);
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.contains("secret")
        || name.contains("credential")
}

fn is_migration(facts: &ChangeFacts<'_>) -> bool {
    let path = facts.path.to_lowercase();
    path.contains("migrations/")
        || path.contains("migration/")
        || path.ends_with(".sql")
        || path.contains("/schema/")
        || path.starts_with("schema/")
}

fn is_ci_config(facts: &ChangeFacts<'_>) -> bool {
    let path = facts.path.to_lowercase();
    path.starts_with(".github/workflows/")
        || path.starts_with(".gitlab-ci")
        || path.contains("jenkinsfile")
        || path.contains(".circleci/")
}

fn is_dependency_manifest(facts: &ChangeFacts<'_>) -> bool {
    matches!(
        basename_lower(facts.path).as_str(),
        "cargo.toml"
            | "cargo.lock"
            | "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "requirements.txt"
            | "pyproject.toml"
            | "poetry.lock"
            | "go.mod"
            | "go.sum"
            | "gemfile"
            | "gemfile.lock"
    )
}

fn is_deleted_test(facts: &ChangeFacts<'_>) -> bool {
    facts.kind == ChangeKind::Deleted && is_test_path(facts.path)
}

/// 「大量删除」：删行数达到绝对下限，且**远超**增行数。
///
/// 只读得到行数时判定；读不到（纯 `status` 路径）一律不命中，避免误报。
/// 100 行是经验下限：低于它的小改动不该占用「警告」等级的注意力。
fn is_mass_deletion(facts: &ChangeFacts<'_>) -> bool {
    const MIN_DELETIONS: u32 = 100;
    let Some(stats) = facts.stats else {
        return false;
    };
    stats.deletions >= MIN_DELETIONS && stats.deletions > stats.additions.saturating_mul(3)
}

fn is_error_handling(facts: &ChangeFacts<'_>) -> bool {
    let name = basename_lower(facts.path);
    name.contains("error") || name.contains("exception") || name.contains("panic")
}

fn is_public_api(facts: &ChangeFacts<'_>) -> bool {
    let path = facts.path.to_lowercase();
    is_source_extension(&path) && !is_test_path(&path)
}

fn is_lockfile(facts: &ChangeFacts<'_>) -> bool {
    let path = facts.path.to_lowercase();
    path.ends_with(".lock") || path.ends_with("-lock.json") || path.ends_with("-lock.yaml")
}

/// 测试文件的统一判定。`public-api` 与 `test-deleted` 共用，避免两处口径漂移。
fn is_test_path(path: &str) -> bool {
    let path = path.to_lowercase();
    let name = basename(&path);
    path.contains("/tests/")
        || path.contains("/test/")
        || path.contains("/__tests__/")
        || path.contains(".test.")
        || path.contains(".spec.")
        || name.starts_with("test_")
        || name.ends_with("_test.rs")
        || name.ends_with("_test.go")
        || name.ends_with("_test.py")
}

fn is_source_extension(path: &str) -> bool {
    matches!(
        extension(path),
        Some("rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "kt" | "swift")
    )
}

/// 取路径最后一段。**输入须已小写**；`status` 与 `--numstat` 给出的都是
/// `/` 分隔，无需处理 `\`。
fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// 小写 basename。规则表按小写比对文件名：`Cargo.toml` 与 `cargo.toml`
/// 是同一个文件，字面量比对必须先统一大小写（归档口径也是先 lowercase 整条路径）。
fn basename_lower(path: &str) -> String {
    path.to_lowercase()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn extension(path: &str) -> Option<&str> {
    // 以 `.` 开头的隐藏文件（`.env`）不算扩展名，否则 `.env` 会被判成 `env`
    let name = basename(path);
    if name.starts_with('.') {
        return None;
    }
    name.rsplit_once('.').map(|(_, extension)| extension)
}

// ---------------------------------------------------------------------------
// 措辞
// ---------------------------------------------------------------------------

fn msg_conflict(facts: &ChangeFacts<'_>) -> String {
    format!("未解决的合并冲突：{}", facts.path)
}

fn msg_env_or_secret(facts: &ChangeFacts<'_>) -> String {
    format!("疑似密钥或凭据文件，确认是否应提交：{}", facts.path)
}

fn msg_migration(facts: &ChangeFacts<'_>) -> String {
    format!("数据库结构或迁移文件，需谨慎审查：{}", facts.path)
}

fn msg_ci_config(facts: &ChangeFacts<'_>) -> String {
    format!("CI 配置变更，可能影响流水线：{}", facts.path)
}

fn msg_dependency_manifest(facts: &ChangeFacts<'_>) -> String {
    format!("依赖清单变更，需关注版本风险：{}", facts.path)
}

fn msg_deleted_test(facts: &ChangeFacts<'_>) -> String {
    format!("测试文件被删除，确认是否有意为之：{}", facts.path)
}

fn msg_mass_deletion(facts: &ChangeFacts<'_>) -> String {
    match facts.stats {
        Some(stats) => format!(
            "单文件大量删除（-{} / +{}），确认不是误删：{}",
            stats.deletions, stats.additions, facts.path
        ),
        None => format!("疑似大量删除：{}", facts.path),
    }
}

fn msg_error_handling(facts: &ChangeFacts<'_>) -> String {
    format!("疑似异常处理路径变更，关注错误分支：{}", facts.path)
}

fn msg_public_api(facts: &ChangeFacts<'_>) -> String {
    format!("源码变更，可能影响外部调用：{}", facts.path)
}

fn msg_lockfile(facts: &ChangeFacts<'_>) -> String {
    format!("锁文件变更，确认是否有源码变更配套：{}", facts.path)
}

// ---------------------------------------------------------------------------
// 摘要
// ---------------------------------------------------------------------------

fn build_summary(total_files: usize, counts: &RiskCounts, risks: &[Risk]) -> String {
    if total_files == 0 {
        return "工作区干净，无变更。".to_string();
    }

    let mut parts = vec![format!("共 {total_files} 个文件变更")];
    if counts.critical > 0 {
        parts.push(format!("{} 项关键风险", counts.critical));
    }
    if counts.warn > 0 {
        parts.push(format!("{} 项警告", counts.warn));
    }

    match leading_risk(risks) {
        // info 级只是「看一眼」，不足以当摘要的落点
        Some(risk) if risk.level != RiskLevel::Info => {
            parts.push(format!("首要关注：{}", risk.message));
        }
        _ => parts.push("无高风险项".to_string()),
    }

    parts.join("，")
}

/// 摘要引用的「最该看的」那条：先关键、再警告、最后退化到第一条。
fn leading_risk(risks: &[Risk]) -> Option<&Risk> {
    risks
        .iter()
        .find(|risk| risk.level == RiskLevel::Critical)
        .or_else(|| risks.iter().find(|risk| risk.level == RiskLevel::Warn))
        .or_else(|| risks.first())
}

// ---------------------------------------------------------------------------
// LLM 摘要层（抽象，默认 Noop）
// ---------------------------------------------------------------------------

/// LLM 摘要层抽象。
///
/// 默认 [`NoopSummarizer`] 恒返回 `None`，即**默认不调用任何模型**。
/// 后续任务卡接入本地 Ollama 时实现本 trait 即可，`ChangeAnalysis` 不必改。
pub trait Summarizer: Send + Sync {
    fn summarize(&self, analysis: &ChangeAnalysis) -> Option<String>;
}

/// 什么都不做的摘要器。默认实现，保证「本地优先」是不需要配置的默认行为。
pub struct NoopSummarizer;

impl Summarizer for NoopSummarizer {
    fn summarize(&self, _analysis: &ChangeAnalysis) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileChange;

    fn change(path: &str, kind: ChangeKind) -> FileChange {
        FileChange {
            path: path.to_string(),
            kind,
        }
    }

    fn analyze(files: Vec<FileChange>) -> ChangeAnalysis {
        let status = StatusInfo {
            staged: files,
            ..StatusInfo::default()
        };
        ChangeAnalysis::from_status(&status)
    }

    fn ids(analysis: &ChangeAnalysis) -> Vec<&str> {
        analysis.risks.iter().map(|r| r.rule_id.as_str()).collect()
    }

    #[test]
    fn conflict_marks_critical() {
        let status = StatusInfo {
            conflicts: vec![change("src/a.rs", ChangeKind::Unmerged)],
            ..StatusInfo::default()
        };
        let analysis = ChangeAnalysis::from_status(&status);

        assert!(ids(&analysis).contains(&"conflict-in-progress"));
        assert!(analysis.by_level.critical >= 1);
        assert_eq!(analysis.total_files, 1);
    }

    #[test]
    fn detects_env_file() {
        let analysis = analyze(vec![change(".env", ChangeKind::Modified)]);
        assert!(ids(&analysis).contains(&"env-or-secret"));
        assert_eq!(analysis.by_level.critical, 1);
    }

    #[test]
    fn detects_migration_file() {
        let analysis = analyze(vec![change(
            "db/migrations/0001_init.sql",
            ChangeKind::Added,
        )]);
        assert!(ids(&analysis).contains(&"migration-file"));
    }

    #[test]
    fn detects_ci_change() {
        let analysis = analyze(vec![change(
            ".github/workflows/ci.yml",
            ChangeKind::Modified,
        )]);
        assert!(ids(&analysis).contains(&"ci-config"));
        assert_eq!(analysis.by_level.warn, 1);
    }

    #[test]
    fn dependency_manifest_is_matched_by_basename() {
        let analysis = analyze(vec![change("crates/x/Cargo.toml", ChangeKind::Modified)]);
        assert!(ids(&analysis).contains(&"dependency-manifest"));
        // Cargo.toml 以 .toml 结尾，但 public-api 只认源码扩展名，不应误命中
        assert!(!ids(&analysis).contains(&"public-api"));
    }

    #[test]
    fn deleted_test_is_flagged_but_added_test_is_not() {
        let deleted = analyze(vec![change("src/lib/diff.test.ts", ChangeKind::Deleted)]);
        assert!(ids(&deleted).contains(&"test-deleted"));

        let added = analyze(vec![change("src/lib/diff.test.ts", ChangeKind::Added)]);
        assert!(!ids(&added).contains(&"test-deleted"));
        // 测试文件本身不算「源码变更」
        assert!(!ids(&added).contains(&"public-api"));
    }

    #[test]
    fn mass_deletion_requires_stats() {
        let status = StatusInfo {
            unstaged: vec![change("src/big.rs", ChangeKind::Modified)],
            ..StatusInfo::default()
        };

        // 读不到行数 -> 不命中，也不误报
        assert!(!ids(&ChangeAnalysis::from_status(&status)).contains(&"mass-deletion"));

        let mut stats = LineStats::new();
        stats.insert(
            "src/big.rs".to_string(),
            LineStat {
                additions: 3,
                deletions: 400,
                binary: false,
            },
        );
        assert!(
            ids(&ChangeAnalysis::from_status_and_stats(&status, &stats)).contains(&"mass-deletion")
        );
    }

    #[test]
    fn balanced_large_change_is_not_mass_deletion() {
        let status = StatusInfo {
            unstaged: vec![change("src/big.rs", ChangeKind::Modified)],
            ..StatusInfo::default()
        };
        let mut stats = LineStats::new();
        stats.insert(
            "src/big.rs".to_string(),
            LineStat {
                additions: 300,
                deletions: 300,
                binary: false,
            },
        );
        // 重写不等于删除：删除量没有远超新增量
        assert!(
            !ids(&ChangeAnalysis::from_status_and_stats(&status, &stats))
                .contains(&"mass-deletion")
        );
    }

    #[test]
    fn detects_error_handling_path() {
        let analysis = analyze(vec![change(
            "src/lib/error_handler.rs",
            ChangeKind::Modified,
        )]);
        let found = ids(&analysis);
        assert!(found.contains(&"error-handling"));
        assert!(found.contains(&"public-api"), "同一文件可命中多条规则");
    }

    #[test]
    fn detects_lockfile() {
        let analysis = analyze(vec![change("pnpm-lock.yaml", ChangeKind::Modified)]);
        let found = ids(&analysis);
        assert!(found.contains(&"lockfile-only"));
        assert!(found.contains(&"dependency-manifest"));
    }

    #[test]
    fn public_api_covers_top_level_src_paths() {
        // 归档口径的 `contains("/src/")` 对顶层 `src/a.rs` 恒为假，这里必须命中
        let analysis = analyze(vec![change("src/a.rs", ChangeKind::Modified)]);
        assert!(ids(&analysis).contains(&"public-api"));
    }

    #[test]
    fn clean_workspace_summary() {
        let analysis = ChangeAnalysis::from_status(&StatusInfo::default());
        assert_eq!(analysis.total_files, 0);
        assert!(analysis.risks.is_empty());
        assert!(analysis.summary.contains("干净"));
    }

    #[test]
    fn summary_leads_with_the_critical_risk() {
        let analysis = analyze(vec![
            change("src/a.rs", ChangeKind::Modified),
            change("db/migrations/0002_x.sql", ChangeKind::Added),
        ]);
        assert!(
            analysis.summary.contains("2 个文件变更"),
            "{}",
            analysis.summary
        );
        assert!(
            analysis.summary.contains("1 项关键风险"),
            "{}",
            analysis.summary
        );
        assert!(
            analysis.summary.contains("首要关注"),
            "{}",
            analysis.summary
        );
        assert!(
            analysis.summary.contains("迁移文件"),
            "摘要应引用关键条目本身：{}",
            analysis.summary
        );
    }

    #[test]
    fn summary_without_high_risks_says_so() {
        let analysis = analyze(vec![change("src/a.rs", ChangeKind::Modified)]);
        assert!(
            analysis.summary.contains("无高风险项"),
            "{}",
            analysis.summary
        );
    }

    #[test]
    fn counts_are_grouped_by_level() {
        let analysis = analyze(vec![
            change(".env", ChangeKind::Modified),
            change(".github/workflows/ci.yml", ChangeKind::Modified),
            change("src/a.rs", ChangeKind::Modified),
        ]);
        assert_eq!(analysis.by_level.critical, 1, ".env");
        assert_eq!(analysis.by_level.warn, 1, "ci.yml");
        assert_eq!(analysis.by_level.info, 1, "src/a.rs 的 public-api");
        assert_eq!(analysis.total_files, 3);
    }

    #[test]
    fn rule_matching_is_case_insensitive() {
        // 大小写不敏感的文件系统上，Cargo.TOML 与 cargo.toml 是同一个文件，
        // 规则表却只写了一种大小写 —— 比对前必须统一（这道坑真踩过）
        let manifest = analyze(vec![change("Crates/X/Cargo.TOML", ChangeKind::Modified)]);
        assert!(ids(&manifest).contains(&"dependency-manifest"));

        let env = analyze(vec![change(".ENV", ChangeKind::Modified)]);
        assert!(ids(&env).contains(&"env-or-secret"));

        let tests = analyze(vec![change("SRC/Lib/Diff.TEST.TS", ChangeKind::Deleted)]);
        assert!(ids(&tests).contains(&"test-deleted"));
    }

    #[test]
    fn change_group_identifiers_are_stable() {
        assert_eq!(ChangeGroup::Conflict.as_str(), "conflict");
        assert_eq!(ChangeGroup::Staged.as_str(), "staged");
        assert_eq!(ChangeGroup::Unstaged.as_str(), "unstaged");
    }

    #[test]
    fn noop_summarizer_returns_none() {
        let analysis = analyze(vec![change("src/a.rs", ChangeKind::Modified)]);
        assert!(NoopSummarizer.summarize(&analysis).is_none());
    }
}
