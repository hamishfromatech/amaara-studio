//! Control server (Phase 3).
//!
//! A loopback HTTP + WebSocket server that bridges the harness tool path:
//!   - `POST /tool/:name` — dispatches tool calls from the harness extension / MCP server
//!   - `WS /events` — streams `StudioEvent`s to all connected clients
//!   - `GET /health` — liveness probe
//!   - `GET /config` — returns the current NavyaConfig
//!
//! The server binds to `127.0.0.1:0` (ephemeral port), generates a random
//! bearer token, writes it to the OS keyring, and makes the URL + token
//! available to the harness adapter so it can pass them as env vars to the
//! spawned harness process.
//!
//! The event broadcast channel is shared with `AppState` so commands can
//! forward `studio://event` emissions to WebSocket clients.

mod dispatch;
mod server;
pub use server::ControlServerHandle;

use tokio::sync::broadcast;

use crate::events::StudioEvent;

/// Shared broadcast channel for studio events. Commands send events here
/// and the control server forwards them to WebSocket clients.
pub type EventChannel = broadcast::Sender<StudioEvent>;

/// The control server state shared across the app.
#[derive(Clone)]
pub struct ControlServerState {
    /// The server's own URL (e.g. `http://127.0.0.1:54321`).
    pub url: String,
    /// The bearer token used for auth on POST /tool/:name.
    pub token: String,
}

impl ControlServerState {
    pub fn new(url: String, token: String) -> Self {
        Self { url, token }
    }
}

/// Launch the control server and return a handle for broadcasting events.
pub async fn launch(
    app: &tauri::AppHandle,
    event_tx: EventChannel,
) -> Result<ControlServerHandle, String> {
    let handle = server::launch(app, event_tx).await?;
    Ok(handle)
}
