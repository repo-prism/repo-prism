//! `repoprism` —— 只读仓库智能工具的 CLI 出口。
//!
//! 四条数据命令（`inspect` / `commits` / `detail` / `blob`）共用**同一个 JSON 信封**，
//! 让 Agent 侧只需实现一次解析，也让 schema 演进有落脚点。
//!
//! `--json` 之外的输出是给人看的摘要：Agent 一律用 `--json`。
//!
//! `blob` 是唯一会读取文件内容的命令，因此它**显式保留**了 P-10 的「先量后读」：
//! `--size-only` 只报大小、一个字节都不读。

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use repo_prism_core::{BlobKind, BlobPreview, Git, MAX_PREVIEW_BYTES};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
const SCHEMA_VERSION: &str = "1";

/// Skill 内容在编译期内联进二进制，随版本一起分发。
const SKILL_MD: &str = include_str!("../../../skill/SKILL.md");

#[derive(Parser)]
#[command(
    name = "repoprism",
    version,
    about = "RepoPrism — read-only repo intelligence for humans and agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print a read-only snapshot of a repository
    Inspect {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Print commit history
    Commits {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        skip: usize,
        #[arg(long)]
        json: bool,
    },
    /// Print a single commit's detail, including the raw diff
    Detail {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        sha: String,
        #[arg(long)]
        json: bool,
    },
    /// Print a file's content preview at a given revision
    Blob {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Revision to read the file from
        #[arg(long, default_value = "HEAD")]
        rev: String,
        /// Path of the file **inside** the repository
        #[arg(long)]
        file: String,
        /// Report the size only and read no content (exactly one subprocess).
        ///
        /// 这是 P-10「先量后读」里「量」的那一半单独暴露成命令：
        /// 调用方可以先花一次便宜的调用判断值不值得读。
        #[arg(long)]
        size_only: bool,
        #[arg(long)]
        json: bool,
    },
    /// Manage the bundled Agent Skill
    Skill {
        /// Print the path to the extracted skill directory
        #[arg(long)]
        path: bool,
        /// Print the skill content to stdout
        #[arg(long)]
        print: bool,
    },
}

/// 所有 `--json` 输出的外层信封。`schema_version` 变化时 Agent 需重新适配。
#[derive(Serialize)]
struct Envelope<T: Serialize> {
    schema_version: &'static str,
    tool: &'static str,
    tool_version: &'static str,
    data: T,
}

/// `blob --size-only` 的输出。带上限是为了让调用方**在读取之前**就能判断值不值得。
#[derive(Serialize)]
struct BlobSize<'a> {
    rev: &'a str,
    path: &'a str,
    size: u64,
    preview_cap_bytes: u64,
}

fn envelope<T: Serialize>(data: T) -> Envelope<T> {
    Envelope {
        schema_version: SCHEMA_VERSION,
        tool: "repoprism",
        tool_version: TOOL_VERSION,
        data,
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Inspect { path, json } => {
            let snapshot = Git::open(&path)?.snapshot()?;
            print_result(&snapshot, json, |s| {
                println!("repo: {}", s.path.display());
                println!("head: {}", s.head.branch.as_deref().unwrap_or("(detached)"));
                println!("commit: {}", short(&s.head.commit));
                match &s.head.upstream {
                    // 「没有上游」与「有上游且已同步」是两件事，不能都印成 ↑0 ↓0
                    None => println!("upstream: (none)"),
                    Some(u) => println!("upstream: {} ↑{} ↓{}", u.name, u.ahead, u.behind),
                }
                println!("branches: {}", s.branches.len());
                println!("tags: {}", s.tags.len());
                println!(
                    "changes: staged={} unstaged={} conflicts={}",
                    s.status.staged.len(),
                    s.status.unstaged.len(),
                    s.status.conflicts.len(),
                );
            })
        }
        Commands::Commits {
            path,
            limit,
            skip,
            json,
        } => {
            let commits = Git::open(&path)?.commits(limit.min(2000), skip)?;
            print_result(&commits, json, |cs| {
                for c in cs {
                    println!("{} {} ({})", c.short_sha, c.subject, c.author_name);
                }
            })
        }
        Commands::Detail { path, sha, json } => {
            let detail = Git::open(&path)?.commit_detail(&sha)?;
            print_result(&detail, json, |d| {
                println!("commit: {}", d.info.sha);
                println!("author: {} <{}>", d.info.author_name, d.info.author_email);
                println!("date: {}", d.info.author_date);
                println!("subject: {}", d.info.subject);
                if let Some(body) = &d.info.body {
                    println!("\n{body}");
                }
                if d.files.is_empty() {
                    println!("\n(合并提交或空 diff，无直接变更内容)");
                    return;
                }
                println!("\nfiles:");
                for f in &d.files {
                    let binary = if f.binary { " [二进制]" } else { "" };
                    println!(
                        "  +{:<5} -{:<5} {}{}",
                        f.additions, f.deletions, f.path, binary
                    );
                }
                if d.truncated {
                    println!("\n(diff 超出上限已截断，内容不完整)");
                }
            })
        }
        Commands::Blob {
            path,
            rev,
            file,
            size_only,
            json,
        } => {
            let git = Git::open(&path)?;
            if size_only {
                let size = git.blob_size(&rev, &file)?.with_context(|| {
                    format!("{rev}:{file} 在这个仓库里不存在（版本或路径不对）")
                })?;
                print_result(
                    &BlobSize {
                        rev: &rev,
                        path: &file,
                        size,
                        preview_cap_bytes: MAX_PREVIEW_BYTES,
                    },
                    json,
                    |s| {
                        println!("blob: {} @ {}", s.path, s.rev);
                        println!("size: {} 字节", s.size);
                    },
                )
            } else {
                let preview = git.blob_preview(&rev, &file)?;
                print_result(&preview, json, |p| print_blob_human(&rev, &file, p))
            }
        }
        Commands::Skill { path, print } => {
            if print {
                print!("{SKILL_MD}");
            } else if path {
                println!("{}", extract_skill()?.display());
            } else {
                eprintln!("usage: repoprism skill --path | --print");
                std::process::exit(2);
            }
            Ok(())
        }
    }
}

