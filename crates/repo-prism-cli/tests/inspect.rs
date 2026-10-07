//! `repoprism` CLI 集成测试（对应 TASK-003 验收标准）。
//!
//! - `inspect <path> --json` 输出合法 JSON
//! - 输出的 schema 与 SPEC.md 数据模型一致
//! - `--help` 显示用法
//! - 非仓库路径应报错退出

mod common;

use common::TempRepo;
use std::process::Command;

fn repoprism() -> Command {
    Command::new(env!("CARGO_BIN_EXE_repoprism"))
}

#[test]
fn inspect_json_emits_valid_snapshot() {
    let repo = TempRepo::new("json");
    repo.write("README.md", "# demo\n");
    repo.commit("first commit");
    repo.git(&["tag", "v0.1.0"]);

    let out = repoprism()
        .arg("inspect")
        .arg(repo.path())
        .arg("--json")
        .output()
        .expect("failed to run repoprism");

    assert!(
        out.status.success(),
        "inspect --json 应成功退出，stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("输出必须是合法 JSON");

    // schema 与 SPEC.md / model.rs 对齐
    assert_eq!(value["head"]["branch"], "main");
    assert_eq!(value["head"]["detached"], false);
    assert!(
        value["head"]["commit"].as_str().unwrap().len() == 40,
        "head.commit 应为完整 SHA"
    );
    assert_eq!(value["head"]["upstream"], serde_json::Value::Null);

    let branches = value["branches"].as_array().expect("branches 应为数组");
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0]["name"], "main");
    assert_eq!(branches[0]["is_current"], true);

    let tags = value["tags"].as_array().expect("tags 应为数组");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0]["name"], "v0.1.0");

    // status 三个分组必须存在
    assert!(value["status"]["conflicts"].is_array());
    assert!(value["status"]["staged"].is_array());
    assert!(value["status"]["unstaged"].is_array());

    // path 字段回填仓库根目录
    let path = value["path"].as_str().expect("path 应为字符串");
    assert!(path.contains("repoprism-cli-json"), "path={path}");
}

#[test]
fn inspect_json_reflects_working_tree_changes() {
    let repo = TempRepo::new("dirty");
    repo.write("tracked.txt", "base\n");
    repo.commit("base");
    repo.write("tracked.txt", "changed\n");
    repo.write("new.txt", "new\n");

    let out = repoprism()
        .arg("inspect")
        .arg(repo.path())
        .arg("--json")
        .output()
        .expect("failed to run repoprism");
    assert!(out.status.success());

    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("合法 JSON");
    let unstaged = value["status"]["unstaged"].as_array().expect("unstaged");
    let paths: Vec<&str> = unstaged
        .iter()
        .map(|f| f["path"].as_str().unwrap_or(""))
        .collect();

    assert!(paths.contains(&"tracked.txt"), "unstaged={paths:?}");
    assert!(paths.contains(&"new.txt"), "unstaged={paths:?}");

    let kinds: Vec<&str> = unstaged
        .iter()
        .map(|f| f["kind"].as_str().unwrap_or(""))
        .collect();
    assert!(kinds.contains(&"modified"), "kinds={kinds:?}");
    assert!(kinds.contains(&"added"), "kinds={kinds:?}");
}

#[test]
fn inspect_without_json_prints_human_summary() {
    let repo = TempRepo::new("plain");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let out = repoprism()
        .arg("inspect")
        .arg(repo.path())
        .output()
        .expect("failed to run repoprism");
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("repo:"), "stdout={stdout}");
    assert!(stdout.contains("branches: 1"), "stdout={stdout}");
    assert!(stdout.contains("tags: 0"), "stdout={stdout}");
}

#[test]
fn inspect_on_non_repository_fails() {
    let dir = std::env::temp_dir();
    let out = repoprism()
        .arg("inspect")
        .arg(&dir)
        .arg("--json")
        .output()
        .expect("failed to run repoprism");

    // 若 temp_dir 恰好位于某个仓库内则跳过
    if out.status.success() {
        return;
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.is_empty(), "失败时应输出错误信息");
}

#[test]
fn help_lists_usage() {
    let out = repoprism().arg("--help").output().expect("failed to run");
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("inspect"), "help 应列出 inspect 子命令");
    assert!(stdout.contains("repoprism"), "help 应显示程序名");
}

#[test]
fn inspect_help_shows_json_flag() {
    let out = repoprism()
        .args(["inspect", "--help"])
        .output()
        .expect("failed to run");
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--json"), "inspect --help 应显示 --json");
}
