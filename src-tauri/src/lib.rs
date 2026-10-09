use repo_prism_core::{CommitDetail, CommitInfo, Diff, Git, RepoSnapshot};

#[tauri::command]
fn inspect_repo(path: String) -> Result<RepoSnapshot, String> {
    Git::open(&path)
        .and_then(|g| g.snapshot())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_commits(
    path: String,
    limit: Option<usize>,
    skip: Option<usize>,
) -> Result<Vec<CommitInfo>, String> {
    let limit = limit.unwrap_or(200).min(2000);
    let skip = skip.unwrap_or(0);
    Git::open(&path)
        .and_then(|g| g.commits(limit, skip))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_commit_detail(path: String, sha: String) -> Result<CommitDetail, String> {
    Git::open(&path)
        .and_then(|g| g.commit_detail(&sha))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_commit_diff(path: String, sha: String) -> Result<Diff, String> {
    Git::open(&path)
        .and_then(|g| g.commit_diff(&sha))
        .map_err(|e| e.to_string())
}

/// 任意两点之间的 Diff。`from` / `to` 皆为空时为「工作区 vs 索引」。
#[tauri::command]
fn get_diff(path: String, from: Option<String>, to: Option<String>) -> Result<Diff, String> {
    Git::open(&path)
        .and_then(|g| g.diff(from.as_deref(), to.as_deref()))
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            inspect_repo,
            get_commits,
            get_commit_detail,
            get_commit_diff,
            get_diff
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
