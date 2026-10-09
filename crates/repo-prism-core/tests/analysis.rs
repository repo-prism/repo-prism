//! 变更分析与风险标记集成测试（对应 TASK-010 验收标准）。
//!
//! 单测覆盖规则本身的判定；这里覆盖**从真实仓库读到的东西**能不能喂进规则：
//! `working_tree_stats()` 的 `--numstat` 解析、暂存/未暂存两个来源的合并、
//! 重命名按新路径建键，以及「没有行数统计时 mass-deletion 不命中」这一条。

mod common;

use common::TempRepo;
use repo_prism_core::{ChangeAnalysis, Git};
use std::fs;

fn rule_ids(analysis: &ChangeAnalysis) -> Vec<&str> {
    analysis.risks.iter().map(|r| r.rule_id.as_str()).collect()
}

/// 构造一个基线提交，再在工作区铺开五类改动。
fn repo_with_mixed_changes(tag: &str) -> TempRepo {
    let repo = TempRepo::new(tag);
    // 基线用**互不相同**的行：重复行的最小 diff 不唯一（git 可能报 398 也可能报
    // 400 行删除），断言就变成看 git 的启发式脸色。行内容各不相同，删除量才是确定的。
    let base: String = (1..=400).map(|n| format!("line-{n}\n")).collect();
    repo.write("src/big.rs", &base);
    repo.write("src/lib/diff.test.ts", "// 测试\n");
    repo.write(".github/workflows/ci.yml", "name: CI\n");
    repo.write("src/keep.rs", "fn keep() {}\n");
    repo.commit("base");

    // 未暂存：大量删除（400 行 -> 2 行，前两行原样保留）
    repo.write("src/big.rs", "line-1\nline-2\n");
    // 未暂存：删掉测试文件
    fs::remove_file(repo.path().join("src/lib/diff.test.ts")).expect("remove test file");
    // 未暂存：CI 配置变更
    repo.write(".github/workflows/ci.yml", "name: CI\non: push\n");
    // 已暂存：数据库迁移
    repo.write("db/migrations/0002_add.sql", "SELECT 1;\n");
    repo.git(&["add", "db/migrations/0002_add.sql"]);
    // 已暂存：疑似密钥
    repo.write(".env", "TOKEN=1\n");
    repo.git(&["add", ".env"]);

    repo
}

#[test]
fn working_tree_stats_reads_additions_and_deletions() {
    let repo = repo_with_mixed_changes("analysis-stats");
    let git = Git::open(repo.path()).expect("open repo");
    let stats = git.working_tree_stats().expect("stats");

    let big = stats.get("src/big.rs").expect("big.rs 应有行数统计");
    assert_eq!(big.additions, 0, "前两行原样保留，没有新增");
    assert_eq!(big.deletions, 398, "其余 398 行全部删除");
    assert!(!big.binary);

    // 暂存与未暂存两个来源都要覆盖到
    assert!(stats.contains_key(".env"), "已暂存的新文件也要有统计");
    assert!(
        stats.contains_key(".github/workflows/ci.yml"),
        "未暂存的修改也要有统计"
    );
}

#[test]
fn stats_keys_renames_by_new_path() {
    let repo = TempRepo::new("analysis-rename");
    repo.write("old-name.txt", "same\n");
    repo.commit("base");
    repo.git(&["mv", "old-name.txt", "new-name.txt"]);

    let git = Git::open(repo.path()).expect("open repo");
    let stats = git.working_tree_stats().expect("stats");

    assert!(stats.contains_key("new-name.txt"), "重命名按新路径建键");
    assert!(
        !stats.contains_key("old-name.txt"),
        "旧路径不该残留，否则前端按 status 报的新路径取不到统计"
    );
}

#[test]
fn analyzes_mixed_changes_end_to_end() {
    let repo = repo_with_mixed_changes("analysis-e2e");
    let git = Git::open(repo.path()).expect("open repo");
    let snapshot = git.snapshot().expect("snapshot");
    let stats = git.working_tree_stats().expect("stats");
    let analysis = ChangeAnalysis::from_status_and_stats(&snapshot.status, &stats);

    let found = rule_ids(&analysis);

    assert!(
        found.contains(&"mass-deletion"),
        "398 行删除应命中：{found:?}"
    );
    assert!(
        found.contains(&"test-deleted"),
        "删掉测试文件应命中：{found:?}"
    );
    assert!(found.contains(&"ci-config"), "CI 配置变更应命中：{found:?}");
    assert!(
        found.contains(&"migration-file"),
        "迁移文件应命中：{found:?}"
    );
    assert!(found.contains(&"env-or-secret"), ".env 应命中：{found:?}");

    assert!(
        analysis.by_level.critical >= 2,
        "迁移 + 密钥，至少 2 项关键"
    );
    assert!(
        analysis.by_level.warn >= 2,
        "大量删除 + 测试删除，至少 2 项警告"
    );
    assert!(
        analysis.summary.contains("关键风险"),
        "摘要要点出关键风险数：{}",
        analysis.summary
    );
}

#[test]
fn without_stats_only_path_rules_fire() {
    let repo = repo_with_mixed_changes("analysis-nostats");
    let git = Git::open(repo.path()).expect("open repo");
    let snapshot = git.snapshot().expect("snapshot");

    let with_stats = git.working_tree_stats().expect("stats");
    let full = ChangeAnalysis::from_status_and_stats(&snapshot.status, &with_stats);
    let path_only = ChangeAnalysis::from_status(&snapshot.status);

    // 文件数必须一致：两条路径看的是同一份 status
    assert_eq!(full.total_files, path_only.total_files);

    // 「大量删除」需要行数，纯 status 路径下必须**不命中**（而不是误报）
    assert!(!rule_ids(&path_only).contains(&"mass-deletion"));
    assert!(rule_ids(&full).contains(&"mass-deletion"));

    // 纯路径规则两条路径都要命中
    assert!(rule_ids(&path_only).contains(&"env-or-secret"));
    assert!(rule_ids(&path_only).contains(&"ci-config"));
}

#[test]
fn clean_repository_yields_an_empty_analysis() {
    let repo = TempRepo::new("analysis-clean");
    repo.write("a.txt", "1\n");
    repo.commit("base");

    let git = Git::open(repo.path()).expect("open repo");
    let snapshot = git.snapshot().expect("snapshot");
    let stats = git.working_tree_stats().expect("stats");
    let analysis = ChangeAnalysis::from_status_and_stats(&snapshot.status, &stats);

    assert_eq!(analysis.total_files, 0);
    assert!(analysis.risks.is_empty());
    assert!(stats.is_empty(), "干净仓库不应有任何行数统计");
    assert!(analysis.summary.contains("干净"));
}
