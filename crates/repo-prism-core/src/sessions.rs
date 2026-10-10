//! 多仓库会话表（US-9 / TASK-019）。
//!
//! # 为什么需要它
//!
//! [`crate::Git`] 的构造本身要起一次 `git rev-parse`（解析仓库根与两个 git 目录），
//! 而 TASK-018 的引用缓存是**挂在实例上**的。没有表的时候，「打开第二个仓库」
//! 就意味着把第一个仓库的实例整个丢掉 —— 切回去要重新付一遍这两样成本。
//!
//! # 键是什么，为什么
//!
//! 键是 **`rev-parse --show-toplevel` 解析出的仓库根**，不是用户输入的字符串。
//! 2026-10-10 实测：同一个仓库的四种写法（根 / 子目录 / 符号链接 / 符号链接下的
//! 子目录）都会被解析成同一个绝对路径。拿解析结果当键才稳定；
//! 用户输入的字符串只作为**别名**记下来，用于省掉下次的那次 `rev-parse`。
//!
//! # 有上限，且淘汰是安全的
//!
//! 长驻进程（桌面端 / MCP）里，无上限的表会随「访问过的仓库数」无限增长内存。
//! 因此表有上限，超出时淘汰**最久未用**的那个。
//!
//! 淘汰只丢缓存，不丢正确性：被淘汰的仓库再打开只是重新读一遍引用，
//! 结果与「从未缓存过」完全一致。退化的方向永远是**多起一个子进程**，
//! 不是「用一份可能过期的数据」—— 与 TASK-018 的取向一致。
//!
//! # 并发形状
//!
//! 表只在「查 / 登记 / 淘汰」这一小段里持锁，随后就把 [`std::sync::Arc`] 交出去，
//! 调用方在**表锁之外**执行命令。于是：
//!
//! - 不同仓库的两条命令不互相阻塞；
//! - 同一个仓库的两条命令也不阻塞（引用缓存自己有锁）。
//!
//! 这比 TASK-018 那把锁松，但没有变差的地方：引用缓存仍然是每个仓库一份。

use crate::Git;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 默认同时保留的仓库会话数。
///
/// 取 8 的理由：每个会话最坏持有一份引用映射（TASK-018 给它的预算是 4 MiB），
/// 8 × 4 MiB 仍在非功能需求「内存 < 300MB」之内，且远大于「同时在看的仓库数」
/// 这类真实使用量。超过这个数才会开始淘汰，而淘汰本身是安全的（见模块文档）。
pub const DEFAULT_CAP: usize = 8;

struct Entry {
    git: Arc<Git>,
    /// 最近一次被取用的时刻（表内单调时钟，不是墙上时间）。
    used: u64,
}

struct Inner {
    /// 键是**已解析的仓库根**。
    by_root: HashMap<PathBuf, Entry>,
    /// `用户输入的路径 → 仓库根`。命中即可省掉一次 `rev-parse`。
    alias: HashMap<PathBuf, PathBuf>,
    clock: u64,
}

/// 一组可同时打开的仓库会话。
///
/// 所有方法都是 `&self`：内部用 `Mutex`，因此可以放在 Tauri 的托管状态里，
/// 也可以放在 MCP 长驻进程的一个全局里。
pub struct RepoSet {
    inner: Mutex<Inner>,
    cap: usize,
}

