//! Shared application state (production wiring).
//!
//! A single `AppState` managed by Tauri (`app.manage(state)`) and accessed by
//! every `#[tauri::command]` via `tauri::State<'_, Mutex<AppState>>`. This is
//! the bridge that makes the app actually work: config persists, projects live
//! in SQLite, the session (current project/model/source/harness) is real, and
//! sidecar status reflects the actual supervisor.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex as PMutex;
use serde::{Deserialize, Serialize};

use crate::config::{self, NavyaConfig};
use crate::engine::EngineClient;
use crate::harness::{Capabilities, HarnessRegistry};
use crate::navya::NavyaClient;
use crate::render::RenderQueue;
use crate::sidecar::{SidecarStatus, Supervisor};
use crate::store::{ProjectRow, ProjectStore};

/// The active session — what the top bar and center panes key off of.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub current_project_id: Option<String>,
    pub current_composition_id: Option<String>,
    pub harness: String,
    pub model: String,
    pub source: String, // "cloud" | "local"
}

impl Default for Session {
    fn default() -> Self {
        Session {
            current_project_id: None,
            current_composition_id: None,
            harness: "a-coder-cli".to_string(),
            model: "navya/auto".to_string(),
            source: "cloud".to_string(),
        }
    }
}

/// A snapshot of sidecar health for the status strip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarHealth {
    pub name: String,
    pub status: String, // "starting" | "running" | "stopped" | "exited" | "unknown"
    pub detail: Option<String>,
}

/// The full state snapshot the UI receives from `get_state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub session: Session,
    pub config: NavyaConfig,
    pub projects: Vec<ProjectRow>,
    pub models: Vec<ModelEntry>,
    pub sidecars: Vec<SidecarHealth>,
    pub has_api_key: bool,
    pub harnesses: Vec<HarnessInfo>,
    pub render_count: usize,
    /// Control server URL (set by the control server at launch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_url: Option<String>,
    /// Control server bearer token (set by the control server at launch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub kind: String, // chat | image | video
    pub source: String, // cloud | local
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessInfo {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub steer: bool,
    pub abort: bool,
    pub persistent: bool,
    pub available: bool, // binary/impl present on this machine
    /// Detection provenance (open-design parity): the exact binary found,
    /// its `--version` banner if readable, and where to get help installing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs_url: Option<String>,
}

/// The managed state. Inner mutability via parking_lot::Mutex for sync access
/// from commands; the store and supervisor are behind Arc so they can be
/// cloned cheaply into background tasks.
pub struct AppState {
    pub session: PMutex<Session>,
    pub config: PMutex<NavyaConfig>,
    pub config_path: PathBuf,
    pub store: Arc<PMutex<ProjectStore>>,
    pub render_queue: PMutex<RenderQueue>,
    pub navya: PMutex<NavyaClient>,
    pub engine: PMutex<EngineClient>,
    pub supervisor: Arc<Supervisor>,
    pub harness_registry: PMutex<HarnessRegistry>,
    /// Harness id that currently has an event pump subscribed (one pump per
    /// harness; re-subscribes when the user switches harness).
    pub pump_harness: PMutex<Option<String>>,
    /// Control server URL (set by the control server at launch).
    pub control_url: Arc<PMutex<Option<String>>>,
    /// Control server bearer token.
    pub control_token: Arc<PMutex<Option<String>>>,
}

impl AppState {
    /// Construct from a resolved config + store. The config path is kept so
    /// `save_config` can persist back to the same file.
    pub fn new(config: NavyaConfig, config_path: PathBuf, store: ProjectStore) -> Self {
        let navya = NavyaClient::new(
            config.navya_base_url.clone(),
            config.use_auto_router,
            config.byok,
        );
        let engine = EngineClient::new(&config.engine_url);
        let mut registry = HarnessRegistry::new();
        // Register the reference adapter + the secondary adapters.
        registry.register(crate::harness::aacoder::AaaCoderCliHarness::new());
        registry.register(crate::harness::claude::ClaudeCodeHarness::new());
        registry.register(crate::harness::codex::CodexHarness::new());
        registry.register(crate::harness::hermes::HermesHarness::new());
        registry.register(crate::harness::antigravity::AntigravityHarness::new());
        registry.register(crate::harness::openclaw::OpenClawHarness::new());

        AppState {
            session: PMutex::new(Session::default()),
            config: PMutex::new(config),
            config_path,
            store: Arc::new(PMutex::new(store)),
            render_queue: PMutex::new(RenderQueue::default()),
            navya: PMutex::new(navya),
            engine: PMutex::new(engine),
            supervisor: Arc::new(Supervisor::new()),
            harness_registry: PMutex::new(registry),
            pump_harness: PMutex::new(None),
            control_url: Arc::new(PMutex::new(None)),
            control_token: Arc::new(PMutex::new(None)),
        }
    }

