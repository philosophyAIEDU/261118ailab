//! 교수님만의 AI 연구실 (Philosophy Research Hub) — 데스크톱 앱 진입점.
mod commands;
mod keystore;
mod state;
mod updater;

use state::AppState;
use std::sync::{Mutex, RwLock};
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let p = app.path();
            let config_dir = p.app_config_dir()?;
            // 연구자료는 앱 프로그램 폴더·앱 데이터 폴더와 분리된 "문서" 폴더에 둔다
            // (업데이트·재설치·제거 시 자동 삭제되지 않도록)
            let base = p.document_dir().or_else(|_| p.home_dir()).or_else(|_| p.app_data_dir())?;
            let state = AppState {
                lib: RwLock::new(None),
                config_dir,
                default_root: base.join(prh_core::paths::DEFAULT_FOLDER_NAME),
                demo_root: base.join(prh_core::paths::DEMO_FOLDER_NAME),
                jobs: Mutex::new(Default::default()),
                pending_update: tokio::sync::Mutex::new(None),
                open_error: RwLock::new(None),
            };
            let loc = state.load_location();
            let root = loc.library.clone().unwrap_or_else(|| state.default_root.clone());
            if let Err(e) = state.switch_to(&root, false) {
                *state.open_error.write().unwrap() = Some(format!("연구실을 열지 못했습니다 ({}): {e}", root.display()));
            }
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::library_open,
            commands::library_open_default,
            commands::demo_open,
            commands::reveal_library,
            commands::open_url,
            commands::dashboard,
            commands::papers_list,
            commands::paper_get,
            commands::paper_update,
            commands::paper_delete,
            commands::paper_reprocess,
            commands::papers_tag,
            commands::tags_list,
            commands::paper_chunks,
            commands::paper_file,
            commands::import_start,
            commands::job_cancel,
            commands::annotations_list,
            commands::annotation_add,
            commands::annotation_update,
            commands::annotation_delete,
            commands::search_passages,
            commands::ask,
            commands::searches_recent,
            commands::search_delete,
            commands::settings_get,
            commands::settings_save,
            commands::ai_status,
            commands::ai_pull,
            commands::ai_unload,
            commands::ai_delete_model,
            commands::index_build,
            commands::compare_aspects,
            commands::compare_run,
            commands::compare_list,
            commands::compare_delete,
            commands::notes_list,
            commands::note_get,
            commands::note_save,
            commands::note_delete,
            commands::note_ai_draft,
            commands::projects_list,
            commands::project_save,
            commands::project_delete,
            commands::project_papers,
            commands::graph_get,
            commands::graph_suggest,
            commands::graph_link,
            commands::graph_resolve,
            commands::graph_unlink,
            commands::review_run,
            commands::review_list,
            commands::review_delete,
            commands::export_file,
            commands::export_obsidian,
            commands::backup_create,
            commands::backup_inspect,
            commands::backup_restore,
            commands::gemini_usage,
            commands::gemini_key_set,
            commands::gemini_key_verify,
            commands::gemini_key_delete,
            commands::gemini_consent,
            updater::update_check,
            updater::update_install,
            updater::app_restart,
        ])
        .run(tauri::generate_context!())
        .expect("교수님만의 AI 연구실을 시작하지 못했습니다");
}