impl RepoSet {
    /// 建一张最多容纳 `cap` 个会话的表。`cap` 至少为 1 —— 0 会让「打开仓库」
    /// 变成「打开后立刻被淘汰」，那不是一个有意义的配置。
    pub fn new(cap: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                by_root: HashMap::new(),
                alias: HashMap::new(),
                clock: 0,
            }),
            cap: cap.max(1),
        }
    }

    /// 用 [`DEFAULT_CAP`] 建表。
    pub fn with_default_cap() -> Self {
        Self::new(DEFAULT_CAP)
    }

    /// 本表最多容纳几个会话。
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// 当前持有几个会话。
    pub fn len(&self) -> usize {
        self.lock().by_root.len()
    }

    /// 表里没有会话。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 取 `path` 所在仓库的会话；必要时打开它。
    ///
    /// 三种命中路径，代价递减：
    ///
    /// 1. `path` 是记过的别名 → 直接取用，**零子进程**
    /// 2. `path` 是新写法，但解析出的根已在表里 → 一次 `rev-parse`，之后**复用**既有会话
    ///    （只补一条别名；否则同一个仓库会有两个实例、两份缓存）
    /// 3. 全新仓库 → 一次 `rev-parse` 并登记，必要时淘汰最久未用的那个
    ///
    /// `path` 不是仓库时返回 `Err`，且**不占槽位**。
    pub fn open(&self, path: impl AsRef<Path>) -> anyhow::Result<Arc<Git>> {
        let requested: PathBuf = path.as_ref().to_path_buf();
        let mut guard = self.lock();
        // 显式借出 `&mut Inner`：经过 MutexGuard 的 DerefMut 时，
        // 不同字段的借用**不再被判定为互不相交**（整块 `*inner` 都被借走）。
        let inner = &mut *guard;

        if let Some(root) = inner.alias.get(&requested).cloned() {
            return Ok(inner.touch(&root));
        }

        // 尚未见过的写法：必须先解析，才知道它到底指向哪个仓库。
        let git = Arc::new(Git::open_cached(&requested)?);
        let root: PathBuf = git.path().to_path_buf();
        inner.alias.insert(requested, root.clone());

        if let Some(entry) = inner.by_root.get_mut(&root) {
            // 同一个仓库的新写法：保留既有会话（连同它的缓存），只补别名。
            inner.clock += 1;
            entry.used = inner.clock;
            return Ok(Arc::clone(&entry.git));
        }

        inner.clock += 1;
        let entry = Entry {
            used: inner.clock,
            git: Arc::clone(&git),
        };
        inner.by_root.insert(root, entry);
        inner.evict_if_needed(self.cap);
        Ok(git)
    }

    /// 关掉 `path` 所在仓库的会话，连它的**所有别名**一起清。
    ///
    /// 「关闭」必须是真的放掉，不能只是从界面上隐藏：否则关掉之后用原写法打开，
    /// 会拿回刚关掉的那个会话（连同它那份还没失效的缓存）。
    ///
    /// 返回是否确实关掉了一个会话。
    pub fn close(&self, path: impl AsRef<Path>) -> bool {
        let requested = path.as_ref().to_path_buf();
        let mut guard = self.lock();
        let inner = &mut *guard;

        let root = match inner.alias.get(&requested).cloned() {
            Some(root) => root,
            // 没记过这个写法，就用解析结果再试一次；解析不出来就不是仓库，无从关闭。
            None => match Git::open_cached(&requested).ok() {
                Some(git) => git.path().to_path_buf(),
                None => return false,
            },
        };

        let gone = inner.by_root.remove(&root).is_some();
        // 别名可能不止一条（同一个仓库可以被多种写法打开过），全部清掉。
        inner.alias.retain(|_, target| target != &root);
        gone
    }

    /// 当前打开的仓库根，**已排序**。
    ///
    /// 必须排序：`HashMap` 的迭代顺序不确定，直接交给界面会让仓库切换条
    /// 在每次渲染时都可能换位置。
    pub fn roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = self.lock().by_root.keys().cloned().collect();
        roots.sort();
        roots
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Inner {
    fn touch(&mut self, root: &Path) -> Arc<Git> {
        self.clock += 1;
        let entry = self
            .by_root
            .get_mut(root)
            .expect("alias points at a root that is present");
        entry.used = self.clock;
        Arc::clone(&entry.git)
    }

    /// 超出 `cap` 时淘汰最久未用的那个，直到回到上限之内。
    fn evict_if_needed(&mut self, cap: usize) {
        while self.by_root.len() > cap {
            let oldest = self
                .by_root
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(root, _)| root.clone());
            let Some(root) = oldest else { break };
            self.by_root.remove(&root);
            self.alias.retain(|_, target| target != &root);
        }
    }
}

impl Default for RepoSet {
    fn default() -> Self {
        Self::with_default_cap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_is_at_least_one() {
        // 0 会让「打开」变成「打开后立刻被淘汰」，不是一个有意义的配置。
        assert_eq!(RepoSet::new(0).cap(), 1);
        assert_eq!(RepoSet::new(3).cap(), 3);
    }

    #[test]
    fn a_fresh_set_is_empty() {
        let set = RepoSet::with_default_cap();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert!(set.roots().is_empty());
    }

    #[test]
    fn the_default_cap_is_the_documented_one() {
        assert_eq!(RepoSet::default().cap(), DEFAULT_CAP);
    }
}
