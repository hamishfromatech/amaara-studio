//! Control server tool dispatch — bridges Tauri AppState → navya-tools crate.
//!
//! `dispatch_tool` is the axum handler for `POST /tool/:name`. It builds a
//! `ToolContext` from the live `AppState`, routes on the tool name, and
//! `await`s the navya-tools backend directly (no `block_on` — we're already on
//! the tokio runtime). The result is wrapped in a `{ok, result}` /
//! `{ok, error}` envelope for the harness extension / MCP server.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap},
    response::IntoResponse,
    Json,
};
use navya_tools::{GenerateImageReq, RenderToVideoReq, StoreRead, ToolContext};

use crate::state::AppState;

use super::server::ServerState;

// ---------------------------------------------------------------------------
// Trait adapters for AppState
// ---------------------------------------------------------------------------

/// StoreRead adapter backed by AppState.
pub struct AppStateStore {
    state: Arc<AppState>,
}

impl AppStateStore {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl StoreRead for AppStateStore {
    fn list_projects(&self) -> Result<Vec<navya_tools::ProjectRow>, anyhow::Error> {
        let store = self.state.store.lock();
        let projects = store.list_projects().map_err(|e| anyhow::anyhow!(e))?;
        Ok(projects
            .into_iter()
            .map(|p| navya_tools::ProjectRow {
                id: p.id,
                name: p.name,
                dir: p.dir,
                created_at_ms: p.created_at_ms,
                harness: p.harness,
                model: p.model,
                source: p.source,
            })
            .collect())
    }

    fn insert_asset(
        &self,
        project_id: &str,
        composition_id: Option<&str>,
        path: &str,
        kind: &str,
        source: &str,
        prompt: Option<&str>,
    ) -> Result<String, anyhow::Error> {
        let store = self.state.store.lock();
        store
            .insert_asset(project_id, composition_id, path, kind, source, prompt)
            .map_err(|e| anyhow::anyhow!(e))
    }
}

/// SidecarView adapter backed by the live supervisor.
pub struct AppStateSidecars {
    state: Arc<AppState>,
}

impl navya_tools::SidecarView for AppStateSidecars {
    fn status(&self, name: &str) -> Option<navya_tools::SidecarStatus> {
        self.state.supervisor.status(name).map(|s| match s {
            crate::sidecar::SidecarStatus::Starting => navya_tools::SidecarStatus::Starting,
            crate::sidecar::SidecarStatus::Running => navya_tools::SidecarStatus::Running,
            crate::sidecar::SidecarStatus::Stopped => navya_tools::SidecarStatus::Stopped,
            crate::sidecar::SidecarStatus::Exited(c) => navya_tools::SidecarStatus::Exited(c),
        })
    }
}

/// Build a ToolContext from the AppState. Resolves the current project dir from
/// the active session so tool fns operate on the real project tree.
pub fn make_tool_ctx(state: &Arc<AppState>) -> ToolContext {
    let config = state.config.lock().clone();
    let project_dir = {
        let session = state.session.lock();
        let store = state.store.lock();
        session
            .current_project_id
            .as_ref()
            .and_then(|id| {
                store
                    .list_projects()
                    .ok()
                    .and_then(|ps| ps.into_iter().find(|p| p.id == *id))
            })
            .map(|p| std::path::PathBuf::from(p.dir))
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    };

    let store = AppStateStore::new(state.clone());
    let sidecars = AppStateSidecars { state: state.clone() };

    ToolContext {
        project_dir,
        config: Box::new(navya_tools::ConfigRef {
            navya_base_url: config.navya_base_url.clone(),
            default_model: config.default_model.clone(),
            use_auto_router: config.use_auto_router,
            byok: config.byok,
            enabled_harnesses: config.enabled_harnesses.clone(),
            local_llama_url: config.local_llama_url.clone(),
        }),
        store: Box::new(store),
        sidecars: Box::new(sidecars),
    }
}

// ---------------------------------------------------------------------------
// axum handler
// ---------------------------------------------------------------------------

/// POST /tool/:name — dispatch a tool call to the navya-tools backend.
///
/// Bearer-token authenticated: the harness extension sends the token the
/// adapter passed to it via `NAVYA_CONTROL_TOKEN`. The server is loopback-
/// only, but the token is the documented contract and stops any other local
/// process from driving the studio's tools.
pub async fn dispatch_tool(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let expected = state.app.control_token.lock().clone();
    if !token_matches(&headers, expected.as_deref()) {
        return (axum::http::StatusCode::UNAUTHORIZED, Json(serde_json::json!({
            "ok": false,
            "error": "unauthorized: missing or invalid bearer token",
        })));
    }
    let ctx = make_tool_ctx(&state.app);

    let out: Result<serde_json::Value, String> = match name.as_str() {
        "generate_image" => dispatch_generate_image(&ctx, body).await,
        "render_to_video" => dispatch_render_to_video(&ctx, body).await,
        "list_local_models" => dispatch_list_models(&ctx).await,
        "set_generation_source" => dispatch_set_source(&ctx, body).await,
        "get_project_state" => dispatch_project_state(&ctx).await,
        "snapshot" => dispatch_snapshot(&ctx, body).await,
        other => Err(format!("unknown tool: {other}")),
    };

    match out {
        Ok(result) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({ "ok": true, "result": result })),
        ),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": error })),
        ),
    }
}

