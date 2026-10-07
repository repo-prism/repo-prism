use anyhow::Result;
use clap::{Parser, Subcommand};
use repo_prism_core::Git;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "repoprism", version, about = "Read-only repo intelligence")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print a read-only JSON snapshot of a repository
    Inspect {
        /// Path to the repository
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Print the path to the bundled Agent Skill
    Skill {
        #[arg(long)]
        path: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Inspect { path, json } => {
            let git = Git::open(&path)?;
            let snapshot = git.snapshot()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            } else {
                println!("repo: {}", snapshot.path.display());
                println!("head: {:?}", snapshot.head);
                println!("branches: {}", snapshot.branches.len());
                println!("tags: {}", snapshot.tags.len());
            }
        }
        Commands::Skill { path: _ } => {
            println!("Skill path not yet implemented");
        }
    }
    Ok(())
}
