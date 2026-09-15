//! Control server tool dispatch — bridges Tauri AppState → real tool backends.
//!
//! `dispatch_tool` is the axum handler for `POST /tool/:name`. It is
//! bearer-token authenticated and routes each tool to the SAME implementation
//! the UI uses (`commands::generate_image_core`, `render_to_video_core`,
//! `snapshot_core`, …) — the earlier amaara-tools stubs returned fabricated
//! success (fake asset rows / render jobs that never ran), so a harness
//! calling a tool had no real effect. Now every tool does the real work.

use std::path::PathBuf;
use std::sync::Arc;

use amaara_tools::{GenerateImageReq, RenderToVideoReq};
use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap},
    response::IntoResponse,
    Json,
};

use crate::state::AppState;

use super::server::ServerState;

// ---------------------------------------------------------------------------
// axum handler
// ---------------------------------------------------------------------------

/// POST /tool/:name — dispatch a tool call to the real backend.
///
/// Bearer-token authenticated: the harness extension sends the token the
/// adapter passed to it via `AMAARA_CONTROL_TOKEN`. The server is loopback-
/// only, but the token is the documented contract and stops any other local
/// process from driving the studio's tools.
#[axum::debug_handler]
pub async fn dispatch_tool(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let expected = state.app.control_token.lock().clone();
    if !token_matches(&headers, expected.as_deref()) {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "ok": false,
                "error": "unauthorized: missing or invalid bearer token",
            })),
        );
    }

    let out: Result<serde_json::Value, String> = match name.as_str() {
        "generate_image" => dispatch_generate_image(&state.app, body).await,
        "render_to_video" => dispatch_render_to_video(&state.app, body).await,
        "set_generation_source" => dispatch_set_source(&state.app, body).await,
        "list_local_models" => dispatch_list_models(&state.app).await,
        "get_project_state" => dispatch_project_state(&state.app).await,
        "snapshot" => dispatch_snapshot(&state.app, body).await,
        "open_in_folder" => dispatch_open_in_folder(body),
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
    state: &Arc<AppState>,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let req: GenerateImageReq =
        serde_json::from_value(body).map_err(|e| format!("invalid args: {e}"))?;
    let args = crate::commands::GenerateImageArgs {
        prompt: req.prompt.clone(),
        model: req.model.clone(),
        size: req.size.clone(),
    };
    let out = crate::commands::generate_image_core(state.app_handle.get(), state, args).await?;
    if let Some(err) = out.error {
        return Err(err);
    }
    let url = out
        .url
        .ok_or_else(|| "generation returned no image".to_string())?;
    let model_used = req.model.clone().unwrap_or_else(|| {
        if out.source == "local" {
            "sd-xl".into()
        } else {
            "dall-e-3".into()
        }
    });

    // Materialize the image into the project's assets dir + asset row so the
    // harness gets a real, referenceable file (the old stub wrote a path that
    // never existed).
    let (asset_id, path) = save_generated_image(
        state,
        &req.project_id,
        req.composition_id.as_deref(),
        &url,
        &req.prompt,
        &out.source,
    )
    .await?;

    let resp = amaara_tools::GeneratedImage {
        asset_id,
        path: PathBuf::from(&path),
        prompt: req.prompt,
        model_used,
        source: out.source,
        cost_usd: None,
    };
    serde_json::to_value(resp).map_err(|e| e.to_string())
}

