//! 规模基准：系统 Git 子进程后端在提交数增长时的耗时曲线（ADR-002 的实测依据）。
//!
//! **默认不跑**（`#[ignore]`）：它要构造 4 万条提交、跑几十秒，不适合放进 CI 常跑集。
//! 手动跑：
//!
//! ```text
//! cargo test -p repo-prism-core --test scale -- --ignored --nocapture
//! ```
//!
//! # 为什么要把 `snapshot()` 的子命令逐个拆开计时
//!
//! `snapshot()` 原先一共 spawn 5 次 `git`（`status` 1 + `symbolic-ref` 1 +
//! `rev-parse` 1 + `for-each-ref` 1 + 1），`commits()` spawn 2 次。
//! 如果耗时**随提交数基本不变**，瓶颈就不是「仓库有多大」而是「起了多少个进程」。
//! 这两者的修法完全不同：
//!
//! - 瓶颈在仓库规模 → 要在算法/数据结构上动手
//! - 瓶颈在进程数 → **合并子进程**就够，不必碰后端（换 `gix` 是其中一种，但不是唯一一种）
//!
//! 所以本文件测三件事，缺一不可：
//!
//! 1. **规模曲线** —— 耗时到底跟不跟提交数走
//! 2. **单命令拆解** —— 每个子命令各自多少，其中多少是「起进程」的固定成本
//! 3. **A/B 序列对比** —— 现状的 7 次 spawn 与合并后的 4 次 spawn，
//!    在同一进程、同一仓库上交替各跑两轮。这样得到的是**修法的实测值**，
//!    而不是把单命令耗时加起来估出来的投影（那样会重复计算固定开销）
//!
//! # `current_sequence()` 是「改动前」的快照，是故意保留的
//!
//! `git.rs` 已按本文件的结论做过 spawn 合并（`ADR/002`），因此
//! [`current_sequence`] **不再对应现行代码**，它保留下来是为了让
//! 「省了多少」这件事**可以被重新测量**，而不是只能引用一次历史结论。
//! 对应的机器无关门禁在 `perf.rs::spawn_counts_are_pinned`。。
//!
//! # 与 TASK-018（引用缓存）的关系
//!
//! 本文件测的是**不缓存**那条路径：全文用的是 `Git::open`，而缓存只在
//! `Git::open_cached` 下开启（理由见 `git.rs` 里 `Git` 的文档）。
//! 因此上面的规模曲线与 `ADR/002` 的原始数据**仍然逐项可比**，
//! 不会因为引入了缓存而悄悄换了含义。
//! 缓存自己的效果在第六节单独测，并同样以「几次 spawn」为单位。
//!
//! # 关于噪声
//!
//! 本机单个 `git` 子进程的墙钟耗时在 37–66ms 之间抖动（沙箱环境下偏高）。
//! 因此：所有聚合指标取 3 次采样的**最小值**；**序列对比**（第 3 项）是唯一可信的
//! 定量结论，因为它把两组测量放在同一个进程里交替进行，噪声同向抵消。
//! 拆开看单命令的那张表只用于**定位**，不用于求和。
//!
//! # 与 `perf.rs` 的分工
//!
//! `perf.rs` 是**门禁**：固定 3000 条提交、断言不超预算，每次 CI 都跑。
//! 本文件是**诊断**：多规模扫描、只打印不断言，用来回答「哪里慢」。

mod common;

use common::TempRepo;
use repo_prism_core::Git;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// 规模扫描点。上限取 4 万：够看出趋势，又不至于让本机磁盘吃紧
/// （本机可用空间约 7GB，单个 4 万提交的仓库约 40MB）。
const SCALES: &[usize] = &[1_000, 10_000, 40_000];

/// 每个指标采样次数，取最小值。
const SAMPLES: usize = 3;

const PAGE_SIZE: usize = 200;

fn best(costs: &[Duration]) -> Duration {
    costs.iter().copied().min().expect("SAMPLES >= 1")
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// 采样 N 次，返回最小时耗与最后一次的返回值。
fn sample<T>(n: usize, mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut costs = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let start = Instant::now();
        let value = f();
        costs.push(start.elapsed());
        last = Some(value);
    }
    (best(&costs), last.expect("SAMPLES >= 1"))
}

fn run_git(repo: Option<&Path>, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new("git");
    if let Some(repo) = repo {
        cmd.arg("-C").arg(repo);
    }
    cmd.args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run `git {args:?}`: {e}"))
}

