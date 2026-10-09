//! 进行中操作状态、工作树与 stash（US-1 的收尾，补丁 P-09）。
//!
//! # 这个文件证明什么
//!
//! 三件事，全部用**真实的** git 仓库构造，而不是伪造命令输出：
//!
//! 1. **状态探测认得出真的在进行的操作**：冲突合并、冲突变基（连进度）、
//!    未完成的摘取、未完成的回退、进行中的二分查找。每一个都由夹具
//!    真实地跑到失败/停住，然后断言读出来的状态。
//! 2. **标志住在链接工作树自己的 git 目录里**：在链接工作树里变基，
//!    主工作树必须仍然干净。这条正是 TASK-018 把 `git_dir` 与 `common_dir`
//!    分开的理由，也是 P-09 用 `git_dir` 而不是 `common_dir` 的依据。
//! 3. **列表解析接的是真输出**：链接工作树、bare 条目、带说明的 stash。
//!
//! # 这里不钉子进程次数
//!
//! 「`snapshot()` 仍是 2 次、`workspace()` 恰好 2 次」是**性能契约**，
//! 而性能契约的唯一归属地是 `tests/perf.rs`（`ADR/002`：钉数字的地方越少，
//! 它越像契约、越不像实现细节）。放在两处只会让其中一处先腐烂。
//!
//! # 为什么断言枚举而不是 JSON
//!
//! `RepoState` 是带数据的枚举。它的序列化形式（`kind` 标签、进度字段名）
//! 属于**对外契约**，由 `model.rs` 的 `#[serde(...)]` 与 `SPEC.md` 定义 ——
//! 在测试里复述一遍只会造出第二个事实来源。

mod common;

use common::TempRepo;
use repo_prism_core::{Git, RepoState};
use std::path::{Path, PathBuf};

/// 打开仓库并取快照。
fn snapshot(path: &Path) -> repo_prism_core::RepoSnapshot {
    Git::open(path)
        .expect("open repo")
        .snapshot()
        .expect("snapshot")
}

/// 链接工作树的路径：放在仓库旁边，名字由仓库名派生（因此进程内唯一），
/// 且**事先不存在** —— `git worktree add` 要求目标目录为空或不存在。
///
/// 用 `with_extension` 而不是拼字符串：临时目录名里没有点，
/// 追加出来的名字不会覆盖任何东西，也不需要再引入一个随机数。
fn linked_path(repo: &TempRepo) -> PathBuf {
    repo.path().with_extension("linked")
}

/// macOS 上 `/tmp` 是 `/var` 的符号链接，而 git 输出的是解析后的路径。
/// 比较路径前必须两侧都规范化，否则测试会在「谁没解析符号链接」上随机失败。
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).expect("canonicalize")
}

// ---------------------------------------------------------------------------
// 进行中的操作
// ---------------------------------------------------------------------------

/// 没有进行中的操作时必须是 `None`，不是某个表示「干净」的变体。
///
/// 这条与 `HeadInfo.upstream` 的 `null` 同构：`None` 是**缺失**。
/// 若造一个 `Clean` 变体，「未知」与「确认无操作」就会混为一谈。
#[test]
fn a_clean_repository_has_no_state() {
    let repo = TempRepo::new("state-clean");
    repo.write("f.txt", "内容\n");
    repo.commit("第一次提交");

    let snapshot = snapshot(repo.path());
    assert_eq!(snapshot.state, None);
    // 正常提交过的仓库不该被任何标志误判
    assert!(snapshot.status.staged.is_empty());
}

#[test]
fn a_conflicted_merge_is_reported() {
    let repo = TempRepo::new("state-merge");
    repo.write("f.txt", "base\n");
    repo.commit("base");
    repo.git(&["checkout", "-b", "feature"]);
    repo.write("f.txt", "feature\n");
    repo.commit("feature 一侧");
    repo.git(&["checkout", "main"]);
    repo.write("f.txt", "main\n");
    repo.commit("main 一侧");

    let (ok, _, _) = repo.git_raw(&["merge", "feature"]);
    assert!(!ok, "这次合并必须冲突，否则本用例测的不是它");

    let snapshot = snapshot(repo.path());
    assert_eq!(snapshot.state, Some(RepoState::Merge));
    assert!(
        !snapshot.status.conflicts.is_empty(),
        "冲突文件应同时出现在工作区状态里；状态与工作区分组是两条互补的信息"
    );
}

