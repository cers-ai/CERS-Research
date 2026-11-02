use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::domain::{RuntimeInfo, SidecarStatus, WorkspaceInfo};
use crate::mcp::{McpServerStatus, McpTool};
use crate::settings;
use crate::skills::{self as skills, SkillInfo};
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

const REPORT_OUTLINE_SYSTEM_PROMPT: &str =
    "你是学术报告提纲助手。根据研究目标、已有来源与本地资料，
    生成一个可编辑的中文报告提纲。只输出一个 JSON 对象，不要 Markdown 代码块：
    {\"title\":\"报告标题\",\"sections\":[{\"heading\":\"章节标题\",\"points\":[\"要点\"]}]}。
    规则：2 到 8 个章节；每章最多 6 个要点；不编造引用、页码、数据或结论；
    仅题录或仅摘要的来源只能提示需要核实，不能写成已验证事实。";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendNodeMessageInput {
    pub node_id: String,
    pub message: String,
    #[serde(default)]
    pub deep_research: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelReadiness {
    pub configured: bool,
    pub role: String,
    pub provider_id: Option<String>,
    pub provider_name: Option<String>,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationReply {
    pub messages: Vec<ChatMessage>,
    pub model: String,
    pub usage: Option<serde_json::Value>,
    pub tool_trace: Vec<ToolEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolEvent {
    pub name: String,
    pub server: String,
    pub ok: bool,
    pub summary: String,
    pub duration_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportChangesApplied {
    pub document: ReportDocument,
    pub proposal: ReportChangeProposalView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineSection {
    pub heading: String,
    #[serde(default)]
    pub points: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportOutline {
    pub title: String,
    pub sections: Vec<OutlineSection>,
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
pub fn model_readiness(app: tauri::AppHandle) -> ModelReadiness {
    let role = "research";
    match settings::load_settings_for_app(&app)
        .and_then(|settings| settings::resolve_role(&settings, role))
    {
        Ok(resolved) => ModelReadiness {
            configured: true,
            role: role.to_string(),
            provider_id: Some(resolved.provider.id),
            provider_name: Some(resolved.provider.name),
            model_id: Some(resolved.model.model_id),
            model_name: Some(resolved.model.display_name),
            message: "模型已就绪。".to_string(),
        },
        Err(error) => ModelReadiness {
            configured: false,
            role: role.to_string(),
            provider_id: None,
            provider_name: None,
            model_id: None,
            model_name: None,
            message: error,
        },
    }
}

#[tauri::command]
pub async fn check_for_updates(
    app: tauri::AppHandle,
    force: Option<bool>,
) -> Result<crate::updates::UpdateInfo, String> {
    let app_for_blocking = app.clone();
    let force = force.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        crate::updates::check_for_updates(&app_for_blocking, force)
    })
    .await
    .map_err(|error| format!("更新检查任务失败：{error}"))?
}

#[tauri::command]
pub fn skip_update_version(app: tauri::AppHandle, version: String) -> Result<(), String> {
    crate::updates::skip_version(&app, &version)
}

#[tauri::command]
pub fn set_auto_update_check(
    app: tauri::AppHandle,
    auto_check: bool,
) -> Result<settings::GlobalSettings, String> {
    crate::updates::set_auto_check(&app, auto_check)
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
    let saved = definition::save_definition(&workspace, content, confirmed_fields, base_revision)?;
    let root_goal = saved
        .content
        .goal
        .clone()
        .or_else(|| saved.content.question.clone())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(goal) = root_goal {
        let graph = graph::load_graph(&workspace)?;
        if let Some(root) = graph
            .nodes
            .iter()
            .find(|node| node.node_type == "root")
            .cloned()
        {
            if root.goal != goal {
                graph::update_node(
                    &workspace,
                    UpdateNodeInput {
                        node_id: root.id,
                        title: root.title,
                        goal: Some(goal),
                    },
                )?;
            }
        }
    }
    Ok(saved)
}

#[tauri::command]
pub async fn run_experiment(
    app: tauri::AppHandle,
    node_id: String,
    asset_id: String,
    goal: String,
    target: Option<String>,
    predictors: Option<Vec<String>>,
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
        "target": target,
        "predictors": predictors,
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

    let project_definition = definition::get_definition(&workspace).ok();
    let mut definition_parts = Vec::new();
    if let Some(definition) = project_definition.as_ref() {
        if let Some(question) = definition
            .content
            .question
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            definition_parts.push(format!("研究问题：{question}"));
        }
        if let Some(goal) = definition
            .content
            .goal
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            definition_parts.push(format!("研究目标：{goal}"));
        }
        if let Some(scope) = definition
            .content
            .scope
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            definition_parts.push(format!("范围与边界：{scope}"));
        }
        if let Some(approach) = definition
            .content
            .approach
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            definition_parts.push(format!("方法方向：{approach}"));
        }
    }
    let definition_text = if definition_parts.is_empty() {
        "尚未形成研究定义；请基于节点目标拆解。".to_string()
    } else {
        definition_parts.join(
            "
",
        )
    };

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
        规则：优先遵循项目研究定义，不要重复已有节点；新增节点必须有明确研究目标；         分解关系从当前节点指向新研究；实验节点只用于能用 CSV 数据做统计分析或机器学习验证的目标；访谈、问卷设计、政策文本比较等应创建研究节点。";
    let user = format!(
        "项目研究定义：\n{definition_text}\n\n当前节点：{type_label}「{title}」\n节点目标：{goal}\n当前节点 ID：{node_id}\n已有节点：{existing}",
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
        "maxTokens": 4000,
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
pub async fn suggest_report_outline(
    app: tauri::AppHandle,
    node_id: String,
) -> Result<ReportOutline, String> {
    let workspace = workspace_for_app(&app)?;
    let context = chat::node_context(&workspace, &node_id)?;
    if context.node_type != "report" {
        return Err("只有报告节点可以生成提纲。".to_string());
    }

    let definition = definition::get_definition(&workspace)?;
    let sources = stored_sources::linked_sources_for_chat(&workspace, &node_id, 6)?;
    let assets = asset_links::list_linked_assets(&workspace, &node_id)?;

    let mut brief = format!(
        "报告节点：{}
节点目标：{}
",
        context.title,
        if context.goal.trim().is_empty() {
            "尚未填写"
        } else {
            context.goal.trim()
        }
    );
    if let Some(question) = definition
        .content
        .question
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        brief.push_str(&format!(
            "研究问题：{question}
"
        ));
    }
    if let Some(goal) = definition
        .content
        .goal
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        brief.push_str(&format!(
            "研究目标：{goal}
"
        ));
    }
    if let Some(approach) = definition
        .content
        .approach
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        brief.push_str(&format!(
            "研究路径：{approach}
"
        ));
    }
    if !sources.is_empty() {
        brief.push_str(
            "
已收藏来源：
",
        );
        for source in &sources {
            let coverage = match source.coverage.as_str() {
                "abstract_only" => "仅摘要",
                "fulltext" => "已获全文",
                "unavailable" => "不可获取",
                _ => "仅题录",
            };
            brief.push_str(&format!(
                "- {}（{}）· {}
",
                source.title,
                source
                    .year
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "年份未知".to_string()),
                coverage
            ));
        }
    }
    if !assets.is_empty() {
        brief.push_str(
            "
已引用本地资料：
",
        );
        for asset in assets.iter().take(6) {
            brief.push_str(&format!(
                "- {}（{}，{}）
",
                asset.name,
                asset.kind,
                if asset.has_text {
                    "有可读正文"
                } else {
                    "无正文索引"
                }
            ));
        }
    }

    let settings = settings::load_settings_for_app(&app)?;
    let resolved = settings::resolve_role(&settings, "report")?;
    let system = REPORT_OUTLINE_SYSTEM_PROMPT;
    let user = format!(
        "请为以下研究形成报告提纲：
{brief}"
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
        "maxTokens": 3000,
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
    .map_err(|error| format!("提纲任务失败：{error}"))??;

    let parsed: serde_json::Value =
        serde_json::from_str(&reply).map_err(|error| format!("模型响应无法解析：{error}"))?;
    if parsed.get("ok").and_then(|item| item.as_bool()) != Some(true) {
        let message = parsed
            .get("message")
            .and_then(|item| item.as_str())
            .unwrap_or("模型未能生成提纲。");
        return Err(message.to_string());
    }
    let content_text = parsed
        .get("content")
        .and_then(|item| item.as_str())
        .unwrap_or("");
    let outline_json = extract_json_object(content_text)
        .ok_or_else(|| "模型没有返回可解析的提纲。".to_string())?;

    let title = outline_json
        .get("title")
        .and_then(|item| item.as_str())
        .unwrap_or(&context.title)
        .trim()
        .to_string();
    let mut sections: Vec<OutlineSection> = Vec::new();
    if let Some(items) = outline_json
        .get("sections")
        .and_then(|item| item.as_array())
    {
        for item in items.iter().take(8) {
            let heading = item
                .get("heading")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if heading.is_empty() {
                continue;
            }
            let points = item
                .get("points")
                .and_then(|value| value.as_array())
                .map(|points| {
                    points
                        .iter()
                        .filter_map(|point| point.as_str())
                        .map(|point| point.trim().to_string())
                        .filter(|point| !point.is_empty())
                        .take(6)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            sections.push(OutlineSection { heading, points });
        }
    }
    if sections.len() < 2 {
        return Err("模型生成的提纲过短，请稍后重试。".to_string());
    }

    Ok(ReportOutline {
        title: if title.is_empty() {
            context.title
        } else {
            title
        },
        sections,
    })
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
        "maxTokens": 3000,
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
pub fn list_pending_report_changes(
    node_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReportChangeProposalView>, String> {
    let workspace = current_workspace(&state)?;
    reports::ensure_report_node(&workspace, &node_id)?;
    reports::list_pending_change_proposals(&workspace, &node_id)
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
pub async fn list_skills(app: tauri::AppHandle) -> Result<Vec<SkillInfo>, String> {
    let settings = settings::load_settings_for_app(&app)?;
    skills::list_skills(&app, &settings)
}

#[tauri::command]
pub async fn set_skill_enabled(
    app: tauri::AppHandle,
    skill_id: String,
    enabled: bool,
) -> Result<Vec<SkillInfo>, String> {
    skills::set_enabled(&app, &skill_id, enabled)
}

#[tauri::command]
pub async fn list_mcp_tools(app: tauri::AppHandle) -> Result<Vec<McpTool>, String> {
    let settings = settings::load_settings_for_app(&app)?;
    let app_for_blocking = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_blocking.state::<AppState>();
        let mut registry = state
            .mcp
            .lock()
            .map_err(|_| "MCP 状态不可用，请重启应用。".to_string())?;
        registry.tools(&settings.mcp_servers)
    })
    .await
    .map_err(|error| format!("MCP 工具列表任务失败：{error}"))?
}

#[tauri::command]
pub async fn test_mcp_server(
    app: tauri::AppHandle,
    server_id: String,
) -> Result<McpServerStatus, String> {
    let settings = settings::load_settings_for_app(&app)?;
    let config = settings
        .mcp_servers
        .iter()
        .find(|server| server.id == server_id)
        .cloned()
        .ok_or_else(|| "MCP 服务不存在。".to_string())?;
    let app_for_blocking = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_blocking.state::<AppState>();
        let mut registry = state
            .mcp
            .lock()
            .map_err(|_| "MCP 状态不可用，请重启应用。".to_string())?;
        Ok(registry.status(&config))
    })
    .await
    .map_err(|error| format!("MCP 连接测试任务失败：{error}"))?
}

#[tauri::command]
pub async fn call_mcp_tool(
    app: tauri::AppHandle,
    server_id: String,
    tool_name: String,
    arguments: serde_json::Value,
) -> Result<String, String> {
    let settings = settings::load_settings_for_app(&app)?;
    let config = settings
        .mcp_servers
        .iter()
        .find(|server| server.id == server_id)
        .cloned()
        .ok_or_else(|| "MCP 服务不存在。".to_string())?;
    let app_for_blocking = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_for_blocking.state::<AppState>();
        let mut registry = state
            .mcp
            .lock()
            .map_err(|_| "MCP 状态不可用，请重启应用。".to_string())?;
        registry.call(&config, &tool_name, arguments)
    })
    .await
    .map_err(|error| format!("MCP 工具调用任务失败：{error}"))?
}