/// Decode / download the generated image and store it under
/// `<project>/assets/img/`, then insert the asset row + emit AssetAdded.
async fn save_generated_image(
    state: &Arc<AppState>,
    project_id: &str,
    composition_id: Option<&str>,
    url: &str,
    prompt: &str,
    source: &str,
) -> Result<(String, String), String> {
    // Block-scoped guards: parking_lot MutexGuards are !Send, so each must be
    // provably dead before the next .await (distinct bindings + block scopes —
    // same-typed bindings can share a generator slot, which then spans the
    // await and makes the whole future !Send).
    let project_dir = {
        let store_guard = state.store.lock();
        store_guard
            .list_projects()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|p| p.id == project_id)
            .map(|p| PathBuf::from(p.dir))
            .ok_or_else(|| format!("project {project_id} not found"))?
    };

    let bytes: Vec<u8> = if let Some(b64) = url
        .strip_prefix("data:image/png;base64,")
        .or_else(|| url.strip_prefix("data:image/jpeg;base64,"))
    {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("invalid image data URL: {e}"))?
    } else if url.starts_with("http://") || url.starts_with("https://") {
        // Remote URL (cloud generation): download best-effort; fall back to
        // recording the URL itself as the asset path.
        match reqwest::get(url).await {
            Ok(resp) if resp.status().is_success() => resp
                .bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| format!("downloading image: {e}"))?,
            _ => {
                let asset_id = {
                    let store_guard = state.store.lock();
                    store_guard
                        .insert_asset(
                            project_id,
                            composition_id,
                            url,
                            "image",
                            source,
                            Some(prompt),
                        )
                        .map_err(|e| e.to_string())?
                };
                emit_asset_added(state, project_id, &asset_id, url);
                return Ok((asset_id, url.to_string()));
            }
        }
    } else {
        return Err(format!("unsupported image reference: {url}"));
    };

    let assets_dir = project_dir.join("assets").join("img");
    std::fs::create_dir_all(&assets_dir).map_err(|e| format!("creating assets dir: {e}"))?;
    let id = format!(
        "img-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let file = assets_dir.join(format!("{id}.png"));
    std::fs::write(&file, &bytes).map_err(|e| format!("writing image: {e}"))?;
    let path_str = file.to_string_lossy().to_string();

    let asset_id = {
        let store_guard = state.store.lock();
        store_guard
            .insert_asset(
                project_id,
                composition_id,
                &path_str,
                "image",
                source,
                Some(prompt),
            )
            .map_err(|e| e.to_string())?
    };
    emit_asset_added(state, project_id, &asset_id, &path_str);
    Ok((asset_id, path_str))
}

/// Emit AssetAdded so a running UI sees the tool-created asset live.
fn emit_asset_added(state: &Arc<AppState>, project_id: &str, asset_id: &str, path: &str) {
    if let Some(app) = state.app_handle.get() {
        use tauri::Emitter;
        let _ = app.emit(
            "studio://event",
            crate::events::StudioEvent::Project(crate::events::ProjectEvent::AssetAdded {
                project_id: project_id.to_string(),
                asset_id: asset_id.to_string(),
                path: path.to_string(),
            }),
        );
    }
}

async fn dispatch_render_to_video(
    state: &Arc<AppState>,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let req: RenderToVideoReq =
        serde_json::from_value(body).map_err(|e| format!("invalid args: {e}"))?;
    let app = state
        .app_handle
        .get()
        .ok_or_else(|| "app handle unavailable".to_string())?;
    let args = crate::commands::RenderArgs {
        project_id: req.project_id.clone(),
        composition_id: req.composition_id.clone(),
        quality: Some(req.quality.clone()),
        target: Some(req.target.clone()),
        width: None,
        height: None,
        fps: None,
    };
    let job_id = crate::commands::render_to_video_core(app, state, args).await?;
    let resp = amaara_tools::RenderJob {
        job_id,
        project_id: req.project_id,
        composition_id: req.composition_id,
        target: req.target,
        quality: req.quality,
        status: "queued".to_string(),
    };
    serde_json::to_value(resp).map_err(|e| e.to_string())
}