    /// Build a full snapshot for the UI.
    pub fn snapshot(&self, has_api_key: bool) -> StateSnapshot {
        let session = self.session.lock().clone();
        let config = self.config.lock().clone();
        let supervisor = &self.supervisor;

        // Sidecar health: prefer the live supervisor status (if a sidecar has
        // been started/stopped within this session), and fall back to binary
        // detection (PATH scan) when the supervisor has no entry yet.
        let sidecars = ["llama-server", "sd-server"]
            .into_iter()
            .map(|name| {
                if let Some(status) = supervisor.status(name) {
                    sidecar_health(name, Some(status))
                } else {
                    detect_sidecar_health(name, name)
                }
            })
            .collect::<Vec<_>>();

        // Harnesses from the registry + capability matrix.
        let registry = self.harness_registry.lock();
        let mut harnesses: Vec<HarnessInfo> = registry
            .list_ids()
            .into_iter()
            .map(|id| harness_info(&id, &config, registry.get(&id).map(|h| h.capabilities())))
            .collect();

        // Models: cloud (Navya) + local (llama.cpp / sd-server).
        let mut models = cloud_models(&session.model);
        if config.sd_server_url.is_some() {
            models.push(ModelEntry {
                id: "sd-xl".to_string(),
                name: "Stable Diffusion XL".to_string(),
                kind: "image".to_string(),
                source: "local".to_string(),
                active: session.model == "sd-xl",
            });
        }
        models.push(ModelEntry {
            id: "llama3-8b".to_string(),
            name: "Llama 3 8B (llama.cpp)".to_string(),
            kind: "chat".to_string(),
            source: "local".to_string(),
            active: session.model == "llama3-8b",
        });

        // mark enabled order
        for h in harnesses.iter_mut() {
            h.enabled = config.enabled_harnesses.iter().any(|e| e == &h.id);
        }

        let render_count = self.render_queue.lock().list().len();

        StateSnapshot {
            session,
            config,
            projects: vec![], // populated by the async command (needs store lock)
            models,
            sidecars,
            has_api_key,
            harnesses,
            render_count,
            control_url: self.control_url.lock().clone(),
            control_token: self.control_token.lock().clone(),
        }
    }
}

fn sidecar_health(name: &str, status: Option<SidecarStatus>) -> SidecarHealth {
    let (s, detail) = match status {
        Some(SidecarStatus::Starting) => ("starting", None),
        Some(SidecarStatus::Running) => ("running", None),
        Some(SidecarStatus::Stopped) => ("stopped", None),
        Some(SidecarStatus::Exited(c)) => ("exited", Some(format!("exit code {c}"))),
        None => ("idle", None),
    };
    SidecarHealth {
        name: name.to_string(),
        status: s.to_string(),
        detail,
    }
}

/// Detect whether a sidecar binary exists on PATH and build an honest health report.
fn detect_sidecar_health(name: &str, bin: &str) -> SidecarHealth {
    let found = crate::sidecar::which_path(bin).is_some();
    if found {
        sidecar_health(name, None) // idle — not yet started, but binary is present
    } else {
        SidecarHealth {
            name: name.to_string(),
            status: "not_installed".to_string(),
            detail: Some(format!("{bin} not on PATH. Install it to enable local {name}.")),
        }
    }
}

fn harness_info(id: &str, _cfg: &NavyaConfig, caps: Option<Capabilities>) -> HarnessInfo {
    // Descriptor-driven (harness/registry.rs): label + install/docs metadata
    // come from the static table; availability comes from binary detection
    // (PATH scan incl. fallback bins, plus a bounded `--version` probe).
    let descriptor = crate::harness::registry::descriptor_for(id);
    let detection = descriptor.map(crate::harness::registry::detect);
    let caps = caps.unwrap_or_default();
    HarnessInfo {
        id: id.to_string(),
        label: descriptor
            .map(|d| d.label)
            .unwrap_or(id)
            .to_string(),
        enabled: false,
        steer: caps.steer,
        abort: caps.abort,
        persistent: caps.persistent,
        available: detection.as_ref().map(|d| d.available).unwrap_or(false),
        path: detection
            .as_ref()
            .and_then(|d| d.path.as_ref())
            .map(|p| p.to_string_lossy().to_string()),
        version: detection.and_then(|d| d.version),
        install_url: descriptor.and_then(|d| d.install_url).map(str::to_string),
        docs_url: descriptor.and_then(|d| d.docs_url).map(str::to_string),
    }
}

pub fn cloud_models(active_model: &str) -> Vec<ModelEntry> {
    vec![
        ModelEntry {
            id: "navya/auto".to_string(),
            name: "Navya Auto Router".to_string(),
            kind: "chat".to_string(),
            source: "cloud".to_string(),
            active: active_model == "navya/auto",
        },
        ModelEntry {
            id: "qwen3-32b".to_string(),
            name: "Qwen3 32B".to_string(),
            kind: "chat".to_string(),
            source: "cloud".to_string(),
            active: active_model == "qwen3-32b",
        },
        ModelEntry {
            id: "dall-e-3".to_string(),
            name: "DALL·E 3".to_string(),
            kind: "image".to_string(),
            source: "cloud".to_string(),
            active: active_model == "dall-e-3",
        },
    ]
}

/// Resolve the config file path in the app data dir, loading it if present or
/// falling back to defaults (and writing the default so the user can edit it).
pub fn load_config(config_path: PathBuf) -> NavyaConfig {
    if config_path.exists() {
        if let Ok(raw) = std::fs::read_to_string(&config_path) {
            if let Ok(c) = serde_json::from_str::<NavyaConfig>(&raw) {
                return c;
            }
        }
    }
    let d = config::default_config();
    // Best-effort persist the default so Settings shows a real file.
    let _ = std::fs::create_dir_all(config_path.parent().unwrap_or(&PathBuf::from(".")));
    let _ = std::fs::write(&config_path, serde_json::to_string_pretty(&d).unwrap_or_default());
    d
}