use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::domain::{RuntimeInfo, SidecarStatus, WorkspaceInfo};
use crate::settings;
use crate::storage::asset_links::{self as asset_links, LinkedAsset};
use crate::storage::assets::{self, AssetItem, ScanSummary};
use crate::storage::chat::{self, ChatMessage, NodeChatContext};
use crate::storage::definition::{
    self as definition, ResearchDefinition, ResearchDefinitionContent,
};
use crate::storage::experiments::{self as experiments, ExperimentRun, NewExperimentRun};
use crate::storage::graph::{self, CreateNodeInput, GraphSnapshot, UpdateNodeInput};
use crate::storage::proposals::{self as proposals, ProposalEdge, ProposalNode, ProposalView};
use crate::storage::reports::{
    self as reports, ReportChange, ReportChangeProposalView, ReportDocument, ReportRevision,
};
use crate::storage::sources::{self as stored_sources, SaveSourceInput, SavedSource};
use crate::storage::{self, now_iso};
use crate::AppState;

const REPORT_REVISION_SYSTEM_PROMPT: &str = "你是学术报告修订助手。只提出局部修改，不要重写整篇。\
    只输出一个 JSON 对象，不要 Markdown 代码块：\
    {\"summary\":\"一句话说明\",\"changes\":[{\"id\":\"c1\",\"oldText\":\"正文中逐字存在的原文片段\",\"newText\":\"替换文本\",\"reason\":\"理由\"}]}。\
    规则：oldText 必须逐字来自正文；每处尽量短；最多 5 项；不得编造引用、页码、数据或结论；保留用户已有的事实表述。";

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportChangesApplied {
    pub document: ReportDocument,
    pub proposal: ReportChangeProposalView,
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
pub fn link_asset_to_node(
    node_id: String,
    asset_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let workspace = current_workspace(&state)?;
    asset_links::link_asset(&workspace, &node_id, &asset_id)
}

#[tauri::command]
pub fn unlink_asset_from_node(
    node_id: String,
    asset_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let workspace = current_workspace(&state)?;
    asset_links::unlink_asset(&workspace, &node_id, &asset_id)
}

#[tauri::command]
pub fn list_node_assets(
    node_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<LinkedAsset>, String> {
    let workspace = current_workspace(&state)?;
    asset_links::list_linked_assets(&workspace, &node_id)
}

#[tauri::command]
pub fn create_project_backup(state: State<'_, AppState>) -> Result<String, String> {
    let workspace = current_workspace(&state)?;
    storage::backup::create_backup(&workspace)
}

#[tauri::command]
pub fn restore_project_backup(
    archive_path: String,
    destination: String,
    state: State<'_, AppState>,
) -> Result<WorkspaceInfo, String> {
    if archive_path.trim().is_empty() || destination.trim().is_empty() {
        return Err("请提供备份文件与目标文件夹。".to_string());
    }
    let restored =
        storage::backup::restore_backup(Path::new(&archive_path), Path::new(&destination))?;
    let mut guard = state
        .workspace
        .lock()
        .map_err(|_| "应用状态不可用，请重启应用。".to_string())?;
    *guard = Some(restored.clone());
    Ok(restored)
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
pub fn get_research_definition(state: State<'_, AppState>) -> Result<ResearchDefinition, String> {
    let workspace = current_workspace(&state)?;
    definition::get_definition(&workspace)
}

#[tauri::command]
pub fn save_research_definition(
    content: ResearchDefinitionContent,
    confirmed_fields: Vec<String>,
    base_revision: i64,
    state: State<'_, AppState>,
) -> Result<ResearchDefinition, String> {
    let workspace = current_workspace(&state)?;
    definition::save_definition(&workspace, content, confirmed_fields, base_revision)
}

#[tauri::command]
pub async fn run_experiment(
    app: tauri::AppHandle,
    node_id: String,
    asset_id: String,
    goal: String,
) -> Result<ExperimentRun, String> {
    let workspace = workspace_for_app(&app)?;
    experiments::ensure_experiment_node(&workspace, &node_id)?;

    let (relative, _kind, status) = assets::asset_for_run(&workspace, &asset_id)?;
    if status == "missing" {
        return Err("数据资产的原文件已不可用，请重新扫描项目。".to_string());
    }
    let extension = Path::new(&relative)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if !matches!(extension.as_str(), "csv" | "tsv") {
        return Err("首版实验只支持 CSV / TSV 数据分析。".to_string());
    }

    let root = dunce::canonicalize(&workspace.workspace_path)
        .map_err(|error| format!("无法访问 Workspace：{error}"))?;
    let candidate = dunce::canonicalize(root.join(&relative))
        .map_err(|error| format!("无法访问数据文件：{error}"))?;
    if !candidate.starts_with(&root) {
        return Err("数据文件越出了当前 Workspace。".to_string());
    }

    let goal = if goal.trim().is_empty() {
        "分析数据的主要特征".to_string()
    } else {
        goal.trim().to_string()
    };
    let payload = serde_json::json!({
        "path": candidate.display().to_string(),
        "goal": goal,
    });

    let app_for_blocking = app.clone();
    let reply = tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_blocking.state::<AppState>();
        let mut manager = state
            .sidecar
            .lock()
            .map_err(|_| "Sidecar 状态不可用，请重启应用。".to_string())?;
        manager.post_json("/experiments/run", &payload)
    })
    .await
    .map_err(|error| format!("实验任务失败：{error}"))?;

    let (outputs, run_state, error) = match reply {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => {
                let ok = value
                    .get("ok")
                    .and_then(|item| item.as_bool())
                    .unwrap_or(false);
                let message = value
                    .get("message")
                    .and_then(|item| item.as_str())
                    .map(str::to_string);
                (
                    value,
                    if ok { "completed" } else { "failed" },
                    if ok {
                        None
                    } else {
                        message.or_else(|| Some("实验分析失败。".to_string()))
                    },
                )
            }
            Err(parse_error) => (
                serde_json::Value::Null,
                "failed",
                Some(format!("实验结果无法解析：{parse_error}")),
            ),
        },
        Err(sidecar_error) => (serde_json::Value::Null, "failed", Some(sidecar_error)),
    };

    let parameters = serde_json::json!({
        "method": "builtin_csv_summary",
        "assetId": asset_id,
    });
    experiments::save_run(
        &workspace,
        NewExperimentRun {
            node_id: &node_id,
            asset_id: Some(&asset_id),
            input_path: Some(&relative),
            goal: &goal,
            parameters: &parameters,
            state: run_state,
            outputs: &outputs,
            error: error.as_deref(),
        },
    )
}

