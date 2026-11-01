use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Manager;
use urlencoding::encode as url_encode;

const KEYRING_SERVICE: &str = "chengsi.academic";
const CONFIG_FILE: &str = "settings.json";
const USER_AGENT: &str = "ChengsiAcademic/0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    pub id: String,
    pub model_id: String,
    pub display_name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub models: Vec<ModelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceConfig {
    pub id: String,
    pub category: String,
    pub name: String,
    pub driver: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub contact_email: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub api_key_required: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleRouting {
    #[serde(default)]
    pub conversation: Option<String>,
    #[serde(default)]
    pub research: Option<String>,
    #[serde(default)]
    pub planning: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub report: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSettings {
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            proxy_url: None,
            timeout_seconds: default_timeout(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacySettings {
    #[serde(default = "default_true")]
    pub allow_remote_egress: bool,
    #[serde(default)]
    pub confirm_before_egress: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            allow_remote_egress: true,
            confirm_before_egress: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GlobalSettings {
    pub providers: Vec<ProviderConfig>,
    pub sources: Vec<SourceConfig>,
    pub roles: RoleRouting,
    pub network: NetworkSettings,
    pub privacy: PrivacySettings,
    pub autonomy: String,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub skills: SkillSettings,
    #[serde(default)]
    pub updates: UpdateSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    #[serde(default = "default_true")]
    pub auto_check: bool,
    #[serde(default)]
    pub skipped_version: Option<String>,
    #[serde(default)]
    pub last_checked_at: Option<String>,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            auto_check: true,
            skipped_version: None,
            last_checked_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillSettings {
    #[serde(default)]
    pub enabled: Vec<String>,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        seed_settings()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub settings: GlobalSettings,
    pub provider_keys: HashMap<String, bool>,
    pub source_keys: HashMap<String, bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub status_code: Option<u16>,
    pub latency_ms: u128,
    pub message: String,
    pub detail: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    30
}

fn seed_settings() -> GlobalSettings {
    let model_ref = "deepseek/deepseek-v4-1-flash".to_string();
    GlobalSettings {
        providers: vec![ProviderConfig {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            protocol: "openai_compatible".to_string(),
            base_url: "https://api.deepseek.com".to_string(),
            enabled: true,
            proxy: None,
            timeout_seconds: 90,
            models: vec![ModelConfig {
                id: "deepseek-v4-1-flash".to_string(),
                model_id: "deepseek-v4.1-flash".to_string(),
                display_name: "DeepSeek V4.1 Flash".to_string(),
                enabled: true,
                capabilities: vec!["chat".to_string(), "stream".to_string()],
            }],
        }],
        sources: vec![
            source(
                "openalex",
                "academic",
                "OpenAlex",
                "openalex",
                "https://api.openalex.org",
                true,
                false,
                None,
                Some("建议配置免费 API Key 以提高配额。"),
            ),
            source(
                "crossref",
                "academic",
                "Crossref",
                "crossref",
                "https://api.crossref.org",
                true,
                false,
                None,
                Some("建议填写联系邮箱进入 polite pool。"),
            ),
            source(
                "arxiv",
                "academic",
                "arXiv",
                "arxiv",
                "http://export.arxiv.org/api/query",
                true,
                false,
                Some("en"),
                None,
            ),
            source(
                "semantic_scholar",
                "academic",
                "Semantic Scholar",
                "semantic_scholar",
                "https://api.semanticscholar.org/graph/v1",
                true,
                false,
                Some("en"),
                Some("匿名访问共享配额较低，建议配置 API Key。"),
            ),
            source(
                "pubmed",
                "academic",
                "PubMed",
                "pubmed",
                "https://eutils.ncbi.nlm.nih.gov/entrez/eutils",
                true,
                false,
                Some("en"),
                Some("建议填写联系邮箱；可选 API Key。"),
            ),
            source(
                "europe_pmc",
                "academic",
                "Europe PMC",
                "europe_pmc",
                "https://www.ebi.ac.uk/europepmc/webservices/rest",
                true,
                false,
                Some("en"),
                None,
            ),
            source(
                "xinhua_tech",
                "news",
                "新华网科技",
                "rss",
                "http://www.news.cn/tech/news_tech.xml",
                true,
                false,
                Some("zh-CN"),
                None,
            ),
            source(
                "sciencenet",
                "news",
                "科学网",
                "rss",
                "https://www.sciencenet.cn/xml/rss.aspx?mod=1",
                true,
                false,
                Some("zh-CN"),
                None,
            ),
            source(
                "ithome",
                "news",
                "IT之家",
                "rss",
                "https://www.ithome.com/rss/",
                true,
                false,
                Some("zh-CN"),
                None,
            ),
            source(
                "sspai",
                "news",
                "少数派",
                "rss",
                "https://sspai.com/feed",
                true,
                false,
                Some("zh-CN"),
                None,
            ),
            source(
                "cnbeta",
                "news",
                "cnBeta",
                "rss",
                "https://www.cnbeta.com.tw/backend.php",
                true,
                false,
                Some("zh-CN"),
                None,
            ),
            source(
                "nature",
                "news",
                "Nature",
                "rss",
                "https://www.nature.com/nature.rss",
                false,
                false,
                Some("en"),
                Some("英文来源，默认停用。"),
            ),
            source(
                "sciencedaily",
                "news",
                "ScienceDaily",
                "rss",
                "https://www.sciencedaily.com/rss/top/science.xml",
                false,
                false,
                Some("en"),
                Some("英文来源，默认停用。"),
            ),
            source(
                "serpapi",
                "news",
                "SerpAPI 新闻检索",
                "serpapi",
                "https://serpapi.com/search.json",
                false,
                true,
                None,
                Some("需要 API Key；默认停用。"),
            ),
            source(
                "newsapi",
                "news",
                "NewsAPI",
                "newsapi",
                "https://newsapi.org/v2/everything",
                false,
                true,
                None,
                Some("需要 API Key；默认停用。"),
            ),
        ],
        roles: RoleRouting {
            conversation: Some(model_ref.clone()),
            research: Some(model_ref.clone()),
            planning: Some(model_ref.clone()),
            code: Some(model_ref.clone()),
            report: Some(model_ref),
        },
        network: NetworkSettings::default(),
        privacy: PrivacySettings::default(),
        autonomy: "balanced".to_string(),
        mcp_servers: Vec::new(),
        skills: SkillSettings::default(),
        updates: UpdateSettings::default(),
    }
}

#[allow(clippy::too_many_arguments)]
fn source(
    id: &str,
    category: &str,
    name: &str,
    driver: &str,
    url: &str,
    enabled: bool,
    api_key_required: bool,
    language: Option<&str>,
    notes: Option<&str>,
) -> SourceConfig {
    SourceConfig {
        id: id.to_string(),
        category: category.to_string(),
        name: name.to_string(),
        driver: driver.to_string(),
        enabled,
        url: Some(url.to_string()),
        contact_email: None,
        language: language.map(str::to_string),
        region: None,
        api_key_required,
        notes: notes.map(str::to_string),
    }
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法定位应用配置目录：{error}"))?;
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建应用配置目录：{error}"))?;
    Ok(directory.join(CONFIG_FILE))
}

fn load_settings(app: &tauri::AppHandle) -> Result<GlobalSettings, String> {
    let path = settings_path(app)?;
    if !path.is_file() {
        let settings = seed_settings();
        write_settings(&path, &settings)?;
        return Ok(settings);
    }

    let text = fs::read_to_string(&path).map_err(|error| format!("无法读取设置文件：{error}"))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("设置文件格式无效（{}）：{error}", path.display()))
}

pub(crate) struct ResolvedModel {
    pub provider: ProviderConfig,
    pub model: ModelConfig,
    pub api_key: Option<String>,
}

pub(crate) fn load_settings_for_app(app: &tauri::AppHandle) -> Result<GlobalSettings, String> {
    load_settings(app)
}

pub(crate) fn save_settings_for_app(
    app: &tauri::AppHandle,
    settings: &GlobalSettings,
) -> Result<(), String> {
    let path = settings_path(app)?;
    write_settings(&path, settings)
}

pub(crate) fn resolve_role(settings: &GlobalSettings, role: &str) -> Result<ResolvedModel, String> {
    let reference = match role {
        "conversation" => settings.roles.conversation.as_ref(),
        "research" => settings.roles.research.as_ref(),
        "planning" => settings.roles.planning.as_ref(),
        "code" => settings.roles.code.as_ref(),
        "report" => settings.roles.report.as_ref(),
        _ => None,
    }
    .or(settings.roles.conversation.as_ref());

    let (provider, model) = if let Some(reference) = reference {
        let (provider_id, model_id) = reference
            .split_once('/')
            .ok_or_else(|| format!("角色路由引用无效：{reference}"))?;
        let provider = settings
            .providers
            .iter()
            .find(|item| item.id == provider_id)
            .ok_or_else(|| format!("角色路由引用了不存在的服务：{provider_id}"))?;
        let model = provider
            .models
            .iter()
            .find(|item| item.id == model_id)
            .ok_or_else(|| format!("角色路由引用了不存在的模型：{model_id}"))?;
        (provider, model)
    } else {
        let provider = settings
            .providers
            .iter()
            .find(|item| item.enabled && item.models.iter().any(|model| model.enabled))
            .ok_or_else(|| "没有可用的模型服务，请先在设置中配置。".to_string())?;
        let model = provider
            .models
            .iter()
            .find(|item| item.enabled)
            .ok_or_else(|| "模型服务没有启用的模型。".to_string())?;
        (provider, model)
    };

    if !provider.enabled {
        return Err(format!("模型服务 {} 尚未启用。", provider.name));
    }
    if !model.enabled {
        return Err(format!("模型 {} 尚未启用。", model.display_name));
    }
    if matches!(provider.protocol.as_str(), "anthropic" | "gemini") {
        return Err("该原生协议适配器尚未接入。".to_string());
    }

    let api_key = get_secret("model", &provider.id);
    if provider.protocol != "ollama" && api_key.is_none() {
        return Err("尚未配置 API Key，请先在设置 → 模型服务中保存密钥。".to_string());
    }

    Ok(ResolvedModel {
        provider: provider.clone(),
        model: model.clone(),
        api_key,
    })
}

pub(crate) fn academic_sources_payload(settings: &GlobalSettings) -> Vec<serde_json::Value> {
    settings
        .sources
        .iter()
        .filter(|source| source.category == "academic" && source.enabled)
        .map(|source| {
            serde_json::json!({
                "id": source.id,
                "name": source.name,
                "driver": source.driver,
                "url": source.url,
                "apiKey": get_secret("source", &source.id),
                "contactEmail": source.contact_email,
                "language": source.language
            })
        })
        .collect()
}

pub(crate) fn news_sources_payload(settings: &GlobalSettings) -> Vec<serde_json::Value> {
    settings
        .sources
        .iter()
        .filter(|source| source.category == "news" && source.enabled)
        .map(|source| {
            serde_json::json!({
                "id": source.id,
                "name": source.name,
                "driver": source.driver,
                "url": source.url,
                "apiKey": get_secret("source", &source.id),
                "language": source.language
            })
        })
        .collect()
}

pub(crate) fn network_payload(settings: &GlobalSettings) -> serde_json::Value {
    serde_json::json!({
        "proxyUrl": settings.network.proxy_url,
        "timeoutSeconds": settings.network.timeout_seconds
    })
}

fn write_settings(path: &PathBuf, settings: &GlobalSettings) -> Result<(), String> {
    validate_settings(settings)?;
    let text = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("序列化设置失败：{error}"))?;
    let temporary = path.with_file_name("settings.json.tmp");
    fs::write(&temporary, text).map_err(|error| format!("写入设置失败：{error}"))?;
    fs::rename(&temporary, path).map_err(|error| format!("保存设置失败：{error}"))
}

fn validate_settings(settings: &GlobalSettings) -> Result<(), String> {
    let mut mcp_ids = std::collections::HashSet::new();
    for server in &settings.mcp_servers {
        let id = server.id.trim();
        if id.is_empty() || !mcp_ids.insert(id.to_string()) {
            return Err("MCP 服务 ID 为空或重复。".to_string());
        }
        if server.name.trim().is_empty() {
            return Err(format!("MCP 服务 {id} 缺少名称。"));
        }
        if server.command.trim().is_empty() {
            return Err(format!("MCP 服务 {id} 缺少启动命令。"));
        }
        if server.args.len() > 32 || server.env.len() > 32 {
            return Err(format!("MCP 服务 {id} 的参数过多。"));
        }
    }
    let mut provider_ids = std::collections::HashSet::new();
    for provider in &settings.providers {
        if provider.id.trim().is_empty() || !provider_ids.insert(provider.id.clone()) {
            return Err("模型服务 ID 为空或重复。".to_string());
        }
        if !matches!(
            provider.protocol.as_str(),
            "openai_compatible" | "ollama" | "anthropic" | "gemini"
        ) {
            return Err(format!("模型服务 {} 的协议无效。", provider.name));
        }
        if !provider.base_url.starts_with("http://") && !provider.base_url.starts_with("https://") {
            return Err(format!(
                "模型服务 {} 的 Base URL 必须是 http(s)。",
                provider.name
            ));
        }
        if provider.models.is_empty() {
            return Err(format!("模型服务 {} 至少需要一个模型。", provider.name));
        }
    }

    let mut source_ids = std::collections::HashSet::new();
    for source in &settings.sources {
        if source.id.trim().is_empty() || !source_ids.insert(source.id.clone()) {
            return Err("来源 ID 为空或重复。".to_string());
        }
        if !matches!(source.category.as_str(), "academic" | "news" | "web") {
            return Err(format!("来源 {} 的类别无效。", source.name));
        }
        if source.driver == "rss" {
            let url = source.url.as_deref().unwrap_or("");
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(format!("RSS 来源 {} 需要有效的 Feed URL。", source.name));
            }
        }
    }

    if let Some(reference) = settings.roles.conversation.as_deref() {
        validate_role_reference(settings, reference)?;
    }
    Ok(())
}

fn validate_role_reference(settings: &GlobalSettings, reference: &str) -> Result<(), String> {
    let Some((provider_id, model_id)) = reference.split_once('/') else {
        return Err(format!("角色路由引用无效：{reference}"));
    };
    let Some(provider) = settings
        .providers
        .iter()
        .find(|item| item.id == provider_id)
    else {
        return Err(format!("角色路由引用了不存在的服务：{provider_id}"));
    };
    if !provider.models.iter().any(|model| model.id == model_id) {
        return Err(format!("角色路由引用了不存在的模型：{model_id}"));
    }
    Ok(())
}

fn credential_entry(kind: &str, id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("{kind}:{id}"))
        .map_err(|error| format!("无法访问系统凭据库：{error}"))
}

fn has_secret(kind: &str, id: &str) -> bool {
    credential_entry(kind, id)
        .and_then(|entry| entry.get_password().map_err(|error| error.to_string()))
        .is_ok()
}

fn get_secret(kind: &str, id: &str) -> Option<String> {
    credential_entry(kind, id)
        .ok()
        .and_then(|entry| entry.get_password().ok())
}

fn save_secret(kind: &str, id: &str, secret: &str) -> Result<bool, String> {
    let entry = credential_entry(kind, id)?;
    if secret.trim().is_empty() {
        let _ = entry.delete_credential();
        return Ok(false);
    }
    entry
        .set_password(secret.trim())
        .map_err(|error| format!("写入系统凭据库失败：{error}"))?;
    Ok(true)
}

fn view(settings: GlobalSettings) -> SettingsView {
    let provider_keys = settings
        .providers
        .iter()
        .map(|provider| (provider.id.clone(), has_secret("model", &provider.id)))
        .collect();
    let source_keys = settings
        .sources
        .iter()
        .map(|source| (source.id.clone(), has_secret("source", &source.id)))
        .collect();
    SettingsView {
        settings,
        provider_keys,
        source_keys,
    }
}

#[tauri::command]
pub fn get_global_settings(app: tauri::AppHandle) -> Result<SettingsView, String> {
    Ok(view(load_settings(&app)?))
}

#[tauri::command]
pub fn save_global_settings(
    app: tauri::AppHandle,
    settings: GlobalSettings,
) -> Result<SettingsView, String> {
    let path = settings_path(&app)?;
    write_settings(&path, &settings)?;
    Ok(view(settings))
}

#[tauri::command]
pub fn set_secret(kind: String, id: String, secret: String) -> Result<bool, String> {
    if !matches!(kind.as_str(), "model" | "source") {
        return Err("凭据类型无效。".to_string());
    }
    save_secret(&kind, &id, &secret)
}

#[tauri::command]
pub fn test_provider(app: tauri::AppHandle, provider_id: String) -> ConnectionTestResult {
    let settings = match load_settings(&app) {
        Ok(settings) => settings,
        Err(error) => return failed(None, error),
    };
    test_provider_config(&settings, &provider_id)
}

#[tauri::command]
pub fn test_source(app: tauri::AppHandle, source_id: String) -> ConnectionTestResult {
    let settings = match load_settings(&app) {
        Ok(settings) => settings,
        Err(error) => return failed(None, error),
    };
    test_source_config(&settings, &source_id)
}

fn test_provider_config(settings: &GlobalSettings, provider_id: &str) -> ConnectionTestResult {
    let Some(provider) = settings
        .providers
        .iter()
        .find(|item| item.id == provider_id)
    else {
        return failed(None, "模型服务不存在。".to_string());
    };
    let Some(model) = provider
        .models
        .iter()
        .find(|item| item.enabled)
        .or_else(|| provider.models.first())
    else {
        return failed(None, "该服务尚未配置模型。".to_string());
    };

    if matches!(provider.protocol.as_str(), "anthropic" | "gemini") {
        return failed(
            None,
            "该原生协议适配器尚未接入，请先使用 OpenAI-compatible 端点。".to_string(),
        );
    }

    let key = get_secret("model", &provider.id);
    if provider.protocol != "ollama" && key.is_none() {
        return failed(None, "尚未配置 API Key。请先在设置中保存密钥。".to_string());
    }

    let endpoint = chat_endpoint(provider);
    let agent = match http_agent(provider.proxy.as_deref(), provider.timeout_seconds) {
        Ok(agent) => agent,
        Err(error) => return failed(None, error),
    };
    let body = serde_json::json!({
        "model": model.model_id,
        "messages": [{"role": "user", "content": "Reply with OK."}],
        "max_tokens": 8,
        "stream": false
    });

    let mut request = agent
        .post(&endpoint)
        .set("Content-Type", "application/json")
        .set("User-Agent", USER_AGENT);
    if let Some(key) = key.as_deref() {
        request = request.set("Authorization", &format!("Bearer {key}"));
    }

    let started = Instant::now();
    match request.send_json(body) {
        Ok(response) => {
            let status = response.status();
            let text = response.into_string().unwrap_or_default();
            let (ok, message) = provider_response_message(&text);
            ConnectionTestResult {
                ok,
                status_code: Some(status),
                latency_ms: started.elapsed().as_millis(),
                message,
                detail: Some(truncate(&text, 400)),
            }
        }
        Err(ureq::Error::Status(code, response)) => {
            let text = response.into_string().unwrap_or_default();
            let message = provider_error_message(&text)
                .unwrap_or_else(|| format!("模型服务返回 HTTP {code}。"));
            failed_with_detail(Some(code), message, &text, started)
        }
        Err(ureq::Error::Transport(error)) => {
            failed_with_detail(None, format!("网络请求失败：{error}"), "", started)
        }
    }
}

fn test_source_config(settings: &GlobalSettings, source_id: &str) -> ConnectionTestResult {
    let Some(source) = settings.sources.iter().find(|item| item.id == source_id) else {
        return failed(None, "来源不存在。".to_string());
    };

    let key = get_secret("source", &source.id);
    if source.api_key_required && key.is_none() {
        return failed(None, "该来源需要 API Key，请先在设置中保存。".to_string());
    }

    let agent = match http_agent(
        settings.network.proxy_url.as_deref(),
        settings.network.timeout_seconds,
    ) {
        Ok(agent) => agent,
        Err(error) => return failed(None, error),
    };

    let (url, headers) = match source_request(source, key.as_deref()) {
        Ok(value) => value,
        Err(error) => return failed(None, error),
    };

    let started = Instant::now();
    let mut request = agent.get(&url).set("User-Agent", USER_AGENT);
    for (name, value) in headers {
        request = request.set(&name, &value);
    }

    match request.call() {
        Ok(response) => {
            let status = response.status();
            let text = response.into_string().unwrap_or_default();
            source_success(source, status, &text, started)
        }
        Err(ureq::Error::Status(code, response)) => {
            let text = response.into_string().unwrap_or_default();
            failed_with_detail(
                Some(code),
                format!("来源返回 HTTP {code}。"),
                &text,
                started,
            )
        }
        Err(ureq::Error::Transport(error)) => {
            failed_with_detail(None, format!("网络请求失败：{error}"), "", started)
        }
    }
}

fn source_success(
    source: &SourceConfig,
    status: u16,
    text: &str,
    started: Instant,
) -> ConnectionTestResult {
    if source.driver == "rss" {
        let looks_like_feed = ["<rss", "<feed", "<rdf:RDF"]
            .iter()
            .any(|needle| text.contains(needle));
        if !looks_like_feed {
            return failed_with_detail(
                Some(status),
                "返回内容不是 RSS/Atom 文档。".to_string(),
                text,
                started,
            );
        }
        let title = first_xml_title(text).unwrap_or_else(|| "未取到标题".to_string());
        return ConnectionTestResult {
            ok: true,
            status_code: Some(status),
            latency_ms: started.elapsed().as_millis(),
            message: format!("RSS 可读取，首条标题：{title}"),
            detail: Some(truncate(text, 300)),
        };
    }

    let value: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    if let Some(error) = value
        .pointer("/error/message")
        .or_else(|| value.get("error"))
        .and_then(|value| value.as_str())
    {
        return failed_with_detail(
            Some(status),
            format!("来源返回错误：{error}"),
            text,
            started,
        );
    }

    let count = result_count(source.driver.as_str(), &value);
    let message = match count {
        Some(count) => format!("HTTP {status}，返回 {count} 条结果。"),
        None => format!("HTTP {status}，返回 {} 字节。", text.len()),
    };

    ConnectionTestResult {
        ok: true,
        status_code: Some(status),
        latency_ms: started.elapsed().as_millis(),
        message,
        detail: Some(truncate(text, 300)),
    }
}

fn source_request(
    source: &SourceConfig,
    key: Option<&str>,
) -> Result<(String, Vec<(String, String)>), String> {
    let base = source.url.clone().unwrap_or_default();
    if base.is_empty() {
        return Err(format!("来源 {} 尚未配置 URL。", source.name));
    }

    let mut headers: Vec<(String, String)> = Vec::new();
    let url = match source.driver.as_str() {
        "openalex" => {
            let mut url = format!(
                "{}/works?search=urban%20heat&per-page=1",
                base.trim_end_matches('/')
            );
            if let Some(key) = key {
                url.push_str(&format!("&api_key={}", url_encode(key)));
            }
            url
        }
        "crossref" => {
            let mut url = format!(
                "{}/works?query=urban+heat&rows=1",
                base.trim_end_matches('/')
            );
            if let Some(email) = source
                .contact_email
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                url.push_str(&format!("&mailto={}", url_encode(email)));
            }
            url
        }
        "arxiv" => format!(
            "{}?search_query=all:urban&max_results=1",
            base.trim_end_matches('/')
        ),
        "semantic_scholar" => {
            if let Some(key) = key {
                headers.push(("x-api-key".to_string(), key.to_string()));
            }
            format!(
                "{}/paper/search?query=urban&limit=1&fields=title,year",
                base.trim_end_matches('/')
            )
        }
        "pubmed" => {
            let mut url = format!(
                "{}/esearch.fcgi?db=pubmed&term=urban&retmode=json&retmax=1&tool=chengsi",
                base.trim_end_matches('/')
            );
            if let Some(email) = source
                .contact_email
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                url.push_str(&format!("&email={}", url_encode(email)));
            }
            if let Some(key) = key {
                url.push_str(&format!("&api_key={}", url_encode(key)));
            }
            url
        }
        "europe_pmc" => format!(
            "{}/search?query=urban&format=json&pageSize=1",
            base.trim_end_matches('/')
        ),
        "rss" | "web" => base,
        "serpapi" => format!(
            "{}?engine=google_news&q=science&api_key={}",
            base.trim_end_matches('/'),
            url_encode(key.unwrap_or_default())
        ),
        "newsapi" => format!(
            "{}?q=science&pageSize=1&apiKey={}",
            base.trim_end_matches('/'),
            url_encode(key.unwrap_or_default())
        ),
        other => return Err(format!("不支持的来源驱动：{other}")),
    };

    Ok((url, headers))
}

