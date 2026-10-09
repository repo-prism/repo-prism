//! 本地 LLM 摘要层（TASK-013）。
//!
//! **安全边界**：只允许 http:// + 回环地址（localhost / 127.0.0.1 / [::1]）。
//! 这是本模块存在的全部理由 —— 用户要的是「看得懂」，不是「把仓库传上去」。
//!
//! 因此送出 prompt 的内容被刻意压到最小：**只送文件的相对路径与规则结果**。
//! 不送仓库绝对路径、不送 remote URL、不送 diff 正文、不送文件内容。
//! 「哪些文件有风险」与「文件里写了什么」是两件事，前者才是摘要需要的。
//!
//! 默认关闭：设置面板里 `enabled` 为 false 时，`Summarizer` 侧根本不会被调用。
//!
//! ## 对归档原稿的三处修正（写进 TASK-013，不埋在代码里）
//!
//! 1. **host 判定必须真解析，不能比前缀**。原稿写
//!    `endpoint.starts_with("http://localhost")`，这会放行
//!    `http://localhost.evil.com`（前缀命中）与 `http://localhost@evil.com`
//!    （真正的 host 是 evil.com）——两道都是把代码送出机器的口子。
//!    这里改为拆出 authority 后精确比对 host，并禁止 URL 内嵌凭据。
//! 2. **关掉 TLS，并同步收窄 scheme 到 http**。原稿允许 `https://localhost`
//!    却把 ureq 的 TLS 依赖带上；回环地址上的 https 既无用（Ollama 是明文
//!    HTTP）又会引入整棵 rustls/ring 依赖树。校验里放行一个实际跑不通的
//!    scheme，比直接拒绝它更糟。
//! 3. **超时参数也纳入校验**。原稿让 `timeout_secs` 原样进 `Duration`，
//!    `0` 会在库里变成「立即超时」这种难以定位的失败。这里限制到 1..=600。
//! 4. **显式声明不使用环境代理**。`ureq::get` / `ureq::post` 这两个顶层便捷函数
//!    构造的是默认 agent，从调用处**看不出作者有没有考虑过代理**；而
//!    `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` 这类环境变量是**会改写连接目标**的。
//!    一旦被遵循，endpoint 上写着的 `127.0.0.1` 就不再是真正要连的地址。
//!    改为显式构造 agent（见 `local_agent`），把这条要求写进代码，
//!    而不是让它依赖某个库版本的默认行为。

use crate::analysis::{ChangeAnalysis, RiskLevel, Summarizer};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 默认 endpoint，与 Ollama 的默认监听地址一致。
pub const DEFAULT_ENDPOINT: &str = "http://localhost:11434";
/// 默认模型。用户可在设置面板里改成 `ollama list` 里的任意模型。
pub const DEFAULT_MODEL: &str = "llama3.2";
/// 生成请求的默认超时（秒）。本地小模型首次加载模型权重可能要几十秒。
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;
/// 生成请求的超时上限。再长就不该让用户在界面上干等了。
pub const MAX_TIMEOUT_SECS: u64 = 600;
/// 健康检查（`/api/tags`）的超时。这里只探活，不该用生成的超时。
const HEALTH_TIMEOUT_SECS: u64 = 5;
/// 单次 prompt 最多列出的文件数，避免超大提交把上下文撑爆。
const MAX_PROMPT_FILES: usize = 40;

/// 本地模型服务的连接配置。前端「AI 设置」面板直接编辑这个结构。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub endpoint: String,
    pub model: String,
    /// 生成请求的超时（秒）。`1..=600`。
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }
}

impl OllamaConfig {
    /// 校验配置是否落在允许的范围内。
    ///
    /// 这是唯一的出网闸门，所以判定从严：见模块文档第 1 条修正。
    pub fn validate(&self) -> Result<(), String> {
        validate_endpoint(&self.endpoint)?;
        if self.model.trim().is_empty() {
            return Err("model name must not be empty".to_string());
        }
        if self.timeout_secs == 0 || self.timeout_secs > MAX_TIMEOUT_SECS {
            return Err(format!(
                "timeout_secs must be within 1..={MAX_TIMEOUT_SECS} (got {})",
                self.timeout_secs
            ));
        }
        Ok(())
    }
}