#[tauri::command]
pub fn shutdown_mcp_server(server_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut registry = state
        .mcp
        .lock()
        .map_err(|_| "MCP 状态不可用，请重启应用。".to_string())?;
    registry.shutdown(&server_id);
    Ok(())
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

    chat::append_message(&workspace, &input.node_id, "user", &message)?;

    let node_id = input.node_id.clone();
    let app_for_blocking = app.clone();
    let workspace_for_blocking = workspace.clone();
    let turn = tauri::async_runtime::spawn_blocking(move || {
        run_agent_turn(
            &app_for_blocking,
            &workspace_for_blocking,
            &node_id,
            input.deep_research,
        )
    })
    .await
    .map_err(|error| format!("研究任务失败：{error}"))??;

    let meta = serde_json::json!({ "toolTrace": turn.trace });
    chat::append_message_with_meta(
        &workspace,
        &input.node_id,
        "assistant",
        &turn.content,
        Some(meta),
    )?;
    let messages = chat::load_messages(&workspace, &input.node_id, 50)?;

    Ok(ConversationReply {
        messages,
        model: turn.model,
        usage: turn.usage,
        tool_trace: turn.trace,
    })
}

struct AgentTurn {
    content: String,
    model: String,
    usage: Option<serde_json::Value>,
    trace: Vec<ToolEvent>,
}