#[tauri::command]
pub fn list_experiment_runs(
    node_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ExperimentRun>, String> {
    let workspace = current_workspace(&state)?;
    experiments::ensure_experiment_node(&workspace, &node_id)?;
    experiments::list_runs(&workspace, &node_id)
}

#[tauri::command]
pub async fn propose_decomposition(
    app: tauri::AppHandle,
    node_id: String,
) -> Result<ProposalView, String> {
    let workspace = workspace_for_app(&app)?;
    let context = chat::node_context(&workspace, &node_id)?;
    let graph = graph::load_graph(&workspace)?;
    if !matches!(context.node_type.as_str(), "root" | "research") {
        return Err("智能分解只能在总课题或研究节点上使用。".to_string());
    }

    let settings = settings::load_settings_for_app(&app)?;
    let resolved = settings::resolve_role(&settings, "planning")?;
    let existing: Vec<serde_json::Value> = graph
        .nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "id": node.id,
                "type": node.node_type,
                "title": node.title,
                "goal": node.goal
            })
        })
        .collect();

    let system = "你是科研图规划助手。基于当前节点目标和已有节点，提出 2-5 个尚未覆盖的独立研究工作。\
        只输出一个 JSON 对象，不要 Markdown 代码块，不要解释。格式：\
        {\"rationale\":\"一句话理由\",\"nodes\":[{\"clientId\":\"n1\",\"type\":\"research|experiment|report\",\"title\":\"名称\",\"goal\":\"目标\"}],\
        \"edges\":[{\"sourceId\":\"当前节点ID或clientId\",\"targetId\":\"clientId\",\"kind\":\"decomposition|association\"}]}。\
        规则：不要重复已有节点；新增节点必须有明确研究目标；分解关系从当前节点指向新研究；实验节点只用于需要独立验证的目标。";
    let user = format!(
        "当前节点：{type_label}「{title}」\n节点目标：{goal}\n当前节点 ID：{node_id}\n已有节点：{existing}",
        type_label = match context.node_type.as_str() {
            "root" => "总课题",
            "research" => "研究",
            other => other,
        },
        title = context.title,
        goal = if context.goal.trim().is_empty() {
            "尚未填写"
        } else {
            context.goal.as_str()
        },
        existing = serde_json::to_string(&existing)
            .map_err(|error| format!("序列化已有节点失败：{error}"))?
    );

    let model_id = resolved.model.model_id.clone();
    let payload = serde_json::json!({
        "provider": {
            "baseUrl": resolved.provider.base_url.clone(),
            "protocol": resolved.provider.protocol.clone(),
            "proxy": resolved.provider.proxy.clone(),
            "timeoutSeconds": resolved.provider.timeout_seconds
        },
        "modelId": model_id,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "apiKey": resolved.api_key.clone(),
        "maxTokens": 1200,
        "temperature": 0.3
    });

    let app_for_blocking = app.clone();
    let reply = tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_blocking.state::<AppState>();
        let mut manager = state
            .sidecar
            .lock()
            .map_err(|_| "Sidecar 状态不可用，请重启应用。".to_string())?;
        manager.post_json("/chat", &payload)
    })
    .await
    .map_err(|error| format!("智能分解任务失败：{error}"))??;

    let parsed: serde_json::Value =
        serde_json::from_str(&reply).map_err(|error| format!("模型响应无法解析：{error}"))?;
    if parsed.get("ok").and_then(|item| item.as_bool()) != Some(true) {
        let message = parsed
            .get("message")
            .and_then(|item| item.as_str())
            .unwrap_or("模型未能生成智能分解提案。");
        return Err(message.to_string());
    }
    let content = parsed
        .get("content")
        .and_then(|item| item.as_str())
        .unwrap_or("");
    let proposal_json = extract_json_object(content)
        .ok_or_else(|| "模型没有返回可解析的提案 JSON。".to_string())?;

    let mut nodes: Vec<ProposalNode> = Vec::new();
    let mut client_ids = std::collections::HashSet::new();
    if let Some(items) = proposal_json.get("nodes").and_then(|item| item.as_array()) {
        for (index, item) in items.iter().enumerate().take(6) {
            let node_type = item
                .get("type")
                .and_then(|value| value.as_str())
                .unwrap_or("research");
            if !matches!(node_type, "research" | "experiment" | "report") {
                continue;
            }
            let title = item
                .get("title")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim();
            if title.is_empty() {
                continue;
            }
            let mut client_id = item
                .get("clientId")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if client_id.is_empty() {
                client_id = format!("n{}", index + 1);
            }
            while !client_ids.insert(client_id.clone()) {
                client_id = format!("{client_id}_{}", index + 1);
            }
            let goal = item
                .get("goal")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            nodes.push(ProposalNode {
                client_id,
                node_type: node_type.to_string(),
                title: title.to_string(),
                goal,
            });
        }
    }
    if nodes.is_empty() {
        return Err("模型没有给出可用的候选节点，请重试。".to_string());
    }

    let mut edges: Vec<ProposalEdge> = Vec::new();
    if let Some(items) = proposal_json.get("edges").and_then(|item| item.as_array()) {
        for item in items {
            let kind = item
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or("decomposition");
            if !matches!(kind, "decomposition" | "association") {
                continue;
            }
            let source = item
                .get("sourceId")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let target = item
                .get("targetId")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if source.is_empty() || target.is_empty() {
                continue;
            }
            let source = if source == "anchor" || source == "root" {
                node_id.clone()
            } else {
                source
            };
            if !nodes.iter().any(|node| node.client_id == target)
                && !graph.nodes.iter().any(|node| node.id == target)
            {
                continue;
            }
            edges.push(ProposalEdge {
                source_id: source,
                target_id: target,
                kind: kind.to_string(),
            });
        }
    }
    if edges.is_empty() {
        for node in &nodes {
            edges.push(ProposalEdge {
                source_id: node_id.clone(),
                target_id: node.client_id.clone(),
                kind: "decomposition".to_string(),
            });
        }
    }

    let rationale = proposal_json
        .get("rationale")
        .and_then(|value| value.as_str())
        .unwrap_or("基于当前节点目标和已有科研图结构生成候选研究工作。")
        .to_string();

    proposals::insert_proposal(
        &workspace,
        &node_id,
        graph.graph_revision,
        &rationale,
        nodes,
        edges,
    )
}

