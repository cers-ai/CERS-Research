use std::path::Path;

use tauri::State;

use crate::domain::{RuntimeInfo, SidecarStatus, WorkspaceInfo};
use crate::storage::assets::{self, AssetItem, ScanSummary};
use crate::storage::graph::{self, CreateNodeInput, GraphSnapshot, UpdateNodeInput};
use crate::storage::{self, now_iso};
use crate::AppState;

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
