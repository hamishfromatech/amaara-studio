//! Tauri commands — the real UI ↔ Rust bridge (production wiring).
//!
//! Every `#[tauri::command]` here is registered in `lib.rs` via
//! `generate_handler!` and called from the webview through `invoke()`. Commands
//! operate on the managed `AppState` (config, store, session, render queue,
//! supervisor) and emit `studio://event` for anything the UI should observe
//! live (project switches, render progress, sidecar status, errors).

pub mod timeline;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use crate::config::{self, NavyaConfig, SERVICE_NAVYA};
use crate::events::{ProjectEvent, PreviewEvent, StudioEvent};
use crate::render::{RenderJob, RenderQuality, RenderStatus, RenderTarget};
use crate::preview::{poll_ready, spawn_preview};
use crate::state::{AppState, ModelEntry, Session, StateSnapshot, cloud_models};

// ---------------------------------------------------------------------------
// State + config
// ---------------------------------------------------------------------------

/// Full snapshot for the UI on mount + after any mutation.
#[tauri::command]
pub async fn get_state(state: State<'_, std::sync::Arc<AppState>>) -> Result<StateSnapshot, String> {
    let has_api_key = config::get_secret(SERVICE_NAVYA, "api-key")
        .map_err(|e| e.to_string())?
        .is_some();
    let mut snap = (*state).snapshot(has_api_key);
    // Projects require the async store lock.
    let store = state.store.lock();
    snap.projects = store.list_projects().map_err(|e| e.to_string())?;
    Ok(snap)
}

#[tauri::command]
pub fn get_config(state: State<'_, std::sync::Arc<AppState>>) -> Result<NavyaConfig, String> {
    Ok(state.config.lock().clone())
}

/// Persist config to the app-data JSON file it was loaded from.
#[tauri::command]
pub fn save_config(state: State<'_, std::sync::Arc<AppState>>, config: NavyaConfig) -> Result<NavyaConfig, String> {
    let path = state.config_path.clone();
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    *state.config.lock() = config.clone();
    // Re-sync the Navya client with the new endpoint/router flags.
    *state.navya.lock() = crate::navya::NavyaClient::new(
        config.navya_base_url.clone(),
        config.use_auto_router,
        config.byok,
    );
    // Re-sync the Engine client with the new local proxy URL.
    *state.engine.lock() = crate::engine::EngineClient::new(&config.engine_url);
    Ok(config)
}

