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
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::broadcast;

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
    // NOTE: no CORS layer. The HTTP consumers (harness extension, MCP server)
    // are non-browser clients that ignore CORS; a permissive layer would let
    // any website the user visits READ responses from this server via
    // CORS-enabled fetch (e.g. GET /config with its MCP env vars).
    let router = Router::new()
        .route("/tool/:name", post(crate::control::dispatch::dispatch_tool))
        .route("/events", get(events_ws))
        .route("/health", get(health))
        .route("/config", get(config_handler))
        .with_state(server_state);

    // Bind to 127.0.0.1:0.
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(l) => l,
        Err(e) => return Err(format!("control server bind failed: {e}")),
    };

    let addr = listener
        .local_addr()
        .map_err(|e| format!("control server addr: {e}"))?;
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
///
/// Browsers do NOT enforce CORS on WebSocket handshakes, so without an Origin
/// check any website in the user's browser could connect and snoop on studio
/// events (prompts, tool output, render paths) — cross-site WebSocket
/// hijacking. Reject upgrade requests that carry a non-Tauri, non-loopback
/// Origin (browsers always send Origin; non-browser clients usually don't).
async fn events_ws(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<ServerState>,
) -> Response {
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        if !origin_allowed(origin) {
            return (StatusCode::FORBIDDEN, "cross-origin websocket denied").into_response();
        }
    }
    ws.on_upgrade(move |socket: WebSocket| handle_ws(socket, state))
}

/// Allow the Tauri webview origins and dev-server loopback origins. A public
/// website's origin never matches — that is the point.
fn origin_allowed(origin: &str) -> bool {
    const TAURI_ORIGINS: [&str; 3] = [
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
    ];
    if TAURI_ORIGINS.contains(&origin) {
        return true;
    }
    // Dev servers (vite/webpack) are loopback — safe: a malicious website is
    // never served from 127.0.0.1/localhost.
    origin.starts_with("http://localhost:")
        || origin.starts_with("http://127.0.0.1:")
        || origin.starts_with("https://localhost:")
        || origin.starts_with("https://127.0.0.1:")
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

/// GET /config — return current config. Bearer-token authenticated like the
/// tool route: the config embeds user MCP server env vars, which may hold API
/// keys. No current consumer lacks the token (the extension gets it via env).
async fn config_handler(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    let expected = state.app.control_token.lock().clone();
    if !crate::control::dispatch::token_matches(&headers, expected.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
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

/// Generate a control-server bearer token: 32 random bytes, hex-encoded.
/// (The previous time-based token was guessable by anything that knew the
/// app's launch time.)
fn generate_token() -> String {
    let bytes: [u8; 32] = rand::random();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("amaara-{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AmaaraConfig;
    use crate::state::AppState;
    use crate::store::ProjectStore;
    use axum::body::{to_bytes, Body};
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    /// The token the "harness" presents in tests.
    const TEST_TOKEN: &str = "amaara-test-token";

    fn test_state() -> ServerState {
        let store = ProjectStore::memory().expect("in-memory store");
        let app = AppState::new(
            AmaaraConfig::default(),
            std::path::PathBuf::from(":memory:"),
            store,
        );
        *app.control_token.lock() = Some(TEST_TOKEN.to_string());
        let (tx, _rx) = broadcast::channel(16);
        ServerState {
            app: Arc::new(app),
            events: tx,
        }
    }

    /// POST /tool/:name with a valid bearer token.
    fn tool_request(name: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(format!("/tool/{name}"))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {TEST_TOKEN}"))
            .body(Body::from(body.to_owned()))
            .unwrap()
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
        let req = tool_request("list_local_models", "{}");
        let res = router(state).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"], serde_json::json!(true));
        assert!(!json["result"]["cloud_models"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn dispatch_unknown_tool_is_error() {
        let state = test_state();
        let req = tool_request("nope", "{}");
        let res = router(state).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"], serde_json::json!(false));
        assert!(json["error"].as_str().unwrap().contains("unknown tool"));
    }

    #[tokio::test]
    async fn dispatch_generate_image_without_key_is_an_honest_error() {
        // Fake keyring with NO key → the cloud path must fail fast and
        // honestly (never attempt a real HTTP call in tests).
        crate::config::set_fake_keyring(true);
        let state = test_state();
        let app = state.app.clone();
        let req = tool_request(
            "generate_image",
            r#"{"project_id":"p1","prompt":"a black hole","composition_id":null,"model":null,"size":null}"#,
        );
        let res = router(state).oneshot(req).await.unwrap();
        // The tool path now runs the REAL generation (cloud, no API key in the
        // fake keyring) — it must fail honestly, and the old stub's fabricated
        // asset row must NOT appear.
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["ok"], serde_json::json!(false));
        let err = json["error"].as_str().unwrap_or("");
        assert!(
            err.contains("API key") || err.contains("keyring"),
            "unexpected error: {err}"
        );
        assert_eq!(app.store.lock().count("assets").unwrap(), 0);
    }

    #[tokio::test]
    async fn dispatch_without_token_is_unauthorized() {
        let state = test_state();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/tool/list_local_models")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let res = router(state).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn dispatch_with_wrong_token_is_unauthorized() {
        let state = test_state();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/tool/list_local_models")
            .header("content-type", "application/json")
            .header("authorization", "Bearer amaara-wrong-token")
            .body(Body::from("{}"))
            .unwrap();
        let res = router(state).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn origin_check_blocks_cross_site_websockets() {
        // A website in the user's browser must NOT be able to open the event
        // stream (cross-site WebSocket hijacking); Tauri + loopback dev
        // origins are fine, and non-browser clients (no Origin) pass.
        assert!(origin_allowed("tauri://localhost"));
        assert!(origin_allowed("http://tauri.localhost"));
        assert!(origin_allowed("https://tauri.localhost"));
        assert!(origin_allowed("http://localhost:5173"));
        assert!(origin_allowed("http://127.0.0.1:1420"));
        assert!(!origin_allowed("https://evil.example.com"));
        assert!(!origin_allowed("http://evil.example.com"));
        assert!(!origin_allowed("null"));
    }

    #[tokio::test]
    async fn config_requires_bearer_token() {
        // /config embeds user MCP env vars (potentially API keys) — it must
        // not be readable without the token.
        let state = test_state();
        let router = Router::new()
            .route("/config", get(config_handler))
            .with_state(state);
        let res = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
}
