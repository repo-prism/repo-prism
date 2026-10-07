//! RepoPrism core — Git 只读读取层。
//!
//! **唯一允许调用 Git 的 crate。**
//! 所有 Git 调用必须通过 `git::Git`，且只允许白名单内的只读命令。

pub mod git;
pub mod model;

pub use git::Git;
pub use model::*;