fn run_agent_turn(
    app: &tauri::AppHandle,
    workspace: &WorkspaceInfo,
    node_id: &str,
    deep_research: bool,
) -> Result<AgentTurn, String> {
    let context = chat::node_context(workspace, node_id)?;
    let settings = settings::load_settings_for_app(app)?;
    let resolved = settings::resolve_role(&settings, "research")?;
    let saved_sources = stored_sources::linked_sources_for_chat(workspace, node_id, 20)?;
    let linked_assets = asset_links::linked_excerpts(workspace, node_id, 4, 1_200)?;
    let enabled_skills = if deep_research {
        skills::enabled_prompts(app, &settings).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut tool_specs = if deep_research {
        builtin_tool_specs()
    } else {
        Vec::new()
    };
    let mut mcp_map: std::collections::HashMap<String, McpTool> = std::collections::HashMap::new();
    let mut mcp_note = String::new();
    if deep_research && !settings.mcp_servers.is_empty() {
        let state = app.state::<AppState>();
        let mut registry = state
            .mcp
            .lock()
            .map_err(|_| "MCP 状态不可用，请重启应用。".to_string())?;
        match registry.tools(&settings.mcp_servers) {
            Ok(tools) => {
                for tool in tools {
                    tool_specs.push(serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.qualified_name,
                            "description": format!("[MCP:{}] {}", tool.server_name, tool.description),
                            "parameters": tool.input_schema
                        }
                    }));
                    mcp_map.insert(tool.qualified_name.clone(), tool);
                }
            }
            Err(error) => mcp_note = error,
        }
    }

    let system = system_prompt(
        &context,
        &saved_sources,
        &linked_assets,
        &enabled_skills,
        &mcp_note,
        deep_research,
    );
    let mut messages = vec![serde_json::json!({
        "role": "system",
        "content": system
    })];
    let history = chat::load_messages(workspace, node_id, 12)?;
    for item in &history {
        messages.push(serde_json::json!({
            "role": item.role,
            "content": item.body
        }));
    }

    let mut trace = Vec::new();
    let mut final_content = String::new();
    let mut model = resolved.model.model_id.clone();
    let mut usage = None;
    let max_steps = if deep_research { 8usize } else { 1usize };
    let _ = app.emit(
        "agent-status",
        serde_json::json!({"nodeId": node_id, "phase": "thinking", "deepResearch": deep_research}),
    );

    for step in 0..max_steps {
        let payload = serde_json::json!({
            "provider": {
                "baseUrl": resolved.provider.base_url,
                "protocol": resolved.provider.protocol,
                "proxy": resolved.provider.proxy,
                "timeoutSeconds": resolved.provider.timeout_seconds
            },
            "modelId": resolved.model.model_id,
            "messages": messages,
            "apiKey": resolved.api_key,
            "maxTokens": 8000,
            "temperature": 0.3,
            "tools": tool_specs
        });
        let _ = app.emit(
            "agent-stream-start",
            serde_json::json!({"nodeId": node_id, "step": step}),
        );
        let parsed = sidecar_chat_stream(app, node_id, &payload)?;
        if parsed.get("ok").and_then(|value| value.as_bool()) != Some(true) {
            let message = parsed
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("模型调用失败。");
            return Err(message.to_string());
        }
        model = parsed
            .get("model")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| resolved.model.model_id.clone());
        usage = parsed.get("usage").cloned().or(usage);

        let content = parsed
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let mut calls = parsed
            .get("toolCalls")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let mut assistant_content = content.clone();
        if calls.is_empty() {
            let mut fallback = parse_text_tool_calls(&content, step);
            let mut stripped = if fallback.is_empty() {
                content.clone()
            } else {
                strip_text_tool_calls(&content)
            };
            if fallback.is_empty() {
                fallback = parse_dsml_tool_calls(&content, step);
                if !fallback.is_empty() {
                    stripped = strip_dsml_tool_calls(&content);
                }
            }
            if !fallback.is_empty() {
                calls = fallback;
                assistant_content = stripped;
            }
        }

        if calls.is_empty() {
            final_content = content;
            break;
        }

        let mut assistant_message = serde_json::json!({
            "role": "assistant",
            "content": assistant_content,
            "tool_calls": calls
        });
        if let Some(reasoning) = parsed
            .get("reasoningContent")
            .and_then(|value| value.as_str())
        {
            assistant_message["reasoning_content"] =
                serde_json::Value::String(reasoning.to_string());
        }
        messages.push(assistant_message);

        for (index, call) in calls.iter().enumerate() {
            let call_id = call
                .get("id")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("call_{step}_{index}"));
            let name = call
                .get("function")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = match call
                .get("function")
                .and_then(|value| value.get("arguments"))
            {
                Some(serde_json::Value::String(text)) => {
                    serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!({}))
                }
                Some(value) => value.clone(),
                None => serde_json::json!({}),
            };

            let server_hint = mcp_map
                .get(&name)
                .map(|tool| format!("MCP · {}", tool.server_name))
                .unwrap_or_else(|| match name.as_str() {
                    "cers_academic_search" => "CERS 学术来源".to_string(),
                    "cers_news_search" => "CERS 新闻来源".to_string(),
                    _ => "CERS".to_string(),
                });
            let _ = app.emit(
                "agent-tool-start",
                serde_json::json!({"nodeId": node_id, "name": name, "server": server_hint}),
            );
            let started = std::time::Instant::now();
            let executed = execute_tool(
                app, workspace, node_id, &settings, &mcp_map, &name, &arguments,
            );
            let (result_text, summary, server, ok) = match executed {
                Ok(value) => value,
                Err(error) => (
                    serde_json::json!({"ok": false, "error": error}).to_string(),
                    format!("工具失败：{error}"),
                    mcp_map
                        .get(&name)
                        .map(|tool| format!("MCP · {}", tool.server_name))
                        .unwrap_or_else(|| "CERS".to_string()),
                    false,
                ),
            };
            let event = ToolEvent {
                name: name.clone(),
                server,
                ok,
                summary,
                duration_ms: started.elapsed().as_millis(),
            };
            let _ = app.emit(
                "agent-tool",
                serde_json::json!({
                    "nodeId": node_id,
                    "name": event.name,
                    "server": event.server,
                    "ok": event.ok,
                    "summary": event.summary,
                    "durationMs": event.duration_ms
                }),
            );
            trace.push(event);
            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": result_text
            }));
        }

        if step + 1 == max_steps {
            final_content = content;
        }
    }

    let _ = app.emit(
        "agent-status",
        serde_json::json!({"nodeId": node_id, "phase": "done"}),
    );

    if final_content.is_empty() {
        return Err("模型调用了研究工具，但没有给出最终回答。请重试或缩小问题范围。".to_string());
    }

    Ok(AgentTurn {
        content: final_content,
        model,
        usage,
        trace,
    })
}

