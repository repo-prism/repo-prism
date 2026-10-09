//! unified diff 与 `--name-status` 输出的解析。
//!
//! 本模块**不做任何 Git 调用**，只负责把已经读到的文本变成结构体，
//! 因此可以脱离仓库独立做单元测试。
//!
//! # 为什么路径取自 `--name-status -z` 而不是 patch 头部
//!
//! patch 里的路径是**有歧义**的，实测（git 2.x）：
//!
//! ```text
//! diff --git a/old.txt b/new name.txt
//! --- a/old.txt
//! +++ b/new name.txt<TAB>
//! ```
//!
//! 含空格的路径会被追加一个制表符，非 ASCII 路径还会被 `core.quotePath` 转成
//! C 风格八进制转义。而 `--name-status -z` 输出的路径是**原字节、NUL 分隔、
//! 不做任何转义**的，是唯一可靠的来源。
//!
//! 于是分工为：路径与类型来自 `--name-status -z`；hunk 内容来自 patch。
//! 两者由 Git 按同一顺序生成，因此按**下标**一一对应即可，无需解析 patch 里的路径。

use crate::model::{ChangeKind, DiffFile, DiffHunk, DiffLine, DiffLineKind};

/// 单个变更文件的身份信息，来自 `--name-status --find-renames -z`。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NameStatus {
    pub kind: ChangeKind,
    /// 新路径（删除时为被删路径）。
    pub path: String,
    /// 重命名 / 复制时的原路径。
    pub old_path: Option<String>,
}

/// 解析 `--name-status --find-renames -z` 的输出。
///
/// 记录格式（NUL 分隔，实测）：
///
/// - 普通变更：`<STATUS>` `NUL` `<path>` `NUL`
/// - 重命名 / 复制：`<STATUS><score>` `NUL` `<old>` `NUL` `<new>` `NUL`
///   （例如 `R050`、`C100`）
pub(crate) fn parse_name_status(raw: &str) -> Vec<NameStatus> {
    let mut entries = Vec::new();
    let mut fields = raw.split('\0');

    while let Some(status) = fields.next() {
        if status.is_empty() {
            continue;
        }
        let code = status.chars().next().unwrap_or('?');
        let kind = ChangeKind::from_status_code(code);

        if code == 'R' || code == 'C' {
            // 重命名 / 复制携带两个路径，少一个都不能猜
            let (Some(old), Some(new)) = (fields.next(), fields.next()) else {
                break;
            };
            entries.push(NameStatus {
                kind,
                path: new.to_string(),
                old_path: Some(old.to_string()),
            });
        } else {
            let Some(path) = fields.next() else { break };
            entries.push(NameStatus {
                kind,
                path: path.to_string(),
                old_path: None,
            });
        }
    }

    entries
}

/// 解析 patch 正文，与 `names` 按下标一一对应。
///
/// `max_lines` 是累计 hunk 行的上限。超出后**不再解析**后续内容，并把受影响
/// 的文件与整体都标记为 `truncated`——截断必须显式可见，不能静默丢弃。
///
/// 返回 `(files, truncated)`。
pub(crate) fn parse_patch(
    patch: &str,
    names: &[NameStatus],
    max_lines: usize,
) -> (Vec<DiffFile>, bool) {
    // 按 `diff --git ` 分节。分节只用于定位内容，路径不从中解析。
    let mut sections: Vec<Vec<&str>> = Vec::new();
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            sections.push(Vec::new());
        }
        if let Some(current) = sections.last_mut() {
            current.push(line);
        }
    }

    let mut files = Vec::with_capacity(names.len());
    let mut budget = max_lines;
    let mut truncated_any = false;
    let mut stopped = false;

    for (index, name) in names.iter().enumerate() {
        let mut file = DiffFile {
            path: name.path.clone(),
            old_path: name.old_path.clone(),
            kind: name.kind,
            binary: false,
            truncated: stopped,
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
        };

        if !stopped {
            if let Some(section) = sections.get(index) {
                let outcome = parse_section(section, &mut file, budget);
                budget = outcome.remaining;
                if outcome.truncated {
                    file.truncated = true;
                    stopped = true;
                }
            }
        }

        if file.truncated {
            truncated_any = true;
        }
        files.push(file);
    }

    (files, truncated_any)
}

struct SectionOutcome {
    remaining: usize,
    truncated: bool,
}

