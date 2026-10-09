//! `repoprism` 的 commits / detail / skill 三个子命令集成测试
//! （对应 TASK-008 验收标准）。
//!
//! 全部通过**真实子进程**执行打包好的二进制：CLI 的价值就在进程边界上，
//! 直接调用库函数测不到参数解析、退出码与 stdout 格式。

mod common;

use common::TempRepo;
use std::fs;
use std::process::Command;

fn repoprism() -> Command {
    Command::new(env!("CARGO_BIN_EXE_repoprism"))
}

fn json_of(out: &[u8]) -> serde_json::Value {
    serde_json::from_slice(out).expect("输出必须是合法 JSON")
}

#[test]
fn commits_json_returns_history_with_envelope() {
    let repo = TempRepo::new("commits");
    for i in 1..=3 {
        repo.write("log.txt", &format!("line {i}\n"));
        repo.commit(&format!("c{i}"));
    }

    let out = repoprism()
        .args(["commits"])
        .arg(repo.path())
        .args(["--limit", "2", "--json"])
        .output()
        .expect("failed to run repoprism");

    assert!(
        out.status.success(),
        "commits 应成功退出，stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let value = json_of(&out.stdout);
    assert_eq!(value["schema_version"], "1");
    assert_eq!(value["tool"], "repoprism");

    let commits = value["data"].as_array().expect("data 应为数组");
    assert_eq!(commits.len(), 2, "--limit 2 必须真的只给 2 条");
    assert_eq!(commits[0]["subject"], "c3", "最新的提交排在最前");
    assert!(
        commits[0]["refs"]
            .as_array()
            .is_some_and(|refs| refs.iter().any(|r| r == "main")),
        "当前分支应出现在 refs 里，实际 {:?}",
        commits[0]["refs"]
    );
}

#[test]
fn detail_json_carries_raw_patch() {
    let repo = TempRepo::new("detail");
    repo.write("a.txt", "one\ntwo\n");
    repo.commit("first");
    repo.write("a.txt", "one\nTWO\nthree\n");
    repo.commit("second");
    let sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let out = repoprism()
        .args(["detail"])
        .arg(repo.path())
        .args(["--sha", &sha, "--json"])
        .output()
        .expect("failed to run repoprism");

    assert!(
        out.status.success(),
        "detail 应成功退出，stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let data = json_of(&out.stdout);
    let data = &data["data"];

    assert_eq!(data["info"]["subject"], "second");
    assert_eq!(data["info"]["sha"].as_str().unwrap().len(), 40);
    assert_eq!(data["truncated"], false);

    let patch = data["patch"].as_str().expect("patch 应为字符串");
    assert!(
        patch.contains("diff --git a/a.txt b/a.txt"),
        "patch 应是原始 diff 正文，实际 {patch:?}"
    );
    assert!(patch.contains("@@"), "patch 应含 hunk 头");
    assert!(
        !patch.contains("commit "),
        "patch 不应含提交信息头（--format= 已清空）"
    );

    let files = data["files"].as_array().expect("files 应为数组");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "a.txt");
    assert_eq!(files[0]["additions"], 2, "新增 TWO 与 three");
    assert_eq!(files[0]["deletions"], 1, "删除 two");
    assert_eq!(files[0]["binary"], false);
}

#[test]
fn detail_json_on_merge_commit_is_empty_not_broken() {
    let repo = TempRepo::new("merge");
    // 两侧改不同文件，保证是无冲突的干净合并
    repo.write("base.txt", "base\n");
    repo.commit("base");
    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.write("feature.txt", "feature\n");
    repo.commit("feature work");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("main.txt", "main\n");
    repo.commit("main work");
    repo.git(&["merge", "--no-ff", "-m", "merge feature", "feature"]);
    let sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let out = repoprism()
        .args(["detail"])
        .arg(repo.path())
        .args(["--sha", &sha, "--json"])
        .output()
        .expect("failed to run repoprism");

    assert!(out.status.success(), "合并提交不应让 detail 失败");

    let data = json_of(&out.stdout);
    let data = &data["data"];
    assert_eq!(data["info"]["parents"].as_array().unwrap().len(), 2);
    assert_eq!(
        data["patch"], "",
        "干净合并没有可直接展示的 diff，patch 应为空字符串"
    );
    assert_eq!(data["files"].as_array().unwrap().len(), 0);
    assert_eq!(data["truncated"], false);

    // 人类可读输出必须给出提示，而不是静默空白
    let human = repoprism()
        .args(["detail"])
        .arg(repo.path())
        .args(["--sha", &sha])
        .output()
        .expect("failed to run repoprism");
    let stdout = String::from_utf8_lossy(&human.stdout);
    assert!(
        stdout.contains("合并提交或空 diff"),
        "空 diff 必须显式提示，stdout={stdout}"
    );
}

#[test]
fn detail_on_unknown_sha_fails_loudly() {
    let repo = TempRepo::new("badsha");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let out = repoprism()
        .args(["detail"])
        .arg(repo.path())
        .args([
            "--sha",
            "0000000000000000000000000000000000000000",
            "--json",
        ])
        .output()
        .expect("failed to run repoprism");

    assert!(!out.status.success(), "不存在的 SHA 必须非零退出");
    assert!(
        !out.stderr.is_empty(),
        "失败时必须给出 stderr，而不是空输出"
    );
}

#[test]
fn skill_print_emits_bundled_markdown() {
    let out = repoprism()
        .args(["skill", "--print"])
        .output()
        .expect("failed to run repoprism");

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("# RepoPrism Skill"), "应打印 Skill 正文");
    assert!(stdout.contains("schema_version"), "应含 JSON 信封说明");
    assert!(stdout.contains("绝不执行任何 Git 写操作"), "应含只读承诺");
}

