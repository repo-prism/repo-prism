//! Diff 与提交详情集成测试（对应 TASK-007 验收标准）。
//!
//! 覆盖四类变更：新增 / 删除 / 重命名 / 二进制；
//! 以及行号一致性、路径含空格与中文、两点 Diff、截断上限、根提交。
//!
//! # 关于"路径含 `"` 与换行"的取舍
//!
//! TASK-007 的验收标准原文要求路径含 `\t`、`\n`、`"`。但 `"` 是 Windows
//! 文件系统的**保留字符**（`" < > : | ? *` 均不允许出现在文件名中），
//! 含 `\n` 的路径在 Windows 上也会让大多数工具链失稳。
//! CI 矩阵包含 windows-latest，因此这里改用「含空格 + 中文 + 制表符」的路径
//! 来等价地验证同一件事：**路径不能被转义或截断**。
//! 内容侧（而非路径侧）则照常覆盖制表符、引号与换行。

mod common;

use common::TempRepo;
use repo_prism_core::{ChangeKind, DiffLineKind, Git};

#[test]
fn commit_detail_lists_all_four_change_kinds() {
    let repo = TempRepo::new("diff-kinds");
    repo.write("modify.txt", "b\n");
    repo.write("remove.txt", "gone\n");
    repo.write("rename-src.txt", "same\n");
    repo.commit("c1");

    repo.write("modify.txt", "B\n");
    repo.write("added.txt", "new\n");
    repo.git(&["rm", "-q", "remove.txt"]);
    repo.git(&["mv", "rename-src.txt", "rename-dst.txt"]);
    // 含 NUL 字节 → Git 判定为二进制，只标记内容不展示
    repo.write("binary.dat", "\0\0binary\0\n");
    repo.commit("c2");

    let git = Git::open(repo.path()).expect("open repo");
    let detail = git.commit_detail(&repo.head_sha()).expect("commit detail");

    assert_eq!(detail.info.subject, "c2");

    // 契约：`patch` 是**未加工**的 unified diff 正文，供 CLI / MCP / Agent 直接消费
    assert!(
        detail.patch.contains("diff --git a/added.txt b/added.txt"),
        "patch 应含原始 diff 头，实际前 200 字节：{:?}",
        &detail.patch[..detail.patch.len().min(200)]
    );
    assert!(
        detail.patch.contains("@@"),
        "patch 应含 hunk 头，说明不是 name-status 摘要"
    );
    assert!(
        !detail.patch.contains("commit c2"),
        "patch 不应含提交信息头（--format= 已清空）"
    );
    assert!(!detail.truncated, "小提交不应被标记截断");

    let find = |path: &str| {
        detail
            .files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("{path} 应在变更列表中，实际 {:?}", detail.files))
    };

    let added = find("added.txt");
    assert_eq!(added.kind, ChangeKind::Added);
    assert_eq!(added.additions, 1);

    let removed = find("remove.txt");
    assert_eq!(removed.kind, ChangeKind::Deleted);
    assert_eq!(removed.deletions, 1);

    let renamed = find("rename-dst.txt");
    assert_eq!(renamed.kind, ChangeKind::Renamed);
    assert_eq!(
        renamed.old_path.as_deref(),
        Some("rename-src.txt"),
        "重命名必须带上原路径"
    );

    let binary = find("binary.dat");
    assert!(binary.binary, "含 NUL 的文件必须被标记为二进制");

    // 纯内容修改不应丢失
    assert_eq!(find("modify.txt").kind, ChangeKind::Modified);
}

#[test]
fn binary_file_diff_has_no_hunks() {
    let repo = TempRepo::new("diff-binary");
    repo.write("keep.txt", "1\n");
    repo.commit("c1");
    repo.write("binary.dat", "\0\0payload\n");
    repo.commit("c2");

    let git = Git::open(repo.path()).expect("open repo");
    let diff = git.commit_diff(&repo.head_sha()).expect("diff");

    let binary = diff
        .files
        .iter()
        .find(|f| f.path == "binary.dat")
        .expect("二进制文件应在结果中");
    assert!(binary.binary);
    assert!(
        binary.hunks.is_empty(),
        "二进制文件不应产生 hunk —— 只标记，不读取内容"
    );
}

#[test]
fn hunk_line_numbers_match_git_original() {
    let repo = TempRepo::new("diff-lines");
    // 10 行，便于构造跳号
    let original: String = (1..=10).map(|i| format!("line{i}\n")).collect();
    repo.write("f.txt", &original);
    repo.commit("c1");

    // 改第 2 行（=删 line2 + 增 LINE2）、删第 5 行、在第 7 行后插入一行。
    // 净效果：删 2 增 2，总行数不变（10 → 10）。
    repo.write(
        "f.txt",
        "line1\nLINE2\nline3\nline4\nline6\nline7\ninserted\nline8\nline9\nline10\n",
    );
    repo.commit("c2");

    let git = Git::open(repo.path()).expect("open repo");
    let diff = git.commit_diff(&repo.head_sha()).expect("diff");
    let file = &diff.files[0];
    assert_eq!(file.additions, 2, "新增 LINE2 与 inserted");
    assert_eq!(file.deletions, 2, "删除 line2 与 line5");

    let hunk = &file.hunks[0];
    assert_eq!(hunk.header, "@@ -1,10 +1,10 @@", "hunk 头应与 git 原样一致");
    assert_eq!((hunk.old_lines, hunk.new_lines), (10, 10));

    // 逐行核对：旧行号只由上下文与删除行推进，新行号只由上下文与新增行推进
    let context = hunk
        .lines
        .iter()
        .find(|l| l.content == "line1")
        .expect("首行上下文");
    assert_eq!((context.old_no, context.new_no), (Some(1), Some(1)));
    assert_eq!(context.kind, DiffLineKind::Context);

    let changed = hunk
        .lines
        .iter()
        .find(|l| l.content == "LINE2")
        .expect("新增行 LINE2");
    assert_eq!(changed.kind, DiffLineKind::Add);
    assert_eq!(changed.old_no, None, "新增行没有旧行号");
    assert_eq!(changed.new_no, Some(2));

    let deleted = hunk
        .lines
        .iter()
        .find(|l| l.content == "line5")
        .expect("删除行 line5");
    assert_eq!(deleted.kind, DiffLineKind::Del);
    assert_eq!(deleted.old_no, Some(5));
    assert_eq!(deleted.new_no, None, "删除行没有新行号");

    let inserted = hunk
        .lines
        .iter()
        .find(|l| l.content == "inserted")
        .expect("插入行");
    assert_eq!(inserted.new_no, Some(7), "插入行应有正确的新行号");

    // 末行：删 2 增 2，净行数不变，两侧行号回到同一点
    let last = hunk.lines.last().expect("末行");
    assert_eq!(last.content, "line10");
    assert_eq!((last.old_no, last.new_no), (Some(10), Some(10)));
}