/// 校验 endpoint 是「http:// + 回环地址 + 合法端口」。
///
/// 刻意不引 URL 解析库：多一棵依赖树就是多一处要审的代码，而这里只需要
/// 一个极窄的判定 —— 这一小段手写解析的可读性比引入 `url` crate 更划算。
fn validate_endpoint(endpoint: &str) -> Result<(), String> {
    let endpoint = endpoint.trim();

    let (scheme, rest) = endpoint
        .split_once("://")
        .ok_or_else(|| format!("endpoint must be an absolute http:// URL (got `{endpoint}`)"))?;
    if !scheme.eq_ignore_ascii_case("http") {
        return Err(format!(
            "only the http scheme is allowed for a local endpoint (got `{scheme}://`)"
        ));
    }

    // 只看 authority：遇到路径 / 查询 / 片段就停。
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err("endpoint has no host".to_string());
    }
    if authority.contains('@') {
        // 带 userinfo 的 URL 一律拒收。`http://localhost@evil.com` 的 host
        // 其实是 evil.com，用户看到的却是 localhost —— 这正是要挡住的那类写法。
        return Err("credentials embedded in the endpoint URL are not allowed".to_string());
    }

    let (raw_host, raw_port) = if let Some(after_bracket) = authority.strip_prefix('[') {
        let (host, tail) = after_bracket
            .split_once(']')
            .ok_or_else(|| "malformed IPv6 host in endpoint".to_string())?;
        // `]` 之后只允许空串或 `:port`。否则 `[::1]evil.com` 会被读成合法 host。
        if !tail.is_empty() && !tail.starts_with(':') {
            return Err("malformed IPv6 host in endpoint".to_string());
        }
        (host, tail.strip_prefix(':'))
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };

    if let Some(port) = raw_port {
        let invalid = port.is_empty()
            || !port.bytes().all(|b| b.is_ascii_digit())
            || port.parse::<u16>().map_or(true, |parsed| parsed == 0);
        if invalid {
            return Err(format!(
                "endpoint port must be a number within 1..=65535 (got `{port}`)"
            ));
        }
    }

    // host 比对不区分大小写；FQDN 的根点（`localhost.`）与不带点等价。
    let host = raw_host.trim_end_matches('.').to_ascii_lowercase();
    if !matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1") {
        return Err(format!(
            "endpoint host must be a loopback address (localhost / 127.0.0.1 / [::1]), got `{host}`. \
             RepoPrism never sends your repository to a remote service."
        ));
    }

    Ok(())
}

/// 构造一个**不使用环境代理**的 HTTP agent。
///
/// 这条要求是安全性的，不是风格问题。`HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY`
/// 这类环境变量**会改写连接目标**：一旦被遵循，endpoint 上写着的 `127.0.0.1`
/// 就不再是真正要连的地址，「只把仓库发给本机」这句承诺当场失效。
/// 而本模块是全项目**唯一**的出网点——`scripts/read-only-guard.sh` 是 Git 动词
/// 白名单、**看不见 HTTP 调用**，没有第二道静态防线兜底。
///
/// 因此这里**刻意不调用** `AgentBuilder::proxy_from_env()`（那是 ureq 的 opt-in）。
/// 「不调用」是有意为之，不是疏漏：`crates/repo-prism-core/tests/summarizer_egress.rs`
/// 用一个指向死端口的代理把这条性质钉住了——若 ureq 改变默认行为，那条测试会红，
/// 而不是静静地让仓库内容绕道代理出去。
fn local_agent() -> ureq::Agent {
    ureq::AgentBuilder::new().build()
}

/// 基于 Ollama 的摘要器。
///
/// 构造即校验，因此**持有一个 `OllamaSummarizer` 就等价于「配置合法」**，
/// 后续调用不必重复判断 endpoint。
pub struct OllamaSummarizer {
    config: OllamaConfig,
}

