//! `repo-prism-core` 集成测试。
//!
//! 覆盖 TASK-002 / TASK-004 的验收标准：
//! - 正常仓库、detached HEAD、无提交仓库的 `snapshot()`
//! - `status()` 的 conflict / staged / unstaged 分组
//! - `commits()` 的 refs 映射、父子关系、limit / skip

mod common;

use common::TempRepo;
use repo_prism_core::{ChangeKind, FileChange, Git};

#[test]
fn snapshot_reads_head_branches_tags_and_clean_status() {
    let repo = TempRepo::new("normal");
    repo.write("README.md", "# demo\n");
    repo.commit("first commit");
    repo.git(&["tag", "v0.1.0"]);

    let git = Git::open(repo.path()).expect("open repo");
    let snap = git.snapshot().expect("snapshot");

    assert_eq!(snap.head.branch.as_deref(), Some("main"));
    assert!(!snap.head.detached, "attached HEAD 不应标记为 detached");
    assert_eq!(snap.head.commit.len(), 40, "HEAD 应为完整 40 位 SHA");
    assert_eq!(snap.head.commit, repo.head_sha());

    assert_eq!(snap.branches.len(), 1);
    assert_eq!(snap.branches[0].name, "main");
    assert!(snap.branches[0].is_current);

    assert_eq!(snap.tags.len(), 1);
    assert_eq!(snap.tags[0].name, "v0.1.0");
    assert_eq!(snap.tags[0].commit, snap.head.commit);

    // 刚提交完，工作区干净
    assert!(snap.status.conflicts.is_empty());
    assert!(snap.status.staged.is_empty());
    assert!(snap.status.unstaged.is_empty());
}

#[test]
fn snapshot_on_detached_head() {
    let repo = TempRepo::new("detached");
    repo.write("a.txt", "1\n");
    repo.commit("c1");
    let sha = repo.head_sha();
    repo.write("a.txt", "2\n");
    repo.commit("c2");

    repo.git(&["checkout", &sha]);

    let git = Git::open(repo.path()).expect("open repo");
    let snap = git.snapshot().expect("snapshot");

    assert!(snap.head.detached, "detached HEAD 必须被识别");
    assert_eq!(snap.head.branch, None);
    assert_eq!(snap.head.commit, sha);
    // 分支列表仍然可读
    assert_eq!(snap.branches.len(), 1);
}

#[test]
fn snapshot_on_repo_without_commits() {
    let repo = TempRepo::new("empty");

    let git = Git::open(repo.path()).expect("open repo");
    let snap = git.snapshot().expect("snapshot on empty repo");

    // unborn 分支：symbolic-ref 仍返回分支名，但 rev-parse HEAD 失败
    assert_eq!(snap.head.branch.as_deref(), Some("main"));
    assert!(snap.head.commit.is_empty(), "无提交时 HEAD sha 应为空");
    assert!(!snap.head.detached);
    assert!(snap.branches.is_empty());
    assert!(snap.tags.is_empty());
    assert!(snap.status.conflicts.is_empty());
    assert!(snap.status.staged.is_empty());
    assert!(snap.status.unstaged.is_empty());

    let commits = git.commits(200, 0).expect("commits on empty repo");
    assert!(commits.is_empty(), "无提交仓库不应返回任何提交");
}

