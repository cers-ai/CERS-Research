use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::domain::{RuntimeInfo, SidecarStatus, WorkspaceInfo};
use crate::settings;
use crate::storage::assets::{self, AssetItem, ScanSummary};
use crate::storage::chat::{self, ChatMessage, NodeChatContext};
use crate::storage::graph::{self, CreateNodeInput, GraphSnapshot, UpdateNodeInput};
use crate::storage::sources::{self as stored_sources, SaveSourceInput, SavedSource};
use crate::storage::{self, now_iso};
use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendNodeMessageInput {
    pub node_id: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationReply {
    pub messages: Vec<ChatMessage>,
    pub model: String,
    pub usage: Option<serde_json::Value>,
}

#[tauri::command]
pub fn runtime_info(state: State<'_, AppState>) -> RuntimeInfo {
    let workspace = state.workspace.lock().ok().and_then(|guard| guard.clone());

    let sidecar = match state.sidecar.lock() {
        Ok(mut manager) => manager.status(),
        Err(_) => unavailable_sidecar("Sidecar 管理器状态不可用，请重启应用。"),
    };

    RuntimeInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: tauri::VERSION.to_string(),
        python_required: "3.13.x".to_string(),
        sidecar,
        workspace,
    }
}

#[tauri::command]
pub fn open_workspace(path: String, state: State<'_, AppState>) -> Result<WorkspaceInfo, String> {
    let workspace = storage::open_workspace(Path::new(&path)).map_err(|error| error.to_string())?;

    let mut guard = state
        .workspace
        .lock()
        .map_err(|_| "应用状态不可用，请重启应用。".to_string())?;
    *guard = Some(workspace.clone());

    Ok(workspace)
}

#[tauri::command]
pub fn close_workspace(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state
        .workspace
        .lock()
        .map_err(|_| "应用状态不可用，请重启应用。".to_string())?;
    *guard = None;
    Ok(())
}

#[tauri::command]
pub fn sidecar_self_check(state: State<'_, AppState>) -> SidecarStatus {
    match state.sidecar.lock() {
        Ok(mut manager) => manager.status(),
        Err(_) => unavailable_sidecar("Sidecar 管理器状态不可用，请重启应用。"),
    }
}

#[tauri::command]
pub fn get_graph(state: State<'_, AppState>) -> Result<GraphSnapshot, String> {
    let workspace = current_workspace(&state)?;
    graph::load_graph(&workspace)
}

#[tauri::command]
pub fn create_node(
    input: CreateNodeInput,
    state: State<'_, AppState>,
) -> Result<GraphSnapshot, String> {
    let workspace = current_workspace(&state)?;
    graph::create_node(&workspace, input)
}

#[tauri::command]
pub fn move_node(
    node_id: String,
    x: f64,
    y: f64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let workspace = current_workspace(&state)?;
    graph::move_node(&workspace, &node_id, x, y)
}

#[tauri::command]
pub fn create_edge(
    source: String,
    target: String,
    kind: String,
    state: State<'_, AppState>,
) -> Result<GraphSnapshot, String> {
    let workspace = current_workspace(&state)?;
    graph::create_edge(&workspace, &source, &target, &kind)
}

#[tauri::command]
pub fn update_node(
    input: UpdateNodeInput,
    state: State<'_, AppState>,
) -> Result<GraphSnapshot, String> {
    let workspace = current_workspace(&state)?;
    graph::update_node(&workspace, input)
}

#[tauri::command]
pub fn delete_node(node_id: String, state: State<'_, AppState>) -> Result<GraphSnapshot, String> {
    let workspace = current_workspace(&state)?;
    graph::delete_node(&workspace, &node_id)
}

#[tauri::command]
pub fn delete_edge(edge_id: String, state: State<'_, AppState>) -> Result<GraphSnapshot, String> {
    let workspace = current_workspace(&state)?;
    graph::delete_edge(&workspace, &edge_id)
}

#[tauri::command]
pub fn save_viewport(x: f64, y: f64, zoom: f64, state: State<'_, AppState>) -> Result<(), String> {
    let workspace = current_workspace(&state)?;
    graph::save_viewport(&workspace, x, y, zoom)
}

#[tauri::command]
pub async fn scan_workspace(state: State<'_, AppState>) -> Result<ScanSummary, String> {
    let workspace = current_workspace(&state)?;
    tauri::async_runtime::spawn_blocking(move || assets::scan_workspace(&workspace))
        .await
        .map_err(|error| format!("扫描任务失败：{error}"))?
}

