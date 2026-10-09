//! 引用映射缓存（TASK-018）。
//!
//! # 这里要证的不是「缓存生效了」，而是「缓存不会撒谎」
//!
//! 缓存唯一不可接受的失效模式是**静默过期**：仓库在两次调用之间被改了，
//! 界面却继续显示旧的分支/标签，而且没有任何提示。所以本文件的重点不在
//! 「省了几次子进程」，而在**每一种引用变动之后，缓存都必须失效**：
//! 新建分支 / 新建标签 / 删除分支 / 松引用被打包 / 换分支（HEAD 变了）。
//!
//! 每条用例都同时断言两件事：
//!
//! 1. **结果正确** —— 变动的确出现在返回的映射里（缓存没掩盖它）
//! 2. **确实重读了** —— `for-each-ref` 的子进程次数回到 2（缓存确实失效了）
//!
//! 只断言第 1 条不够：一个「每次都重读」的实现也能通过，那样就测不出缓存；
//! 只断言第 2 条也不够：一个「失效了但结果算错」的实现也能通过。
//!
//! 反向的用例同样重要 —— [`a_cached_instance_reads_the_ref_map_once`] 断言
//! 缓存**确实命中**（第二次只起 1 个进程）。否则「指纹每次都在变」这种
//! 退化会让上面所有失效用例照样通过，而缓存其实从未生效。

mod common;

use common::TempRepo;
use repo_prism_core::Git;
use std::path::Path;

/// 递归数一个目录下的文件个数（用于证明「松引用确实被打包了」这类前提）。
fn file_count(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                file_count(&path)
            } else {
                1
            }
        })
        .sum()
}

fn branch_names(snapshot: &repo_prism_core::RepoSnapshot) -> Vec<String> {
    let mut names: Vec<String> = snapshot.branches.iter().map(|b| b.name.clone()).collect();
    names.sort();
    names
}

// ---------------------------------------------------------------------------
// 反向：缓存确实命中
// ---------------------------------------------------------------------------

#[test]
fn a_cached_instance_reads_the_ref_map_once() {
    let repo = TempRepo::new("cache-hit");
    repo.seed_fast_import(20);
    let git = Git::open_cached(repo.path()).expect("open repo");

    assert_eq!(git.spawns(), 1, "open_cached() 也只起一次 rev-parse");

    let before = git.spawns();
    git.snapshot().expect("snapshot");
    assert_eq!(
        git.spawns() - before,
        2,
        "首次 snapshot() 仍要 status(1) + for-each-ref(1)"
    );

    let before = git.spawns();
    git.snapshot().expect("snapshot");
    assert_eq!(
        git.spawns() - before,
        1,
        "引用映射已缓存，第二次 snapshot() 只该起 status(1)；\
         若这里是 2，说明指纹每次都在变，缓存从未生效"
    );

    let before = git.spawns();
    git.commits(10, 0).expect("commits");
    assert_eq!(
        git.spawns() - before,
        1,
        "commits() 复用缓存的引用映射，只该起 log(1)"
    );

    let first = git.commits(1, 0).expect("head commit")[0].sha.clone();

    let before = git.spawns();
    git.commit(&first).expect("commit");
    assert_eq!(
        git.spawns() - before,
        1,
        "commit() 只该起 log(1)，引用映射走缓存"
    );
}

// ---------------------------------------------------------------------------
// 正向：引用一变，缓存必须失效
// ---------------------------------------------------------------------------

#[test]
fn a_new_branch_invalidates_the_cached_ref_map() {
    let repo = TempRepo::new("cache-new-branch");
    repo.seed_fast_import(10);
    let git = Git::open_cached(repo.path()).expect("open repo");

    assert_eq!(branch_names(&git.snapshot().expect("snapshot")), ["main"]);

    // 在 Git 实例之外改动引用 —— 缓存不可能「知道」，只能靠指纹发现
    repo.git(&["branch", "sprout"]);

    let before = git.spawns();
    let after = git.snapshot().expect("snapshot");
    assert_eq!(
        branch_names(&after),
        ["main", "sprout"],
        "新分支必须可见 —— 缓存不得掩盖它"
    );
    assert_eq!(
        git.spawns() - before,
        2,
        "引用变了，for-each-ref 必须重跑（status 1 + refs 1）"
    );
}