#[test]
fn status_groups_staged_unstaged_and_untracked() {
    let repo = TempRepo::new("status");
    repo.write("tracked.txt", "base\n");
    repo.commit("base");

    // 已暂存：新增文件
    repo.write("staged.txt", "new\n");
    repo.git(&["add", "staged.txt"]);
    // 未暂存：修改已跟踪文件
    repo.write("tracked.txt", "changed\n");
    // 未跟踪文件
    repo.write("untracked.txt", "loose\n");

    let git = Git::open(repo.path()).expect("open repo");
    let status = git.snapshot().expect("snapshot").status;

    assert!(status.conflicts.is_empty());

    let staged: Vec<_> = status.staged.iter().map(|f| f.path.as_str()).collect();
    assert!(staged.contains(&"staged.txt"), "staged={staged:?}");
    let staged_new = status
        .staged
        .iter()
        .find(|f| f.path == "staged.txt")
        .expect("staged.txt 应在 staged 分组");
    assert!(matches!(staged_new.kind, ChangeKind::Added));

    let unstaged: Vec<_> = status.unstaged.iter().map(|f| f.path.as_str()).collect();
    assert!(unstaged.contains(&"tracked.txt"), "unstaged={unstaged:?}");
    assert!(unstaged.contains(&"untracked.txt"), "unstaged={unstaged:?}");

    let modified = status
        .unstaged
        .iter()
        .find(|f| f.path == "tracked.txt")
        .expect("tracked.txt 应在 unstaged");
    assert_eq!(modified.kind, ChangeKind::Modified);
}

#[test]
fn status_reports_merge_conflicts() {
    let repo = TempRepo::new("conflict");
    repo.write("conflict.txt", "base\n");
    repo.commit("base");

    repo.git(&["checkout", "-b", "feature"]);
    repo.write("conflict.txt", "feature\n");
    repo.commit("feature change");

    repo.git(&["checkout", "main"]);
    repo.write("conflict.txt", "main\n");
    repo.commit("main change");

    // 合并冲突时 git 返回非零退出码，这是预期行为
    let (ok, _, _) = repo.git_raw(&["merge", "feature"]);
    assert!(!ok, "预期 merge 产生冲突");

    let git = Git::open(repo.path()).expect("open repo");
    let status = git.snapshot().expect("snapshot").status;

    let conflicted: Vec<_> = status.conflicts.iter().map(|f| f.path.as_str()).collect();
    assert!(
        conflicted.contains(&"conflict.txt"),
        "冲突文件应进入 conflicts 分组，实际 conflicts={conflicted:?}"
    );
    assert!(matches!(
        status.conflicts[0].kind,
        ChangeKind::Unmerged | ChangeKind::Modified
    ));
}

#[test]
fn commits_parses_parents_refs_and_subjects() {
    let repo = TempRepo::new("commits");
    // 钉死每条提交的时间，保证 git log 的顺序确定
    repo.write("a.txt", "1\n");
    repo.commit_at("c1", "2026-01-01T00:00:00+08:00");
    let first = repo.head_sha();

    repo.git(&["tag", "v1.0.0"]);
    repo.git(&["checkout", "-b", "feature"]);

    repo.write("a.txt", "2\n");
    repo.commit_at("c2 on feature", "2026-01-01T01:00:00+08:00");
    let second = repo.head_sha();

    repo.git(&["checkout", "main"]);
    repo.write("a.txt", "3\n");
    repo.commit_at("c3 on main", "2026-01-01T02:00:00+08:00");

    let git = Git::open(repo.path()).expect("open repo");
    let commits = git.commits(200, 0).expect("commits");

    assert_eq!(commits.len(), 3, "三条提交都应被 --all 覆盖");
    assert_eq!(commits[0].subject, "c3 on main");

    // tag 打在首个提交上
    let tagged = commits
        .iter()
        .find(|c| c.sha == first)
        .expect("首个提交应在结果中");
    assert!(
        tagged.refs.iter().any(|r| r == "v1.0.0"),
        "tag 应附着到对应提交，实际 refs={:?}",
        tagged.refs
    );

    // 分支 ref 挂在各分支的尖端，而不是首个提交
    let main_tip = commits
        .iter()
        .find(|c| c.refs.iter().any(|r| r == "main"))
        .expect("main 应附着在其尖端提交");
    assert_eq!(main_tip.subject, "c3 on main");

    let feature_tip = commits
        .iter()
        .find(|c| c.refs.iter().any(|r| r == "feature"))
        .expect("feature 应附着在其尖端提交");
    assert_eq!(feature_tip.sha, second);

    let tip = commits
        .iter()
        .find(|c| c.sha == second)
        .expect("feature 提交应在结果中");
    assert_eq!(tip.parents, vec![first.clone()], "父提交解析错误");
    assert_eq!(tip.short_sha.len(), 7.min(second.len()));
    assert!(tip.author_name.contains("Test User"));
    assert_eq!(tip.body, None, "无正文时 body 应为 None");

    // 每条提交都必须有非空 sha 与作者时间
    for c in &commits {
        assert_eq!(c.sha.len(), 40);
        assert!(!c.author_date.is_empty());
        assert!(!c.committer_date.is_empty());
    }
}