fn parse_section(section: &[&str], file: &mut DiffFile, budget: usize) -> SectionOutcome {
    let mut remaining = budget;
    let mut truncated = false;
    let mut current: Option<DiffHunk> = None;
    let mut old_no: u32 = 0;
    let mut new_no: u32 = 0;
    let mut inside_hunks = false;

    for line in section {
        if line.starts_with("@@") {
            if let Some(hunk) = current.take() {
                file.hunks.push(hunk);
            }
            current =
                parse_hunk_header(line).map(|(old_start, old_lines, new_start, new_lines)| {
                    old_no = old_start;
                    new_no = new_start;
                    DiffHunk {
                        header: (*line).to_string(),
                        old_start,
                        old_lines,
                        new_start,
                        new_lines,
                        lines: Vec::new(),
                    }
                });
            inside_hunks = true;
            continue;
        }

        if !inside_hunks {
            // hunk 之前的元信息行：只关心二进制判定
            if line.starts_with("Binary files ") {
                file.binary = true;
            }
            continue;
        }

        let Some(prefix) = line.chars().next() else {
            break;
        };

        // "\ No newline at end of file" 不是内容行，不占预算也不推进行号
        if prefix == '\\' {
            continue;
        }

        if remaining == 0 {
            truncated = true;
            break;
        }

        let content = line.get(1..).unwrap_or("").to_string();
        match prefix {
            ' ' => {
                push_line(
                    &mut current,
                    DiffLineKind::Context,
                    Some(old_no),
                    Some(new_no),
                    content,
                );
                old_no += 1;
                new_no += 1;
            }
            '+' => {
                push_line(&mut current, DiffLineKind::Add, None, Some(new_no), content);
                new_no += 1;
                file.additions += 1;
            }
            '-' => {
                push_line(&mut current, DiffLineKind::Del, Some(old_no), None, content);
                old_no += 1;
                file.deletions += 1;
            }
            // 既不是 hunk 行也不是 hunk 头，说明本文件的 diff 已经结束
            _ => break,
        }
        remaining -= 1;
    }

    if let Some(hunk) = current.take() {
        file.hunks.push(hunk);
    }

    SectionOutcome {
        remaining,
        truncated,
    }
}

fn push_line(
    current: &mut Option<DiffHunk>,
    kind: DiffLineKind,
    old_no: Option<u32>,
    new_no: Option<u32>,
    content: String,
) {
    if let Some(hunk) = current.as_mut() {
        hunk.lines.push(DiffLine {
            kind,
            old_no,
            new_no,
            content,
        });
    }
}

/// 解析 `@@ -1,3 +1,4 @@ 可选上下文`。
///
/// 行数为 1 时 Git 省略 `,1`（例如 `@@ -1 +1,2 @@`），因此不能依赖逗号一定存在。
fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    // split_whitespace 已自动跳过首尾空白，无需先 trim
    let mut parts = line.trim_start_matches('@').split_whitespace();
    let old = parts.next()?;
    let new = parts.next()?;
    let (old_start, old_lines) = parse_range(old.strip_prefix('-')?)?;
    let (new_start, new_lines) = parse_range(new.strip_prefix('+')?)?;
    Some((old_start, old_lines, new_start, new_lines))
}