fn result_count(driver: &str, value: &Value) -> Option<u64> {
    match driver {
        "openalex" => value.pointer("/meta/count").and_then(|item| item.as_u64()),
        "crossref" => value
            .pointer("/message/total-results")
            .and_then(|item| item.as_u64()),
        "semantic_scholar" => value.get("total").and_then(|item| item.as_u64()),
        "pubmed" => value
            .pointer("/esearchresult/count")
            .and_then(|item| item.as_str())
            .and_then(|item| item.parse().ok()),
        "europe_pmc" => value.get("hitCount").and_then(|item| item.as_u64()),
        _ => None,
    }
}

fn chat_endpoint(provider: &ProviderConfig) -> String {
    let base = provider.base_url.trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        return base.to_string();
    }
    if provider.protocol == "ollama" && !base.ends_with("/v1") {
        return format!("{base}/v1/chat/completions");
    }
    format!("{base}/chat/completions")
}

pub(crate) fn http_agent(proxy: Option<&str>, timeout_seconds: u64) -> Result<ureq::Agent, String> {
    let mut builder =
        ureq::AgentBuilder::new().timeout(Duration::from_secs(timeout_seconds.clamp(5, 120)));
    if let Some(proxy) = proxy.map(str::trim).filter(|value| !value.is_empty()) {
        let proxy = ureq::Proxy::new(proxy).map_err(|error| format!("代理地址无效：{error}"))?;
        builder = builder.proxy(proxy);
    }
    Ok(builder.build())
}