#[test]
fn a_new_tag_invalidates_the_cached_ref_map_and_reaches_the_commit_labels() {
    let repo = TempRepo::new("cache-new-tag");
    repo.seed_fast_import(10);
    let git = Git::open_cached(repo.path()).expect("open repo");

    let _ = git.snapshot().expect("snapshot");
    let labels = git.commits(1, 0).expect("commits")[0].refs.clone();
    assert!(
        !labels.iter().any(|r| r == "v0"),
        "标签尚不存在时不该出现在提交标注里：{labels:?}"
    );

    repo.git(&["tag", "v0"]);

    let before = git.spawns();
    let after = git.snapshot().expect("snapshot");
    assert!(after.tags.iter().any(|t| t.name == "v0"), "新标签必须可见");
    assert_eq!(git.spawns() - before, 2, "引用变了，必须重读");

    // 缓存的是 by_sha 映射，而它直接决定提交列表上的标签 —— 一起验
    let labels = git.commits(1, 0).expect("commits")[0].refs.clone();
    assert!(
        labels.iter().any(|r| r == "v0"),
        "新标签必须出现在提交标注里：{labels:?}"
    );
}

#[test]
fn a_deleted_branch_invalidates_the_cached_ref_map() {
    let repo = TempRepo::new("cache-del-branch");
    repo.seed_fast_import(10);
    let git = Git::open_cached(repo.path()).expect("open repo");

    repo.git(&["branch", "doomed"]);
    assert_eq!(
        branch_names(&git.snapshot().expect("snapshot")),
        ["doomed", "main"]
    );

    repo.git(&["branch", "--delete", "doomed"]);

    let before = git.spawns();
    let after = git.snapshot().expect("snapshot");
    assert_eq!(branch_names(&after), ["main"], "删掉的分支不该还留在缓存里");
    assert_eq!(git.spawns() - before, 2, "删除同样改变引用数据，必须重读");
}

#[test]
fn packing_the_refs_invalidates_the_cached_ref_map() {
    let repo = TempRepo::new("cache-pack-refs");
    repo.seed_fast_import(10);
    repo.git(&["branch", "sprout"]);
    repo.git(&["tag", "v0"]);

    let git = Git::open_cached(repo.path()).expect("open repo");
    let before = git.snapshot().expect("snapshot");
    assert_eq!(branch_names(&before), ["main", "sprout"]);

    let loose_before = file_count(&repo.path().join(".git").join("refs"));
    assert_eq!(loose_before, 3, "前提：main / sprout / v0 三个松引用");

    // 松引用被打包进 packed-refs，`refs/` 下的文件被删除。
    // 这一步是缓存失效里最容易漏的一类：文件不是被「改写」而是被「搬走」了，
    // 只盯着松引用的内容变化会看到「文件变少了」而不是「内容变了」。
    repo.git(&["pack-refs", "--all"]);
    assert_eq!(
        file_count(&repo.path().join(".git").join("refs")),
        0,
        "前提：打包后 refs/ 下应不再有松引用（否则这条用例没测到 packed-refs 那条路径）"
    );
    assert!(
        repo.path().join(".git").join("packed-refs").exists(),
        "前提：packed-refs 应已生成"
    );

    let before = git.spawns();
    let after = git.snapshot().expect("snapshot");
    assert_eq!(
        branch_names(&after),
        ["main", "sprout"],
        "打包不得让分支消失"
    );
    assert!(after.tags.iter().any(|t| t.name == "v0"));
    assert_eq!(
        git.spawns() - before,
        2,
        "引用数据换了存放位置，指纹必须变（只哈希松引用内容会漏掉这一步）"
    );
}

