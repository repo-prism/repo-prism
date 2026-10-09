use repo_prism_core::{
    build_commit_prompt, ChangeAnalysis, CommitDetail, CommitInfo, Diff, Git, LineStats,
    OllamaConfig, OllamaSummarizer, RemoteInfo, RepoSnapshot, Summarizer, DEFAULT_ENDPOINT,
    DEFAULT_MODEL, DEFAULT_TIMEOUT_SECS,
};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

// ---------------------------------------------------------------------------
// 只读仓库命令
// ---------------------------------------------------------------------------

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
/// 取不到行数时**退化为纯路径规则**而不是让整个分析失败 —— 少一条规则命中
/// 是可接受的降级，整个面板报错不可接受。
#[tauri::command]
fn analyze_changes(path: String) -> Result<ChangeAnalysis, String> {
    let git = Git::open(&path).map_err(|e| e.to_string())?;
    analyze(&git)
}

/// `origin` remote 的结构化信息（US-8）。没有 remote 时是 `null`，不是错误。
#[tauri::command]
fn get_remote_info(path: String) -> Result<Option<RemoteInfo>, String> {
    Git::open(&path)
        .and_then(|g| g.remote_info())
        .map_err(|e| e.to_string())
}

fn analyze(git: &Git) -> Result<ChangeAnalysis, String> {
    let snapshot = git.snapshot().map_err(|e| e.to_string())?;
    let stats = git
        .working_tree_stats()
        .unwrap_or_else(|_| LineStats::new());
    Ok(ChangeAnalysis::from_status_and_stats(
        &snapshot.status,
        &stats,
    ))
}

// ---------------------------------------------------------------------------
// AI 设置（TASK-013）
// ---------------------------------------------------------------------------

/// 前端「AI 设置」面板读写的数据。
///
/// `#[serde(default)]` 让旧版本写下的、缺字段的配置文件仍能读出来。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AiSettings {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_endpoint")]
    endpoint: String,
    #[serde(default = "default_model")]
    model: String,
}

fn default_endpoint() -> String {
    DEFAULT_ENDPOINT.to_string()
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_endpoint(),
            model: default_model(),
        }
    }
}

struct AppState {
    ai: Mutex<AiSettings>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            ai: Mutex::new(load_settings()),
        }
    }
}

/// 设置文件位置：`<系统配置目录>/RepoPrism/ai-settings.json`。
///
/// **这不是对仓库的写操作**：只读宪法约束的是被观察的仓库，应用自己的配置
/// 落在用户配置目录，与被打开的仓库无关。
fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("RepoPrism").join("ai-settings.json"))
}

/// 读设置。任何失败都回默认值（默认关闭）—— 设置文件坏了不该让应用起不来。
fn load_settings() -> AiSettings {
    let Some(path) = settings_path() else {
        return AiSettings::default();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return AiSettings::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_settings(settings: &AiSettings) -> Result<(), String> {
    let path = settings_path()
        .ok_or_else(|| "this system has no writable config directory".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// 由已存设置构造摘要器。构造即校验，因此这里的 `Err` 就是「endpoint 不合法」。
fn summarizer_for(settings: &AiSettings) -> Result<OllamaSummarizer, String> {
    OllamaSummarizer::new(OllamaConfig {
        endpoint: settings.endpoint.clone(),
        model: settings.model.clone(),
        timeout_secs: DEFAULT_TIMEOUT_SECS,
    })
}

fn current_settings(state: &State<'_, AppState>) -> Result<AiSettings, String> {
    state
        .ai
        .lock()
        .map(|guard| guard.clone())
        .map_err(|_| "AI settings are unavailable: the lock was poisoned".to_string())
}

#[tauri::command]
fn get_ai_settings(state: State<'_, AppState>) -> Result<AiSettings, String> {
    current_settings(&state)
}

/// 保存设置。**先校验再落盘再入内存** —— 允许存下一个非法 endpoint，
/// 等于允许用户绕开唯一那道出网闸门。
#[tauri::command]
fn set_ai_settings(settings: AiSettings, state: State<'_, AppState>) -> Result<(), String> {
    summarizer_for(&settings)?;
    save_settings(&settings)?;
    let mut guard = state
        .ai
        .lock()
        .map_err(|_| "AI settings are unavailable: the lock was poisoned".to_string())?;
    *guard = settings;
    Ok(())
}

/// 探活本地模型服务，顺带返回已安装的模型名供输入框候选。
///
/// **只测不存**：入参是面板里当前填的值。否则「测试连接」会悄悄把配置写盘，
/// 用户点了取消也回不去。
#[tauri::command]
fn test_ai_connection(settings: AiSettings) -> Result<Vec<String>, String> {
    summarizer_for(&settings)?.list_models()
}

/// 用本地模型给未提交改动写摘要。未启用时返回 `None`，前端据此不渲染摘要条。
#[tauri::command]
fn summarize_changes(path: String, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let settings = current_settings(&state)?;
    if !settings.enabled {
        return Ok(None);
    }
    let git = Git::open(&path).map_err(|e| e.to_string())?;
    let analysis = analyze(&git)?;
    if analysis.total_files == 0 {
        return Ok(Some("工作区干净，没有可总结的改动。".to_string()));
    }
    Ok(summarizer_for(&settings)?.summarize(&analysis))
}

/// 用本地模型分析单个提交。
#[tauri::command]
fn summarize_commit(
    path: String,
    sha: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let settings = current_settings(&state)?;
    if !settings.enabled {
        return Ok(None);
    }
    let detail = Git::open(&path)
        .and_then(|g| g.commit_detail(&sha))
        .map_err(|e| e.to_string())?;
    let files: Vec<String> = detail.files.iter().map(|f| f.path.clone()).collect();
    let prompt = build_commit_prompt(&detail.info.subject, detail.info.body.as_deref(), &files);
    Ok(summarizer_for(&settings)?.generate(&prompt))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            inspect_repo,
            get_commits,
            get_commit_detail,
            get_commit_diff,
            get_diff,
            analyze_changes,
            get_remote_info,
            get_ai_settings,
            set_ai_settings,
            test_ai_connection,
            summarize_changes,
            summarize_commit
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