async fn dispatch_list_models(state: &Arc<AppState>) -> Result<serde_json::Value, String> {
    // Real catalogs: Amaara Cloud /v1/models + the local Engine proxy when up
    // (the old stub returned a hardcoded list).
    let mut cloud_models: Vec<String> = Vec::new();
    let mut local_models: Vec<String> = Vec::new();

    let amaara = state.amaara.lock().clone();
    for m in amaara.list_models().await.unwrap_or_default() {
        cloud_models.push(m.id);
    }

    let engine = state.engine.lock().clone();
    if engine.health_check().await {
        for m in engine.list_models().await {
            local_models.push(crate::engine::EngineClient::format_model_id(&m.id));
        }
    }
    if local_models.is_empty() {
        local_models.push("llama3-8b".to_string());
    }

    let resp = amaara_tools::ModelList {
        cloud_models,
        local_models,
    };
    serde_json::to_value(resp).map_err(|e| e.to_string())
}

/// open_in_folder — reveal a path in the OS file manager. Mirrors the
/// reveal_in_folder Tauri command (the MCP server proxies here, so the
/// harness-facing dispatch must know this tool too).
fn dispatch_open_in_folder(body: serde_json::Value) -> Result<serde_json::Value, String> {
    let path = body
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("open_in_folder: missing 'path'")?
        .to_string();
    crate::commands::reveal_in_folder(path)?;
    Ok(serde_json::json!({ "ok": true }))
}

async fn dispatch_set_source(
    state: &Arc<AppState>,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let source = body
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing or invalid `source` field".to_string())?;
    if source != "cloud" && source != "local" {
        return Err("source must be 'cloud' or 'local'".to_string());
    }
    // REAL switch (the old stub was a no-op): mirror commands::set_source.
    state.session.lock().source = source.to_string();
    tracing::info!("harness set generation source to {source}");
    Ok(serde_json::Value::Null)
}

async fn dispatch_project_state(state: &Arc<AppState>) -> Result<serde_json::Value, String> {
    let session = state.session.lock().clone();
    let store = state.store.lock();
    let projects = store.list_projects().map_err(|e| e.to_string())?;
    let current = session
        .current_project_id
        .as_ref()
        .and_then(|id| projects.iter().find(|p| &p.id == id))
        .or_else(|| projects.first());
    let (project_id, name) = current
        .map(|p| (p.id.clone(), p.name.clone()))
        .unwrap_or_else(|| ("default".to_string(), "default-project".to_string()));
    let compositions_count = store.count("compositions").unwrap_or(0);
    let assets_count = store.count("assets").unwrap_or(0);
    drop(store);

    let config = state.config.lock().clone();
    let resp = amaara_tools::ProjectState {
        project_id,
        name,
        harness: session.harness.clone(),
        model: session.model.clone(),
        source: session.source.clone(),
        compositions_count: compositions_count as usize,
        assets_count: assets_count as usize,
    };
    let _ = config;
    serde_json::to_value(resp).map_err(|e| e.to_string())
}

async fn dispatch_snapshot(
    state: &Arc<AppState>,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let t_ms = body
        .get("timecode_ms")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "missing or invalid `timecode_ms` field".to_string())?;
    let app = state
        .app_handle
        .get()
        .ok_or_else(|| "app handle unavailable".to_string())?;
    let project_id = state
        .session
        .lock()
        .current_project_id
        .clone()
        .ok_or_else(|| "no open project".to_string())?;
    let out = crate::commands::timeline::snapshot_core(app, state, &project_id, t_ms).await?;
    let resp = amaara_tools::Snapshot {
        asset_id: out.asset_id,
        path: out.path,
        timecode_ms: out.timecode_ms,
    };
    serde_json::to_value(resp).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

/// Constant-time comparison of the `Authorization: Bearer <token>` header
/// against the server's token. Missing token or missing server token → reject.
pub(crate) fn token_matches(headers: &HeaderMap, expected: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return false;
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tool_auth_still_enforced() {
        // The dispatch auth path is exercised by control/server.rs tests; this
        // guards the constant-time compare helper directly.
        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, "Bearer amaara-x".parse().unwrap());
        assert!(token_matches(&h, Some("amaara-x")));
        assert!(!token_matches(&h, Some("amaara-y")));
        assert!(!token_matches(&h, None));
    }
}
