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
use crate::events::{ProjectEvent, StudioEvent};
use crate::render::{RenderJob, RenderQuality, RenderStatus, RenderTarget};
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
    let store = (*state).store.lock();
    snap.projects = store.list_projects().map_err(|e| e.to_string())?;
    Ok(snap)
}

#[tauri::command]
pub fn get_config(state: State<'_, std::sync::Arc<AppState>>) -> Result<NavyaConfig, String> {
    Ok((*state).config.lock().clone())
}

/// Persist config to the app-data JSON file it was loaded from.
#[tauri::command]
pub fn save_config(state: State<'_, std::sync::Arc<AppState>>, config: NavyaConfig) -> Result<NavyaConfig, String> {
    let path = (*state).config_path.clone();
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    *(*state).config.lock() = config.clone();
    // Re-sync the Navya client with the new endpoint/router flags.
    *(*state).navya.lock() = crate::navya::NavyaClient::new(
        config.navya_base_url.clone(),
        config.use_auto_router,
        config.byok,
    );
    // Re-sync the Engine client with the new local proxy URL.
    *(*state).engine.lock() = crate::engine::EngineClient::new(&config.engine_url);
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
    let session = (*state).session.lock().clone();
    let store = (*state).store.lock();
    let row = store
        .create_project(&id, &args.name, &dir.to_string_lossy().as_ref(), &session.harness, &session.model, &session.source)
        .map_err(|e| e.to_string())?;

    // Switch the session to the new project.
    {
        let mut s = (*state).session.lock();
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
    let store = (*state).store.lock();
    store.list_projects().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_project(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    project_id: String,
) -> Result<(), String> {
    let store = (*state).store.lock();
    let projects = store.list_projects().map_err(|e| e.to_string())?;
    let proj = projects
        .into_iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("project {project_id} not found"))?;
    {
        let mut s = (*state).session.lock();
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
pub fn set_model(state: State<'_, std::sync::Arc<AppState>>, model: String) -> Result<Session, String> {
    {
        let mut s = (*state).session.lock();
        s.model = model.clone();
    }
    Ok((*state).session.lock().clone())
}

#[tauri::command]
pub fn set_source(state: State<'_, std::sync::Arc<AppState>>, source: String) -> Result<Session, String> {
    if source != "cloud" && source != "local" {
        return Err("source must be 'cloud' or 'local'".to_string());
    }
    {
        let mut s = (*state).session.lock();
        s.source = source;
    }
    Ok((*state).session.lock().clone())
}

#[tauri::command]
pub fn set_harness(state: State<'_, std::sync::Arc<AppState>>, harness: String) -> Result<Session, String> {
    {
        let mut s = (*state).session.lock();
        s.harness = harness;
    }
    Ok((*state).session.lock().clone())
}

#[tauri::command]
pub async fn list_models(state: State<'_, std::sync::Arc<AppState>>) -> Result<Vec<ModelEntry>, String> {
    let session = (*state).session.lock().clone();
    let mut models = cloud_models(&session.model);

    // Try to discover Navya Engine and list its models.
    let engine = (*state).engine.lock().clone();
    if engine.health_check().await {
        let engine_models = engine.list_models().await;
        for em in engine_models {
            let model_id = crate::engine::EngineClient::format_model_id(&em.id);
            models.push(ModelEntry {
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
    let session = (*state).session.lock().clone();
    let harness = {
        let registry = (*state).harness_registry.lock();
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
        control_url: (*state).control_url.lock().clone(),
        control_token: (*state).control_token.lock().clone(),
    };
    let prompt_mode = match mode.as_str() {
        "steer" => crate::harness::PromptMode::Steer,
        "follow_up" => crate::harness::PromptMode::FollowUp,
        _ => crate::harness::PromptMode::Normal,
    };
    harness.start(&ctx).await.map_err(|e| e.to_string())?;

    // Ensure exactly one event pump per harness: subscribe to the harness event
    // stream and re-emit every event on the studio://event channel for the UI.
    let need_pump = {
        let mut pump = (*state).pump_harness.lock();
        if pump.as_deref() != Some(session.harness.as_str()) {
            *pump = Some(session.harness.clone());
            true
        } else {
            false
        }
    };
    if need_pump {
        let mut rx = harness.subscribe();
        let app_pump = app.clone();
        tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                emit_harness_event(&app_pump, ev);
            }
        });
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
) -> Result<(), String> {
    let session = (*state).session.lock().clone();
    let harness = {
        let registry = (*state).harness_registry.lock();
        registry
            .get(&session.harness)
            .ok_or_else(|| format!("harness '{}' not registered", session.harness))?
            .clone()
    };

    // Persist a scoped allow rule when the user checks "always allow".
    if always_allow && approved {
        let req = crate::harness::approvals::ApprovalRequest {
            id: request_id.clone(),
            kind: kind.clone(),
            payload: payload.clone(),
        };
        crate::harness::approvals::write_allow_rule(&session.harness, &req, true)?;
    }

    // Relay the answer back to the running harness process.
    match harness.answer_approval(&request_id, approved).await {
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

#[tauri::command]
pub async fn steer(app: AppHandle, state: State<'_, std::sync::Arc<AppState>>, msg: String) -> Result<(), String> {
    let session = (*state).session.lock().clone();
    let harness = {
        let registry = (*state).harness_registry.lock();
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
    let session = (*state).session.lock().clone();
    let harness = {
        let registry = (*state).harness_registry.lock();
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
    (*state).render_queue.lock().add(job.clone());

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

    // Mark the job running.
    {
        let mut q = (*state).render_queue.lock();
        q.update_status(&job.job_id, RenderStatus::Running);
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
        let session = (**state).session.lock().clone();
        current_project_dir(&**state, &session)
    };

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
                let out = parsed.get("output_path").and_then(|v| v.as_str()).unwrap_or("renders/out.mp4").to_string();
                { let mut q = (*state).render_queue.lock(); q.complete(&job_id_for_stdout, &out); }
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
                let err = parsed.get("error").and_then(|v| v.as_str()).unwrap_or("render failed").to_string();
                { let mut q = (*state).render_queue.lock(); q.update_status(&job_id_for_stdout, RenderStatus::Failed); }
                let _ = app_for_stdout.emit(
                    "studio://event",
                    StudioEvent::Render(crate::events::RenderEvent::Failed {
                        job_id: job_id_for_stdout.clone(),
                        error: err,
                    }),
                );
            }
            _ => { /* log / pong — ignore for now */ }
        }
    }

    // If the worker exited without sending completed/failed, mark failed — in
    // both the queue and the event stream, so the UI never shows a stuck job.
    let still_running = state
        .render_queue
        .lock()
        .get(&job_id)
        .map(|j| j.status == RenderStatus::Running)
        .unwrap_or(false);
    if still_running {
        {
            let mut q = (*state).render_queue.lock();
            q.update_status(&job_id, RenderStatus::Failed);
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
    Ok((*state).render_queue.lock().list())
}

#[tauri::command]
pub async fn cancel_render(state: State<'_, std::sync::Arc<AppState>>, job_id: String) -> Result<bool, String> {
    Ok((*state).render_queue.lock().update_status(&job_id, RenderStatus::Cancelled))
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
    state: State<'_, std::sync::Arc<AppState>>,
    args: GenerateImageArgs,
) -> Result<GeneratedImageResult, String> {
    let session = (*state).session.lock().clone();
    let model = args.model.unwrap_or_else(|| if session.source == "local" { "sd-xl".to_string() } else { "dall-e-3".to_string() });

    if session.source == "cloud" {
        let client = (*state).navya.lock().clone();
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
        // Local path: sd-server sidecar. Not yet spawned — surface honestly.
        Ok(GeneratedImageResult {
            source: "local".to_string(),
            url: None,
            revised_prompt: None,
            error: Some("Local image generation (sd-server) is not running. Start it in Settings.".to_string()),
        })
    }
}

#[tauri::command]
pub async fn list_cloud_models(state: State<'_, std::sync::Arc<AppState>>) -> Result<Vec<crate::navya::ModelSummary>, String> {
    let client = (*state).navya.lock().clone();
    client.list_models().await
}

#[tauri::command]
pub async fn detect_engine(state: State<'_, std::sync::Arc<AppState>>) -> Result<Vec<crate::engine::EngineModel>, String> {
    let engine = (*state).engine.lock().clone();
    if !engine.health_check().await {
        return Ok(Vec::new());
    }
    Ok(engine.list_models().await)
}

#[tauri::command]
pub fn get_sidecar_status(state: State<'_, std::sync::Arc<AppState>>) -> Result<Vec<crate::state::SidecarHealth>, String> {
    let snap = (*state).snapshot(false);
    Ok(snap.sidecars)
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