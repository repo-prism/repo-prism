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
//!
//! # 为什么取「N 次采样的最小值」而不是单次
//!
//! 单次采样会把调度抖动算成代码性能。2026-10-09 实测：**同一份代码**在本机连跑三次
//! `snapshot()` 得到 208ms / 470ms / 576ms —— 第一次直接超出 500ms 预算。
//! 这说明单次采样测的是「机器当时有多忙」，不是「代码有多快」。
//!
//! 取最小值：抖动只会让某次采样变慢，**不会让某次采样变快**；
//! 而真正的算法退化会让所有采样一起变慢。CI 上的 runner 比开发机更吵，
//! 这一点尤其重要。

mod common;

use common::TempRepo;
use repo_prism_core::Git;
use std::time::{Duration, Instant};

/// 代理仓库的提交数。够大到让 `--all` 的遍历成本显现，又不至于拖慢 CI。
const SEED_COMMITS: usize = 3_000;
const SNAPSHOT_BUDGET: Duration = Duration::from_millis(500);
const COMMITS_BUDGET: Duration = Duration::from_millis(800);
const PAGE_SIZE: usize = 200;

/// 每个指标采样次数。取最小值，理由见文件头。
const SAMPLES: usize = 3;

/// 采样 `n` 次，返回每次耗时与最后一次的结果。
fn sample<T>(n: usize, mut f: impl FnMut() -> T) -> (Vec<Duration>, T) {
    let mut costs = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let start = Instant::now();
        let value = f();
        costs.push(start.elapsed());
        last = Some(value);
    }
    (costs, last.expect("SAMPLES >= 1"))
}

fn best(costs: &[Duration]) -> Duration {
    costs.iter().copied().min().expect("SAMPLES >= 1")
}

fn render(costs: &[Duration]) -> String {
    costs
        .iter()
        .map(|c| format!("{:.0}ms", c.as_secs_f64() * 1000.0))
        .collect::<Vec<_>>()
        .join(" / ")
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

    let (snapshot_costs, snap) = sample(SAMPLES, || git.snapshot().expect("snapshot"));
    assert_eq!(snap.head.branch.as_deref(), Some("main"));
    assert_eq!(snap.branches.len(), 1, "代理仓库应只有一个分支");

    let (commits_costs, commits) = sample(SAMPLES, || git.commits(PAGE_SIZE, 0).expect("commits"));
    assert_eq!(
        commits.len(),
        PAGE_SIZE,
        "limit 生效时应该正好返回 {} 条",
        PAGE_SIZE
    );

    let snapshot_best = best(&snapshot_costs);
    let commits_best = best(&commits_costs);

    // 先打印全部采样再断言：CI 失败时日志里能直接看到分布，
    // 便于区分「代码退化」与「runner 抖动」。
    println!("seed: {SEED_COMMITS} commits (git fast-import)");
    println!(
        "snapshot():        {} -> best {snapshot_best:?} / budget {SNAPSHOT_BUDGET:?}",
        render(&snapshot_costs)
    );
    println!(
        "commits({PAGE_SIZE}, 0): {} -> best {commits_best:?} / budget {COMMITS_BUDGET:?}",
        render(&commits_costs)
    );

    assert!(
        snapshot_best < SNAPSHOT_BUDGET,
        "snapshot() 超出预算：{SAMPLES} 次采样 {snapshot_best:?}（最小），预算 {SNAPSHOT_BUDGET:?}"
    );
    assert!(
        commits_best < COMMITS_BUDGET,
        "commits({PAGE_SIZE}, 0) 超出预算：{SAMPLES} 次采样 {commits_best:?}（最小），预算 {COMMITS_BUDGET:?}"
    );
}

// ---------------------------------------------------------------------------
// 子进程次数门禁（ADR-002）
// ---------------------------------------------------------------------------

/// **性能契约：子进程次数。**
///
/// `ADR/002` 的实测结论是：耗时几乎完全由「起了几个 `git` 进程」决定，
/// 与仓库大小无关 —— 4 万提交仓库上的 `for-each-ref`（约 29ms）与
/// 完全不读仓库的 `git --version`（约 26ms）耗时相同，即每条命令 85% 以上的
/// 时间花在起进程上。提交数从 1 万涨到 4 万，`snapshot()` 耗时没有变化。
///
/// 所以**「起了几次」就是性能本身**，而它是一个比毫秒更适合做门禁的量：
///
/// | | 毫秒阈值 | spawn 次数 |
/// |---|---|---|
/// | 随 runner 抖动 | 会（实测同机连跑三次 208/470/576ms） | 不会 |
/// | 本机与 CI 可比 | 否 | 是 |
/// | 能否定位到具体退化 | 不能，只知「变慢了」 | 能，指出是哪个方法多起了进程 |
///
/// 这条断言在任何机器上都成立，因此它比上面的毫秒阈值更硬。
/// **改动 `git.rs` 的任何公开方法后都要先看这里。**
#[test]
fn spawn_counts_are_pinned() {
    let repo = TempRepo::new("spawns");
    repo.seed_fast_import(50);
    let git = Git::open(repo.path()).expect("open repo");

    assert_eq!(git.spawns(), 1, "open() 应只起一次 rev-parse");

    let before = git.spawns();
    git.snapshot().expect("snapshot");
    assert_eq!(
        git.spawns() - before,
        2,
        "snapshot() 应是 status(1) + for-each-ref(1)；\
         若变成 5，说明有人把分支/标签/HEAD 又拆回了独立子进程（ADR-002 已合并掉）"
    );

    let before = git.spawns();
    git.commits(20, 0).expect("commits");
    assert_eq!(
        git.spawns() - before,
        2,
        "commits() 应是 log(1) + for-each-ref(1)"
    );

    let sha = git.commits(1, 0).expect("head commit")[0].sha.clone();

    let before = git.spawns();
    git.commit(&sha).expect("commit");
    assert_eq!(git.spawns() - before, 2, "commit() 应是 log(1) + refs(1)");

    let before = git.spawns();
    git.commit_detail(&sha).expect("commit_detail");
    assert_eq!(
        git.spawns() - before,
        4,
        "commit_detail() 应是 commit(2) + name-status(1) + patch(1)"
    );

    let before = git.spawns();
    git.working_tree_stats().expect("working_tree_stats");
    assert_eq!(
        git.spawns() - before,
        2,
        "working_tree_stats() 是两次 --numstat（索引 vs HEAD、工作区 vs 索引）"
    );

    let before = git.spawns();
    let _ = git.remote_info().expect("remote_info");
    assert_eq!(git.spawns() - before, 1, "remote_info() 应只起一次");

    let before = git.spawns();
    git.diff(None, None).expect("diff");
    assert_eq!(git.spawns() - before, 2, "diff() 是 names(1) + patch(1)");
}
