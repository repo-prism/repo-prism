use repo_prism_core::{
    ChangeAnalysis, CommitDetail, CommitInfo, Diff, Git, RemoteInfo, RepoSnapshot,
};

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

/// 未提交改动的本地风险分析（US-7 的本地部分）。
///
/// 行数统计单独取一次：`snapshot()` 不带它，而「大量删除」这条规则需要。
/// 取不到行数时该规则不命中，不会误报。
#[tauri::command]
fn analyze_changes(path: String) -> Result<ChangeAnalysis, String> {
    Git::open(&path)
        .and_then(|g| {
            let snapshot = g.snapshot()?;
            let stats = g.working_tree_stats()?;
            Ok(ChangeAnalysis::from_status_and_stats(
                &snapshot.status,
                &stats,
            ))
        })
        .map_err(|e| e.to_string())
}

/// `origin` remote 的结构化信息（US-8）。没有 remote 时是 `null`，不是错误。
#[tauri::command]
fn get_remote_info(path: String) -> Result<Option<RemoteInfo>, String> {
    Git::open(&path)
        .and_then(|g| g.remote_info())
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
            get_diff,
            analyze_changes,
            get_remote_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
