//! 性能门禁（对应 TASK-011）。
//!
//! `AGENTS.md` 的质量门禁写着「性能：大仓库（Linux 内核级）首屏 < 3s，内存 < 300MB」，
//! 各任务卡也给了具体指标。但在这张卡之前，CI 里**没有任何性能检查**——
//! 指标只存在于文档里，退化了也没人知道。
//!
//! 本文件把它们变成会真实失败的断言：
//! - TASK-002：`snapshot()` < 500ms
//! - TASK-004：`commits(200, 0)` < 800ms
//!
//! # 为什么不是 Linux 内核仓库
//!
//! CI 里无法下载并长期维护一份内核级快照。这里改用 `git fast-import` 在测试内
//! 构造 3000 条提交的仓库作为**可复现的代理仓库**：构造耗时 < 1s、
//! 不依赖网络、结果确定。
//!
//! 阈值沿用任务卡原值而**不**因为仓库变小而下调——放松阈值会让门禁失去意义。

mod common;

use common::TempRepo;
use repo_prism_core::Git;
use std::time::{Duration, Instant};

/// 代理仓库的提交数。够大到让 `--all` 的遍历成本显现，又不至于拖慢 CI。
const SEED_COMMITS: usize = 3_000;
const SNAPSHOT_BUDGET: Duration = Duration::from_millis(500);
const COMMITS_BUDGET: Duration = Duration::from_millis(800);
const PAGE_SIZE: usize = 200;

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let value = f();
    (value, start.elapsed())
}

#[test]
fn snapshot_and_commits_stay_within_budget() {
    let repo = TempRepo::new("perf");
    repo.seed_fast_import(SEED_COMMITS);
    let git = Git::open(repo.path()).expect("open repo");

    // 预热：第一次调用要付进程冷启动与文件页缓存的成本。
    // 把它算进门禁，测的就成了「机器当时的状态」而不是「代码的性能」。
    let _ = git.snapshot().expect("warmup snapshot");
    let _ = git.commits(PAGE_SIZE, 0).expect("warmup commits");

    let (snap, snapshot_cost) = timed(|| git.snapshot().expect("snapshot"));
    assert_eq!(snap.head.branch.as_deref(), Some("main"));
    assert_eq!(snap.branches.len(), 1, "代理仓库应只有一个分支");

    let (commits, commits_cost) = timed(|| git.commits(PAGE_SIZE, 0).expect("commits"));
    assert_eq!(
        commits.len(),
        PAGE_SIZE,
        "limit 生效时应该正好返回 {} 条",
        PAGE_SIZE
    );

    // 先打印再断言：CI 失败时日志里能直接看到实测值，
    // 便于区分「代码退化」与「runner 抖动」。
    println!("seed: {SEED_COMMITS} commits (git fast-import)");
    println!("snapshot():        {snapshot_cost:?} / budget {SNAPSHOT_BUDGET:?}");
    println!("commits({PAGE_SIZE}, 0): {commits_cost:?} / budget {COMMITS_BUDGET:?}");

    assert!(
        snapshot_cost < SNAPSHOT_BUDGET,
        "snapshot() 超出预算：实测 {snapshot_cost:?}，预算 {SNAPSHOT_BUDGET:?}"
    );
    assert!(
        commits_cost < COMMITS_BUDGET,
        "commits({PAGE_SIZE}, 0) 超出预算：实测 {commits_cost:?}，预算 {COMMITS_BUDGET:?}"
    );
}