fn parse_range(text: &str) -> Option<(u32, u32)> {
    match text.split_once(',') {
        Some((start, lines)) => Some((start.parse().ok()?, lines.parse().ok()?)),
        None => Some((text.parse().ok()?, 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 多行样本整体是一个字符串字面量，read-only guard 不会把它当成 CLI 参数。
    const SAMPLE_NS: &str = "M\0a.txt\0A\0bin.dat\0D\0gone.txt\0R050\0old.txt\0new name.txt\0";

    #[test]
    fn parses_name_status_with_rename() {
        let entries = parse_name_status(SAMPLE_NS);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].kind, ChangeKind::Modified);
        assert_eq!(entries[0].path, "a.txt");
        assert_eq!(entries[0].old_path, None);
        assert_eq!(entries[3].kind, ChangeKind::Renamed);
        assert_eq!(entries[3].path, "new name.txt", "含空格的新路径不应被转义");
        assert_eq!(entries[3].old_path.as_deref(), Some("old.txt"));
    }

    #[test]
    fn name_status_tolerates_empty_input() {
        assert!(parse_name_status("").is_empty());
    }

    #[test]
    fn parses_hunk_header_with_and_without_count() {
        assert_eq!(parse_hunk_header("@@ -1,3 +1,4 @@"), Some((1, 3, 1, 4)));
        // 行数为 1 时 git 省略 ",1"
        assert_eq!(
            parse_hunk_header("@@ -1 +1,2 @@ fn main()"),
            Some((1, 1, 1, 2))
        );
        assert_eq!(parse_hunk_header("@@ -12,0 +13,5 @@"), Some((12, 0, 13, 5)));
        assert_eq!(parse_hunk_header("@@ 坏掉的头 @@"), None);
    }

    /// 注意：这里必须用**多行字面量**，不能用 `\` 续行。
    /// Rust 的 `\` 续行会吃掉下一行的前导空格，而 diff 的上下文行前缀恰好是空格——
    /// 用续行写会把 ` a` 变成 `a`，样本就废了（这个坑真踩过一次）。
    /// 多行字面量整体仍是一个字符串字面量，read-only guard 不会把它当成 CLI 参数。
    const SAMPLE_PATCH: &str = "diff --git a/a.txt b/a.txt
index 1..2 100644
--- a/a.txt
+++ b/a.txt
@@ -1,3 +1,4 @@
 a
-b
+B
 c
+d
diff --git a/bin.dat b/bin.dat
new file mode 100644
index 0..1
Binary files /dev/null and b/bin.dat differ
";

    #[test]
    fn parses_hunks_and_line_numbers() {
        let names = parse_name_status("M\0a.txt\0A\0bin.dat\0");
        let (files, truncated) = parse_patch(SAMPLE_PATCH, &names, 5000);

        assert!(!truncated);
        assert_eq!(files.len(), 2);

        let a = &files[0];
        assert_eq!(a.additions, 2, "两行新增：B 与 d");
        assert_eq!(a.deletions, 1, "一行删除：b");
        assert!(!a.binary);
        assert_eq!(a.hunks.len(), 1);

        let hunk = &a.hunks[0];
        assert_eq!(hunk.lines.len(), 5);
        // 上下文行同时占两侧行号
        assert_eq!(hunk.lines[0].old_no, Some(1));
        assert_eq!(hunk.lines[0].new_no, Some(1));
        // 删除行只有旧行号
        assert_eq!(hunk.lines[1].kind, DiffLineKind::Del);
        assert_eq!(hunk.lines[1].old_no, Some(2));
        assert_eq!(hunk.lines[1].new_no, None);
        // 新增行只有新行号
        assert_eq!(hunk.lines[2].kind, DiffLineKind::Add);
        assert_eq!(hunk.lines[2].old_no, None);
        assert_eq!(hunk.lines[2].new_no, Some(2));
        assert_eq!(hunk.lines[2].content, "B", "内容不应包含前导 + 号");
        // 删除与新增各占一侧，后续行号因此错位
        assert_eq!(hunk.lines[3].old_no, Some(3));
        assert_eq!(hunk.lines[3].new_no, Some(3));
        assert_eq!(hunk.lines[4].new_no, Some(4));

        let bin = &files[1];
        assert!(bin.binary, "二进制文件必须被标记");
        assert!(bin.hunks.is_empty(), "二进制文件不应产生 hunk");
    }

    #[test]
    fn truncates_at_line_budget_and_marks_it() {
        let names = parse_name_status("M\0a.txt\0A\0bin.dat\0");
        // 预算 2 行：第 3 行触发截断
        let (files, truncated) = parse_patch(SAMPLE_PATCH, &names, 2);

        assert!(truncated, "超出预算必须显式标记");
        assert!(files[0].truncated);
        assert_eq!(files[0].hunks[0].lines.len(), 2);
        // 后续文件同样被标记，而不是悄悄给出空 diff
        assert!(files[1].truncated);
        assert!(files[1].hunks.is_empty());
    }

    #[test]
    fn survives_exact_budget_exhaustion() {
        let names = parse_name_status("M\0a.txt\0");
        // a.txt 的 hunk 正好 5 行；预算给足就不应标记截断
        let (files, truncated) = parse_patch(SAMPLE_PATCH, &names, 5);
        assert!(!truncated);
        assert_eq!(files[0].hunks[0].lines.len(), 5);
    }
}
