// Navya Studio — Tauri 2 application core.
// The GUI entry point lives here; main.rs only delegates so the crate also
// builds as an rlib for tests and future headless binaries.

pub mod commands;
pub mod config;
pub mod control;
pub mod engine;
pub mod errors;
pub mod events;
pub(crate) mod harness;
pub(crate) mod navya;
pub mod preview;
pub(crate) mod render;
pub(crate) mod sd;
pub(crate) mod sidecar;
pub mod state;
pub(crate) mod store;
pub mod timeline;

use std::path::PathBuf;

use tauri::Manager;
use tokio::sync::broadcast;

use crate::control::EventChannel;
use crate::state::AppState;

/// Resolve the app-data dir + config path.
fn config_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("navya-config.json")
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();

            // Config: load from app-data JSON (or default + persist).
            let cfg_path = config_path(&handle);
            let config = state::load_config(cfg_path.clone());

            // Store: SQLite in app-data dir.
            let data_dir = handle
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let store = match store::ProjectStore::new(data_dir.join("navya.db")) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("opening store failed, falling back to in-memory: {e}");
                    store::ProjectStore::new(":memory:").unwrap_or_else(|_| {
                        // Last-resort: an in-memory connection we know works.
                        store::ProjectStore {
                            conn: rusqlite::Connection::open_in_memory()
                                .expect("in-memory sqlite"),
                        }
                    })
                }
            };

            // Build + manage the shared state (wrapped in Arc for sharing).
            let state = AppState::new(config, cfg_path, store);
            app.manage(std::sync::Arc::new(state));

            // Create the event broadcast channel for the control server.
            // Commands send events here; the control server forwards to WS clients.
            let event_tx: EventChannel = broadcast::Sender::new(100);
            let event_tx_clone = event_tx.clone();
            app.manage(event_tx);

            // Launch the control server in the background. Use Tauri's async
            // runtime handle (not tokio::spawn) — the setup closure runs on the
            // main thread outside a Tokio runtime context, so tokio::spawn would
            // panic with "no reactor running".
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match control::launch(&app_handle, event_tx_clone).await {
                    Ok(handle) => {
                        tracing::info!("control server started at {}", handle.url);
                    }
                    Err(e) => {
                        tracing::warn!("control server launch failed: {e}");
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_config,
            commands::save_config,
            commands::set_api_key,
            commands::clear_api_key,
            commands::has_api_key,
            commands::new_project,
            commands::list_projects,
            commands::open_project,
            commands::set_model,
            commands::set_source,
            commands::set_harness,
            commands::list_models,
            commands::send_prompt,
            commands::steer,
            commands::abort,
            commands::render_to_video,
            commands::list_renders,
            commands::cancel_render,
            commands::preview_start,
            commands::preview_stop,
            commands::preview_status,
            commands::get_sidecar_status,
            commands::timeline::get_timeline,
            commands::timeline::snapshot,
            commands::reveal_in_folder,
            commands::generate_image,
            commands::list_cloud_models,
            commands::detect_engine,
            commands::approve,
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}