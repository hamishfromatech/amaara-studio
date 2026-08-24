// Navya Studio — Tauri 2 application core (Phase 0 scaffold).
// The GUI entry point lives here; main.rs only delegates so the crate also
// builds as an rlib for tests and future headless binaries.

pub mod config;
pub mod events;
pub(crate) mod harness;
pub(crate) mod render;
pub(crate) mod sidecar;
pub(crate) mod store;

use std::sync::{OnceLock, Mutex};

use tauri::Manager;

static SUPERVISOR: OnceLock<sidecar::Supervisor> = OnceLock::new();

/// Shared supervisor instance (safe to clone the guard).
pub fn supervisor() -> &'static sidecar::Supervisor {
    SUPERVISOR.get_or_init(sidecar::Supervisor::new)
}

fn ensure_store(app_handle: &tauri::app::Handle<()>) -> Result<(), store::StoreError> {
    let data_dir = app_handle.path().app_data_dir();
    store::ProjectStore::new(data_dir.join("navya.db"))
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Shell + store plugins are wired in `main`; sql (Phase 1) is optional.
    let _ = tauri_plugin_shell::init();
    let _ = tauri_plugin_store::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::init())
        .setup(|app| {
            // Open the project store so later phases can query history.
            let _ = ensure_store(app.handle());
            // Publish initial sidecar status to the webview (design.md status strip).
            let _ = sidecar::Supervisor::emit(app.handle());
            // Onboarding hook (Phase 16). No-op in Phase 0.
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