/// 变基要连**进度**一起报出来 —— 「变基中」和「变基到第几步」是两种用途。
#[test]
fn a_conflicted_rebase_is_reported_with_its_progress() {
    let repo = TempRepo::new("state-rebase");
    repo.write("f.txt", "base\n");
    repo.commit("base");
    repo.git(&["checkout", "-b", "feature"]);
    repo.write("f.txt", "feature-1\n");
    repo.commit("第一条要重放的提交（会冲突）");
    repo.write("g.txt", "g\n");
    repo.commit("第二条要重放的提交");
    repo.git(&["checkout", "main"]);
    repo.write("f.txt", "main-1\n");
    repo.commit("main 前进一格");
    repo.git(&["checkout", "feature"]);

    let (ok, _, _) = repo.git_raw(&["rebase", "main"]);
    assert!(!ok, "这次变基必须冲突，否则本用例测的不是它");

    assert_eq!(
        snapshot(repo.path()).state,
        Some(RepoState::Rebase {
            step: Some(1),
            total: Some(2)
        }),
        "要重放 2 条提交、第一条就冲突，所以进度是 1/2"
    );
}

/// 摘取与回退冲突时**不**留下 `MERGE_HEAD`（已由探针实测确认），
/// 所以它们各自靠自己的标记识别。这条断言正是防止有人把它们归并到「合并」。
#[test]
fn an_unfinished_pick_is_reported_as_a_pick_and_not_as_a_merge() {
    let repo = TempRepo::new("state-pick");
    repo.write("f.txt", "base\n");
    repo.commit("base");
    repo.git(&["checkout", "-b", "side"]);
    repo.write("f.txt", "side\n");
    repo.commit("side 一侧");
    let sha = repo.head_sha();
    repo.git(&["checkout", "main"]);
    repo.write("f.txt", "main\n");
    repo.commit("main 一侧");

    let (ok, _, _) = repo.git_raw(&["cherry-pick", &sha]);
    assert!(!ok, "这次摘取必须冲突");

    assert_eq!(snapshot(repo.path()).state, Some(RepoState::CherryPick));
}

#[test]
fn an_unfinished_revert_is_reported() {
    let repo = TempRepo::new("state-revert");
    repo.write("f.txt", "base\n");
    repo.commit("base");
    repo.write("f.txt", "one\n");
    repo.commit("要回退的那一条");
    let sha = repo.head_sha();
    repo.write("f.txt", "two\n");
    repo.commit("之后又改了一次");

    let (ok, _, _) = repo.git_raw(&["revert", "--no-edit", &sha]);
    assert!(!ok, "这次回退必须冲突");

    assert_eq!(snapshot(repo.path()).state, Some(RepoState::Revert));
}

/// 二分查找的标记在 `git bisect start` 之后就在，**不需要**先标 good/bad。
/// 复位之后必须回到 `None` —— 状态是观测出来的，不是粘住的。
#[test]
fn a_running_bisect_is_reported_and_clears_on_reset() {
    let repo = TempRepo::new("state-bisect");
    for i in 1..=6 {
        repo.write("f.txt", &format!("版本 {i}\n"));
        repo.commit(&format!("第 {i} 次提交"));
    }

    repo.git(&["bisect", "start"]);
    assert_eq!(snapshot(repo.path()).state, Some(RepoState::Bisect));

    repo.git(&["bisect", "reset"]);
    assert_eq!(
        snapshot(repo.path()).state,
        None,
        "复位之后不该还报二分查找"
    );
}

// ---------------------------------------------------------------------------
// 链接工作树：标志住在这个工作树自己的 git 目录里
// ---------------------------------------------------------------------------

/// 链接工作树里跑变基，**主工作树必须仍然干净**。
///
/// 这是 P-09 用 `git_dir`（每个工作树各一份）而不是 `common_dir`（共享）的原因，
/// 也是 TASK-018 那处拆分的第一个真实用途。若把探测改成读共享目录，
/// 这条会红。
#[test]
fn state_markers_live_in_the_worktree_that_owns_them() {
    let repo = TempRepo::new("linked-state");
    repo.write("f.txt", "base\n");
    repo.commit("base");
    repo.git(&["checkout", "-b", "feature"]);
    repo.write("f.txt", "feature-1\n");
    repo.commit("第一条要重放的提交（会冲突）");
    repo.write("g.txt", "g\n");
    repo.commit("第二条要重放的提交");
    repo.git(&["checkout", "main"]);
    repo.write("f.txt", "main-1\n");
    repo.commit("main 前进一格");

    let linked = linked_path(&repo);
    repo.git(&["worktree", "add", &linked.to_string_lossy(), "feature"]);

    // 在**链接工作树**里变基
    let (ok, _, _) = repo.git_at_raw(&linked, &["rebase", "main"]);
    assert!(!ok, "链接工作树里的这次变基必须冲突");

    assert_eq!(
        snapshot(&linked).state,
        Some(RepoState::Rebase {
            step: Some(1),
            total: Some(2)
        }),
        "链接工作树应报出自己的变基"
    );
    assert_eq!(
        snapshot(repo.path()).state,
        None,
        "链接工作树里的变基不该让主工作树看起来也在变基 —— 标志不在共享目录里"
    );

    let _ = std::fs::remove_dir_all(&linked);
}

