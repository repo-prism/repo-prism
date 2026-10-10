//! 多仓库会话表（`TASK-019` / US-9）。
//!
//! 判据一律是**子进程次数**与**实例同一性**，不是「返回值看起来对」：
//! 「两种写法命中同一个会话」这件事，返回值相等证明不了（两个实例也会返回相等的值），
//! 只有 `Arc::ptr_eq` 与 spawn 次数能分开「复用了」与「又开了一个」。

mod common;

use common::TempRepo;
use repo_prism_core::{Git, RepoSet, DEFAULT_CAP};
use std::path::PathBuf;
use std::sync::Arc;

/// git 自己解析出的仓库根。
///
/// 在 macOS 上它与夹具的路径**字面不同**：`std::env::temp_dir()` 给的是
/// `/var/folders/...`，而 `rev-parse --show-toplevel` 会解析符号链接、
/// 输出 `/private/var/folders/...`。所以比较一律用这一份，不用夹具的原始路径。
fn resolved(repo: &TempRepo) -> PathBuf {
    Git::open(repo.path())
        .expect("fixture repo must open")
        .path()
        .to_path_buf()
}

/// 一个有提交的仓库，分支名可指定（用于证明两个会话互不干扰）。
fn repo_with_branch(tag: &str, branch: &str) -> TempRepo {
    let repo = TempRepo::new(tag);
    repo.write("a.txt", "one\n");
    repo.commit("first");
    if branch != "main" {
        repo.git(&["checkout", "-b", branch]);
    }
    repo
}

#[test]
fn the_same_session_is_reused_for_the_same_spelling() {
    let repo = repo_with_branch("sess-same", "main");
    let set = RepoSet::with_default_cap();

    let first = set.open(repo.path()).expect("open");
    let second = set.open(repo.path()).expect("open again");

    // 实例同一性，而不是「两次返回值的字段相等」
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(set.len(), 1);
    // 命中别名不该再起子进程：open 自己那一次之后仍是 1
    assert_eq!(second.spawns(), 1);
}

#[test]
fn a_subdirectory_spelling_lands_on_the_same_session() {
    let repo = repo_with_branch("sess-sub", "main");
    repo.write("nested/deep/a.txt", "x\n");
    repo.commit("nested");
    let set = RepoSet::with_default_cap();

    let root = set.open(repo.path()).expect("open root");
    // 子目录是一种新写法：需要一次 rev-parse 才知道它指向哪个仓库，
    // 但**不该**因此建出第二个会话。
    let from_sub = set
        .open(repo.path().join("nested").join("deep"))
        .expect("open subdir");

    assert!(
        Arc::ptr_eq(&root, &from_sub),
        "同一仓库的两个实例 = 两份引用缓存"
    );
    assert_eq!(set.len(), 1);
    // 复用了既有会话，所以它的计数没被第二个实例带跑
    assert_eq!(from_sub.spawns(), 1);
}

#[cfg(unix)]
#[test]
fn a_symlink_spelling_lands_on_the_same_session() {
    use std::os::unix::fs::symlink;

    let repo = repo_with_branch("sess-link", "main");
    let link = repo.path().with_extension("link");
    symlink(repo.path(), &link).expect("create symlink");

    let set = RepoSet::with_default_cap();
    let root = set.open(repo.path()).expect("open root");
    let via_link = set.open(&link).expect("open through symlink");
    let via_link_sub = set.open(link.join("nested")).ok();

    assert!(Arc::ptr_eq(&root, &via_link));
    assert_eq!(set.len(), 1);
    // 符号链接下的子目录：该目录存在时才断言，避免夹具细节影响结论
    if let Some(got) = via_link_sub {
        assert!(Arc::ptr_eq(&root, &got));
        assert_eq!(set.len(), 1);
    }
}