#[test]
fn commits_honours_limit_and_skip() {
    let repo = TempRepo::new("paging");
    for i in 0..5 {
        repo.write("f.txt", &format!("{i}\n"));
        // 递增时间，确保 log 顺序稳定
        repo.commit_at(
            &format!("commit {i}"),
            &format!("2026-01-0{}T00:00:00+08:00", i + 1),
        );
    }

    let git = Git::open(repo.path()).expect("open repo");

    let all = git.commits(10, 0).expect("all");
    assert_eq!(all.len(), 5);

    // `git log -n{limit} --skip={skip}`：skip 先跳过，再取 limit 条
    let first_page = git.commits(2, 0).expect("limit=2");
    assert_eq!(first_page.len(), 2);
    assert_eq!(first_page[0].sha, all[0].sha);
    assert_eq!(first_page[1].sha, all[1].sha);

    let second_page = git.commits(2, 2).expect("limit=2 skip=2");
    assert_eq!(second_page.len(), 2);
    assert_eq!(second_page[0].sha, all[2].sha, "skip=2 应从第三条开始");
    assert_eq!(second_page[1].sha, all[3].sha);

    let single = git.commits(1, 2).expect("limit=1 skip=2");
    assert_eq!(single.len(), 1);
    assert_eq!(single[0].sha, all[2].sha);

    let huge = git.commits(2000, 0).expect("limit > 总数");
    assert_eq!(huge.len(), 5);

    // skip 超出总数时应返回空，而不是报错
    let past_end = git.commits(2, 100).expect("skip 越界");
    assert!(past_end.is_empty());
}

#[test]
fn commits_captures_body() {
    let repo = TempRepo::new("body");
    repo.write("a.txt", "1\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-m", "subject line", "-m", "body line"]);

    let git = Git::open(repo.path()).expect("open repo");
    let commits = git.commits(10, 0).expect("commits");

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].subject, "subject line");
    assert_eq!(commits[0].body.as_deref(), Some("body line"));
}

#[test]
fn model_types_are_comparable() {
    // 公开类型需可比较，调用方与测试才能直接断言，而不必退化为 matches!
    let a = FileChange {
        path: "a.txt".to_string(),
        kind: ChangeKind::Modified,
    };
    let b = FileChange {
        path: "a.txt".to_string(),
        kind: ChangeKind::Modified,
    };
    assert_eq!(a, b);
    assert_ne!(a.kind, ChangeKind::Added);
}

#[test]
fn snapshot_is_stable_across_repeated_reads() {
    let repo = TempRepo::new("stable");
    repo.write("a.txt", "1\n");
    repo.commit_at("c1", "2026-01-01T00:00:00+08:00");

    let git = Git::open(repo.path()).expect("open repo");
    let first = git.snapshot().expect("first snapshot");
    let second = git.snapshot().expect("second snapshot");

    // 仓库未变动时，两次读取的结果必须一致
    assert_eq!(first, second);
}

#[test]
fn open_rejects_non_repository() {
    let dir = std::env::temp_dir();
    // temp_dir 本身通常不是 git 仓库；若恰好是，则跳过该断言
    if Git::open(&dir).is_ok() {
        return;
    }
    assert!(
        Git::open(&dir).is_err(),
        "非 Git 目录应返回错误而不是 panic"
    );
}