#[test]
fn linked_worktrees_are_listed_with_their_branches() {
    let repo = TempRepo::new("worktrees");
    repo.write("f.txt", "内容\n");
    repo.commit("第一次提交");

    let linked = linked_path(&repo);
    repo.git(&[
        "worktree",
        "add",
        &linked.to_string_lossy(),
        "-b",
        "feature",
    ]);

    let trees = Git::open(repo.path())
        .expect("open repo")
        .worktrees()
        .expect("worktrees");

    assert_eq!(trees.len(), 2, "主工作树 + 一个链接工作树");

    // git 保证主工作树排在最前，解析器据此认定 is_main
    assert!(trees[0].is_main, "第一条必须是主工作树");
    assert_eq!(canonical(&trees[0].path), canonical(repo.path()));
    assert_eq!(trees[0].branch.as_deref(), Some("main"));
    assert!(!trees[0].detached);
    assert!(!trees[0].bare);

    assert!(!trees[1].is_main);
    assert_eq!(canonical(&trees[1].path), canonical(&linked));
    assert_eq!(
        trees[1].branch.as_deref(),
        Some("feature"),
        "`refs/heads/` 前缀必须削掉，与分支列表同一口径"
    );

    let _ = std::fs::remove_dir_all(&linked);
}

/// 每个工作树的 `commit` 是它自己的 HEAD。这条排除了「把主工作树的 HEAD
/// 复制给了所有工作树」这种坏法。
#[test]
fn each_worktree_reports_its_own_head() {
    let repo = TempRepo::new("worktree-heads");
    repo.write("f.txt", "base\n");
    repo.commit("base");
    let main_head = repo.head_sha();

    let linked = linked_path(&repo);
    repo.git(&[
        "worktree",
        "add",
        &linked.to_string_lossy(),
        "-b",
        "feature",
    ]);
    repo.write("g.txt", "g\n");
    repo.commit("主工作树再走一格");
    let advanced_main = repo.head_sha();

    let trees = Git::open(repo.path())
        .expect("open repo")
        .worktrees()
        .expect("worktrees");

    let main = trees.iter().find(|t| t.is_main).expect("主工作树");
    let side = trees.iter().find(|t| !t.is_main).expect("链接工作树");

    assert_eq!(main.commit, advanced_main);
    assert_eq!(side.commit, main_head, "链接工作树停在建它时的那个提交上");
    assert_ne!(main.commit, side.commit, "两个工作树的 HEAD 确实不同");

    let _ = std::fs::remove_dir_all(&linked);
}

// ---------------------------------------------------------------------------
// stash
// ---------------------------------------------------------------------------

#[test]
fn stashes_are_listed_with_their_message() {
    let repo = TempRepo::new("stashes");
    repo.write("f.txt", "base\n");
    repo.commit("base");

    let git = Git::open(repo.path()).expect("open repo");
    assert!(git.stashes().expect("stashes").is_empty(), "还没存取过");

    repo.write("f.txt", "改动\n");
    repo.git(&["stash", "push", "-m", "一条说明"]);

    let stashes = git.stashes().expect("stashes");
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].reference, "stash@{0}");
    assert_eq!(stashes[0].commit.len(), 40, "stash 本身也是一个提交");
    assert!(
        stashes[0].message.contains("一条说明"),
        "说明文字应带上，实际是：{}",
        stashes[0].message
    );
}

/// `workspace()` 把两块合起来 —— 前端一次调用拿全。
#[test]
fn workspace_combines_worktrees_and_stashes() {
    let repo = TempRepo::new("workspace");
    repo.write("f.txt", "base\n");
    repo.commit("base");

    let linked = linked_path(&repo);
    repo.git(&[
        "worktree",
        "add",
        &linked.to_string_lossy(),
        "-b",
        "feature",
    ]);
    repo.write("f.txt", "改动\n");
    repo.git(&["stash", "push", "-m", "暂存一下"]);

    let workspace = Git::open(repo.path())
        .expect("open repo")
        .workspace()
        .expect("workspace");

    assert_eq!(workspace.worktrees.len(), 2);
    assert_eq!(workspace.stashes.len(), 1);

    let _ = std::fs::remove_dir_all(&linked);
}