impl OllamaSummarizer {
    pub fn new(config: OllamaConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &OllamaConfig {
        &self.config
    }

    /// endpoint 去掉尾部斜杠，便于拼接 `/api/...`。
    fn base(&self) -> String {
        self.config
            .endpoint
            .trim()
            .trim_end_matches('/')
            .to_string()
    }

    /// 健康检查：`GET /api/tags`，返回本地已安装的模型名。
    ///
    /// 设置面板用它做「测试连接」，也顺便给模型名输入框提供候选。
    pub fn list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/api/tags", self.base());
        let response = local_agent()
            .get(&url)
            .timeout(Duration::from_secs(HEALTH_TIMEOUT_SECS))
            .call()
            .map_err(|e| format!("cannot reach the local model server at {url}: {e}"))?;
        let body: serde_json::Value = response
            .into_json()
            .map_err(|e| format!("the local model server returned invalid JSON: {e}"))?;
        Ok(body
            .get("models")
            .and_then(|value| value.as_array())
            .map(|models| {
                models
                    .iter()
                    .filter_map(|entry| entry.get("name").and_then(|name| name.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default())
    }

    /// 通用生成接口，提交级分析（TASK-014）复用它。
    ///
    /// 失败一律 `None`：摘要层是锦上添花的功能，不该把「模型没启动」
    /// 升级成让整个界面报错的错误。调用方需要提示时自己另作判断。
    pub fn generate(&self, prompt: &str) -> Option<String> {
        let url = format!("{}/api/generate", self.base());
        let body = serde_json::json!({
            "model": self.config.model,
            "prompt": prompt,
            "stream": false,
            "options": { "temperature": 0.2 }
        });
        let response = local_agent()
            .post(&url)
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .send_json(body)
            .ok()?;
        let body: serde_json::Value = response.into_json().ok()?;
        body.get("response")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    }
}

impl Summarizer for OllamaSummarizer {
    fn summarize(&self, analysis: &ChangeAnalysis) -> Option<String> {
        self.generate(&build_changes_prompt(analysis))
    }
}

/// 把风险分析压成 prompt。**只含相对路径与规则结果**，见模块文档。
pub fn build_changes_prompt(analysis: &ChangeAnalysis) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "You are a senior engineer reviewing uncommitted changes in a Git working tree. \
         Answer in the same language the file paths and messages suggest (use Chinese when \
         they contain Chinese, otherwise English). Reply with 2-3 sentences, no preamble, \
         no bullet list.\n\n",
    );
    prompt.push_str(&format!("Changed files: {}\n", analysis.total_files));

    if analysis.risks.is_empty() {
        prompt.push_str("The rule engine reported no findings.\n");
    } else {
        prompt.push_str("\nRule engine findings, one per line as `[level] rule - path`:\n");
        for risk in &analysis.risks {
            prompt.push_str(&format!(
                "- [{}] {} - {}\n",
                level_label(risk.level),
                risk.rule_id,
                risk.path
            ));
        }
    }

    prompt.push_str(
        "\nTask: infer what these changes are trying to accomplish, then name the single \
         thing a reviewer should look at first. Synthesize; do not restate the list.\n",
    );
    prompt
}

/// 为单个提交生成 prompt。文件列表超过 [`MAX_PROMPT_FILES`] 时截断并注明余量。
pub fn build_commit_prompt(subject: &str, body: Option<&str>, files: &[String]) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "You are a senior engineer. Below is a single Git commit. Reply with 1-2 sentences \
         describing what it does, plus one caveat if there is one. Match the language of the \
         subject line, no preamble.\n\n",
    );
    prompt.push_str(&format!("Subject: {subject}\n"));

    if let Some(body) = body {
        if !body.trim().is_empty() {
            prompt.push_str(&format!("Body:\n{body}\n"));
        }
    }

    if !files.is_empty() {
        prompt.push_str("\nFiles touched:\n");
        for path in files.iter().take(MAX_PROMPT_FILES) {
            prompt.push_str(&format!("- {path}\n"));
        }
        if files.len() > MAX_PROMPT_FILES {
            prompt.push_str(&format!(
                "- ... and {} more, not listed\n",
                files.len() - MAX_PROMPT_FILES
            ));
        }
    }

    prompt
}