/// 别名的作用是「命中即不再问 git」。
///
/// 只断言 `ptr_eq` 证明不了它 —— 因为「解析出的根已在表里」这条兜底路径
/// 同样会交出同一个实例。要分开这两条路径，必须让**这一次**没法再去问 git：
/// 把当初那种写法本身删掉（仓库还在），于是「没走别名」必然失败。
#[cfg(unix)]
#[test]
fn an_alias_hit_is_answered_without_asking_git_again() {
    use std::os::unix::fs::symlink;

    let repo = repo_with_branch("sess-alias-gone", "main");
    let link = repo.path().with_extension("link");
    symlink(repo.path(), &link).expect("create symlink");

    let set = RepoSet::with_default_cap();
    let first = set.open(&link).expect("open through symlink");
    std::fs::remove_file(&link).expect("drop the spelling we used");

    // 仓库还在、会话还在，但这个**写法**已经指向不存在的地方了。
    // 走别名 → 直接命中；不走别名 → 会去跑 rev-parse 并失败。
    let again = set.open(&link).expect("alias hit must not need the path");
    assert!(Arc::ptr_eq(&first, &again));
    assert!(
        again.snapshot().is_ok(),
        "会话本身仍然可用：它记的是解析后的仓库根，不是当初的写法"
    );
}

#[test]
fn two_repos_are_two_independent_sessions() {
    let left = repo_with_branch("sess-a", "main");
    let right = repo_with_branch("sess-b", "feature");
    let set = RepoSet::with_default_cap();

    let a = set.open(left.path()).expect("open left");
    let b = set.open(right.path()).expect("open right");

    assert_eq!(set.len(), 2);
    assert!(!Arc::ptr_eq(&a, &b));

    // 互不干扰：各自的快照给出各自的分支
    let snap_a = a.snapshot().expect("snapshot left");
    let snap_b = b.snapshot().expect("snapshot right");
    assert_eq!(snap_a.head.branch.as_deref(), Some("main"));
    assert_eq!(snap_b.head.branch.as_deref(), Some("feature"));
}

#[test]
fn each_repo_has_its_own_cache_and_neither_changes_the_other() {
    // 注意会话是 `open_cached` 的，所以**首次**快照是 2 次（status + 读引用），
    // 之后引用命中缓存、降到 1 次。「snapshot 恰好 2 次」那条**与状态无关**的契约
    // 由 `perf.rs` 用不缓存的 `Git::open` 钉住，两者不是同一条断言。
    //
    // 本条要证明的是另一件事：**缓存是每个仓库一份**，读 B 不会把 A 的缓存顶掉。
    let left = repo_with_branch("sess-cost-a", "main");
    let right = repo_with_branch("sess-cost-b", "main");
    let set = RepoSet::with_default_cap();

    let a = set.open(left.path()).expect("open left");
    let b = set.open(right.path()).expect("open right");

    let a0 = a.spawns();
    let first_a = a.snapshot().expect("snapshot left");
    assert_eq!(a.spawns() - a0, 2, "A 的首次快照是 2 次");

    let b0 = b.spawns();
    let first_b = b.snapshot().expect("snapshot right");
    assert_eq!(b.spawns() - b0, 2, "B 的首次快照也是 2 次 —— 与 A 无关");

    // 读过 B 之后，A 的第二次快照仍应命中**它自己**的缓存
    let a1 = a.spawns();
    let second_a = a.snapshot().expect("snapshot left again");
    assert_eq!(a.spawns() - a1, 1, "A 的缓存没有被 B 顶掉");

    assert_eq!(first_a.head.commit, second_a.head.commit);
    assert_ne!(first_a.path, first_b.path);
}

#[test]
fn the_least_recently_used_session_is_the_one_that_goes() {
    let a = repo_with_branch("sess-lru-a", "main");
    let b = repo_with_branch("sess-lru-b", "main");
    let c = repo_with_branch("sess-lru-c", "main");
    let set = RepoSet::new(2);

    set.open(a.path()).expect("open a");
    set.open(b.path()).expect("open b");
    // 再取一次 a：于是最久未用的是 b
    set.open(a.path()).expect("touch a");
    set.open(c.path()).expect("open c");

    assert_eq!(set.len(), 2);
    let roots = set.roots();
    assert!(roots.contains(&resolved(&a)), "a 刚被取用过，不该被淘汰");
    assert!(roots.contains(&resolved(&c)));
    assert!(!roots.contains(&resolved(&b)), "b 最久未用，应当被淘汰");
}

#[test]
fn eviction_costs_the_cache_but_not_correctness() {
    let a = repo_with_branch("sess-evict-a", "main");
    let b = repo_with_branch("sess-evict-b", "main");
    let set = RepoSet::new(1);

    set.open(a.path()).expect("open a");
    set.open(b.path()).expect("open b"); // a 被淘汰
    assert_eq!(set.len(), 1);

    // 被淘汰后重新打开：结果必须与「从未缓存过」一致
    let again = set.open(a.path()).expect("reopen a");
    let snap = again.snapshot().expect("snapshot after eviction");
    assert_eq!(snap.head.branch.as_deref(), Some("main"));
    assert_eq!(snap.path, resolved(&a));
}

