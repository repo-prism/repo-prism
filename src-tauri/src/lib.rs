use repo_prism_core::{CommitInfo, Git, RepoSnapshot};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![inspect_repo, get_commits])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}