fn level_label(level: RiskLevel) -> &'static str {
    match level {
        RiskLevel::Critical => "critical",
        RiskLevel::Warn => "warning",
        RiskLevel::Info => "info",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{Risk, RiskCounts};

    fn config(endpoint: &str) -> OllamaConfig {
        OllamaConfig {
            endpoint: endpoint.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout_secs: 30,
        }
    }

    fn analysis_with(risks: Vec<Risk>) -> ChangeAnalysis {
        let mut by_level = RiskCounts::default();
        for risk in &risks {
            match risk.level {
                RiskLevel::Critical => by_level.critical += 1,
                RiskLevel::Warn => by_level.warn += 1,
                RiskLevel::Info => by_level.info += 1,
            }
        }
        ChangeAnalysis {
            summary: "rule engine summary".to_string(),
            total_files: 2,
            by_level,
            risks,
        }
    }

    #[test]
    fn accepts_loopback_endpoints() {
        for endpoint in [
            "http://localhost:11434",
            "http://localhost",
            "http://localhost/",
            "http://127.0.0.1:11434",
            "http://127.0.0.1:11434/",
            "http://[::1]:11434",
            "HTTP://LOCALHOST:11434",
            "  http://localhost:11434  ",
            "http://localhost.:11434",
        ] {
            assert!(
                config(endpoint).validate().is_ok(),
                "should accept {endpoint}"
            );
        }
    }

    #[test]
    fn rejects_remote_endpoints() {
        for endpoint in [
            "https://api.openai.com",
            "http://api.openai.com",
            "http://192.168.1.10:11434",
            "http://10.0.0.5:11434",
            "http://0.0.0.0:11434",
            "http://example.com/localhost",
        ] {
            assert!(
                config(endpoint).validate().is_err(),
                "should reject {endpoint}"
            );
        }
    }

    /// 这组是本模块最该留下的测试：每一条都能骗过「比前缀」的写法。
    #[test]
    fn rejects_hosts_that_only_look_local() {
        for endpoint in [
            "http://localhost.evil.com",
            "http://localhost.evil.com:11434",
            "http://localhostevil.com",
            "http://127.0.0.1.evil.com:11434",
            "http://localhost@evil.com",
            "http://user:pass@localhost:11434",
            "http://[::1]evil.com",
            "http://[::1]:11434@evil.com",
        ] {
            assert!(
                config(endpoint).validate().is_err(),
                "should reject {endpoint}"
            );
        }
    }

    #[test]
    fn malformed_endpoints_are_rejected() {
        for endpoint in [
            "",
            "   ",
            "localhost:11434",
            "ftp://localhost:11434",
            "http://",
            "http://:11434",
            "http://localhost:abc",
            "http://localhost:0",
            "http://localhost:70000",
            "http://[::1",
        ] {
            assert!(
                config(endpoint).validate().is_err(),
                "should reject {endpoint}"
            );
        }
    }

    #[test]
    fn rejects_empty_model() {
        let mut cfg = config("http://localhost:11434");
        cfg.model = "   ".to_string();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_out_of_range_timeout() {
        let mut cfg = config("http://localhost:11434");
        cfg.timeout_secs = 0;
        assert!(cfg.validate().is_err(), "0 秒会在库里变成立即超时");
        cfg.timeout_secs = MAX_TIMEOUT_SECS + 1;
        assert!(cfg.validate().is_err());
        cfg.timeout_secs = MAX_TIMEOUT_SECS;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn new_is_the_only_way_in_and_it_validates() {
        assert!(OllamaSummarizer::new(config("http://localhost:11434")).is_ok());
        assert!(OllamaSummarizer::new(config("https://api.openai.com")).is_err());
    }

    #[test]
    fn default_config_is_valid_and_local() {
        let cfg = OllamaConfig::default();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.endpoint, DEFAULT_ENDPOINT);
        assert_eq!(cfg.timeout_secs, DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn changes_prompt_carries_findings_but_not_repo_paths() {
        let analysis = analysis_with(vec![Risk {
            rule_id: "env-or-secret".to_string(),
            level: RiskLevel::Critical,
            message: "疑似密钥".to_string(),
            path: "config/app.env".to_string(),
        }]);

        let prompt = build_changes_prompt(&analysis);

        assert!(prompt.contains("Changed files: 2"));
        assert!(prompt.contains("env-or-secret"));
        assert!(prompt.contains("config/app.env"));
        assert!(prompt.contains("critical"));
        // 规则自带的叙述性 message 不进 prompt：prompt 里只要有事实，不要有二次加工。
        assert!(!prompt.contains("疑似密钥"));
    }

    #[test]
    fn changes_prompt_says_so_when_the_rule_engine_is_quiet() {
        let prompt = build_changes_prompt(&analysis_with(Vec::new()));
        assert!(prompt.contains("no findings"));
    }

    #[test]
    fn commit_prompt_caps_the_file_list() {
        let files: Vec<String> = (0..60).map(|i| format!("src/f{i}.rs")).collect();
        let prompt = build_commit_prompt("tidy up", None, &files);

        assert!(prompt.contains("src/f39.rs"), "第 40 个文件应仍被列出");
        assert!(!prompt.contains("src/f40.rs"), "第 41 个文件应被截掉");
        assert!(prompt.contains("and 20 more"), "截断要注明余量");
    }

    #[test]
    fn commit_prompt_includes_body_only_when_meaningful() {
        let with_body = build_commit_prompt("fix parser", Some("it choked on tabs"), &[]);
        assert!(with_body.contains("it choked on tabs"));

        let blank_body = build_commit_prompt("fix parser", Some("   \n  "), &[]);
        assert!(
            !blank_body.contains("Body:"),
            "空白 body 不该占 prompt 位置"
        );
    }
}