#[test]
fn closing_releases_the_session() {
    let repo = repo_with_branch("sess-close", "main");
    let set = RepoSet::with_default_cap();

    let first = set.open(repo.path()).expect("open");
    first.snapshot().expect("snapshot");
    assert_eq!(first.spawns(), 3, "open 1 次 + snapshot 2 次");

    assert!(set.close(repo.path()), "关闭一个已打开的仓库应当返回 true");
    assert_eq!(set.len(), 0);

    // 重新打开必须是**新的**实例：spawns 回到 1 才证明旧的那个真的被放掉了
    let second = set.open(repo.path()).expect("reopen");
    assert_eq!(second.spawns(), 1);
    assert!(!Arc::ptr_eq(&first, &second));
}

#[test]
fn closing_clears_every_alias_to_that_repo() {
    let repo = repo_with_branch("sess-alias", "main");
    repo.write("nested/a.txt", "x\n");
    repo.commit("nested");
    let sub = repo.path().join("nested");
    let set = RepoSet::with_default_cap();

    let root = set.open(repo.path()).expect("open by root");
    let by_sub = set.open(&sub).expect("open by subdir");
    assert!(Arc::ptr_eq(&root, &by_sub));
    root.snapshot().expect("snapshot");

    assert!(set.close(repo.path()));

    // 用**另一种写法**打开：别名若没清干净，这里会拿回刚关掉的那个实例（spawns = 3）
    let reborn = set.open(&sub).expect("reopen by subdir");
    assert_eq!(reborn.spawns(), 1, "关闭必须连别名一起清");
    assert!(!Arc::ptr_eq(&root, &reborn));
}

#[test]
fn closing_something_unknown_reports_false() {
    let set = RepoSet::with_default_cap();
    let not_a_repo = std::env::temp_dir().join("repoprism-sess-never-opened");
    assert!(!set.close(&not_a_repo));
    assert_eq!(set.len(), 0);
}

#[test]
fn roots_are_the_resolved_roots_and_come_back_sorted() {
    // 名字按「打开顺序」与字母序相反，于是排序这件事本身被测到
    let z = repo_with_branch("sess-order-z", "main");
    let m = repo_with_branch("sess-order-m", "main");
    let a = repo_with_branch("sess-order-a", "main");
    let set = RepoSet::with_default_cap();

    set.open(z.path()).expect("open z");
    set.open(m.path()).expect("open m");
    set.open(a.path()).expect("open a");

    let roots = set.roots();
    let expected = vec![resolved(&a), resolved(&m), resolved(&z)];
    // 每条都是**已解析的仓库根**，且顺序与打开顺序无关
    assert_eq!(roots, {
        let mut e = expected;
        e.sort();
        e
    });
}

#[test]
fn a_path_that_is_not_a_repo_fails_and_takes_no_slot() {
    let repo = repo_with_branch("sess-bad", "main");
    let set = RepoSet::with_default_cap();
    set.open(repo.path()).expect("open the good one");

    // 所有临时仓库的**父目录**：它在任何仓库之外
    let outside = std::env::temp_dir();
    let before = set.len();
    let err = set.open(&outside);
    assert!(err.is_err(), "{:?} 不该被当成仓库", outside);
    assert_eq!(set.len(), before, "失败的打开不该占槽位");
    assert_eq!(
        set.roots(),
        vec![resolved(&repo)],
        "失败的打开也不该留下别名"
    );
}

#[test]
fn a_cap_of_one_is_the_old_single_session_behaviour() {
    let a = repo_with_branch("sess-cap1-a", "main");
    let b = repo_with_branch("sess-cap1-b", "main");
    let set = RepoSet::new(1);

    set.open(a.path()).expect("open a");
    let only = set.open(b.path()).expect("open b");
    assert_eq!(set.len(), 1);
    assert_eq!(only.snapshot().expect("snapshot").path, resolved(&b));
}

#[test]
fn the_documented_default_cap_is_what_default_gives() {
    assert_eq!(RepoSet::with_default_cap().cap(), DEFAULT_CAP);
    assert_eq!(RepoSet::default().cap(), DEFAULT_CAP);
    assert!(RepoSet::default().is_empty());
}
