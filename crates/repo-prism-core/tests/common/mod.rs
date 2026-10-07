//! 测试夹具：在临时目录中构造只读的 Git 仓库。
//!
//! # 安全说明
//!
//! 本文件位于 `crates/repo-prism-core/tests/`，**不属于产品代码**。
//! 这里的 `git init` / `git commit` 等操作只作用于 `std::env::temp_dir()`
//! 下临时创建的仓库，用于构造测试输入，绝不触碰任何真实仓库。
//!
//! 产品代码侧的约束以 `AGENTS.md` 为准：`src/` 下只允许白名单内的只读命令，
//! 且 CI 的 read-only guard 静态扫描只覆盖 `crates/repo-prism-core/src/`。

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 临时仓库。离开作用域时自动删除。
pub struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    pub fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX epoch")
            .as_nanos();
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("repoprism-{tag}-{nanos}-{seq}"));
        fs::create_dir_all(&path).expect("failed to create temp dir");

        let repo = Self { path };
        repo.git(&["init"]);
        repo.git(&["config", "user.name", "Test User"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.git(&["config", "tag.gpgsign", "false"]);
        repo
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// 写入文件（相对仓库根目录）。
    pub fn write(&self, rel: &str, content: &str) {
        let full = self.path.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dir");
        }
        fs::write(full, content).expect("failed to write file");
    }

    /// 暂存所有改动并提交。
    ///
    /// 需要确定的 log 顺序时请改用 [`Self::commit_at`]。
    pub fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", message]);
    }

    /// 以固定的作者/提交者时间提交，使 `git log` 的输出顺序稳定。
    ///
    /// 连续创建的提交若落在同一秒，git 的排序在同秒提交间并不稳定，
    /// 会让依赖顺序的断言变成随机失败的 flaky 测试。
    pub fn commit_at(&self, message: &str, iso_date: &str) {
        self.git(&["add", "-A"]);
        self.git_with_env(
            &["commit", "-m", message],
            &[
                ("GIT_AUTHOR_DATE", iso_date),
                ("GIT_COMMITTER_DATE", iso_date),
            ],
        );
    }

    /// 执行 git 命令，附加额外环境变量，失败即 panic。
    pub fn git_with_env(&self, args: &[&str], envs: &[(&str, &str)]) -> String {
        let mut cmd = Command::new("git");
        // 隔离真实用户配置，避免污染测试结果
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        for (key, value) in envs {
            cmd.env(key, value);
        }
        let out = cmd
            .args(["-c", "init.defaultBranch=main", "-C"])
            .arg(&self.path)
            .args(args)
            .output()
            .expect("failed to run git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// 执行 git 命令，失败即 panic。
    pub fn git(&self, args: &[&str]) -> String {
        let (status, stdout, stderr) = self.git_raw(args);
        assert!(
            status,
            "git {:?} unexpectedly failed\nstdout: {}\nstderr: {}",
            args, stdout, stderr
        );
        stdout
    }

    /// 执行 git 命令并允许失败（如制造合并冲突）。
    pub fn git_raw(&self, args: &[&str]) -> (bool, String, String) {
        let out = Command::new("git")
            // 隔离真实用户配置，避免污染测试结果
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .args(["-c", "init.defaultBranch=main", "-C"])
            .arg(&self.path)
            .args(args)
            .output()
            .expect("failed to run git");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }

    pub fn head_sha(&self) -> String {
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        // 只读回收临时目录；失败不影响测试结果
        let _ = fs::remove_dir_all(&self.path);
    }
}