/// 一条「命令序列」的耗时：把若干次 `git` 调用**顺序执行一遍**算一次采样。
///
/// 这是本文件里唯一可以拿来做定量结论的测量方式 —— 它把「N 次 spawn」
/// 当成一个整体来计时，不会像「把单命令耗时相加」那样重复计入固定开销。
fn time_sequence(repo: Option<&Path>, sequence: &[Vec<&str>]) -> Duration {
    // 预热一次，付掉进程冷启动与页缓存
    for args in sequence {
        let out = run_git(repo, args);
        assert!(out.status.success(), "warmup `git {args:?}` failed");
    }
    let (cost, ()) = sample(SAMPLES, || {
        for args in sequence {
            let out = run_git(repo, args);
            assert!(out.status.success(), "probe `git {args:?}` failed");
        }
    });
    cost
}

const BRANCHES_FMT: &str = "--format=%(refname:short)%09%(objectname)%09%(HEAD)";
const TAGS_FMT: &str = "--format=%(refname:short)%09%(objectname)";
const REFMAP_FMT: &str = "--format=%(objectname)%09%(refname:short)";
/// 合并后：一次 `for-each-ref` 同时给出分支、标签、远端引用。
/// `%(HEAD)` 只在 `refs/heads/` 上非空，用 `%(refname:lstrip=1)` 无法区分来源，
/// 因此改为在格式里带上完整 refname 的前缀判断位。
const MERGED_FMT: &str = "--format=%(objectname)%09%(refname:short)%09%(HEAD)%09%(refname)";

/// 现状：`snapshot()` 5 次（`status` / `symbolic-ref` / `rev-parse` / 两次 `for-each-ref`）
/// + `commits()` 2 次（`log` / `for-each-ref`）= **7 次**。
fn current_sequence() -> Vec<Vec<&'static str>> {
    vec![
        vec!["status", "--porcelain=v2", "--branch", "-z"],
        vec!["symbolic-ref", "--quiet", "--short", "HEAD"],
        vec!["rev-parse", "HEAD"],
        vec!["for-each-ref", BRANCHES_FMT, "refs/heads/"],
        vec!["for-each-ref", TAGS_FMT, "refs/tags/"],
        vec!["log", "--all", "-n200", "--skip=0"],
        vec![
            "for-each-ref",
            REFMAP_FMT,
            "refs/heads/",
            "refs/tags/",
            "refs/remotes/",
        ],
    ]
}

/// 提议：**4 次**。
///
/// - `snapshot()` 2 次：`status`（分支名与 HEAD 取自 `# branch.head` / `# branch.oid`，
///   不再另起 `symbolic-ref` 与 `rev-parse HEAD`）+ 一次合并后的 `for-each-ref`
/// - `commits()` 2 次：`log` + 再一次 `for-each-ref`
///
/// **注意这里诚实的地方**：合并 `for-each-ref` 省不掉 `commits()` 那一次 ——
/// 它本来就只有一次。想把两者共用（降到 3 次）需要**跨调用缓存引用映射**，
/// 那是 `TASK-018 增量缓存` 的范围，不属于本次改动。
/// 早先版本的本函数只写了 3 次，那是把「两次 API 调用」误当成「一次」，
/// 属于对自身改动的夸大，已修正。
fn proposed_sequence() -> Vec<Vec<&'static str>> {
    vec![
        vec!["status", "--porcelain=v2", "--branch", "-z"],
        vec![
            "for-each-ref",
            MERGED_FMT,
            "refs/heads/",
            "refs/tags/",
            "refs/remotes/",
        ],
        vec!["log", "--all", "-n200", "--skip=0"],
        vec![
            "for-each-ref",
            MERGED_FMT,
            "refs/heads/",
            "refs/tags/",
            "refs/remotes/",
        ],
    ]
}

/// TASK-018：开启引用缓存之后，同样的 `snapshot() + commits()` 是 **3 次**。
///
/// 与 [`proposed_sequence`] 逐字相同，只少一次 `for-each-ref` ——
/// 因为 `commits()` 复用了 `snapshot()` 已经读到的引用映射。
///
/// **这条序列是「模型」而不是「测量」**：它手工略去了那一次调用，
/// 用来回答「若缓存生效，spawn 数会是多少」。缓存本身是否真的生效，
/// 由 `cache.rs` 的断言负责（那些用例会数真实子进程次数）。
/// 两者分开写，是为了不让「我以为我省了」冒充「我测到我省了」。
fn cached_sequence() -> Vec<Vec<&'static str>> {
    vec![
        vec!["status", "--porcelain=v2", "--branch", "-z"],
        vec![
            "for-each-ref",
            MERGED_FMT,
            "refs/heads/",
            "refs/tags/",
            "refs/remotes/",
        ],
        vec!["log", "--all", "-n200", "--skip=0"],
    ]
}