#[test]
fn deleting_an_already_packed_ref_invalidates_the_cached_ref_map() {
    let repo = TempRepo::new("cache-pack-delete");
    repo.seed_fast_import(10);
    repo.git(&["branch", "alpha"]);
    repo.git(&["branch", "beta"]);
    repo.git(&["pack-refs", "--all"]);

    let loose_dir = repo.path().join(".git").join("refs");
    assert_eq!(
        file_count(&loose_dir),
        0,
        "前提：打包后 refs/ 下应无松引用，否则这条用例测不到 packed-refs 那条路径"
    );

    let git = Git::open_cached(repo.path()).expect("open repo");
    assert_eq!(
        branch_names(&git.snapshot().expect("snapshot")),
        ["alpha", "beta", "main"]
    );

    // 全部引用都已打包 ⇒ 删一个分支**只重写 packed-refs**，refs/ 下依然空空如也。
    // 这是「指纹只看松引用」会漏掉的唯一一类改动：松引用集合与内容都没变，
    // 变的是另一份文件。上面的 pack-refs 用例抓不到它（那一步是文件被搬走，
    // 松引用集合变小了），必须单独用例。
    repo.git(&["update-ref", "-d", "refs/heads/beta"]);
    assert_eq!(
        file_count(&loose_dir),
        0,
        "前提：删除已打包的引用不应产生松引用（否则这条用例失去意义）"
    );

    let before = git.spawns();
    let after = git.snapshot().expect("snapshot");
    assert_eq!(
        branch_names(&after),
        ["alpha", "main"],
        "已删除的打包分支不该还留在缓存里"
    );
    assert_eq!(
        git.spawns() - before,
        2,
        "指纹必须覆盖 packed-refs 本身，否则这一步会被漏掉"
    );
}

#[test]
fn moving_a_branch_to_another_commit_invalidates_the_cached_ref_map() {
    let repo = TempRepo::new("cache-move-branch");
    repo.write("f.txt", "one\n");
    repo.commit("one");
    repo.write("f.txt", "two\n");
    repo.commit("two");
    let older = repo.git(&["rev-parse", "HEAD~1"]).trim().to_string();

    let git = Git::open_cached(repo.path()).expect("open repo");
    let first = git.snapshot().expect("snapshot");
    let main_tip = |snap: &repo_prism_core::RepoSnapshot| {
        snap.branches
            .iter()
            .find(|b| b.name == "main")
            .map(|b| b.commit.clone())
            .expect("main 应存在")
    };
    assert_ne!(main_tip(&first), older);

    // 文件名没变、内容变了，而且新旧 sha 都是 40 个十六进制字符 —— 大小也不变。
    // 这正是文件时间戳与文件大小都判不出来的那一类改写；只有哈希**内容**才能发现。
    repo.git(&["update-ref", "refs/heads/main", &older]);

    let before = git.spawns();
    let after = git.snapshot().expect("snapshot");
    assert_eq!(
        main_tip(&after),
        older,
        "分支被移到别的提交上，引用映射必须跟着变"
    );
    assert_eq!(
        git.spawns() - before,
        2,
        "只按文件名或大小判失效会漏掉这一步"
    );
}

#[test]
fn renaming_a_branch_invalidates_the_cached_ref_map() {
    let repo = TempRepo::new("cache-rename-branch");
    repo.seed_fast_import(10);
    repo.git(&["branch", "alpha"]);

    let git = Git::open_cached(repo.path()).expect("open repo");
    assert_eq!(
        branch_names(&git.snapshot().expect("snapshot")),
        ["alpha", "main"]
    );

    // alpha 与 main 指向同一个提交，所以松引用**内容逐字节相同**：
    // 改名后如果指纹只哈希文件内容、不哈希文件名，缓存不会失效，
    // 界面会继续显示一个已经不存在的分支。这条用例钉的就是这一点。
    repo.git(&["branch", "--move", "alpha", "beta"]);

    let before = git.spawns();
    let after = git.snapshot().expect("snapshot");
    assert_eq!(
        branch_names(&after),
        ["beta", "main"],
        "改名后的分支必须可见，旧名必须消失"
    );
    assert_eq!(
        git.spawns() - before,
        2,
        "文件名也是引用数据的一部分，指纹必须覆盖它"
    );
}