#[tauri::command]
pub fn confirm_proposal(
    proposal_id: String,
    nodes: Vec<ProposalNode>,
    edges: Vec<ProposalEdge>,
    state: State<'_, AppState>,
) -> Result<GraphSnapshot, String> {
    let workspace = current_workspace(&state)?;
    proposals::apply_proposal(&workspace, &proposal_id, nodes, edges)
}

#[tauri::command]
pub async fn suggest_report_changes(
    app: tauri::AppHandle,
    node_id: String,
    instruction: String,
) -> Result<ReportChangeProposalView, String> {
    let workspace = workspace_for_app(&app)?;
    reports::ensure_report_node(&workspace, &node_id)?;
    let report = reports::get_report(&workspace, &node_id)?;
    if report.content_md.trim().is_empty() {
        return Err("请先写入并保存报告正文。".to_string());
    }
    let instruction = instruction.trim();
    if instruction.is_empty() {
        return Err("请说明希望如何修订报告。".to_string());
    }

    let settings = settings::load_settings_for_app(&app)?;
    let resolved = settings::resolve_role(&settings, "report")?;
    let content: String = report.content_md.chars().take(16_000).collect();
    let system = REPORT_REVISION_SYSTEM_PROMPT;
    let user = format!("修订要求：{instruction}\n\n报告正文：\n{content}");

    let model_id = resolved.model.model_id.clone();
    let payload = serde_json::json!({
        "provider": {
            "baseUrl": resolved.provider.base_url.clone(),
            "protocol": resolved.provider.protocol.clone(),
            "proxy": resolved.provider.proxy.clone(),
            "timeoutSeconds": resolved.provider.timeout_seconds
        },
        "modelId": model_id,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "apiKey": resolved.api_key.clone(),
        "maxTokens": 2000,
        "temperature": 0.2
    });

    let app_for_blocking = app.clone();
    let reply = tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_blocking.state::<AppState>();
        let mut manager = state
            .sidecar
            .lock()
            .map_err(|_| "Sidecar 状态不可用，请重启应用。".to_string())?;
        manager.post_json("/chat", &payload)
    })
    .await
    .map_err(|error| format!("报告修订任务失败：{error}"))??;

    let parsed: serde_json::Value =
        serde_json::from_str(&reply).map_err(|error| format!("模型响应无法解析：{error}"))?;
    if parsed.get("ok").and_then(|item| item.as_bool()) != Some(true) {
        let message = parsed
            .get("message")
            .and_then(|item| item.as_str())
            .unwrap_or("模型未能生成修改建议。");
        return Err(message.to_string());
    }
    let content_text = parsed
        .get("content")
        .and_then(|item| item.as_str())
        .unwrap_or("");
    let proposal_json = extract_json_object(content_text)
        .ok_or_else(|| "模型没有返回可解析的修改提案。".to_string())?;

    let summary = proposal_json
        .get("summary")
        .and_then(|item| item.as_str())
        .unwrap_or("根据当前正文提出局部修改。")
        .to_string();

    let mut changes: Vec<ReportChange> = Vec::new();
    if let Some(items) = proposal_json
        .get("changes")
        .and_then(|item| item.as_array())
    {
        for (index, item) in items.iter().enumerate() {
            if changes.len() >= 5 {
                break;
            }
            let old_text = item
                .get("oldText")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let new_text = item
                .get("newText")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            if old_text.trim().is_empty()
                || new_text.trim().is_empty()
                || old_text == new_text
                || !report.content_md.contains(&old_text)
            {
                continue;
            }
            let id = item
                .get("id")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("c{}", index + 1));
            changes.push(ReportChange {
                id,
                old_text,
                new_text,
                reason: item
                    .get("reason")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
                review: "pending".to_string(),
            });
        }
    }

    if changes.is_empty() {
        return Err("模型没有给出可应用的局部修改，请调整修订要求后重试。".to_string());
    }

    reports::insert_report_changes(&workspace, &node_id, report.revision, &summary, changes)
}