/// 固定成本基线：1 次与 5 次 `git --version`（不读仓库）的顺序执行耗时。
fn spawn_baseline(repo: Option<&Path>) -> (Duration, Duration) {
    let one = time_sequence(repo, &[vec!["--version"]]);
    let five = time_sequence(
        repo,
        &[
            vec!["--version"],
            vec!["--version"],
            vec!["--version"],
            vec!["--version"],
            vec!["--version"],
        ],
    );
    (one, five)
}

#[test]
#[ignore = "规模基准：要构造 4 万条提交、跑几十秒，用 --ignored 手动跑"]
fn scaling_curve_of_the_system_git_backend() {
    // 全局预热：先构造一个小仓库跑一遍，避免「第一次迭代最慢」被误读成趋势。
    {
        let warmup = TempRepo::new("scale-warmup");
        warmup.seed_fast_import(100);
        let git = Git::open(warmup.path()).expect("open warmup repo");
        let _ = git.snapshot();
        let _ = git.commits(PAGE_SIZE, 0);
    }

    let (spawn_1, spawn_5) = spawn_baseline(None);

    println!();
    println!("=== 一、规模曲线（每个规模重建一次临时仓库，采样 {SAMPLES} 次取最小）===");
    println!(
        "{:>8}  {:>10}  {:>11}  {:>12}  {:>12}",
        "提交数", "构造耗时", "open(ms)", "snapshot(ms)", "commits(ms)"
    );

    let mut rows: Vec<(usize, Duration)> = Vec::new();

    for &scale in SCALES {
        let repo = TempRepo::new("scale");
        let build_start = Instant::now();
        repo.seed_fast_import(scale);
        let build = build_start.elapsed();

        let (open_ms, git) = sample(SAMPLES, || Git::open(repo.path()).expect("open repo"));

        let _ = git.snapshot().expect("warmup snapshot");
        let _ = git.commits(PAGE_SIZE, 0).expect("warmup commits");

        let (snapshot_ms, snap) = sample(SAMPLES, || git.snapshot().expect("snapshot"));
        assert_eq!(snap.branches.len(), 1, "代理仓库应只有一个分支");

        let (commits_ms, commits) = sample(SAMPLES, || git.commits(PAGE_SIZE, 0).expect("commits"));
        assert_eq!(commits.len(), PAGE_SIZE, "limit 应正好返回 {PAGE_SIZE} 条");

        println!(
            "{:>8}  {:>9.0}ms  {:>9.0}ms  {:>10.0}ms  {:>10.0}ms",
            scale,
            ms(build),
            ms(open_ms),
            ms(snapshot_ms),
            ms(commits_ms)
        );

        rows.push((scale, snapshot_ms));
        drop(repo); // 立刻回收，避免三个仓库堆在磁盘上
    }

    let largest = rows.last().expect("SCALES 非空").0;

    println!();
    println!("=== 二、最大规模（{largest} 提交）上把各子命令拆开：谁在花时间 ===");
    println!("（这张表只用于**定位**，不要横向相加 —— 相加会重复计入固定开销）");

    let repo = TempRepo::new("scale-breakdown");
    repo.seed_fast_import(largest);
    let _ = Git::open(repo.path()).expect("open repo");

    let probes: &[(&str, Vec<&str>)] = &[
        ("（不读仓库）--version", vec!["--version"]),
        (
            "rev-parse --show-toplevel",
            vec!["rev-parse", "--show-toplevel"],
        ),
        (
            "status --porcelain=v2 --branch",
            vec!["status", "--porcelain=v2", "--branch", "-z"],
        ),
        (
            "symbolic-ref HEAD",
            vec!["symbolic-ref", "--quiet", "--short", "HEAD"],
        ),
        ("rev-parse HEAD", vec!["rev-parse", "HEAD"]),
        (
            "for-each-ref refs/heads",
            vec!["for-each-ref", BRANCHES_FMT, "refs/heads/"],
        ),
        (
            "for-each-ref refs/tags",
            vec!["for-each-ref", TAGS_FMT, "refs/tags/"],
        ),
        (
            "for-each-ref（ref_map 三条）",
            vec![
                "for-each-ref",
                REFMAP_FMT,
                "refs/heads/",
                "refs/tags/",
                "refs/remotes/",
            ],
        ),
        (
            "for-each-ref（三条合并成一次）",
            vec![
                "for-each-ref",
                MERGED_FMT,
                "refs/heads/",
                "refs/tags/",
                "refs/remotes/",
            ],
        ),
        (
            "log --all -n200",
            vec![
                "log",
                "--all",
                "--date=iso-strict",
                "--format=%H%x1f%h",
                "-n200",
                "--skip=0",
            ],
        ),
    ];

    for (label, probe_args) in probes.iter() {
        let cost = time_sequence(Some(repo.path()), std::slice::from_ref(probe_args));
        println!("  {label:<34} {:.0}ms", ms(cost));
    }

    println!();
    println!("=== 三、`status --branch` 的头部行到底给不给我们要的东西 ===");
    let raw = run_git(
        Some(repo.path()),
        &["status", "--porcelain=v2", "--branch", "-z"],
    );
    let text = String::from_utf8_lossy(&raw.stdout).to_string();
    for entry in text.split('\0') {
        if entry.starts_with("# ") {
            println!("  {entry:?}");
        }
    }
    println!("  → 含 `# branch.oid` 与 `# branch.head` 则分支名与 HEAD 无需另起子进程");

    println!();
    println!("=== 四、A/B 序列对比（同一进程、同一仓库，唯一可作定量结论的测量）===");
    let current = current_sequence();
    let proposed = proposed_sequence();
    // 交替跑两轮，让噪声同向作用在两组上
    let a1 = time_sequence(Some(repo.path()), &current);
    let b1 = time_sequence(Some(repo.path()), &proposed);
    let a2 = time_sequence(Some(repo.path()), &current);
    let b2 = time_sequence(Some(repo.path()), &proposed);
    let current_best = a1.min(a2);
    let proposed_best = b1.min(b2);

    println!(
        "  现状 {} 次 spawn（snapshot + commits 全量）：{:>6.0}ms   [ {:.0} / {:.0} ]",
        current.len(),
        ms(current_best),
        ms(a1),
        ms(a2)
    );
    println!(
        "  提议 {} 次 spawn（合并 for-each-ref、分支与 HEAD 取自 status）：{:>6.0}ms   [ {:.0} / {:.0} ]",
        proposed.len(),
        ms(proposed_best),
        ms(b1),
        ms(b2)
    );
    println!(
        "  省下 {:.0}%，即 {:.0}ms（每次 spawn 固定成本约 {:.0}ms）",
        (1.0 - ms(proposed_best) / ms(current_best)) * 100.0,
        ms(current_best) - ms(proposed_best),
        ms(spawn_1)
    );

    println!();
    println!("=== 五、固定成本基线（在测试开头与结尾各测一次，看它对环境状态的敏感度）===");
    let (spawn_1_post, spawn_5_post) = spawn_baseline(Some(repo.path()));
    println!(
        "  1 次 spawn  开头 {:.0}ms / 结尾 {:.0}ms",
        ms(spawn_1),
        ms(spawn_1_post)
    );
    println!(
        "  5 次 spawn  开头 {:.0}ms / 结尾 {:.0}ms   → 折合每次 {:.0}ms / {:.0}ms",
        ms(spawn_5),
        ms(spawn_5_post),
        ms(spawn_5) / 5.0,
        ms(spawn_5_post) / 5.0
    );
    println!(
        "  snapshot()+commits() 现状 7 次 spawn 实测      {:.0}ms → 折合每次 {:.0}ms",
        ms(current_best),
        ms(current_best) / 7.0
    );
    println!(
        "  snapshot() 实测（{largest} 提交）              {:.0}ms",
        ms(rows.last().expect("non-empty").1)
    );
    println!();
    println!("  若「折合每次」在两组测量间明显不一致，以 A/B 对比为准：");
    println!("  A/B 把两组序列放在同一进程内交替执行，环境噪声同向作用于两边；");
    println!("  而单独重复同一条命令会受进程创建节流/缓存淘汰影响，系统性偏高。");

    println!();
    println!("=== 六、引用缓存（TASK-018）把 snapshot()+commits() 从 4 次降到 3 次 ===");
    println!("  注意：这一节比的是**序列里的 spawn 次数**（spawn 数与机器无关），");
    println!("  不是毫秒。缓存是否真的生效由 cache.rs 的断言负责，本节只给量级。");
    let merged = proposed_sequence();
    let cached = cached_sequence();
    let m = time_sequence(Some(repo.path()), &merged);
    let c = time_sequence(Some(repo.path()), &cached);
    println!(
        "  不缓存 {} 次 spawn（ADR-002 的成果）：{:>6.0}ms",
        merged.len(),
        ms(m)
    );
    println!(
        "  开缓存 {} 次 spawn（commits 复用 snapshot 读到的引用）：{:>6.0}ms",
        cached.len(),
        ms(c)
    );
    println!(
        "  差 {:.0}ms —— **不要把这个数当结论**：同一次运行里单次 spawn 的固定成本\n  \
         就有 {:.0}-{:.0}ms，毫秒差完全落在噪声带内（见 ADR-002 第 2.5 节）。\n  \
         可靠的结论只有一条：spawn 次数 4 → 3。",
        ms(m) - ms(c),
        ms(spawn_1_post),
        ms(spawn_5_post) / 5.0
    );

    drop(repo);
}
