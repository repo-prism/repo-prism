//! `origin` remote 解析集成测试（对应 TASK-011 验收标准）。
//!
//! 全程离线：`git remote add` 只写本仓库配置，不访问网络；
//! URL 用的是 GitHub / GitLab 形式，但从不 fetch。

mod common;

use common::TempRepo;
use repo_prism_core::{Git, RemoteInfo};

fn repo_with_remote(tag: &str, name: &str, url: &str) -> TempRepo {
    let repo = TempRepo::new(tag);
    repo.write("a.txt", "1\n");
    repo.commit("base");
    repo.git(&["remote", "add", name, url]);
    repo
}

fn remote_of(repo: &TempRepo) -> Option<RemoteInfo> {
    Git::open(repo.path())
        .expect("open repo")
        .remote_info()
        .expect("remote info")
}

#[test]
fn returns_none_when_origin_is_absent() {
    let repo = TempRepo::new("remote-none");
    repo.write("a.txt", "1\n");
    repo.commit("base");

    // 「没有远端」不是错误，是 None —— 界面据此禁用跳转按钮
    assert!(remote_of(&repo).is_none());
}

#[test]
fn only_origin_is_considered() {
    // 别的名字的 remote 不算数：跳转按钮必须指向用户真正在用的那一个
    let repo = repo_with_remote("remote-other", "upstream", "git@github.com:o/r.git");
    assert!(remote_of(&repo).is_none());
}

#[test]
fn parses_ssh_shorthand_origin() {
    let repo = repo_with_remote(
        "remote-ssh",
        "origin",
        "git@github.com:repo-prism/repo-prism.git",
    );
    let info = remote_of(&repo).expect("应解析出 origin");

    assert_eq!(info.host, "github.com");
    assert_eq!(info.owner, "repo-prism");
    assert_eq!(info.repo, "repo-prism");
    assert_eq!(info.url, "git@github.com:repo-prism/repo-prism.git");
}

#[test]
fn parses_https_origin_with_user_and_without_suffix() {
    let repo = repo_with_remote(
        "remote-https",
        "origin",
        "https://someone@gitlab.com/group/repo",
    );
    let info = remote_of(&repo).expect("应解析出 origin");

    assert_eq!(info.host, "gitlab.com");
    assert_eq!(info.owner, "group");
    assert_eq!(info.repo, "repo", "无 .git 后缀也要能解析");
}

#[test]
fn unresolvable_origin_url_yields_none() {
    // 本地路径形式的 remote 无法拼出外部跳转地址，返回 None 而不是编一个
    let repo = TempRepo::new("remote-localpath");
    repo.write("a.txt", "1\n");
    repo.commit("base");
    let bare = TempRepo::new_bare("remote-localpath-upstream");
    let bare_path = bare.path().to_string_lossy().to_string();
    repo.git(&["remote", "add", "origin", &bare_path]);

    assert!(remote_of(&repo).is_none());
}