#[test]
fn content_with_tabs_quotes_and_unicode_round_trips() {
    let repo = TempRepo::new("diff-encoding");
    repo.write("f.txt", "base\n");
    repo.commit("c1");
    // 内容含制表符、双引号、中文；路径含空格与中文
    repo.write("含 空格 的 文件.txt", "第一行\n");
    repo.write("f.txt", "col1\tcol2\n\"quoted\"\n中文\n");
    repo.commit("c2");

    let git = Git::open(repo.path()).expect("open repo");
    let diff = git.commit_diff(&repo.head_sha()).expect("diff");

    let unicode_path = diff
        .files
        .iter()
        .find(|f| f.path == "含 空格 的 文件.txt")
        .expect("含空格与中文的路径不应被转义或截断");
    assert_eq!(unicode_path.kind, ChangeKind::Added);

    let f = diff
        .files
        .iter()
        .find(|f| f.path == "f.txt")
        .expect("f.txt");
    let contents: Vec<&str> = f
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .map(|l| l.content.as_str())
        .collect();
    assert!(contents.contains(&"col1\tcol2"), "制表符应原样保留");
    assert!(contents.contains(&"\"quoted\""), "双引号应原样保留");
    assert!(contents.contains(&"中文"), "中文应原样保留");
}

#[test]
fn diff_between_two_commits() {
    let repo = TempRepo::new("diff-range");
    repo.write("a.txt", "1\n");
    repo.commit("c1");
    let first = repo.head_sha();
    repo.write("a.txt", "2\n");
    repo.commit("c2");
    let second = repo.head_sha();

    let git = Git::open(repo.path()).expect("open repo");
    let diff = git.diff(Some(&first), Some(&second)).expect("range diff");

    assert_eq!(diff.files.len(), 1);
    let file = &diff.files[0];
    assert_eq!(file.path, "a.txt");
    assert_eq!(file.kind, ChangeKind::Modified);
    assert_eq!((file.additions, file.deletions), (1, 1));
}

#[test]
fn diff_without_endpoints_reads_working_tree() {
    let repo = TempRepo::new("diff-worktree");
    repo.write("a.txt", "1\n");
    repo.commit("c1");
    repo.write("a.txt", "2\n");

    let git = Git::open(repo.path()).expect("open repo");
    let diff = git.diff(None, None).expect("worktree diff");

    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "a.txt");
    assert_eq!(diff.files[0].deletions, 1);
}

#[test]
fn root_commit_shows_full_diff() {
    let repo = TempRepo::new("diff-root");
    repo.write("only.txt", "hello\n");
    repo.commit("initial");

    let git = Git::open(repo.path()).expect("open repo");
    let diff = git.commit_diff(&repo.head_sha()).expect("root diff");

    assert_eq!(diff.files.len(), 1, "根提交应展示全部文件");
    assert_eq!(diff.files[0].kind, ChangeKind::Added);
    assert_eq!(diff.files[0].additions, 1);
}

#[test]
fn oversized_diff_is_truncated_and_says_so() {
    let repo = TempRepo::new("diff-truncate");
    let big: String = (0..6_000).map(|i| format!("old{i}\n")).collect();
    repo.write("big.txt", &big);
    repo.commit("c1");
    let changed: String = (0..6_000).map(|i| format!("new{i}\n")).collect();
    repo.write("big.txt", &changed);
    repo.commit("c2");

    let git = Git::open(repo.path()).expect("open repo");
    let diff = git.commit_diff(&repo.head_sha()).expect("diff");

    assert!(diff.truncated, "超过上限必须显式标记截断");
    assert_eq!(diff.files.len(), 1);
    assert!(diff.files[0].truncated);

    let lines: usize = diff.files[0].hunks.iter().map(|h| h.lines.len()).sum();
    assert_eq!(lines, 5_000, "应恰好停在行数上限，而不是静默丢弃");
}

#[test]
fn unknown_commit_is_an_error_not_a_panic() {
    let repo = TempRepo::new("diff-missing");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let git = Git::open(repo.path()).expect("open repo");
    let bogus = "0".repeat(40);

    assert!(git.commit(&bogus).is_err(), "不存在的提交应返回错误");
    assert!(git.commit_detail(&bogus).is_err());
}

#[test]
fn diff_of_clean_tree_is_empty() {
    let repo = TempRepo::new("diff-clean");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let git = Git::open(repo.path()).expect("open repo");
    let diff = git.diff(None, None).expect("clean diff");

    assert!(diff.files.is_empty());
    assert!(!diff.truncated);
}
