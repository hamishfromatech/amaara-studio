//! Control server implementation (Phase 3).
//!
//! Axum HTTP + WebSocket server bound to 127.0.0.1:0 (ephemeral port).
//! All tool calls from the harness extension / MCP server route through here.
//!
//! Routes:
//!   POST /tool/:name  — dispatch tool calls (bearer token auth)
//!   WS   /events      — stream StudioEvents to all connected clients
//!   GET  /health      — liveness probe
//!   GET  /config      — return current config

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tauri::Manager;

use crate::config::SERVICE_CONTROL;
use crate::events::StudioEvent;
use crate::state::AppState;

/// The axum router state: the app state (for tool dispatch + config) and the
/// event broadcast sender (for the WS /events stream).
#[derive(Clone)]
pub struct ServerState {
    pub app: Arc<AppState>,
    pub events: broadcast::Sender<StudioEvent>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Handle for broadcasting studio events from the control server.
#[derive(Clone)]
pub struct ControlServerHandle {
    /// The server's URL (e.g. `http://127.0.0.1:54321`).
    pub url: String,
    /// The bearer token used for auth on POST /tool/:name.
    pub token: String,
    event_tx: broadcast::Sender<StudioEvent>,
}

impl ControlServerHandle {
    /// Broadcast a studio event to all WebSocket clients.
    pub fn broadcast_event(&self, event: StudioEvent) {
        let _ = self.event_tx.send(event);
    }
}

/// Launch the control server. Returns a handle with the server URL and token.
pub async fn launch(
    app: &tauri::AppHandle,
    event_tx: broadcast::Sender<StudioEvent>,
) -> Result<ControlServerHandle, String> {
    // Generate a random token.
    let token = generate_token();

    // Write the token to the OS keyring.
    if let Err(e) = crate::config::set_secret(SERVICE_CONTROL, "token", token.clone()) {
        tracing::warn!("failed to write control token to keyring: {e}");
    }

    // Build the app state reference (Arc<AppState> stored in Tauri).
    let state_ref = app.try_state::<Arc<AppState>>().ok_or("no AppState")?;
    let app_state: Arc<AppState> = Arc::clone(&*state_ref);

    // Router state carries both the app state and the event broadcast sender.
    let server_state = ServerState {
        app: app_state.clone(),
        events: event_tx.clone(),
    };

    // Build the router with named handlers.
    let router = Router::new()
        .route("/tool/:name", post(crate::control::dispatch::dispatch_tool))
        .route("/events", get(events_ws))
        .route("/health", get(health))
        .route("/config", get(config_handler))
        .with_state(server_state)
        .layer(CorsLayer::permissive());

    // Bind to 127.0.0.1:0.
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(l) => l,
        Err(e) => return Err(format!("control server bind failed: {e}")),
    };

    let addr = listener.local_addr().map_err(|e| format!("control server addr: {e}"))?;
    let url = format!("http://{addr}");

    // Store the URL + token in the app state.
    {
        let inner = Arc::clone(&*state_ref);
        *inner.control_url.lock() = Some(url.clone());
        *inner.control_token.lock() = Some(token.clone());
    }

    // Spawn the server. It lives for the lifetime of the process (see
    // shutdown_signal); when the Tauri app exits the spawned task is dropped.
    tokio::spawn(async move {
        let server = axum::serve(listener, router).with_graceful_shutdown(shutdown_signal());
        if let Err(e) = server.await {
            tracing::error!("control server error: {e}");
        }
    });

    Ok(ControlServerHandle {
        url,
        token,
        event_tx,
    })
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// WS /events — stream StudioEvents to all connected clients.
async fn events_ws(
    ws: WebSocketUpgrade,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket: WebSocket| handle_ws(socket, state))
}

async fn handle_ws(socket: WebSocket, state: ServerState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.events.subscribe();

    // Forward broadcast events to this client until the socket closes.
    let forward = async {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let Some(json) = serde_json::to_string(&event).ok() else {
                        continue;
                    };
                    if sender.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
                // We fell behind; skip the missed events and keep going.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                // The broadcast channel was dropped (app shutting down).
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    // Drain inbound frames (pings / client-initiated close) until the client
    // disconnects.
    let drain = async {
        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }
    };

    // End when either direction finishes.
    tokio::select! {
        _ = forward => {}
        _ = drain => {}
    }
}

/// GET /health — liveness probe.
async fn health() -> axum::response::Response {
    "ok".into_response()
}

/// GET /config — return current config.
async fn config_handler(
    State(state): State<ServerState>,
) -> axum::response::Response {
    let config = state.app.config.lock().clone();
    serde_json::to_string_pretty(&config)
        .map(|s| s.into_response())
        .unwrap_or_else(|_| "error".into_response())
}

// ---------------------------------------------------------------------------
// Graceful shutdown
// ---------------------------------------------------------------------------

async fn shutdown_signal() {
    // The control server lives for the lifetime of the process. A GUI app has
    // no Ctrl+C; when the Tauri window closes the process exits and this
    // spawned task is dropped, tearing the server down. So we simply wait
    // forever — the graceful-shutdown future never completes on its own.
    std::future::pending::<()>().await;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a unique-enough token for development.
fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("navya-{:x}", now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NavyaConfig;
    use crate::state::AppState;
    use crate::store::ProjectStore;
    use axum::body::{to_bytes, Body};
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    fn test_state() -> ServerState {
        let store = ProjectStore::memory().expect("in-memory store");
        let app = AppState::new(
            NavyaConfig::default(),
            std::path::PathBuf::from(":memory:"),
            store,
        );
        let (tx, _rx) = broadcast::channel(16);
        ServerState {
            app: Arc::new(app),
            events: tx,
        }
    }

    fn router(state: ServerState) -> Router<()> {
        Router::new()
            .route("/tool/:name", post(crate::control::dispatch::dispatch_tool))
            .route("/health", get(health))
            .with_state(state)
    }

    #[tokio::test]
    async fn dispatch_list_models_round_trip() {
        let state = test_state();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/tool/list_local_models")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let res = router(state).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"], serde_json::json!(true));
        assert!(json["result"]["cloud_models"].as_array().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn dispatch_unknown_tool_is_error() {
        let state = test_state();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/tool/nope")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let res = router(state).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"], serde_json::json!(false));
        assert!(json["error"].as_str().unwrap().contains("unknown tool"));
    }

    #[tokio::test]
    async fn dispatch_generate_image_inserts_asset() {
        let state = test_state();
        let app = state.app.clone();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/tool/generate_image")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"project_id":"p1","prompt":"a black hole","composition_id":null,"model":null,"size":null}"#,
            ))
            .unwrap();
        let res = router(state).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"], serde_json::json!(true));
        // The asset row should now exist in the store (real insert, not a stub).
        assert_eq!(app.store.lock().count("assets").unwrap(), 1);
    }
}