fn sidecar_chat_stream(
    app: &tauri::AppHandle,
    node_id: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let state = app.state::<AppState>();
    let mut manager = state
        .sidecar
        .lock()
        .map_err(|_| "Sidecar 状态不可用，请重启应用。".to_string())?;
    let mut done: Option<serde_json::Value> = None;
    let mut error: Option<String> = None;
    manager.post_json_stream("/chat/stream", payload, |line| {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        match value.get("type").and_then(|item| item.as_str()) {
            Some("delta") => {
                let kind = value
                    .get("kind")
                    .and_then(|item| item.as_str())
                    .unwrap_or("content");
                let text = value
                    .get("text")
                    .and_then(|item| item.as_str())
                    .unwrap_or("");
                if !text.is_empty() {
                    let _ = app.emit(
                        "agent-delta",
                        serde_json::json!({
                            "nodeId": node_id,
                            "kind": kind,
                            "text": text
                        }),
                    );
                }
            }
            Some("done") => done = Some(value),
            Some("error") => {
                error = Some(
                    value
                        .get("message")
                        .and_then(|item| item.as_str())
                        .unwrap_or("模型流式调用失败。")
                        .to_string(),
                )
            }
            _ => {}
        }
    })?;
    if let Some(message) = error {
        return Err(message);
    }
    let Some(mut value) = done else {
        return Err("模型流式响应不完整。".to_string());
    };
    value["ok"] = serde_json::Value::Bool(true);
    Ok(value)
}

fn sidecar_json(
    app: &tauri::AppHandle,
    path: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let state = app.state::<AppState>();
    let mut manager = state
        .sidecar
        .lock()
        .map_err(|_| "Sidecar 状态不可用，请重启应用。".to_string())?;
    let reply = manager.post_json(path, payload)?;
    serde_json::from_str(&reply).map_err(|error| format!("侧车响应无法解析：{error}"))
}

fn builtin_tool_specs() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "cers_academic_search",
                "description": "检索已启用的学术来源（OpenAlex、Crossref、arXiv 等），返回标题、作者、年份、DOI、摘要与覆盖级别；结果会自动收藏到当前节点。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "检索关键词，可用空格分隔多组概念"},
                        "yearFrom": {"type": "integer", "description": "起始年份，可选"},
                        "yearTo": {"type": "integer", "description": "结束年份，可选"}
                    },
                    "required": ["query"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "cers_fetch_web_page",
                "description": "读取公开网页并提取标题与正文，用于核实监管文件、新闻原文或论文摘要页；只返回文本，不执行网页代码。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string", "description": "http 或 https 网页地址"},
                        "maxChars": {"type": "integer", "description": "最多返回字符数"}
                    },
                    "required": ["url"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "cers_list_feed",
                "description": "读取公开 RSS/Atom 订阅，返回标题、链接、摘要和发布时间。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string", "description": "RSS/Atom 地址"},
                        "limit": {"type": "integer", "description": "最多返回条目数"}
                    },
                    "required": ["url"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "cers_news_search",
                "description": "检索已启用的新闻与 RSS 来源，返回近期资讯标题、原站链接、发布时间与摘要；结果会自动收藏到当前节点。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "检索关键词；留空则读取已启用订阅的最新条目"}
                    }
                }
            }
        }),
    ]
}