fn print_result<T, F>(data: &T, json: bool, human: F) -> Result<()>
where
    T: Serialize,
    F: FnOnce(&T),
{
    if json {
        println!("{}", serde_json::to_string_pretty(&envelope(data))?);
    } else {
        human(data);
    }
    Ok(())
}

/// `blob` 的人类可读输出。
///
/// 刻意**不**把图片 base64 打到 stdout —— 那是给程序用的，人看一屏乱码没有意义，
/// 而且会把终端塞满。这里只说清楚「它是什么、多大、怎么拿内容」。
fn print_blob_human(rev: &str, file: &str, p: &BlobPreview) {
    println!("blob: {file} @ {rev}");
    println!("size: {} 字节", p.size);
    match &p.kind {
        BlobKind::Image { format } => {
            println!("kind: image ({})", format.media_type());
            println!("（图片内容以 base64 提供，加 --json 取用）");
        }
        BlobKind::Text => {
            println!("kind: text");
            if p.truncated {
                println!("（文本预览已截断，加 --json 可看到截断标记）");
            }
            if let Some(text) = &p.text {
                println!("\n{text}");
            }
        }
        BlobKind::Binary => {
            println!("kind: binary");
            if p.truncated {
                println!("（十六进制转储只显示前一段）");
            }
            if let Some(hex) = &p.hex {
                println!("\n{hex}");
            }
        }
        BlobKind::LfsPointer { oid, size } => {
            println!("kind: lfs_pointer");
            println!("oid: {oid}");
            println!("真实大小: {size} 字节");
            println!("（这是 LFS 指针本身，真实内容不在仓库里，也未下载）");
        }
        BlobKind::TooLarge => {
            println!("kind: too_large");
            println!(
                "（超过 {} 字节读取上限，一个字节都没读；size 是它在仓库里的真实大小）",
                MAX_PREVIEW_BYTES
            );
        }
    }
}

/// SHA 截断显示。空 SHA（无提交仓库）返回 `(none)`，不做越界切片。
fn short(sha: &str) -> &str {
    if sha.is_empty() {
        return "(none)";
    }
    &sha[..8.min(sha.len())]
}

/// 展开 Skill 的目录。
///
/// 优先读 `REPOPRISM_CACHE_DIR`：测试用它改写目标，避免污染真实缓存目录。
fn skill_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("REPOPRISM_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(dirs::cache_dir()
        .context("cannot determine cache directory")?
        .join("repoprism")
        .join("skill"))
}

/// 把内联的 Skill 落到缓存目录，返回目录路径。
///
/// 内容相同则不重写，避免每次调用都无谓地改动 mtime。
fn extract_skill() -> Result<PathBuf> {
    let dir = skill_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let file = dir.join("SKILL.md");
    let up_to_date = fs::read_to_string(&file).is_ok_and(|old| old == SKILL_MD);
    if !up_to_date {
        fs::write(&file, SKILL_MD)
            .with_context(|| format!("failed to write {}", file.display()))?;
    }
    Ok(dir)
}