#[test]
fn skill_path_extracts_once_and_is_idempotent() {
    let cache = std::env::temp_dir().join(format!("repoprism-skill-cache-{}", std::process::id()));
    let _ = fs::remove_dir_all(&cache);

    let first = repoprism()
        .args(["skill", "--path"])
        .env("REPOPRISM_CACHE_DIR", &cache)
        .output()
        .expect("failed to run repoprism");
    assert!(first.status.success());

    let printed = String::from_utf8_lossy(&first.stdout).trim().to_string();
    assert_eq!(
        printed,
        cache.to_string_lossy(),
        "skill --path 应打印展开目录"
    );

    let skill_md = cache.join("SKILL.md");
    let content = fs::read_to_string(&skill_md).expect("SKILL.md 应被展开到缓存目录");
    assert!(content.starts_with("# RepoPrism Skill"));
    assert!(
        content.contains("## JSON Schema"),
        "展开的必须是完整 Skill 而非截断片段"
    );

    // 二次调用不应失败（幂等）
    let second = repoprism()
        .args(["skill", "--path"])
        .env("REPOPRISM_CACHE_DIR", &cache)
        .output()
        .expect("failed to run repoprism");
    assert!(second.status.success());
    assert_eq!(
        fs::read_to_string(&skill_md).unwrap(),
        content,
        "重复调用不应改变内容"
    );

    let _ = fs::remove_dir_all(&cache);
}

#[test]
fn skill_without_flags_exits_with_usage() {
    let out = repoprism()
        .args(["skill"])
        .output()
        .expect("failed to run repoprism");

    assert_eq!(out.status.code(), Some(2), "缺参数应返回用法错误码 2");
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage:"));
}

#[test]
fn version_flag_reports_tool_version() {
    let out = repoprism()
        .arg("--version")
        .output()
        .expect("failed to run");
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("repoprism"), "stdout={stdout}");
    assert!(
        stdout.chars().any(|c| c.is_ascii_digit()),
        "版本号应含数字，stdout={stdout}"
    );
}
