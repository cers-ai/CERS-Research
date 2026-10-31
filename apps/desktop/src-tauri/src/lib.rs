mod commands;
mod domain;
mod settings;
mod sidecar;
mod storage;

use std::sync::Mutex;

use domain::WorkspaceInfo;
use sidecar::SidecarManager;
use tauri::Manager;

pub struct AppState {
    pub workspace: Mutex<Option<WorkspaceInfo>>,
    pub sidecar: Mutex<SidecarManager>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            workspace: Mutex::new(None),
            sidecar: Mutex::new(SidecarManager::default()),
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            if let Ok(resource_dir) = app.path().resource_dir() {
                let state = app.state::<AppState>();
                let mut manager = state.sidecar.lock().ok();
                if let Some(manager) = manager.as_mut() {
                    manager.set_resource_dir(resource_dir);
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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::runtime_info,
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
            commands::get_conversation,
            commands::send_node_message,
            commands::search_academic_sources,
            commands::search_news,
            commands::get_research_definition,
            commands::save_research_definition,
            commands::run_experiment,
            commands::list_experiment_runs,
            commands::propose_decomposition,
            commands::confirm_proposal,
            commands::suggest_report_changes,
            commands::apply_report_changes,
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