#[tauri::command]
pub fn list_assets(
    query: Option<String>,
    kind: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<AssetItem>, String> {
    let workspace = current_workspace(&state)?;
    assets::list_assets(&workspace, query.as_deref(), kind.as_deref())
}

#[tauri::command]
pub fn get_asset_text(
    asset_id: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let workspace = current_workspace(&state)?;
    assets::get_asset_text(&workspace, &asset_id, limit)
}

#[tauri::command]
pub fn save_academic_source(
    input: SaveSourceInput,
    state: State<'_, AppState>,
) -> Result<SavedSource, String> {
    let workspace = current_workspace(&state)?;
    stored_sources::save_source(&workspace, input)
}

#[tauri::command]
pub fn list_saved_sources(
    node_id: Option<String>,
    query: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<SavedSource>, String> {
    let workspace = current_workspace(&state)?;
    stored_sources::list_sources(&workspace, node_id.as_deref(), query.as_deref())
}

#[tauri::command]
pub fn remove_saved_source(
    node_id: String,
    source_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let workspace = current_workspace(&state)?;
    stored_sources::remove_source_link(&workspace, &node_id, &source_id)
}

#[tauri::command]
pub fn delete_saved_source(source_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let workspace = current_workspace(&state)?;
    stored_sources::delete_source(&workspace, &source_id)
}

#[tauri::command]
pub async fn search_academic_sources(
    app: tauri::AppHandle,
    query: String,
    year_from: Option<i32>,
    year_to: Option<i32>,
) -> Result<serde_json::Value, String> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Err("检索关键词不能为空。".to_string());
    }
    if query.chars().count() > 200 {
        return Err("检索关键词过长。".to_string());
    }

    let settings = settings::load_settings_for_app(&app)?;
    let sources = settings::academic_sources_payload(&settings);
    if sources.is_empty() {
        return Err("没有启用的学术来源，请先在设置中启用。".to_string());
    }

    let payload = serde_json::json!({
        "query": query,
        "maxResults": 5,
        "yearFrom": year_from,
        "yearTo": year_to,
        "sources": sources,
        "network": settings::network_payload(&settings)
    });

    let app_for_blocking = app.clone();
    let reply = tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_blocking.state::<AppState>();
        let mut manager = state
            .sidecar
            .lock()
            .map_err(|_| "Sidecar 状态不可用，请重启应用。".to_string())?;
        manager.post_json("/sources/search", &payload)
    })
    .await
    .map_err(|error| format!("检索任务失败：{error}"))??;

    serde_json::from_str(&reply).map_err(|error| format!("检索结果无法解析：{error}"))
}

#[tauri::command]
pub fn get_conversation(
    node_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChatMessage>, String> {
    let workspace = current_workspace(&state)?;
    chat::node_context(&workspace, &node_id)?;
    chat::load_messages(&workspace, &node_id, 50)
}

#[tauri::command]
pub async fn send_node_message(
    app: tauri::AppHandle,
    input: SendNodeMessageInput,
) -> Result<ConversationReply, String> {
    let workspace = workspace_for_app(&app)?;
    let message = input.message.trim().to_string();
    if message.is_empty() {
        return Err("消息不能为空。".to_string());
    }
    if message.chars().count() > 8_000 {
        return Err("消息过长，请缩短到 8000 字以内。".to_string());
    }

    let context = chat::node_context(&workspace, &input.node_id)?;
    let settings = settings::load_settings_for_app(&app)?;
    let resolved = settings::resolve_role(&settings, "research")?;

    chat::append_message(&workspace, &input.node_id, "user", &message)?;
    let history = chat::load_messages(&workspace, &input.node_id, 12)?;
    let saved_sources = stored_sources::linked_sources_for_chat(&workspace, &input.node_id, 20)?;
    let system = system_prompt(&context, &saved_sources);
    let mut messages = vec![serde_json::json!({
        "role": "system",
        "content": system
    })];
    for item in &history {
        messages.push(serde_json::json!({
            "role": item.role,
            "content": item.body
        }));
    }

    let model_id = resolved.model.model_id.clone();
    let payload = serde_json::json!({
        "provider": {
            "baseUrl": resolved.provider.base_url.clone(),
            "protocol": resolved.provider.protocol.clone(),
            "proxy": resolved.provider.proxy.clone(),
            "timeoutSeconds": resolved.provider.timeout_seconds
        },
        "modelId": model_id.clone(),
        "messages": messages,
        "apiKey": resolved.api_key.clone(),
        "maxTokens": 2048,
        "temperature": 0.4
    });

    let app_for_blocking = app.clone();
    let reply_text = tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_blocking.state::<AppState>();
        let mut manager = state
            .sidecar
            .lock()
            .map_err(|_| "Sidecar 状态不可用，请重启应用。".to_string())?;
        manager.post_json("/chat", &payload)
    })
    .await
    .map_err(|error| format!("模型请求任务失败：{error}"))??;

    let parsed: serde_json::Value =
        serde_json::from_str(&reply_text).map_err(|error| format!("模型响应无法解析：{error}"))?;

    if parsed.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        let message = parsed
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("模型调用失败。");
        return Err(message.to_string());
    }

    let content = parsed
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if content.is_empty() {
        return Err("模型返回了空回复。".to_string());
    }

    chat::append_message(&workspace, &input.node_id, "assistant", &content)?;
    let messages = chat::load_messages(&workspace, &input.node_id, 50)?;
    let model = parsed
        .get("model")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or(model_id);

    Ok(ConversationReply {
        messages,
        model,
        usage: parsed.get("usage").cloned(),
    })
}