fn execute_tool(
    app: &tauri::AppHandle,
    workspace: &WorkspaceInfo,
    node_id: &str,
    settings: &settings::GlobalSettings,
    mcp_map: &std::collections::HashMap<String, McpTool>,
    name: &str,
    arguments: &serde_json::Value,
) -> Result<(String, String, String, bool), String> {
    if name == "cers_academic_search" {
        let query = arguments
            .get("query")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if query.is_empty() {
            return Err("学术检索需要 query。".to_string());
        }
        let year_from = arguments
            .get("yearFrom")
            .and_then(|value| value.as_i64())
            .map(|value| value as i32);
        let year_to = arguments
            .get("yearTo")
            .and_then(|value| value.as_i64())
            .map(|value| value as i32);
        let payload = serde_json::json!({
            "query": query,
            "maxResults": 6,
            "yearFrom": year_from,
            "yearTo": year_to,
            "sources": settings::academic_sources_payload(settings),
            "network": settings::network_payload(settings)
        });
        let result = sidecar_json(app, "/sources/search", &payload)?;
        if result.get("ok").and_then(|value| value.as_bool()) != Some(true) {
            return Err(result
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("学术检索失败。")
                .to_string());
        }
        let records = result
            .get("results")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let saved = save_records(workspace, node_id, &records);
        let compact = compact_records(&records);
        let text = serde_json::json!({
            "ok": true,
            "count": records.len(),
            "saved": saved,
            "results": compact
        })
        .to_string();
        return Ok((
            text,
            format!("学术检索：{} 条，收藏 {} 条", records.len(), saved),
            "CERS 学术来源".to_string(),
            true,
        ));
    }

    if name == "cers_fetch_web_page" {
        let url = arguments
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if url.is_empty() {
            return Err("网页抓取需要 url。".to_string());
        }
        let payload = serde_json::json!({
            "url": url,
            "maxChars": arguments.get("maxChars").and_then(|value| value.as_i64()).unwrap_or(6000)
        });
        let result = sidecar_json(app, "/web/fetch", &payload)?;
        if result.get("ok").and_then(|value| value.as_bool()) != Some(true) {
            return Err(result
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("网页抓取失败。")
                .to_string());
        }
        let title = result
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let chars = result
            .get("chars")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        return Ok((
            result.to_string(),
            format!("网页抓取：{title}（{chars} 字符）"),
            "CERS 网页工具".to_string(),
            true,
        ));
    }

    if name == "cers_list_feed" {
        let url = arguments
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if url.is_empty() {
            return Err("订阅读取需要 url。".to_string());
        }
        let payload = serde_json::json!({
            "url": url,
            "limit": arguments.get("limit").and_then(|value| value.as_i64()).unwrap_or(10)
        });
        let result = sidecar_json(app, "/feed/list", &payload)?;
        if result.get("ok").and_then(|value| value.as_bool()) != Some(true) {
            return Err(result
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("订阅读取失败。")
                .to_string());
        }
        let count = result
            .get("items")
            .and_then(|value| value.as_array())
            .map(|items| items.len())
            .unwrap_or(0);
        return Ok((
            result.to_string(),
            format!("订阅读取：{count} 条"),
            "CERS 网页工具".to_string(),
            true,
        ));
    }

    if name == "cers_news_search" {
        let query = arguments
            .get("query")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let payload = serde_json::json!({
            "query": query,
            "maxResults": 10,
            "sources": settings::news_sources_payload(settings),
            "network": settings::network_payload(settings)
        });
        let result = sidecar_json(app, "/news/search", &payload)?;
        if result.get("ok").and_then(|value| value.as_bool()) != Some(true) {
            return Err(result
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("资讯检索失败。")
                .to_string());
        }
        let records = result
            .get("results")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let saved = save_records(workspace, node_id, &records);
        let compact = compact_records(&records);
        let text = serde_json::json!({
            "ok": true,
            "count": records.len(),
            "saved": saved,
            "results": compact
        })
        .to_string();
        return Ok((
            text,
            format!("资讯检索：{} 条，收藏 {} 条", records.len(), saved),
            "CERS 新闻来源".to_string(),
            true,
        ));
    }

    let tool = mcp_map
        .get(name)
        .ok_or_else(|| format!("未知工具：{name}"))?;
    let config = settings
        .mcp_servers
        .iter()
        .find(|server| server.id == tool.server_id)
        .ok_or_else(|| "MCP 服务配置已不存在。".to_string())?;
    let state = app.state::<AppState>();
    let mut registry = state
        .mcp
        .lock()
        .map_err(|_| "MCP 状态不可用，请重启应用。".to_string())?;
    let output = registry.call(config, &tool.name, arguments.clone())?;
    let summary = output
        .replace('\n', " ")
        .chars()
        .take(160)
        .collect::<String>();
    Ok((output, summary, format!("MCP · {}", tool.server_name), true))
}

fn save_records(workspace: &WorkspaceInfo, node_id: &str, records: &[serde_json::Value]) -> usize {
    let mut saved = 0usize;
    for record in records {
        let input = SaveSourceInput {
            provider: record
                .get("provider")
                .and_then(|value| value.as_str())
                .unwrap_or("cers")
                .to_string(),
            provider_name: record
                .get("providerName")
                .and_then(|value| value.as_str())
                .unwrap_or("CERS")
                .to_string(),
            external_id: record
                .get("externalId")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            doi: record
                .get("doi")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            url: record
                .get("url")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            title: record
                .get("title")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            authors: record
                .get("authors")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            year: record.get("year").and_then(|value| value.as_i64()),
            venue: record
                .get("venue")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            abstract_text: record
                .get("abstract")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            coverage: record
                .get("coverage")
                .and_then(|value| value.as_str())
                .unwrap_or("metadata_only")
                .to_string(),
            record_type: record
                .get("type")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            retrieved_at: record
                .get("retrievedAt")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            node_id: Some(node_id.to_string()),
        };
        if stored_sources::save_source(workspace, input).is_ok() {
            saved += 1;
        }
    }
    saved
}

fn compact_records(records: &[serde_json::Value]) -> Vec<serde_json::Value> {
    records
        .iter()
        .map(|record| {
            let abstract_text = record
                .get("abstract")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .chars()
                .take(400)
                .collect::<String>();
            serde_json::json!({
                "title": record.get("title"),
                "authors": record.get("authors"),
                "year": record.get("year"),
                "venue": record.get("venue"),
                "doi": record.get("doi"),
                "url": record.get("url"),
                "coverage": record.get("coverage"),
                "type": record.get("type"),
                "publishedAt": record.get("publishedAt"),
                "abstract": abstract_text
            })
        })
        .collect()
}

fn system_prompt(
    context: &NodeChatContext,
    sources: &[SavedSource],
    linked_assets: &[(String, String)],
    skills: &[(String, String)],
    mcp_note: &str,
    deep_research: bool,
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
         4. 需要外部检索时调用工具，不要伪造检索结果或来源。\n\
         5. 工具返回的来源会自动收藏；引用时写明来源与覆盖级别。\n\
         6. 使用中文回答。",
        title = context.title,
    );
    prompt.push_str(&format!(
        "\n当前日期：{}\n",
        chrono::Local::now().format("%Y-%m-%d")
    ));
    if deep_research {
        prompt.push_str(
            "\n工具使用：需要学术证据时调用 cers_academic_search；需要近期动态时调用 cers_news_search；\
             需要核实监管文件、新闻原文或论文摘要页时调用 cers_fetch_web_page；             自定义 MCP 工具以 mcp_ 开头。\
             最多进行 8 轮工具调用，得到足够证据后给出结论。\n",
        );
    } else {
        prompt.push_str(
            "\n本轮是快速对话，不调用任何工具：直接基于已有上下文回答；\
             涉及最新事实、文献或数据时，明确说明需要切换到深度研究核实。\n",
        );
    }

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

    if !mcp_note.is_empty() {
        prompt.push_str(&format!("\n注意：部分 MCP 服务不可用：{mcp_note}\n"));
    }

    if !skills.is_empty() {
        prompt.push_str("\n本轮启用的技能（按其步骤执行，但仍须遵守证据规则）：\n");
        for (name, body) in skills {
            prompt.push_str(&format!("\n【{name}】\n{body}\n"));
        }
    }

    prompt
}