fn provider_response_message(text: &str) -> (bool, String) {
    let value: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    if let Some(error) = value
        .pointer("/error/message")
        .and_then(|item| item.as_str())
    {
        return (false, format!("模型服务返回错误：{error}"));
    }
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(|item| item.as_str())
        .unwrap_or("");
    if content.is_empty() {
        (true, "连接成功，模型返回了空文本响应。".to_string())
    } else {
        (
            true,
            format!("连接成功，模型返回：{}", truncate(content, 80)),
        )
    }
}

fn provider_error_message(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    value
        .pointer("/error/message")
        .and_then(|item| item.as_str())
        .map(|message| format!("模型服务返回错误：{message}"))
}

fn first_xml_title(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let start = lower.find("<title")?;
    let after = &text[start..];
    let open_end = after.find('>')?;
    let rest = &after[open_end + 1..];
    let end = rest.find("</title>")?;
    let raw = rest[..end].trim();
    let cleaned = raw
        .trim_start_matches("<![CDATA[")
        .trim_end_matches("]]>")
        .trim();
    if cleaned.is_empty() {
        None
    } else {
        Some(truncate(cleaned, 80))
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        let truncated: String = value.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

fn failed(code: Option<u16>, message: String) -> ConnectionTestResult {
    ConnectionTestResult {
        ok: false,
        status_code: code,
        latency_ms: 0,
        message,
        detail: None,
    }
}

fn failed_with_detail(
    code: Option<u16>,
    message: String,
    detail: &str,
    started: Instant,
) -> ConnectionTestResult {
    ConnectionTestResult {
        ok: false,
        status_code: code,
        latency_ms: started.elapsed().as_millis(),
        message,
        detail: if detail.is_empty() {
            None
        } else {
            Some(truncate(detail, 400))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_settings_match_initial_configuration() {
        let settings = seed_settings();
        assert_eq!(settings.providers.len(), 1);
        assert_eq!(settings.providers[0].id, "deepseek");
        assert_eq!(
            settings.providers[0].models[0].model_id,
            "deepseek-v4.1-flash"
        );
        assert_eq!(
            settings.roles.conversation.as_deref(),
            Some("deepseek/deepseek-v4-1-flash")
        );
        assert_eq!(
            settings
                .sources
                .iter()
                .filter(|item| item.category == "academic")
                .count(),
            6
        );
        assert_eq!(
            settings
                .sources
                .iter()
                .filter(|item| item.category == "news")
                .count(),
            9
        );
        validate_settings(&settings).expect("默认设置有效");
    }

    #[test]
    fn builds_chat_endpoint_for_compatible_and_ollama() {
        let compatible = ProviderConfig {
            id: "p".into(),
            name: "p".into(),
            protocol: "openai_compatible".into(),
            base_url: "https://api.deepseek.com/".into(),
            enabled: true,
            proxy: None,
            timeout_seconds: 30,
            models: vec![],
        };
        assert_eq!(
            chat_endpoint(&compatible),
            "https://api.deepseek.com/chat/completions"
        );

        let ollama = ProviderConfig {
            protocol: "ollama".into(),
            base_url: "http://127.0.0.1:11434".into(),
            ..compatible
        };
        assert_eq!(
            chat_endpoint(&ollama),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
    }

    #[test]
    fn extracts_first_rss_title() {
        let xml = r#"<?xml version="1.0"?><rss><channel><title>科学网</title><item><title>首条新闻</title></item></channel></rss>"#;
        assert_eq!(first_xml_title(xml).as_deref(), Some("科学网"));
    }

    #[cfg(windows)]
    #[test]
    fn keyring_roundtrip() {
        let id = format!("roundtrip_{}", std::process::id());
        let entry = credential_entry("model", &id).expect("open credential entry");
        entry.set_password("test-secret").expect("write credential");
        assert_eq!(
            entry.get_password().expect("read credential"),
            "test-secret"
        );
        entry.delete_credential().expect("delete credential");
    }

    /// 手动烟雾测试：需要本机已配置 DeepSeek API Key。
    /// 运行 `cargo test real_chat_smoke -- --ignored --nocapture`。
    #[test]
    #[ignore]
    fn real_chat_smoke() {
        let appdata =
            std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA available"));
        let path = appdata.join("com.chengsi.academic").join("settings.json");
        let settings: GlobalSettings =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read settings"))
                .expect("parse settings");
        let resolved = resolve_role(&settings, "research").expect("resolve model");

        let mut manager = crate::sidecar::SidecarManager::default();
        let payload = serde_json::json!({
            "provider": {
                "baseUrl": resolved.provider.base_url,
                "protocol": resolved.provider.protocol,
                "proxy": resolved.provider.proxy,
                "timeoutSeconds": resolved.provider.timeout_seconds
            },
            "modelId": resolved.model.model_id,
            "messages": [{"role": "user", "content": "只回复两个字：你好"}],
            "apiKey": resolved.api_key,
            "maxTokens": 512,
            "temperature": 0.2
        });

        let reply = manager.post_json("/chat", &payload).expect("sidecar chat");
        let value: serde_json::Value = serde_json::from_str(&reply).expect("parse reply");
        assert_eq!(
            value.get("ok").and_then(|item| item.as_bool()),
            Some(true),
            "reply={reply}"
        );
        println!(
            "model={} content={}",
            value
                .get("model")
                .and_then(|item| item.as_str())
                .unwrap_or(""),
            value
                .get("content")
                .and_then(|item| item.as_str())
                .unwrap_or("")
        );
        manager.shutdown();
    }

    /// 手动烟雾测试：需要本机已配置学术来源。
    /// 运行 `cargo test real_source_search_smoke -- --ignored --nocapture`。
    #[test]
    #[ignore]
    fn real_source_search_smoke() {
        let appdata =
            std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA available"));
        let path = appdata.join("com.chengsi.academic").join("settings.json");
        let settings: GlobalSettings =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read settings"))
                .expect("parse settings");

        let payload = serde_json::json!({
            "query": "urban green space heat island",
            "maxResults": 3,
            "sources": academic_sources_payload(&settings),
            "network": network_payload(&settings)
        });

        let mut manager = crate::sidecar::SidecarManager::default();
        let reply = manager
            .post_json("/sources/search", &payload)
            .expect("sidecar search");
        let value: serde_json::Value = serde_json::from_str(&reply).expect("parse reply");
        assert_eq!(
            value.get("ok").and_then(|item| item.as_bool()),
            Some(true),
            "reply={reply}"
        );
        println!(
            "results={} statuses={}",
            value
                .get("results")
                .and_then(|item| item.as_array())
                .map(|items| items.len())
                .unwrap_or(0),
            value
                .get("sourceStatuses")
                .and_then(|item| item.as_array())
                .map(|items| items.len())
                .unwrap_or(0)
        );
        manager.shutdown();
    }

    /// 手动烟雾测试：需要本机已启用 RSS 新闻源。
    /// 运行 `cargo test real_news_smoke -- --ignored --nocapture`。
    #[test]
    #[ignore]
    fn real_news_smoke() {
        let appdata =
            std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA available"));
        let path = appdata.join("com.chengsi.academic").join("settings.json");
        let settings: GlobalSettings =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read settings"))
                .expect("parse settings");

        let payload = serde_json::json!({
            "query": "",
            "maxResults": 5,
            "sources": news_sources_payload(&settings),
            "network": network_payload(&settings)
        });

        let mut manager = crate::sidecar::SidecarManager::default();
        let reply = manager
            .post_json("/news/search", &payload)
            .expect("sidecar news");
        let value: serde_json::Value = serde_json::from_str(&reply).expect("parse reply");
        assert_eq!(
            value.get("ok").and_then(|item| item.as_bool()),
            Some(true),
            "reply={reply}"
        );
        let results = value
            .get("results")
            .and_then(|item| item.as_array())
            .expect("results array");
        assert!(!results.is_empty());
        println!(
            "results={} first={}",
            results.len(),
            results[0]
                .get("title")
                .and_then(|item| item.as_str())
                .unwrap_or("")
        );
        manager.shutdown();
    }
}