#[tauri::command]
pub fn apply_report_changes(
    node_id: String,
    proposal_id: String,
    accepted_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<ReportChangesApplied, String> {
    let workspace = current_workspace(&state)?;
    let (document, proposal) =
        reports::apply_report_changes(&workspace, &node_id, &proposal_id, &accepted_ids)?;
    Ok(ReportChangesApplied { document, proposal })
}

#[tauri::command]
pub fn get_report(node_id: String, state: State<'_, AppState>) -> Result<ReportDocument, String> {
    let workspace = current_workspace(&state)?;
    reports::ensure_report_node(&workspace, &node_id)?;
    reports::get_report(&workspace, &node_id)
}

#[tauri::command]
pub fn save_report(
    node_id: String,
    content_md: String,
    base_revision: i64,
    state: State<'_, AppState>,
) -> Result<ReportDocument, String> {
    let workspace = current_workspace(&state)?;
    reports::save_report(&workspace, &node_id, &content_md, base_revision)
}

#[tauri::command]
pub fn list_report_revisions(
    node_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReportRevision>, String> {
    let workspace = current_workspace(&state)?;
    reports::ensure_report_node(&workspace, &node_id)?;
    reports::list_revisions(&workspace, &node_id)
}

#[tauri::command]
pub fn export_report_markdown(
    node_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let workspace = current_workspace(&state)?;
    reports::ensure_report_node(&workspace, &node_id)?;
    reports::export_markdown(&workspace, &node_id)
}

#[tauri::command]
pub async fn export_report_document(
    app: tauri::AppHandle,
    node_id: String,
    format: String,
) -> Result<String, String> {
    let workspace = workspace_for_app(&app)?;
    reports::ensure_report_node(&workspace, &node_id)?;
    let format = format.to_lowercase();
    let (content, title, path) = reports::export_target(&workspace, &node_id, &format)?;

    let payload = serde_json::json!({
        "content": content,
        "title": title,
        "format": format,
        "path": path,
    });
    let app_for_blocking = app.clone();
    let reply = tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_blocking.state::<AppState>();
        let mut manager = state
            .sidecar
            .lock()
            .map_err(|_| "Sidecar 状态不可用，请重启应用。".to_string())?;
        manager.post_json("/reports/export", &payload)
    })
    .await
    .map_err(|error| format!("报告导出任务失败：{error}"))??;

    let parsed: serde_json::Value =
        serde_json::from_str(&reply).map_err(|error| format!("报告导出响应无法解析：{error}"))?;
    if parsed.get("ok").and_then(|item| item.as_bool()) != Some(true) {
        let message = parsed
            .get("message")
            .and_then(|item| item.as_str())
            .unwrap_or("报告导出失败。");
        return Err(message.to_string());
    }
    let exported = parsed
        .get("path")
        .and_then(|item| item.as_str())
        .map(str::to_string)
        .unwrap_or(path);
    if !Path::new(&exported).is_file() {
        return Err("报告导出后未找到目标文件。".to_string());
    }
    Ok(exported)
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
pub async fn search_news(
    app: tauri::AppHandle,
    query: Option<String>,
    max_results: Option<u32>,
) -> Result<serde_json::Value, String> {
    let query = query.unwrap_or_default().trim().to_string();
    if query.chars().count() > 200 {
        return Err("检索关键词过长。".to_string());
    }

    let settings = settings::load_settings_for_app(&app)?;
    let sources = settings::news_sources_payload(&settings);
    if sources.is_empty() {
        return Err("没有启用的新闻来源，请先在设置中启用。".to_string());
    }

    let payload = serde_json::json!({
        "query": query,
        "maxResults": max_results.unwrap_or(12).min(30),
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
        manager.post_json("/news/search", &payload)
    })
    .await
    .map_err(|error| format!("资讯检索任务失败：{error}"))??;

    serde_json::from_str(&reply).map_err(|error| format!("资讯结果无法解析：{error}"))
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
    let linked_assets = asset_links::linked_excerpts(&workspace, &input.node_id, 4, 1_200)?;
    let system = system_prompt(&context, &saved_sources, &linked_assets);
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

fn system_prompt(
    context: &NodeChatContext,
    sources: &[SavedSource],
    linked_assets: &[(String, String)],
) -> String {
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

    if !linked_assets.is_empty() {
        prompt.push_str("\n本节点引用的本地资料（摘录可能不完整，不得声称已通读全文）：\n");
        for (index, (name, excerpt)) in linked_assets.iter().enumerate() {
            prompt.push_str(&format!(
                "{}. 《{}》：{}\n",
                index + 1,
                name,
                excerpt.replace('\n', " ")
            ));
        }
        prompt.push_str("只能基于上述摘录陈述本地资料内容；需要更多内容时请让用户打开原文。\n");
    }

    prompt
}

fn extract_json_object(content: &str) -> Option<serde_json::Value> {
    let start = content.find('{')?;
    let end = content.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&content[start..=end]).ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 手动烟雾测试：需要本机已配置 report 角色模型。
    /// 运行 `cargo test real_report_revision_smoke -- --ignored --nocapture`。
    #[test]
    #[ignore]
    fn real_report_revision_smoke() {
        let appdata =
            std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA available"));
        let path = appdata.join("com.chengsi.academic").join("settings.json");
        let settings: settings::GlobalSettings =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read settings"))
                .expect("parse settings");
        let resolved = settings::resolve_role(&settings, "report").expect("resolve report model");

        let report = "# 结论\n样本量偏小，结果具有普遍性。\n\n# 方法\n采用问卷调查收集数据。";
        let user = format!("修订要求：把结论写得更谨慎，不要夸大普遍性。\n\n报告正文：\n{report}");
        let payload = serde_json::json!({
            "provider": {
                "baseUrl": resolved.provider.base_url,
                "protocol": resolved.provider.protocol,
                "proxy": resolved.provider.proxy,
                "timeoutSeconds": resolved.provider.timeout_seconds
            },
            "modelId": resolved.model.model_id,
            "messages": [
                {"role": "system", "content": REPORT_REVISION_SYSTEM_PROMPT},
                {"role": "user", "content": user}
            ],
            "apiKey": resolved.api_key,
            "maxTokens": 2000,
            "temperature": 0.2
        });

        let mut manager = crate::sidecar::SidecarManager::default();
        let reply = manager.post_json("/chat", &payload).expect("sidecar chat");
        manager.shutdown();
        let value: serde_json::Value = serde_json::from_str(&reply).expect("parse reply");
        assert_eq!(
            value.get("ok").and_then(|item| item.as_bool()),
            Some(true),
            "reply={reply}"
        );
        let content = value
            .get("content")
            .and_then(|item| item.as_str())
            .unwrap_or("");
        let proposal = extract_json_object(content).expect("JSON proposal");
        let changes = proposal
            .get("changes")
            .and_then(|item| item.as_array())
            .expect("changes array");
        assert!(!changes.is_empty(), "content={content}");
        for change in changes {
            let old_text = change
                .get("oldText")
                .and_then(|item| item.as_str())
                .expect("oldText");
            assert!(report.contains(old_text), "oldText={old_text}");
        }
        println!("content={content}");
    }
}
