mod commands;
mod domain;
mod mcp;
mod settings;
mod sidecar;
mod skills;
mod storage;
mod updates;

use std::sync::Mutex;

use domain::WorkspaceInfo;
use mcp::McpRegistry;
use sidecar::SidecarManager;
use tauri::Manager;

pub struct AppState {
    pub workspace: Mutex<Option<WorkspaceInfo>>,
    pub sidecar: Mutex<SidecarManager>,
    pub mcp: Mutex<McpRegistry>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            workspace: Mutex::new(None),
            sidecar: Mutex::new(SidecarManager::default()),
            mcp: Mutex::new(McpRegistry::default()),
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            if let Ok(resource_dir) = app.path().resource_dir() {
                let state = app.state::<AppState>();
                let mut manager = state.sidecar.lock().ok();
                if let Some(manager) = manager.as_mut() {
                    manager.set_resource_dir(resource_dir.clone());
                }
                let mut mcp = state.mcp.lock().ok();
                if let Some(registry) = mcp.as_mut() {
                    registry.set_resource_dir(resource_dir);
                }
            }

            #[cfg(debug_assertions)]
            if let Ok(path) = std::env::var("CERS_OPEN_WORKSPACE") {
                if let Ok(workspace) = storage::open_workspace(std::path::Path::new(&path)) {
                    let state = app.state::<AppState>();
                    let mut guard = state.workspace.lock().ok();
                    if let Some(guard) = guard.as_mut() {
                        **guard = Some(workspace);
                    }
                }
            }

            #[cfg(debug_assertions)]
            if let (Ok(path), Ok(question)) = (
                std::env::var("CERS_DEMO_WORKSPACE"),
                std::env::var("CERS_DEMO_QUESTION"),
            ) {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    match commands::run_demo_research(
                        &handle,
                        std::path::Path::new(&path),
                        &question,
                    ) {
                        Ok(metrics) => eprintln!("[CERS demo] {metrics}"),
                        Err(error) => eprintln!("[CERS demo] failed: {error}"),
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::runtime_info,
            commands::model_readiness,
            commands::check_for_updates,
            commands::skip_update_version,
            commands::set_auto_update_check,
            commands::open_workspace,
            commands::create_project_backup,
            commands::restore_project_backup,
            commands::close_workspace,
            commands::sidecar_self_check,
            commands::get_graph,
            commands::create_node,
            commands::move_node,
            commands::create_edge,
            commands::update_node,
            commands::delete_node,
            commands::delete_edge,
            commands::save_viewport,
            commands::scan_workspace,
            commands::list_assets,
            commands::get_asset_text,
            commands::link_asset_to_node,
            commands::unlink_asset_from_node,
            commands::list_node_assets,
            commands::get_conversation,
            commands::send_node_message,
            commands::search_academic_sources,
            commands::search_news,
            commands::list_skills,
            commands::set_skill_enabled,
            commands::list_mcp_tools,
            commands::test_mcp_server,
            commands::call_mcp_tool,
            commands::shutdown_mcp_server,
            commands::get_research_definition,
            commands::save_research_definition,
            commands::run_experiment,
            commands::list_experiment_runs,
            commands::propose_decomposition,
            commands::confirm_proposal,
            commands::suggest_report_outline,
            commands::suggest_report_changes,
            commands::apply_report_changes,
            commands::list_pending_report_changes,
            commands::get_report,
            commands::save_report,
            commands::list_report_revisions,
            commands::export_report_markdown,
            commands::export_report_document,
            commands::save_academic_source,
            commands::list_saved_sources,
            commands::remove_saved_source,
            commands::delete_saved_source,
            settings::get_global_settings,
            settings::save_global_settings,
            settings::set_secret,
            settings::test_provider,
            settings::test_source
        ])
        .build(tauri::generate_context!())
        .expect("启动澄思学术失败");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            let state = app_handle.state::<AppState>();
            let mut manager = state.sidecar.lock().ok();
            if let Some(manager) = manager.as_mut() {
                manager.shutdown();
            }
        }
    });
}