fn parse_dsml_tool_calls(content: &str, step: usize) -> Vec<serde_json::Value> {
    let mut calls = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find("invoke name=\"") {
        let after = &rest[start + "invoke name=\"".len()..];
        let Some(quote) = after.find('"') else {
            break;
        };
        let name = &after[..quote];
        let tail = &after[quote + 1..];
        let block_end = tail.find("invoke name=\"").unwrap_or(tail.len());
        let block = &tail[..block_end];
        let mut arguments = serde_json::Map::new();
        let mut parameters = block;
        while let Some(position) = parameters.find("parameter name=\"") {
            let after = &parameters[position + "parameter name=\"".len()..];
            let Some(quote) = after.find('"') else {
                break;
            };
            let key = &after[..quote];
            let value_rest = &after[quote + 1..];
            let Some(gt) = value_rest.find('>') else {
                break;
            };
            let value_rest = &value_rest[gt + 1..];
            let Some(close) = value_rest.find("</") else {
                break;
            };
            let value = value_rest[..close].trim();
            let numeric = matches!(key, "limit" | "maxChars" | "yearFrom" | "yearTo")
                && value.parse::<i64>().is_ok();
            if numeric {
                arguments.insert(
                    key.to_string(),
                    serde_json::Value::Number(value.parse::<i64>().unwrap_or(0).into()),
                );
            } else {
                arguments.insert(
                    key.to_string(),
                    serde_json::Value::String(value.to_string()),
                );
            }
            parameters = &value_rest[close + 2..];
        }
        calls.push(serde_json::json!({
            "id": format!("dsml_call_{step}_{}", calls.len()),
            "type": "function",
            "function": {
                "name": name,
                "arguments": serde_json::Value::Object(arguments).to_string()
            }
        }));
        rest = &tail[block_end..];
    }
    calls
}

fn strip_dsml_tool_calls(content: &str) -> String {
    match content.find("<｜") {
        Some(index) => content[..index].trim().to_string(),
        None => content.trim().to_string(),
    }
}

fn parse_text_tool_calls(content: &str, step: usize) -> Vec<serde_json::Value> {
    let mut calls = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find("<tool_call>") {
        let body = &rest[start + "<tool_call>".len()..];
        let Some(end) = body.find("</tool_call>") else {
            break;
        };
        if let Some(value) = extract_json_object(body[..end].trim()) {
            if let Some(call) = text_call_value(&value, step, calls.len()) {
                calls.push(call);
            }
        }
        rest = &body[end + "</tool_call>".len()..];
    }
    if calls.is_empty() {
        let mut rest = content;
        while let Some(start) = rest.find("```tool_call") {
            let body = &rest[start + "```tool_call".len()..];
            let body = body.trim_start_matches(['\n', '\r']);
            let Some(end) = body.find("```") else {
                break;
            };
            if let Some(value) = extract_json_object(body[..end].trim()) {
                if let Some(call) = text_call_value(&value, step, calls.len()) {
                    calls.push(call);
                }
            }
            rest = &body[end + 3..];
        }
    }
    calls
}

