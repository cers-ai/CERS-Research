use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarStatus {
    pub ok: bool,
    pub mode: String,
    pub python: Option<String>,
    pub version: Option<String>,
    pub port: Option<u16>,
    pub pid: Option<u32>,
    pub restarts: u32,
    pub message: String,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub project_id: String,
    pub name: String,
    pub alias: Option<String>,
    pub workspace_path: String,
    pub root_node_id: String,
    pub created_at: String,
    pub schema_version: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub app_version: String,
    pub tauri_version: String,
    pub python_required: String,
    pub sidecar: SidecarStatus,
    pub workspace: Option<WorkspaceInfo>,
}