fn system_prompt(context: &NodeChatContext, sources: &[SavedSource]) -> String {
    let type_label = match context.node_type.as_str() {
        "root" => "总课题",
        "research" => "研究",
        "experiment" => "实验",
        "report" => "报告",
        other => other,
    };
    let goal = if context.goal.trim().is_empty() {
        "尚未填写".to_string()
    } else {
        context.goal.clone()
    };

    let mut prompt = format!(
        "你是澄思学术的科研伙伴，正在与用户协作研究当前节点。\n\n\
         当前节点：{type_label}「{title}」\n节点目标：{goal}\n\n\
         工作规则：\n\
         1. 不编造文献、DOI、作者、页码、实验结果或数据。\n\
         2. 没有证据时明确说明“尚未验证/证据不足”，不要把推断写成事实。\n\
         3. 回答简洁、可执行，优先给出下一步建议。\n\
         4. 需要外部检索时说明将使用哪些来源；本轮不要伪造检索结果。\n\
         5. 使用中文回答。",
        title = context.title,
    );

    if !sources.is_empty() {
        prompt.push_str("\n\n已收藏来源（只能引用这些元数据，不得编造其他文献）：\n");
        for (index, source) in sources.iter().enumerate() {
            let authors = if source.authors.is_empty() {
                "作者未知".to_string()
            } else {
                source.authors.join("、")
            };
            let year = source
                .year
                .map(|value| value.to_string())
                .unwrap_or_else(|| "年份未知".to_string());
            let venue = source.venue.as_deref().unwrap_or("");
            let doi = source.doi.as_deref().unwrap_or("无 DOI");
            let coverage = match source.coverage.as_str() {
                "abstract_only" => "仅摘要",
                "fulltext" => "已获全文",
                "unavailable" => "不可获取",
                _ => "仅题录",
            };
            prompt.push_str(&format!(
                "{}. {}（{}）. {} {}. {}. 覆盖：{}。\n",
                index + 1,
                authors,
                year,
                source.title,
                venue,
                doi,
                coverage
            ));
        }
        prompt.push_str("引用已收藏来源时必须说明覆盖级别；不要为仅题录或仅摘要的来源生成页码。\n");
    }

    prompt
}

fn workspace_for_app(app: &tauri::AppHandle) -> Result<WorkspaceInfo, String> {
    let state = app.state::<AppState>();
    let guard = state
        .workspace
        .lock()
        .map_err(|_| "应用状态不可用，请重启应用。".to_string())?;
    guard
        .clone()
        .ok_or_else(|| "尚未打开项目，请先在项目页选择本地文件夹。".to_string())
}

fn current_workspace(state: &State<'_, AppState>) -> Result<WorkspaceInfo, String> {
    let guard = state
        .workspace
        .lock()
        .map_err(|_| "应用状态不可用，请重启应用。".to_string())?;
    guard
        .clone()
        .ok_or_else(|| "尚未打开项目，请先在项目页选择本地文件夹。".to_string())
}

fn unavailable_sidecar(message: &str) -> SidecarStatus {
    SidecarStatus {
        ok: false,
        mode: "supervised".to_string(),
        python: None,
        version: None,
        port: None,
        pid: None,
        restarts: 0,
        message: message.to_string(),
        checked_at: now_iso(),
    }
}