// --- per-tool dispatchers (each returns the serialized result or an error) ---

async fn dispatch_generate_image(
    ctx: &ToolContext,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let req: GenerateImageReq =
        serde_json::from_value(body).map_err(|e| format!("invalid args: {e}"))?;
    let out = navya_tools::generate_image(ctx, req).await.map_err(|e| e.to_string())?;
    serde_json::to_value(out).map_err(|e| e.to_string())
}

async fn dispatch_render_to_video(
    ctx: &ToolContext,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let req: RenderToVideoReq =
        serde_json::from_value(body).map_err(|e| format!("invalid args: {e}"))?;
    let out = navya_tools::render_to_video(ctx, req).await.map_err(|e| e.to_string())?;
    serde_json::to_value(out).map_err(|e| e.to_string())
}

async fn dispatch_list_models(ctx: &ToolContext) -> Result<serde_json::Value, String> {
    let out = navya_tools::list_local_models(ctx).await.map_err(|e| e.to_string())?;
    serde_json::to_value(out).map_err(|e| e.to_string())
}

async fn dispatch_set_source(
    ctx: &ToolContext,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let source = body
        .get("source")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "missing or invalid `source` field".to_string())?;
    navya_tools::set_generation_source(ctx, source)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::Value::Null)
}

async fn dispatch_project_state(ctx: &ToolContext) -> Result<serde_json::Value, String> {
    let out = navya_tools::get_project_state(ctx).await.map_err(|e| e.to_string())?;
    serde_json::to_value(out).map_err(|e| e.to_string())
}

async fn dispatch_snapshot(
    ctx: &ToolContext,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let t_ms = body
        .get("timecode_ms")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "missing or invalid `timecode_ms` field".to_string())?;
    let out = navya_tools::snapshot(ctx, t_ms).await.map_err(|e| e.to_string())?;
    serde_json::to_value(out).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

/// Constant-time comparison of the `Authorization: Bearer <token>` header
/// against the server's token. Missing token or missing server token → reject.
fn token_matches(headers: &HeaderMap, expected: Option<&str>) -> bool {
    let Some(expected) = expected else { return false };
    let Some(auth) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let provided = match auth.strip_prefix("Bearer ") {
        Some(rest) => rest,
        None => match auth.strip_prefix("bearer ") {
            Some(rest) => rest,
            None => return false,
        },
    };
    constant_time_eq(provided.as_bytes(), expected.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