fn text_call_value(
    value: &serde_json::Value,
    step: usize,
    index: usize,
) -> Option<serde_json::Value> {
    let name = value
        .get("name")
        .or_else(|| value.get("tool"))
        .and_then(|item| item.as_str())?;
    let arguments = value
        .get("arguments")
        .or_else(|| value.get("args"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let arguments = if arguments.is_string() {
        arguments
    } else {
        serde_json::Value::String(arguments.to_string())
    };
    Some(serde_json::json!({
        "id": format!("text_call_{step}_{index}"),
        "type": "function",
        "function": {"name": name, "arguments": arguments}
    }))
}

fn strip_text_tool_calls(content: &str) -> String {
    let mut output = content.to_string();
    while let Some(start) = output.find("<tool_call>") {
        let Some(relative_end) = output[start..].find("</tool_call>") else {
            break;
        };
        let end = start + relative_end + "</tool_call>".len();
        output.replace_range(start..end, "");
    }
    while let Some(start) = output.find("```tool_call") {
        let body = &output[start + 12..];
        let Some(relative_end) = body.find("```") else {
            break;
        };
        let end = start + 12 + relative_end + 3;
        output.replace_range(start..end, "");
    }
    output.trim().to_string()
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

pub(crate) fn run_demo_research(
    app: &tauri::AppHandle,
    workspace_path: &Path,
    question: &str,
) -> Result<serde_json::Value, String> {
    let workspace = storage::open_workspace(workspace_path).map_err(|error| error.to_string())?;
    {
        let state = app.state::<AppState>();
        let mut guard = state
            .workspace
            .lock()
            .map_err(|_| "应用状态不可用。".to_string())?;
        *guard = Some(workspace.clone());
    }
    let _ = skills::skills_dir(app)?;

    let snapshot = graph::load_graph(&workspace)?;
    let research_title = "商业银行反欺诈模式研究";
    let report_title = "研究报告：商业银行反欺诈模式";
    let research_node = snapshot
        .nodes
        .iter()
        .find(|node| node.title == research_title)
        .map(|node| node.id.clone());
    let research_node_id = match research_node {
        Some(id) => id,
        None => {
            let created = graph::create_node(
                &workspace,
                CreateNodeInput {
                    node_type: "research".to_string(),
                    title: research_title.to_string(),
                    goal: Some(question.to_string()),
                    x: 260.0,
                    y: 140.0,
                    parent_id: Some(workspace.root_node_id.clone()),
                    edge_kind: Some("decomposition".to_string()),
                },
            )?;
            created
                .nodes
                .iter()
                .find(|node| node.title == research_title)
                .map(|node| node.id.clone())
                .ok_or_else(|| "研究节点创建失败。".to_string())?
        }
    };

    let report_node_id = match snapshot
        .nodes
        .iter()
        .find(|node| node.title == report_title)
        .map(|node| node.id.clone())
    {
        Some(id) => id,
        None => {
            let created = graph::create_node(
                &workspace,
                CreateNodeInput {
                    node_type: "report".to_string(),
                    title: report_title.to_string(),
                    goal: Some("汇总反欺诈模式研究的证据与趋势判断".to_string()),
                    x: 520.0,
                    y: 260.0,
                    parent_id: Some(research_node_id.clone()),
                    edge_kind: Some("association".to_string()),
                },
            )?;
            created
                .nodes
                .iter()
                .find(|node| node.title == report_title)
                .map(|node| node.id.clone())
                .ok_or_else(|| "报告节点创建失败。".to_string())?
        }
    };

    let prompt = format!(
        "请使用已启用技能，通过工具完成研究并输出一份可直接作为阶段报告的中文 Markdown。\n\
         研究主题：{question}\n\n\
         要求：\n\
         1. 先用 cers_academic_search 检索至少两轮（中英文关键词各一组），再用 cers_news_search 获取近期动态；\n\
         2. 对关键来源用 MCP 的 fetch_web_page 核验原文或摘要页；\n\
         3. 明确区分监管文件、同行评议研究、行业报告、厂商观点与媒体报道；\n\
         4. 正文中的每个事实性判断都要有来源链接或 DOI，并在参考文献列出；\n\
         5. 输出结构：摘要、最新动态、主要反欺诈模式、发展趋势、证据表、风险与空白、参考文献；\n\
         6. 不得编造数据、法规条款、市场份额或页码；证据不足处明确写“待核实”。"
    );
    chat::append_message(&workspace, &research_node_id, "user", &prompt)?;
    let turn = run_agent_turn(app, &workspace, &research_node_id, true)?;

    let sources = stored_sources::list_sources(&workspace, Some(&research_node_id), None)?;
    let mut content = strip_model_references(&turn.content);
    let (section, cited) = reference_section(&sources, &content);
    if cited > 0 {
        content.push_str("\n\n");
        content.push_str(&section);
    }

    let report = reports::get_report(&workspace, &report_node_id)?;
    let saved = reports::save_report(&workspace, &report_node_id, &content, report.revision)?;
    let markdown_path = reports::export_markdown(&workspace, &report_node_id)?;
    let docx_path = export_demo_document(app, &workspace, &report_node_id, "docx")?;
    let pdf_path = export_demo_document(app, &workspace, &report_node_id, "pdf")?;

    let baseline = run_direct_baseline(app, question)?;
    let research_dir = Path::new(&workspace.workspace_path)
        .join("Research Outputs")
        .join("research");
    std::fs::create_dir_all(&research_dir).map_err(|error| format!("创建目录失败：{error}"))?;
    let baseline_path = research_dir.join("直连基线-反欺诈.md");
    std::fs::write(&baseline_path, &baseline).map_err(|error| format!("写入基线失败：{error}"))?;

    let metrics = serde_json::json!({
        "question": question,
        "reportNodeId": report_node_id,
        "reportRevision": saved.revision,
        "toolTrace": turn.trace,
        "toolCalls": turn.trace.len(),
        "sourcesSaved": sources.len(),
        "sourcesCited": reference_section(&sources, &content).1,
        "reportChars": content.chars().count(),
        "reportLinks": count_links(&content),
        "baselineChars": baseline.chars().count(),
        "baselineLinks": count_links(&baseline),
        "markdown": markdown_path,
        "docx": docx_path,
        "pdf": pdf_path,
        "baseline": baseline_path.display().to_string(),
    });
    let metrics_path = research_dir.join("mcp-research-metrics.json");
    std::fs::write(
        &metrics_path,
        serde_json::to_string_pretty(&metrics)
            .map_err(|error| format!("序列化指标失败：{error}"))?,
    )
    .map_err(|error| format!("写入指标失败：{error}"))?;

    Ok(metrics)
}

fn export_demo_document(
    app: &tauri::AppHandle,
    workspace: &WorkspaceInfo,
    node_id: &str,
    format: &str,
) -> Result<String, String> {
    let (content, title, path) = reports::export_target(workspace, node_id, format)?;
    let payload = serde_json::json!({
        "content": content,
        "title": title,
        "format": format,
        "path": path,
    });
    let result = sidecar_json(app, "/reports/export", &payload)?;
    if result.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return Err(result
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("报告导出失败。")
            .to_string());
    }
    if !Path::new(&path).is_file() {
        return Err("导出的报告文件不存在。".to_string());
    }
    Ok(path)
}

fn reference_section(sources: &[SavedSource], content: &str) -> (String, usize) {
    let cited: Vec<&SavedSource> = sources
        .iter()
        .filter(|source| source_cited(source, content))
        .take(40)
        .collect();
    if cited.is_empty() {
        return (String::new(), 0);
    }
    let mut output = String::from("## 参考文献\n\n");
    for (index, source) in cited.iter().enumerate() {
        let authors = if source.authors.is_empty() {
            "佚名".to_string()
        } else {
            source.authors.join(", ")
        };
        let year = source
            .year
            .map(|value| value.to_string())
            .unwrap_or_else(|| "年份未知".to_string());
        let link = source
            .doi
            .as_deref()
            .map(|doi| {
                if doi.starts_with("http") {
                    doi.to_string()
                } else {
                    format!("https://doi.org/{doi}")
                }
            })
            .or_else(|| source.url.clone())
            .unwrap_or_default();
        let coverage = match source.coverage.as_str() {
            "abstract_only" => "仅摘要",
            "fulltext" => "已获全文",
            "unavailable" => "不可获取",
            _ => "仅题录",
        };
        output.push_str(&format!(
            "{}. {}. {}，{}. {}. （{}）\n",
            index + 1,
            authors,
            source.title,
            year,
            link,
            coverage
        ));
    }
    (output, cited.len())
}

fn strip_model_references(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if let Some(index) = lines.iter().rposition(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("##") && trimmed.contains("参考文献")
    }) {
        if index >= lines.len() / 2 {
            return lines[..index]
                .join(
                    "
",
                )
                .trim_end()
                .to_string();
        }
    }
    content.trim().to_string()
}

fn source_cited(source: &SavedSource, content: &str) -> bool {
    let lowered = content.to_lowercase();
    if let Some(doi) = source.doi.as_deref() {
        if lowered.contains(&doi.to_lowercase()) {
            return true;
        }
    }
    if let Some(url) = source.url.as_deref() {
        if content.contains(url) {
            return true;
        }
    }
    let title = source.title.trim();
    if !title.is_empty() && content.contains(title) {
        return true;
    }
    let prefix: String = title.chars().take(20).collect();
    prefix.chars().count() >= 12 && content.contains(&prefix)
}

fn count_links(content: &str) -> usize {
    content.matches("https://").count() + content.matches("http://").count()
}

fn run_direct_baseline(app: &tauri::AppHandle, question: &str) -> Result<String, String> {
    let settings = settings::load_settings_for_app(app)?;
    let resolved = settings::resolve_role(&settings, "research")?;
    let payload = serde_json::json!({
        "provider": {
            "baseUrl": resolved.provider.base_url,
            "protocol": resolved.provider.protocol,
            "proxy": resolved.provider.proxy,
            "timeoutSeconds": resolved.provider.timeout_seconds
        },
        "modelId": resolved.model.model_id,
        "messages": [
            {
                "role": "system",
                "content": "你是一名金融研究顾问。直接回答，不要使用外部工具或检索。"
            },
            {
                "role": "user",
                "content": format!("请完成一份关于“{question}”的研究报告，包含最新动态与趋势判断。")
            }
        ],
        "apiKey": resolved.api_key,
        "maxTokens": 6000,
        "temperature": 0.3
    });
    let result = sidecar_json(app, "/chat", &payload)?;
    if result.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return Err(result
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("基线回答失败。")
            .to_string());
    }
    Ok(result
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_model_reference_tail_and_matches_citations() {
        let mut content = String::from(
            "## 一、摘要

正文引用 [DOI](https://doi.org/10.1000/ABC)。

",
        );
        content.push_str(
            "## 七、参考文献

1. 不完整的条目 https://doi.org/10.1000/",
        );
        let stripped = strip_model_references(&content);
        assert!(!stripped.contains("七、参考文献"));
        assert!(stripped.contains("正文引用"));

        let source = SavedSource {
            id: "src".to_string(),
            external_id: None,
            provider: "crossref".to_string(),
            provider_name: "Crossref".to_string(),
            doi: Some("10.1000/abc".to_string()),
            url: None,
            title: "Test".to_string(),
            authors: vec![],
            year: None,
            venue: None,
            abstract_text: None,
            coverage: "metadata_only".to_string(),
            record_type: None,
            retrieved_at: None,
            created_at: String::new(),
        };
        assert!(source_cited(&source, &stripped));
    }

    #[test]
    fn parses_text_and_dsml_tool_calls() {
        let dsml = "<｜｜DSML｜｜ calls>\n\
                    <｜｜DSML｜｜ invoke name=\"cers_academic_search\">\n\
                    <｜｜DSML｜｜ parameter name=\"query\" string=\"true\">商业银行 反欺诈</｜｜DSML｜｜ parameter>\n\
                    <｜｜DSML｜｜ parameter name=\"yearFrom\" string=\"true\">2023</｜｜DSML｜｜ parameter>\n\
                    </｜｜DSML｜｜ invoke>\n\
                    <｜｜DSML｜｜ invoke name=\"cers_news_search\">\n\
                    <｜｜DSML｜｜ parameter name=\"query\" string=\"true\">fraud</｜｜DSML｜｜ parameter>\n\
                    </｜｜DSML｜｜ invoke>\n\
                    </｜｜DSML｜｜ calls>";
        let calls = parse_dsml_tool_calls(dsml, 0);
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0]["function"]["name"].as_str(),
            Some("cers_academic_search")
        );
        let arguments: serde_json::Value =
            serde_json::from_str(calls[0]["function"]["arguments"].as_str().unwrap_or("{}"))
                .unwrap_or_default();
        assert_eq!(arguments["query"].as_str(), Some("商业银行 反欺诈"));
        assert_eq!(arguments["yearFrom"].as_i64(), Some(2023));
        assert!(
            strip_dsml_tool_calls(dsml).is_empty(),
            "preamble should be empty"
        );

        let text = "先检索。\n<tool_call>{\"name\":\"cers_news_search\",\"arguments\":{\"query\":\"fraud\"}}</tool_call>";
        let calls = parse_text_tool_calls(text, 0);
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0]["function"]["name"].as_str(),
            Some("cers_news_search")
        );
        assert_eq!(strip_text_tool_calls(text), "先检索。");
    }

    /// 手动烟雾测试：需要本机已配置 research 角色模型，验证工具调用协议。
    /// 运行 `cargo test real_tool_call_smoke -- --ignored --nocapture`。
    #[test]
    #[ignore]
    fn real_tool_call_smoke() {
        let appdata =
            std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA available"));
        let path = appdata.join("com.chengsi.academic").join("settings.json");
        let settings: settings::GlobalSettings =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read settings"))
                .expect("parse settings");
        let resolved = settings::resolve_role(&settings, "research").expect("resolve model");

        let payload = serde_json::json!({
            "provider": {
                "baseUrl": resolved.provider.base_url,
                "protocol": resolved.provider.protocol,
                "proxy": resolved.provider.proxy,
                "timeoutSeconds": resolved.provider.timeout_seconds
            },
            "modelId": resolved.model.model_id,
            "messages": [
                {"role": "system", "content": "你必须先调用工具获取资料，再回答。"},
                {"role": "user", "content": "请调用 cers_news_search 检索 fraud 的最新资讯，然后只回复工具名称。"}
            ],
            "apiKey": resolved.api_key,
            "maxTokens": 800,
            "temperature": 0.2,
            "tools": builtin_tool_specs()
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
        let calls = value
            .get("toolCalls")
            .and_then(|item| item.as_array())
            .expect("toolCalls");
        assert!(!calls.is_empty(), "reply={reply}");
        let name = calls[0]
            .get("function")
            .and_then(|item| item.get("name"))
            .and_then(|item| item.as_str())
            .unwrap_or("");
        assert_eq!(name, "cers_news_search");
        println!(
            "toolCalls={}",
            serde_json::to_string(&calls).unwrap_or_default()
        );
    }

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

    /// 手动烟雾测试：需要本机已配置 report 角色模型。
    /// 运行 `cargo test real_report_outline_smoke -- --ignored --nocapture`。
    #[test]
    #[ignore]
    fn real_report_outline_smoke() {
        let appdata =
            std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA available"));
        let path = appdata.join("com.chengsi.academic").join("settings.json");
        let settings: settings::GlobalSettings =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read settings"))
                .expect("parse settings");
        let resolved = settings::resolve_role(&settings, "report").expect("resolve report model");

        let brief = "报告节点：阶段报告
节点目标：总结城市绿地与局地热环境的关联
                     已收藏来源：
- Urban green space and heat（2024）· 仅摘要
";
        let payload = serde_json::json!({
            "provider": {
                "baseUrl": resolved.provider.base_url,
                "protocol": resolved.provider.protocol,
                "proxy": resolved.provider.proxy,
                "timeoutSeconds": resolved.provider.timeout_seconds
            },
            "modelId": resolved.model.model_id,
            "messages": [
                {"role": "system", "content": REPORT_OUTLINE_SYSTEM_PROMPT},
                {"role": "user", "content": format!("请为以下研究形成报告提纲：
        {brief}")}
            ],
            "apiKey": resolved.api_key,
            "maxTokens": 1400,
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
        let outline = extract_json_object(content).expect("JSON outline");
        let sections = outline
            .get("sections")
            .and_then(|item| item.as_array())
            .expect("sections");
        assert!(sections.len() >= 2, "content={content}");
        println!("content={content}");
    }
}