// ---------------------------------------------------------------------------
// Secrets (Navya API key) — keyring only, never on disk.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn set_api_key(key: String) -> Result<(), String> {
    config::set_secret(SERVICE_NAVYA, "api-key", key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_api_key() -> Result<(), String> {
    // Best-effort delete via set empty (keyring delete is backend-specific).
    config::set_secret(SERVICE_NAVYA, "api-key", String::new()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn has_api_key() -> Result<bool, String> {
    Ok(
        config::get_secret(SERVICE_NAVYA, "api-key")
            .map_err(|e| e.to_string())?
            .filter(|k| !k.is_empty())
            .is_some(),
    )
}

// ---------------------------------------------------------------------------
// Projects (SQLite)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct NewProjectArgs {
    pub name: String,
    pub dir: String,
}

#[tauri::command]
pub async fn new_project(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    args: NewProjectArgs,
) -> Result<crate::store::ProjectRow, String> {
    let id = format!("p{}", chrono_like_id());
    // Normalize the requested dir to a real absolute path (onboarding passes
    // "." — a relative dir would make the harness spawn in the app's CWD).
    let dir = resolve_project_dir(&app, &id, &args.dir)?;
    let session = state.session.lock().clone();
    let store = state.store.lock();
    let row = store
        .create_project(&id, &args.name, dir.to_string_lossy().as_ref(), &session.harness, &session.model, &session.source)
        .map_err(|e| e.to_string())?;

    // Switch the session to the new project.
    {
        let mut s = state.session.lock();
        s.current_project_id = Some(row.id.clone());
    }
    let _ = app.emit(
        "studio://event",
        StudioEvent::Project(ProjectEvent::Created {
            project_id: row.id.clone(),
            name: row.name.clone(),
        }),
    );
    Ok(row)
}

#[tauri::command]
pub async fn list_projects(state: State<'_, std::sync::Arc<AppState>>) -> Result<Vec<crate::store::ProjectRow>, String> {
    let store = state.store.lock();
    store.list_projects().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_project(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    project_id: String,
) -> Result<(), String> {
    let store = state.store.lock();
    let projects = store.list_projects().map_err(|e| e.to_string())?;
    let proj = projects
        .into_iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("project {project_id} not found"))?;
    {
        let mut s = state.session.lock();
        s.current_project_id = Some(proj.id.clone());
        s.harness = proj.harness.clone();
        s.model = proj.model.clone();
        s.source = proj.source.clone();
    }
    let _ = app.emit(
        "studio://event",
        StudioEvent::Project(ProjectEvent::Switched {
            project_id: proj.id,
            name: proj.name,
        }),
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Session: model / source / harness pickers
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn set_model(state: State<'_, std::sync::Arc<AppState>>, model: String) -> Result<Session, String> {
    {
        let mut s = state.session.lock();
        s.model = model.clone();
    }
    // Forward mid-session to the running harness process (a-coder-cli: the
    // set_model RPC switches models without ending the session). A
    // not-yet-started harness picks the model up from the session context on
    // its next start, so failures here are logged, never fatal.
    let session = state.session.lock().clone();
    let harness = {
        let registry = state.harness_registry.lock();
        registry.get(&session.harness)
    };
    if let Some(harness) = harness {
        if let Err(e) = harness.set_model(&model).await {
            tracing::debug!("set_model not forwarded to {}: {e}", session.harness);
        }
    }
    Ok(session)
}

#[tauri::command]
pub fn set_source(state: State<'_, std::sync::Arc<AppState>>, source: String) -> Result<Session, String> {
    if source != "cloud" && source != "local" {
        return Err("source must be 'cloud' or 'local'".to_string());
    }
    {
        let mut s = state.session.lock();
        s.source = source;
    }
    Ok(state.session.lock().clone())
}

#[tauri::command]
pub fn set_harness(state: State<'_, std::sync::Arc<AppState>>, harness: String) -> Result<Session, String> {
    {
        let mut s = state.session.lock();
        s.harness = harness;
    }
    Ok(state.session.lock().clone())
}

#[tauri::command]
pub async fn list_models(state: State<'_, std::sync::Arc<AppState>>) -> Result<Vec<ModelEntry>, String> {
    let session = state.session.lock().clone();
    let mut models: Vec<ModelEntry> = Vec::new();

    // 1. The ACTIVE harness's models lead the picker: the prompt runs through
    //    that harness, so its catalog is what "set model" actually controls.
    //    a-coder-cli answers with a live RPC (get_available_models) once its
    //    process is up; the other adapters expose static descriptor catalogs.
    //    If the harness process isn't started yet, start it lazily (idempotent)
    //    so a-coder-cli can answer live; on failure fall back to the static
    //    catalog so the picker is never empty.
    let harness = {
        let registry = state.harness_registry.lock();
        registry.get(&session.harness)
    };
    if let Some(harness) = harness {
        let mut infos = match harness.available_models().await {
            Ok(list) => list,
            Err(_) => {
                // Not started (or harness has no listing yet) — lazy-start
                // with the current session context, then ask again.
                let ctx = crate::harness::HarnessCtx {
                    project_dir: current_project_dir(&state, &session),
                    model: session.model.clone(),
                    source: session.source.clone(),
                    control_url: state.control_url.lock().clone(),
                    control_token: state.control_token.lock().clone(),
                };
                match harness.start(&ctx).await {
                    Ok(()) => harness.available_models().await.unwrap_or_default(),
                    Err(_) => vec![],
                }
            }
        };
        if infos.is_empty() {
            // Static descriptor catalog — still the selected harness's models,
            // just not live.
            infos = crate::harness::registry::fallback_models(&session.harness);
        }
        for m in infos {
            let active = session.model == m.id;
            models.push(ModelEntry {
                id: m.id.clone(),
                name: m.name.unwrap_or_else(|| m.id.clone()),
                kind: m.kind,
                source: "harness".to_string(),
                active,
            });
        }
    }

    // 2. Cloud (image generation) + Engine + local models — unchanged.
    let mut cloud = cloud_models(&session.model);

    // Try to discover Navya Engine and list its models.
    let engine = state.engine.lock().clone();
    if engine.health_check().await {
        let engine_models = engine.list_models().await;
        for em in engine_models {
            let model_id = crate::engine::EngineClient::format_model_id(&em.id);
            cloud.push(ModelEntry {
                id: model_id.clone(),
                name: em.name,
                kind: "chat".to_string(),
                source: "engine".to_string(),
                active: session.model == model_id,
            });
        }
    }

    // Static fallback local models (shown when engine is offline).
    models.push(ModelEntry {
        id: "llama3-8b".to_string(),
        name: "Llama 3 8B (llama.cpp)".to_string(),
        kind: "chat".to_string(),
        source: "local".to_string(),
        active: session.model == "llama3-8b",
    });

    // Dedupe: a harness model id shadowing a cloud/engine id wins (the
    // harness is what actually runs).
    let harness_ids: std::collections::HashSet<String> = models
        .iter()
        .filter(|m| m.source == "harness")
        .map(|m| m.id.clone())
        .collect();
    models.retain(|m| m.source != "cloud" || !harness_ids.contains(&m.id));

    Ok(models)
}

// ---------------------------------------------------------------------------
// Agent (harness) — graceful degradation when the binary is missing.
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn send_prompt(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    msg: String,
    mode: String,
) -> Result<(), String> {
    let session = state.session.lock().clone();
    let harness = {
        let registry = state.harness_registry.lock();
        registry
            .get(&session.harness)
            .ok_or_else(|| format!("harness '{}' is not registered", session.harness))?
            .clone()
    };

    // Surface availability honestly before attempting a spawn. Detection is
    // descriptor-driven (binary resolution incl. fallback bins, plus a bounded
    // version probe), so a forked CLI like `openclaude` counts too.
    let descriptor = crate::harness::registry::descriptor_for(&session.harness);
    let available = descriptor
        .map(|d| crate::harness::registry::is_available(d.id))
        .unwrap_or(false);
    if !available {
        let install_hint = descriptor
            .and_then(|d| d.install_url)
            .map(|url| format!(" See {url} for install instructions."))
            .unwrap_or_default();
        let _ = app.emit(
            "studio://event",
            StudioEvent::Harness(crate::events::HarnessEvent::Error(format!(
                "{} is not installed or not on PATH.{}",
                descriptor.map(|d| d.label).unwrap_or(&session.harness),
                install_hint
            ))),
        );
        return Err(format!("{} is not installed", session.harness));
    }

    let ctx = crate::harness::HarnessCtx {
        project_dir: current_project_dir(&state, &session),
        model: session.model.clone(),
        source: session.source.clone(),
        control_url: state.control_url.lock().clone(),
        control_token: state.control_token.lock().clone(),
    };
    let prompt_mode = match mode.as_str() {
        "steer" => crate::harness::PromptMode::Steer,
        "follow_up" => crate::harness::PromptMode::FollowUp,
        _ => crate::harness::PromptMode::Normal,
    };
    harness.start(&ctx).await.map_err(|e| e.to_string())?;

    // Ensure exactly one event pump per harness: subscribe to the harness event
    // stream and re-emit every event on the studio://event channel for the UI.
    // The previous pump task is ABORTED when the harness changes — without
    // this, switching A→B→A left the original A pump alive alongside a new
    // one, duplicating every A event (and double-sending queued prompts).
    let need_pump = {
        let mut pump = state.pump_harness.lock();
        if pump.as_deref() != Some(session.harness.as_str()) {
            *pump = Some(session.harness.clone());
            true
        } else {
            false
        }
    };
    if need_pump {
        if let Some(old) = state.pump_handle.lock().take() {
            old.abort();
        }
        let mut rx = harness.subscribe();
        let app_pump = app.clone();
        let handle = tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                emit_harness_event(&app_pump, ev);
            }
        });
        *state.pump_handle.lock() = Some(handle);
    }

    harness.prompt(&msg, prompt_mode).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Convert a harness-side HarnessEvent to the UI-side events::HarnessEvent via a
/// serde round-trip (both enums share the same normalized shape), then emit it
/// on the studio://event channel.
fn emit_harness_event(app: &AppHandle, ev: crate::harness::event::HarnessEvent) {
    let Ok(value) = serde_json::to_value(&ev) else { return };
    let Ok(ui_ev) = serde_json::from_value::<crate::events::HarnessEvent>(value) else { return };
    let _ = app.emit("studio://event", StudioEvent::Harness(ui_ev));
}

// ---------------------------------------------------------------------------
// Approvals — user answers an ApprovalRequest; optionally persist a scoped
// allow rule, then relay the answer back to the harness.
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn approve(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    request_id: String,
    kind: String,
    payload: serde_json::Value,
    approved: bool,
    always_allow: bool,
    value: Option<String>,
) -> Result<(), String> {
    let session = state.session.lock().clone();
    let harness = {
        let registry = state.harness_registry.lock();
        registry
            .get(&session.harness)
            .ok_or_else(|| format!("harness '{}' not registered", session.harness))?
            .clone()
    };

    // Relay the answer back FIRST — persisting an "always allow" rule must
    // never block or swallow the user's decision (a failed rule write used to
    // return early here, dropping the approval entirely).
    let relay = harness.answer_approval(&request_id, approved, value).await;

    // Persist a scoped allow rule when the user checks "always allow". Best
    // effort: failures are surfaced as an event, never as a lost approval.
    if always_allow && approved {
        let req = crate::harness::approvals::ApprovalRequest {
            id: request_id.clone(),
            kind: kind.clone(),
            payload: payload.clone(),
        };
        let project_dir = current_project_dir(&state, &session);
        if let Err(e) =
            crate::harness::approvals::write_allow_rule(&session.harness, &project_dir, &req, true)
        {
            tracing::warn!("always-allow rule not persisted: {e}");
            let _ = app.emit(
                "studio://event",
                StudioEvent::Harness(crate::events::HarnessEvent::Error(e)),
            );
        }
    }

    match relay {
        Ok(()) => Ok(()),
        Err(crate::harness::HarnessError::NoApprovals) => {
            let _ = app.emit(
                "studio://event",
                StudioEvent::Harness(crate::events::HarnessEvent::Error(
                    "This harness does not support approval dialogs.".to_string(),
                )),
            );
            Err("approvals not supported".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Attachments — composer file/image attachments (design.md §3 compose box).
// The webview reads the picked file into base64 (no fs plugin needed) and the
// Rust core copies it into the project's assets dir so the harness (which
// spawns in the project dir) can read it. The prompt is referenced by
// project-relative path by the UI before sending.
// ---------------------------------------------------------------------------

/// One saved attachment, as reported back to the composer.
#[derive(Debug, Clone, Serialize)]
pub struct AttachmentInfo {
    pub id: String,
    pub name: String,
    /// Absolute path of the copied file (in the project assets dir).
    pub path: String,
    /// Project-relative path (what the prompt references).
    pub rel_path: String,
    pub kind: String, // image | file
    pub size_bytes: u64,
    pub created_at_ms: i64,
}

#[tauri::command]
pub async fn save_attachment(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    name: String,
    data_b64: String,
    kind: String,
) -> Result<AttachmentInfo, String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64.trim())
        .map_err(|e| format!("invalid attachment payload: {e}"))?;
    if bytes.is_empty() {
        return Err("attachment is empty".to_string());
    }
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("attachment exceeds the 64MB limit".to_string());
    }

    let session = state.session.lock().clone();
    let Some(project_id) = &session.current_project_id else {
        return Err("open a project before attaching files".to_string());
    };
    let project_dir = current_project_dir(&state, &session);
    let assets_dir = project_dir.join("assets");
    std::fs::create_dir_all(&assets_dir)
        .map_err(|e| format!("creating assets dir: {e}"))?;

    // Sanitize the display name (strip any path components the picker might
    // include) and make the on-disk name unique.
    let safe = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("attachment")
        .chars()
        .filter(|c| !matches!(c, '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect::<String>();
    let stem = safe.trim().to_string();
    let stem = if stem.is_empty() { "attachment".to_string() } else { stem };
    let mut path = assets_dir.join(&stem);
    let mut n = 1;
    while path.exists() {
        let (file_stem, ext) = match stem.rfind('.') {
            Some(i) if i > 0 => (stem[..i].to_string(), Some(stem[i..].to_string())),
            _ => (stem.clone(), None),
        };
        path = match ext {
            Some(e) => assets_dir.join(format!("{file_stem}~{n}{e}")),
            None => assets_dir.join(format!("{stem}~{n}")),
        };
        n += 1;
    }
    std::fs::write(&path, &bytes).map_err(|e| format!("writing attachment: {e}"))?;

    let rel = path
        .strip_prefix(&project_dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| stem.clone());
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let asset_kind = if kind == "image" { "image" } else { "file" };
    let id = state
        .store
        .lock()
        .insert_asset(project_id, None, &path.to_string_lossy().to_string(), asset_kind, "local", None)
        .map_err(|e| e.to_string())?;

    let _ = app.emit(
        "studio://event",
        StudioEvent::Project(ProjectEvent::AssetAdded {
            project_id: project_id.clone(),
            asset_id: id.clone(),
            path: path.to_string_lossy().to_string(),
        }),
    );

    Ok(AttachmentInfo {
        id,
        name: stem,
        path: path.to_string_lossy().to_string(),
        rel_path: rel,
        kind: asset_kind.to_string(),
        size_bytes: bytes.len() as u64,
        created_at_ms: now_ms,
    })
}

// ---------------------------------------------------------------------------
// MCP server status (Tools view) — where the built-in navya-mcp server lives,
// whether its runtime (uv) is installed, and the user's configured servers.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct McpStatus {
    /// Resolved navya-mcp workspace directory (NAVYA_MCP_DIR or bundled path).
    pub mcp_dir: String,
    /// Path to the FastMCP server entrypoint.
    pub server_py: String,
    pub server_exists: bool,
    /// `uv` on PATH (the runtime the harness adapters spawn the server with).
    pub uv_available: bool,
    pub uv_version: Option<String>,
    /// Control server the MCP tools proxy to (set at launch).
    pub control_url: Option<String>,
    pub has_control_token: bool,
    /// User-configured additional MCP servers (config JSON).
    pub mcp_servers: Vec<crate::config::McpServerConfig>,
}

#[tauri::command]
pub async fn get_mcp_status(state: State<'_, std::sync::Arc<AppState>>) -> Result<McpStatus, String> {
    let mcp_dir = crate::harness::common::mcp_workspace_dir();
    let server_py = mcp_dir.join("navya_mcp").join("server.py");

    let (uv_available, uv_version) = match crate::harness::registry::which_path("uv") {
        Some(p) => (true, crate::harness::registry::probe_version(&p, &["--version"])),
        None => (false, None),
    };

    let cfg = state.config.lock().clone();
    Ok(McpStatus {
        mcp_dir: mcp_dir.to_string_lossy().to_string(),
        server_py: server_py.to_string_lossy().to_string(),
        server_exists: server_py.is_file(),
        uv_available,
        uv_version,
        control_url: state.control_url.lock().clone(),
        has_control_token: state.control_token.lock().as_deref().map(|t| !t.is_empty()).unwrap_or(false),
        mcp_servers: cfg.mcp_servers,
    })
}

#[tauri::command]
pub async fn steer(app: AppHandle, state: State<'_, std::sync::Arc<AppState>>, msg: String) -> Result<(), String> {
    let session = state.session.lock().clone();
    let harness = {
        let registry = state.harness_registry.lock();
        registry
            .get(&session.harness)
            .ok_or_else(|| format!("harness '{}' not registered", session.harness))?
            .clone()
    };
    match harness.steer(&msg).await {
        Ok(()) => Ok(()),
        Err(crate::harness::HarnessError::NoSteer) => {
            let _ = app.emit(
                "studio://event",
                StudioEvent::Harness(crate::events::HarnessEvent::Error(
                    "This harness has no mid-stream steer — use Stop then re-send.".to_string(),
                )),
            );
            Err("no mid-stream steer".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn abort(state: State<'_, std::sync::Arc<AppState>>) -> Result<(), String> {
    let session = state.session.lock().clone();
    let harness = {
        let registry = state.harness_registry.lock();
        registry
            .get(&session.harness)
            .ok_or_else(|| format!("harness '{}' not registered", session.harness))?
            .clone()
    };
    harness.abort().await.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Render queue — real persistence + graceful degradation.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RenderArgs {
    pub project_id: String,
    pub composition_id: String,
    pub quality: Option<String>,
    pub target: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<u32>,
}

#[tauri::command]
pub async fn render_to_video(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    args: RenderArgs,
) -> Result<String, String> {
    render_to_video_core(&app, &state, args).await
}

/// Core render enqueuing shared by the UI command and the control-server tool
/// dispatch (harness-driven renders go through the exact same worker path).
pub async fn render_to_video_core(
    app: &AppHandle,
    state: &std::sync::Arc<AppState>,
    args: RenderArgs,
) -> Result<String, String> {
    let quality = match args.quality.as_deref().unwrap_or("draft") {
        "high" => RenderQuality::High,
        _ => RenderQuality::Draft,
    };
    let target = match args.target.as_deref().unwrap_or("local") {
        "docker" => RenderTarget::Docker,
        "cloud" => RenderTarget::Cloud,
        "lambda" => RenderTarget::Lambda,
        "cloudrun" => RenderTarget::CloudRun,
        _ => RenderTarget::Local,
    };

    let job_id = format!("r{}", chrono_like_id());
    let job = RenderJob {
        job_id: job_id.clone(),
        project_id: args.project_id,
        composition_id: args.composition_id,
        target,
        quality,
        width: args.width.unwrap_or(1280),
        height: args.height.unwrap_or(720),
        fps: args.fps.unwrap_or(30),
        status: RenderStatus::Queued,
        started_at_ms: None,
        finished_at_ms: None,
        output_path: None,
        error: None,
    };
    state.render_queue.lock().add(job.clone());
    persist_render_job(state, &job);

    let _ = app.emit(
        "studio://event",
        StudioEvent::Render(crate::events::RenderEvent::Started {
            job_id: job_id.clone(),
            target: format!("{:?}", job.target).to_lowercase(),
            quality: format!("{:?}", job.quality).to_lowercase(),
        }),
    );

    // Spawn the real render worker (Node + render-worker.mjs) in the background.
    let app2 = app.clone();
    let job2 = job.clone();
    tokio::spawn(async move {
        run_render(app2, job2).await;
    });

    Ok(job_id)
}

async fn run_render(app: AppHandle, job: RenderJob) {
    run_render_attempt(app, job, 0).await;
}

/// One attempt of a render job. `attempt` counts automatic retries (0 = the
/// initial run): a worker that exits without a completion event is retried
/// once after equal-jitter backoff (`retry.rs`) — a transient worker failure
/// shouldn't fail the user's render outright.
async fn run_render_attempt(app: AppHandle, job: RenderJob, attempt: u32) {
    // The managed state is `Arc<AppState>` (see lib.rs) — looking up the bare
    // `AppState` type would panic with "state not found" on every render.
    let state = app.state::<std::sync::Arc<AppState>>();
    // Resolve the render worker:
    //  1. bundled Tauri resource (installed app): <resource_dir>/binaries/render-worker.mjs
    //  2. dev fallback: src-tauri/binaries/render-worker.mjs via CARGO_MANIFEST_DIR
    let worker = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("binaries").join("render-worker.mjs"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("binaries")
                .join("render-worker.mjs")
        });
    if !worker.exists() {
        let _ = app.emit(
            "studio://event",
            StudioEvent::Render(crate::events::RenderEvent::Failed {
                job_id: job.job_id.clone(),
                error: format!("render worker not found at {}", worker.display()),
            }),
        );
        return;
    }

    // Spawn `node render-worker.mjs` with piped stdio.
    let mut cmd = tokio::process::Command::new("node");
    cmd.arg(&worker)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = app.emit(
                "studio://event",
                StudioEvent::Render(crate::events::RenderEvent::Failed {
                    job_id: job.job_id.clone(),
                    error: format!("failed to spawn node render worker: {e}. Is Node.js installed?"),
                }),
            );
            return;
        }
    };
    // Forward the worker's stderr to the sidecar log drawer (Phase 15) — the
    // worker prints node/npx diagnostics there. Stdout is pumped as JSONL below.
    if let Some(err) = child.stderr.take() {
        crate::sidecar::spawn_log_forwarder("render", err, "error", app.clone());
    }

    // Mark the job running.
    {
        let mut q = state.render_queue.lock();
        q.update_status(&job.job_id, RenderStatus::Running);
        if let Some(j) = q.get(&job.job_id) {
            persist_render_job(&state, j);
        }
    }
    let _ = app.emit(
        "studio://event",
        StudioEvent::Render(crate::events::RenderEvent::Progress {
            job_id: job.job_id.clone(),
            stage: "starting render worker".to_string(),
            frame: 0,
            total_frames: None,
        }),
    );

    let job_id = job.job_id.clone();
    let app_for_stdout = app.clone();
    let job_id_for_stdout = job_id.clone();

    // The worker shells out to `npx hyperframes render` with cwd = project_dir,
    // so the job must carry the real project directory (not the app's CWD).
    let project_dir = {
        let session = state.session.lock().clone();
        current_project_dir(&state, &session)
    };

    // Telemetry is opt-in (Phase 15): only pass `--no-telemetry` to HyperFrames
    // when the user has NOT enabled analytics, so no usage leaves the machine by
    // default. The flag rides on the job so the Node worker can honour it.
    let allow_telemetry = state.config.lock().share_analytics;

    // Write the render job to the worker's stdin.
    let mut stdin = child.stdin.take().expect("worker stdin");
    let payload = serde_json::json!({
        "type": "render",
        "job": {
            "job_id": job.job_id,
            "project_id": job.project_id,
            "composition_id": job.composition_id,
            "target": format!("{:?}", job.target).to_lowercase(),
            "quality": format!("{:?}", job.quality).to_lowercase(),
            "width": job.width,
            "height": job.height,
            "fps": job.fps,
            "project_dir": project_dir.to_string_lossy(),
            "telemetry": allow_telemetry,
        }
    });
    if let Err(e) = stdin.write_all(format!("{}\n", payload).as_bytes()).await {
        let _ = app.emit(
            "studio://event",
            StudioEvent::Render(crate::events::RenderEvent::Failed {
                job_id: job_id.clone(),
                error: format!("failed to write job to worker: {e}"),
            }),
        );
        return;
    }
    drop(stdin); // signal EOF when done (worker exits its readline loop)

    // Pump stdout: parse JSONL and emit RenderEvents.
    let stdout = child.stdout.take().expect("worker stdout");

    // Track the live worker so cancel_render can kill it. Registered after all
    // stdio handles were taken (they needed &mut child before the move).
    state.render_children.lock().insert(job_id.clone(), child);
    let reader = tokio::io::BufReader::new(stdout);
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let parsed: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let kind = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "progress" => {
                let _ = app_for_stdout.emit(
                    "studio://event",
                    StudioEvent::Render(crate::events::RenderEvent::Progress {
                        job_id: job_id_for_stdout.clone(),
                        stage: parsed.get("stage").and_then(|v| v.as_str()).unwrap_or("rendering").to_string(),
                        frame: parsed.get("frame").and_then(|v| v.as_u64()).unwrap_or(0),
                        total_frames: parsed.get("total_frames").and_then(|v| v.as_u64()),
                    }),
                );
            }
            "completed" => {
                // Never let a late completion override a user cancellation.
                let cancelled = state
                    .render_queue
                    .lock()
                    .get(&job_id_for_stdout)
                    .map(|j| j.status == RenderStatus::Cancelled)
                    .unwrap_or(false);
                if cancelled {
                    continue;
                }
                let out = parsed.get("output_path").and_then(|v| v.as_str()).unwrap_or("renders/out.mp4").to_string();
                {
                    let mut q = state.render_queue.lock();
                    q.complete(&job_id_for_stdout, &out);
                    if let Some(j) = q.get(&job_id_for_stdout) {
                        persist_render_job(&state, j);
                    }
                }
                let _ = app_for_stdout.emit(
                    "studio://event",
                    StudioEvent::Render(crate::events::RenderEvent::Completed {
                        job_id: job_id_for_stdout.clone(),
                        output_path: out,
                        duration_ms: None,
                    }),
                );
            }
            "failed" => {
                let cancelled = state
                    .render_queue
                    .lock()
                    .get(&job_id_for_stdout)
                    .map(|j| j.status == RenderStatus::Cancelled)
                    .unwrap_or(false);
                if cancelled {
                    continue;
                }
                let err = parsed.get("error").and_then(|v| v.as_str()).unwrap_or("render failed").to_string();
                {
                    let mut q = state.render_queue.lock();
                    q.update_status(&job_id_for_stdout, RenderStatus::Failed);
                    if let Some(j) = q.get(&job_id_for_stdout) {
                        persist_render_job(&state, j);
                    }
                }
                let _ = app_for_stdout.emit(
                    "studio://event",
                    StudioEvent::Render(crate::events::RenderEvent::Failed {
                        job_id: job_id_for_stdout.clone(),
                        error: err,
                    }),
                );
            }
            _ => {
                // Worker informational lines → sidecar log drawer (Phase 15).
                if kind == "log" {
                    let msg = parsed
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&line) // was the literal "&line" — a string
                        .to_string();
                    let _ = app_for_stdout.emit(
                        "studio://event",
                        StudioEvent::Sidecar(crate::events::SidecarEvent::LogLine {
                            name: "render".to_string(),
                            level: "info".to_string(),
                            message: msg,
                        }),
                    );
                }
            }
        }
    }

    // The worker exited (loop ended) — drop its handle from the live-children
    // map so cancel_render doesn't later kill a stale pid slot.
    state.render_children.lock().remove(&job_id);

    // If the worker exited without sending completed/failed, mark failed — in
    // both the queue and the event stream, so the UI never shows a stuck job.
    let still_running = state
        .render_queue
        .lock()
        .get(&job_id)
        .map(|j| j.status == RenderStatus::Running)
        .unwrap_or(false);
    if still_running {
        if attempt < 1 {
            // Transient retry (retry.rs, open-design B2): re-queue the job and
            // rerun once after backoff. Progress surfaces via the log drawer.
            let delay = crate::retry::compute_retry_backoff_ms(
                1,
                Some(crate::retry::FailureCategory::Transient),
                rand::random::<f64>,
            );
            tracing::info!("render {job_id}: worker exited without completion; retrying in {delay}ms");
            let _ = app.emit(
                "studio://event",
                StudioEvent::Sidecar(crate::events::SidecarEvent::LogLine {
                    name: "render".to_string(),
                    level: "info".to_string(),
                    message: format!("worker exited without completion; retrying in {delay}ms"),
                }),
            );
            {
                let mut q = state.render_queue.lock();
                q.update_status(&job_id, RenderStatus::Queued);
                if let Some(j) = q.get(&job_id) {
                    persist_render_job(&state, j);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            // The user may have cancelled during the backoff sleep — respect
            // it instead of resurrecting the job to Running.
            let cancelled_during_backoff = state
                .render_queue
                .lock()
                .get(&job_id)
                .map(|j| j.status == RenderStatus::Cancelled)
                .unwrap_or(false);
            if cancelled_during_backoff {
                return;
            }
            Box::pin(run_render_attempt(app, job, attempt + 1)).await;
            return;
        }
        {
            let mut q = state.render_queue.lock();
            q.update_status(&job_id, RenderStatus::Failed);
            if let Some(j) = q.get(&job_id) {
                persist_render_job(&state, j);
            }
        }
        let _ = app.emit(
            "studio://event",
            StudioEvent::Render(crate::events::RenderEvent::Failed {
                job_id,
                error: "render worker exited without a completion event".to_string(),
            }),
        );
    }
}

#[tauri::command]
pub async fn list_renders(state: State<'_, std::sync::Arc<AppState>>) -> Result<Vec<RenderJob>, String> {
    // SQLite is the durable source of truth (survives restarts); fall back to
    // the in-memory queue only if the store read fails.
    match state.store.lock().list_renders() {
        Ok(rows) if !rows.is_empty() => Ok(rows),
        Ok(_) => Ok(state.render_queue.lock().list()),
        Err(e) => {
            tracing::warn!("reading persisted renders failed: {e}");
            Ok(state.render_queue.lock().list())
        }
    }
}

#[tauri::command]
pub async fn cancel_render(state: State<'_, std::sync::Arc<AppState>>, job_id: String) -> Result<bool, String> {
    // Kill the live worker process (if any) BEFORE flipping the status — a
    // cancelled job whose `node render-worker.mjs` keeps running would race
    // the queue and could later report Completed over the Cancelled state.
    // (Bind first, await after: the parking_lot guard is not Send, so holding
    // it across .kill().await made the command future non-Send.)
    let mut child = state.render_children.lock().remove(&job_id);
    if let Some(c) = child.as_mut() {
        let _ = c.kill().await;
    }
    drop(child);
    let ok = state.render_queue.lock().update_status(&job_id, RenderStatus::Cancelled);
    if ok {
        if let Some(j) = state.render_queue.lock().get(&job_id) {
            persist_render_job(&state, j);
        }
    }
    Ok(ok)
}


// ---------------------------------------------------------------------------
// Preview server — live HyperFrames preview for the Timeline tab (Phase 10).
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn preview_start(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
) -> Result<(), String> {
    // No-op if already running (avoids spawning a second server on a port).
    if state.preview.lock().is_running() {
        return Ok(());
    }

    let project_dir = {
        let session = state.session.lock().clone();
        current_project_dir(&state, &session)
    };
    if !project_dir.is_dir() {
        return Err("no project open. Open or create a project first.".to_string());
    }

    const PORT: u16 = crate::preview::DEFAULT_PREVIEW_PORT;

    // Spawn the preview server, then wait for it to answer on its port.
    let mut child = spawn_preview(&project_dir, PORT).await?;
    // Drain the server's output into the sidecar log drawer (Phase 15). This
    // also keeps the pipes from filling and stalling the server during startup.
    if let Some(out) = child.stdout.take() {
        crate::sidecar::spawn_log_forwarder("preview", out, "info", app.clone());
    }
    if let Some(err) = child.stderr.take() {
        crate::sidecar::spawn_log_forwarder("preview", err, "error", app.clone());
    }
    match poll_ready(PORT, std::time::Duration::from_secs(30)).await {
        Ok(url) => {
            let mut guard = state.preview.lock();
            guard.child = Some(child);
            guard.port = Some(PORT);
            guard.running = true;
            drop(guard);
            let _ = app.emit(
                "studio://event",
                StudioEvent::Preview(PreviewEvent::Started {
                    url,
                    port: PORT,
                }),
            );
            Ok(())
        }
        Err(err) => {
            // Spawned but never became ready — tear it down and report.
            let _ = child.kill().await;
            let _ = app.emit(
                "studio://event",
                StudioEvent::Preview(PreviewEvent::Failed { error: err.clone() }),
            );
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn preview_stop(state: State<'_, std::sync::Arc<AppState>>) -> Result<(), String> {
    // Take the child out before awaiting so we never hold the MutexGuard
    // (which is not Send) across an await — the future must be Send.
    let child = {
        let mut server = state.preview.lock();
        let child = server.child.take();
        server.running = false;
        server.port = None;
        child
    };
    if let Some(mut c) = child {
        let _ = c.kill().await;
    }
    Ok(())
}

#[tauri::command]
pub fn preview_status(state: State<'_, std::sync::Arc<AppState>>) -> Result<PreviewStatus, String> {
    let server = state.preview.lock();
    Ok(PreviewStatus {
        running: server.is_running(),
        port: server.port,
    })
}

/// Preview-server health reported to the UI (iframe source + status).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewStatus {
    pub running: bool,
    pub port: Option<u16>,
}

// ---------------------------------------------------------------------------
// Image generation — Cloud (Navya) vs Local (sd-server) routing.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct GenerateImageArgs {
    pub prompt: String,
    pub model: Option<String>,
    pub size: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GeneratedImageResult {
    pub source: String,
    pub url: Option<String>,
    pub revised_prompt: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn generate_image(
    app: tauri::AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    args: GenerateImageArgs,
) -> Result<GeneratedImageResult, String> {
    generate_image_core(Some(&app), &state, args).await
}

/// Core image generation shared by the UI command and the control-server tool
/// dispatch — harness-driven generation takes exactly the cloud/local routing
/// the UI uses. `app` is optional: None suppresses progress/log events (the
/// tool path passes the app handle via AppState.app_handle).
pub async fn generate_image_core(
    app: Option<&AppHandle>,
    state: &std::sync::Arc<AppState>,
    args: GenerateImageArgs,
) -> Result<GeneratedImageResult, String> {
    let session = state.session.lock().clone();
    let model = args.model.unwrap_or_else(|| if session.source == "local" { "sd-xl".to_string() } else { "dall-e-3".to_string() });

    if session.source == "cloud" {
        let client = state.navya.lock().clone();
        match client.generate_image(&args.prompt, &model, args.size.as_deref()).await {
            Ok(img) => Ok(GeneratedImageResult {
                source: "cloud".to_string(),
                url: img.url,
                revised_prompt: img.revised_prompt,
                error: None,
            }),
            Err(e) => Ok(GeneratedImageResult {
                source: "cloud".to_string(),
                url: None,
                revised_prompt: None,
                error: Some(e),
            }),
        }
    } else {
        // Local path: lazily start the sd-server sidecar (download-on-first-
        // run + checksum via sidecar::bootstrap), then generate. Child output
        // and bootstrap progress stream into the sidecar log drawer.
        let Some(app) = app else {
            return Ok(GeneratedImageResult {
                source: "local".to_string(),
                url: None,
                revised_prompt: None,
                error: Some("local generation requires the app handle (unavailable)".to_string()),
            });
        };
        let cfg = state.config.lock().clone();
        let sup = state.sd.clone();
        let bin = cfg.sd_binary_path.clone();
        let models_dir = cfg.sd_models_dir.clone();
        // Honour the user's configured GPU flavor (was ignored — always
        // defaulted to Cpu, downloading the wrong build for CUDA machines).
        let flavor = match cfg.sd_gpu_backend {
            crate::config::SdGpuBackend::Cuda => crate::sd::SdGpuBackend::Cuda,
            crate::config::SdGpuBackend::Vulkan => crate::sd::SdGpuBackend::Vulkan,
            crate::config::SdGpuBackend::Cpu => crate::sd::SdGpuBackend::Cpu,
        };
        let progress_app = app.clone();
        let mut on_progress = move |m: &str| {
            let _ = progress_app.emit(
                "studio://event",
                StudioEvent::Sidecar(crate::events::SidecarEvent::LogLine {
                    name: "sd-server".to_string(),
                    level: "info".to_string(),
                    message: m.to_string(),
                }),
            );
        };
        let app_for_attach = app.clone();
        let attach = move |stream: &str, stream_io: Box<dyn tokio::io::AsyncRead + Send + Unpin>| {
            let level = if stream == "error" { "error" } else { "info" };
            crate::sidecar::spawn_log_forwarder("sd-server", stream_io, level, app_for_attach.clone());
        };
        let sup_ref: &crate::sd::OutputForwarder = &attach;
        match sup
            .ensure_running(
                &bin,
                Path::new(&models_dir),
                flavor,
                Some(sup_ref),
                &mut on_progress,
            )
            .await
        {
            Ok(_) => match sup.generate(&args.prompt).await {
                Ok(data_url) => Ok(GeneratedImageResult {
                    source: "local".to_string(),
                    url: Some(data_url),
                    revised_prompt: None,
                    error: None,
                }),
                Err(e) => Ok(GeneratedImageResult {
                    source: "local".to_string(),
                    url: None,
                    revised_prompt: None,
                    error: Some(e),
                }),
            },
            Err(e) => Ok(GeneratedImageResult {
                source: "local".to_string(),
                url: None,
                revised_prompt: None,
                error: Some(e),
            }),
        }
    }
}

#[tauri::command]
pub async fn list_cloud_models(state: State<'_, std::sync::Arc<AppState>>) -> Result<Vec<crate::navya::ModelSummary>, String> {
    let client = state.navya.lock().clone();
    client.list_models().await
}

#[tauri::command]
pub async fn detect_engine(state: State<'_, std::sync::Arc<AppState>>) -> Result<Vec<crate::engine::EngineModel>, String> {
    let engine = state.engine.lock().clone();
    if !engine.health_check().await {
        return Ok(Vec::new());
    }
    Ok(engine.list_models().await)
}

/// Persist a render-queue mutation to the renders table (Phase 14 hardening:
/// queue state survives restarts). Failures are logged, never surfaced — the
/// in-memory queue keeps working.
fn persist_render_job(state: &AppState, job: &crate::render::RenderJob) {
    if let Err(e) = state.store.lock().upsert_render(job) {
        tracing::warn!("render row upsert failed for {}: {e}", job.job_id);
    }
}

#[tauri::command]
pub fn get_sidecar_status(state: State<'_, std::sync::Arc<AppState>>) -> Result<Vec<crate::state::SidecarHealth>, String> {
    let snap = (*state).snapshot(false);
    Ok(snap.sidecars)
}

/// "Send feedback" packager (Phase 15): bundle redacted recent logs + a
/// minimal session summary into a zip under the app-data dir. Returns the
/// zip path so the UI can reveal it. Config contains no secrets (they live
/// in the keyring — enforced by the leak test in config.rs).
#[tauri::command]
pub fn package_feedback(app: tauri::AppHandle, state: State<'_, std::sync::Arc<AppState>>) -> Result<String, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve app-data dir: {e}"))?;
    let cfg = state.config.lock().clone();
    let session = state.session.lock().clone();
    let summary = serde_json::json!({
        "config": {
            "theme": format!("{:?}", cfg.theme).to_lowercase(),
            "density": format!("{:?}", cfg.density).to_lowercase(),
            "share_analytics": cfg.share_analytics,
        },
        "session": {
            "project_id": session.current_project_id,
            "composition_id": session.current_composition_id,
            "harness": session.harness,
            "model": session.model,
            "source": session.source,
        },
        "version": env!("CARGO_PKG_VERSION"),
    });
    let path = crate::feedback::package(&data_dir, &data_dir.join("feedback"), Some(&summary))?;
    tracing::info!("feedback package written: {}", path.display());
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn reveal_in_folder(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("path does not exist: {path}"));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(p.as_os_str())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Normalize a user-supplied project directory into an existing absolute path.
///
/// - empty / `"."` (onboarding's blank project) → `<app_data>/projects/<id>`
/// - leading `~` → expanded against the home dir
/// - relative paths → joined under `<app_data>/projects/`
/// - absolute paths → used as-is
///
/// The directory is created when missing so the harness can spawn in it.
fn resolve_project_dir(app: &AppHandle, project_id: &str, raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("projects");
    let dir = if trimmed.is_empty() || trimmed == "." {
        base.join(project_id)
    } else if let Some(rest) = trimmed.strip_prefix('~') {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .ok_or_else(|| "cannot expand ~ (no home directory found)".to_string())?;
        PathBuf::from(home).join(rest.trim_start_matches(['/', '\\']))
    } else if Path::new(trimmed).is_absolute() {
        PathBuf::from(trimmed)
    } else {
        base.join(trimmed)
    };
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("creating project dir {}: {e}", dir.display()))?;
    Ok(dir)
}

/// A monotonic-ish id from the current time (no chrono dep needed).
fn chrono_like_id() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Resolve the on-disk directory of the session's current project.
///
/// The store is behind a `parking_lot::Mutex` (sync), so a plain lock is fine
/// here — the earlier `"."` fallback made every harness spawn in the app's
/// CWD instead of the project tree.
fn current_project_dir(state: &AppState, session: &Session) -> PathBuf {
    let Some(pid) = &session.current_project_id else {
        return PathBuf::from(".");
    };
    let store = state.store.lock();
    store
        .list_projects()
        .ok()
        .and_then(|ps| ps.into_iter().find(|p| p.id == *pid))
        .map(|p| PathBuf::from(p.dir))
        .unwrap_or_else(|| PathBuf::from("."))
}