#[test]
fn switching_the_checked_out_branch_invalidates_the_cached_ref_map() {
    let repo = TempRepo::new("cache-head");
    repo.write("f.txt", "one\n");
    repo.commit("one");
    repo.git(&["branch", "other"]);

    let git = Git::open_cached(repo.path()).expect("open repo");
    let first = git.snapshot().expect("snapshot");
    assert!(
        first
            .branches
            .iter()
            .any(|b| b.name == "main" && b.is_current),
        "初始应在 main 上"
    );

    // HEAD 是「当前分支标记」的唯一来源，而它既不在 refs/ 下、也不在 packed-refs 里
    repo.git(&["checkout", "--quiet", "other"]);

    let before = git.spawns();
    let after = git.snapshot().expect("snapshot");
    assert!(
        after
            .branches
            .iter()
            .any(|b| b.name == "other" && b.is_current),
        "切换后 other 应成为当前分支"
    );
    assert!(
        !after
            .branches
            .iter()
            .any(|b| b.name == "main" && b.is_current),
        "main 不该还是当前分支"
    );
    assert_eq!(
        git.spawns() - before,
        2,
        "HEAD 变了，指纹必须跟着变（漏掉 HEAD 会让「当前分支」永远停在旧值）"
    );
}

// ---------------------------------------------------------------------------
// 不该被缓存的：工作区状态
// ---------------------------------------------------------------------------

#[test]
fn working_tree_state_is_never_served_from_the_cache() {
    let repo = TempRepo::new("cache-status");
    repo.write("f.txt", "one\n");
    repo.commit("one");

    let git = Git::open_cached(repo.path()).expect("open repo");
    assert_eq!(
        git.snapshot().expect("snapshot").status.unstaged.len(),
        0,
        "初始工作区应干净"
    );

    repo.write("f.txt", "two\n");

    let before = git.spawns();
    let after = git.snapshot().expect("snapshot");
    assert_eq!(
        after.status.unstaged.len(),
        1,
        "工作区状态从不走缓存 —— 缓存只覆盖引用映射"
    );
    assert_eq!(
        git.spawns() - before,
        1,
        "这一次只需要 status(1)：引用没变，走缓存"
    );
}

// ---------------------------------------------------------------------------
// 缓存是显式开启的：不开启时行为必须与 TASK-018 之前一致
// ---------------------------------------------------------------------------

#[test]
fn the_uncached_reader_always_re_reads_the_ref_map() {
    let repo = TempRepo::new("cache-optin");
    repo.seed_fast_import(10);
    let git = Git::open(repo.path()).expect("open repo");

    let before = git.spawns();
    git.snapshot().expect("snapshot");
    assert_eq!(git.spawns() - before, 2);

    let before = git.spawns();
    git.snapshot().expect("snapshot");
    assert_eq!(
        git.spawns() - before,
        2,
        "Git::open() 不缓存 —— 这是刻意的：spawn 次数门禁钉的就是这个与状态无关的量"
    );

    let before = git.spawns();
    git.commits(5, 0).expect("commits");
    assert_eq!(git.spawns() - before, 2);
}

// ---------------------------------------------------------------------------
// 链接工作树：refs 在 common dir，HEAD 在 per-worktree dir
// ---------------------------------------------------------------------------

#[test]
fn a_linked_worktree_caches_refs_from_the_common_dir() {
    let repo = TempRepo::new("cache-worktree");
    repo.write("f.txt", "one\n");
    repo.commit("one");

    // 另建一个平级目录作为链接工作树
    let wt = std::env::temp_dir().join(format!(
        "repoprism-wt-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    repo.git(&["worktree", "add", &wt.to_string_lossy(), "-b", "carved"]);

    let git = Git::open_cached(&wt).expect("open worktree");
    let first = git.snapshot().expect("snapshot");
    assert_eq!(
        branch_names(&first),
        ["carved", "main"],
        "工作树里应能看到公共引用目录下的全部分支"
    );
    assert!(
        first
            .branches
            .iter()
            .any(|b| b.name == "carved" && b.is_current),
        "当前分支取自本工作树的 HEAD，而不是 common dir 的"
    );

    let before = git.spawns();
    git.snapshot().expect("snapshot");
    assert_eq!(
        git.spawns() - before,
        1,
        "缓存应命中 —— 指纹同时覆盖 common dir 的 refs 与本工作树的 HEAD"
    );

    let _ = std::fs::remove_dir_all(&wt);
    let _ = repo.git_raw(&["worktree", "prune"]);
}
