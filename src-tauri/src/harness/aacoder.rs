//! a-coder-cli harness adapter (production wiring).
//!
//! Drives `a-coder-cli --mode rpc` over stdio JSONL, matching the contract in
//! a-coder-cli `docs/rpc.md` exactly:
//!
//! **Commands (stdin, one JSON object per line):**
//!   prompt        {"type":"prompt","message":...}
//!   steer         {"type":"steer","message":...}
//!   follow_up     {"type":"follow_up","message":...}
//!   abort         {"type":"abort"}            (session stays alive)
//!   set_model     {"type":"set_model","provider":...,"modelId":...}
//!   get_available_models {"type":"get_available_models"}
//!   extension_ui_response {"type":"extension_ui_response","id":...,...}
//!
//! **Events (stdout JSONL):** agent_start, agent_end, message_update (with
//! assistantMessageEvent deltas), tool_execution_{start,update,end},
//! queue_update, auto_retry_{start,end}, extension_ui_request,
//! extension_error, response (command acks).
//!
//! Framing: strict JSONL, split on \n only, strip trailing \r.

use async_trait::async_trait;
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc, mpsc::Receiver, oneshot};

use crate::harness::{
    event::{HarnessEvent, ModelInfo},
    Capabilities, Harness as HarnessTrait, HarnessCtx, HarnessError, PromptMode,
};

/// a-coder-cli harness adapter.
#[derive(Clone)]
pub struct AaaCoderCliHarness {
    id: String,
    capabilities: Capabilities,
    inner: Arc<std::sync::Mutex<Inner>>,
}

/// Per-process mutable state for the a-coder-cli RPC session.
struct Inner {
    started: bool,
    stopping: bool,       // stop() was requested — suppresses the EOF error event
    project_dir: PathBuf, // project dir the child was spawned in
    model: String,        // model from HarnessCtx, injected into AgentStart events
    /// Bumped on every spawn/stop. Each stdout reader captures its generation;
    /// a reader whose generation is no longer current belongs to a replaced or
    /// killed process and must NEVER report against the newer generation —
    /// without this, the old reader's EOF fired after stop()+spawn() reset the
    /// shared flags and broadcast a false "process exited" over the new
    /// session's reply (the stale-reader bug that masked every agent reply).
    generation: u64,
    stdin: std::sync::Mutex<Option<std::process::ChildStdin>>,
    child_handle: std::sync::Mutex<Option<std::process::Child>>,
    /// Every live subscriber gets every event (event pump + future consumers).
    subscribers: Vec<mpsc::Sender<HarnessEvent>>,
    /// Dialog extension_ui_requests awaiting an answer (id -> full request),
    /// needed to build the correct extension_ui_response shape per method.
    pending_ui: HashMap<String, serde_json::Value>,
    /// Pending get_available_models round-trip.
    pending_models: Option<oneshot::Sender<Vec<ModelInfo>>>,
    /// Last successful get_available_models result + fetch time. The CLI needs
    /// ~20s to boot its MCP servers before it can answer, so the model picker
    /// reuses this cache instead of re-paying the boot cost on every refresh.
    models_cache: Option<(std::time::Instant, Vec<ModelInfo>)>,
}

/// get_available_models waits for the CLI to boot its MCP servers — measured
/// ~20-60s cold on a real install (MCP boot); 90s leaves headroom (the old 20s timeout
/// made the picker fall back to the static catalog almost every time).
const MODELS_RPC_TIMEOUT: Duration = Duration::from_secs(90);
/// Successful model listings are cached for the picker for this long.
const MODELS_CACHE_TTL: Duration = Duration::from_secs(300);

/// The amaara-studio extension (tool bridge) source, compiled into the binary
/// so the installed app can stage it without packaging harness-pack.
const STUDIO_EXTENSION_TS: &str =
    include_str!("../../../harness-pack/a-coder-cli/extensions/amaara-studio.ts");

/// Stage the studio tool-bridge extension into the project-local a-coder-cli
/// extensions dir (`.a-coder-cli/extensions/`, auto-discovered by the CLI).
/// Idempotent: rewrites only when the staged content differs.
///
/// Without this the harness can prompt and stream fine but can never CALL the
/// studio's tools (render_to_video, generate_image, …) — the extension is what
/// proxies tool calls to the control server, and project-local staging keeps
/// the bridge scoped to the project instead of touching the global config.
fn stage_project_extension(project_dir: &std::path::Path) {
    let dir = project_dir.join(".a-coder-cli").join("extensions");
    let file = dir.join("amaara-studio.ts");
    let needs_write = match std::fs::read_to_string(&file) {
        Ok(existing) => existing != STUDIO_EXTENSION_TS,
        Err(_) => true,
    };
    if needs_write && std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(&file, STUDIO_EXTENSION_TS);
    }
}

/// Probe whether a binary is on PATH. Returns path or None.
pub fn which(bin: &str) -> Option<PathBuf> {
    // Test/CI override (headless e2e): drive the adapter against a stub binary
    // without mutating PATH. Checked only for a-coder-cli.
    if bin == "a-coder-cli" {
        if let Ok(over) = std::env::var("AMAARA_AACODER_BIN") {
            if !over.is_empty() {
                return Some(PathBuf::from(over));
            }
        }
    }
    let cmd_bin = if cfg!(windows) { "where" } else { "which" };
    let mut cmd = std::process::Command::new(cmd_bin);
    cmd.arg(bin);
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout);
            let path = path.lines().next()?.trim();
            Some(PathBuf::from(path))
        }
        _ => None,
    }
}

/// Check if the binary is available on PATH.
#[cfg(test)]
pub fn is_available() -> bool {
    which("a-coder-cli").is_some()
}

impl AaaCoderCliHarness {
    pub fn new() -> Self {
        Self {
            id: "a-coder-cli".into(),
            capabilities: Capabilities {
                steer: true,
                abort: true,
                list_models: true,
                approvals: true,
                persistent: true,
            },
            inner: Arc::new(std::sync::Mutex::new(Inner {
                started: false,
                stopping: false,
                project_dir: PathBuf::new(),
                model: String::new(),
                generation: 0,
                stdin: std::sync::Mutex::new(None),
                child_handle: std::sync::Mutex::new(None),
                subscribers: Vec::new(),
                pending_ui: HashMap::new(),
                pending_models: None,
                models_cache: None,
            })),
        }
    }

    /// Write one JSONL command line to the process stdin. Caller must hold no
    /// other locks on Inner (this locks inner then stdin).
    fn send_cmd(&self, cmd: serde_json::Value) -> Result<(), HarnessError> {
        let inner = self.inner.lock().unwrap();
        if !inner.started {
            return Err(HarnessError::NotStarted);
        }
        let mut stdin = inner.stdin.lock().unwrap();
        let child_stdin = stdin.as_mut().ok_or(HarnessError::NotStarted)?;
        writeln!(child_stdin, "{}", cmd)
            .map_err(|e| HarnessError::Process(format!("write to a-coder-cli stdin: {e}")))?;
        child_stdin
            .flush()
            .map_err(|e| HarnessError::Process(format!("flush a-coder-cli stdin: {e}")))?;
        Ok(())
    }
}

impl Default for AaaCoderCliHarness {
    fn default() -> Self {
        Self::new()
    }
}

// --- Harness trait ----------------------------------------------------------

#[async_trait]
impl HarnessTrait for AaaCoderCliHarness {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    async fn start(&self, ctx: &HarnessCtx) -> Result<(), HarnessError> {
        // Idempotent: if already running with the same project_dir, return Ok.
        // If the project_dir changed, stop the old process and restart.
        let need_restart = {
            let inner = self.inner.lock().unwrap();
            inner.started && inner.project_dir != ctx.project_dir
        };
        if need_restart {
            self.stop().await?; // stop the old process before restarting
        }

        // Already running in this project — nothing to do. Without this early
        // return, every send_prompt (which calls start) spawned a second
        // a-coder-cli process, leaked the previous child, and the old
        // reader's EOF broadcast a spurious "process exited" error.
        {
            let inner = self.inner.lock().unwrap();
            if inner.started && inner.project_dir == ctx.project_dir {
                return Ok(());
            }
        }

        // Ensure the studio tool-bridge extension is staged in the project so
        // the harness can call studio tools over the control server.
        stage_project_extension(&ctx.project_dir);

        let binary = match which("a-coder-cli") {
            Some(b) => b,
            None => {
                return Err(HarnessError::Process(
                    "a-coder-cli not found on PATH".into(),
                ))
            }
        };

        // On Windows the CLI is usually a .cmd/.bat shim which CreateProcess
        // cannot execute directly — run it through `cmd /c`.
        let ext = binary
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let (prog, prefix): (PathBuf, Vec<String>) =
            if cfg!(windows) && matches!(ext.as_str(), "cmd" | "bat") {
                (
                    "cmd".into(),
                    vec!["/c".into(), binary.to_string_lossy().into_owned()],
                )
            } else {
                (binary, Vec::new())
            };

        // Spawn: `a-coder-cli --mode rpc` in the project directory.
        // Pass the control server URL + token as env vars so the harness
        // extension can proxy tool calls to the control server.
        let mut command = std::process::Command::new(&prog);
        command.args(&prefix);
        if let Some(url) = &ctx.control_url {
            command.env("AMAARA_CONTROL_URL", url);
        }
        if let Some(token) = &ctx.control_token {
            command.env("AMAARA_CONTROL_TOKEN", token);
        }
        let mut c = command
            .arg("--mode")
            .arg("rpc")
            .current_dir(&ctx.project_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            // Piped (not null): a CLI crash reason printed to stderr used to
            // vanish, leaving only "process exited" in the chat. Lines are
            // drained continuously (a full stderr pipe stalls the child) and
            // logged so failures are diagnosable from the studio log.
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| HarnessError::Process(format!("failed to spawn a-coder-cli: {e}")))?;

        let child_pid = c.id();
        let stdout = c.stdout.take().expect("stdout should be open");
        let stdin = c.stdin.take().expect("stdin should be open");
        let stderr = c.stderr.take().expect("stderr should be open");

        {
            let mut inner = self.inner.lock().unwrap();
            inner.started = true;
            inner.stopping = false;
            inner.project_dir = ctx.project_dir.clone();
            inner.model = ctx.model.clone();
            *inner.stdin.lock().unwrap() = Some(stdin);
            inner.child_handle.lock().unwrap().replace(c);
            inner.generation += 1;
        }
        let my_generation = self.inner.lock().unwrap().generation;

        // Reader loop: parse stdout JSONL per docs/rpc.md and broadcast to all
        // subscribers. Locks Inner per event so late subscribe() calls are seen.
        tracing::info!(
            "a-coder-cli spawned (pid {child_pid}) in {}",
            ctx.project_dir.display()
        );
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            let mut reader = std::io::BufReader::new(stdout);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf) {
                    Ok(0) => {
                        tracing::warn!("a-coder-cli (pid {child_pid}) stdout EOF");
                        break; // EOF — process exited (or closed stdout)
                    }
                    Ok(_) => {
                        let line = buf.trim_end_matches(['\n', '\r']);
                        if line.is_empty() {
                            continue;
                        }
                        // Deeply nested session/event trees exceed serde_json's
                        // default 128-level recursion limit. The desktop app hit
                        // this as the "lost-response bug": unparseable lines were
                        // silently dropped. Parse with the limit disabled + a
                        // heap-allocated deserialize stack (same as desktop).
                        let mut de = serde_json::Deserializer::from_str(line);
                        de.disable_recursion_limit();
                        match serde_json::Value::deserialize(serde_stacker::Deserializer::new(
                            &mut de,
                        )) {
                            Ok(json) => handle_stdout_line(&inner, &json),
                            Err(e) => tracing::warn!(
                                "a-coder-cli: unparseable stdout line ({e}): {}",
                                &line[..line.len().min(200)]
                            ),
                        }
                    }
                    Err(e) => {
                        tracing::warn!("a-coder-cli (pid {child_pid}) stdout read error: {e}");
                        break;
                    }
                }
            }
            // EOF/read-error: report only if this reader's generation is still
            // current — a replaced/killed process's reader must stay silent
            // (the stale-reader bug broadcast a false "process exited" over
            // the new session's reply). Otherwise include the exit code when
            // the child has already been reaped so the chat shows "exited
            // (code 1)"; if the child is STILL RUNNING the CLI closed its own
            // stdout — a different failure mode than an exit.
            let (notify, code, still_running) = {
                let mut g = inner.lock().unwrap();
                if g.generation != my_generation {
                    tracing::debug!(
                        "a-coder-cli (pid {child_pid}) stale reader EOF (gen {my_generation} != current) — ignoring"
                    );
                    (false, None, false)
                } else {
                    let was_started = g.started;
                    g.started = false;
                    let mut handle = g.child_handle.lock().unwrap();
                    let status = handle.as_mut().and_then(|c| c.try_wait().ok().flatten());
                    let still_running = !handle.is_none() && status.is_none();
                    let code = status.and_then(|s| s.code());
                    (was_started && !g.stopping, code, still_running)
                }
            };
            if notify {
                let detail = if still_running {
                    format!("a-coder-cli (pid {child_pid}) closed its output while still running")
                } else {
                    match code {
                        Some(code) => format!("a-coder-cli process exited (code {code})"),
                        None => "a-coder-cli process exited".into(),
                    }
                };
                tracing::warn!("{detail}");
                broadcast(&inner, HarnessEvent::Error(detail));
            }
        });

        // Drain stderr so the child never blocks on a full pipe, and log every
        // line — this is where CLI crash reasons surface (the chat only ever
        // saw "process exited" before).
        tokio::spawn(async move {
            let mut reader = std::io::BufReader::new(stderr);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = buf.trim_end_matches(['\n', '\r']);
                        if !line.is_empty() {
                            tracing::warn!("a-coder-cli stderr: {line}");
                        }
                    }
                }
            }
        });

        // Apply the session model to the fresh CLI process. The studio-side
        // set_model IPC can't do this for a not-yet-started harness, and the
        // spawn is the only chance before the first prompt (previously the
        // picker's selection was silently ignored and the CLI ran its own
        // default model). "amaara/auto" is the studio's cloud-router concept —
        // the CLI doesn't know it, so it means "use the CLI default".
        if !ctx.model.is_empty() && ctx.model != "amaara/auto" {
            let cmd = match ctx.model.split_once('/') {
                Some((provider, model_id)) => {
                    serde_json::json!({ "type": "set_model", "provider": provider, "modelId": model_id })
                }
                None => serde_json::json!({ "type": "set_model", "modelId": ctx.model }),
            };
            if let Err(e) = self.send_cmd(cmd) {
                tracing::warn!("set_model after spawn failed: {e}");
            } else {
                tracing::info!("harness model set to {}", ctx.model);
            }
        }

        Ok(())
    }

    async fn prompt(&self, msg: &str, mode: PromptMode) -> Result<(), HarnessError> {
        // docs/rpc.md: prompt / steer / follow_up are distinct commands, all
        // carrying the text in a `message` field.
        let cmd = match mode {
            PromptMode::Normal => serde_json::json!({ "type": "prompt", "message": msg }),
            PromptMode::Steer => serde_json::json!({ "type": "steer", "message": msg }),
            PromptMode::FollowUp => serde_json::json!({ "type": "follow_up", "message": msg }),
        };
        self.send_cmd(cmd)
    }

    async fn steer(&self, msg: &str) -> Result<(), HarnessError> {
        self.send_cmd(serde_json::json!({ "type": "steer", "message": msg }))
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        // docs/rpc.md: abort cancels the current operation; the session lives on.
        self.send_cmd(serde_json::json!({ "type": "abort" }))
    }

    async fn set_model(&self, model: &str) -> Result<(), HarnessError> {
        // Amaara model ids are "provider/modelId"; rpc wants them split.
        let cmd = match model.split_once('/') {
            Some((provider, model_id)) => {
                serde_json::json!({ "type": "set_model", "provider": provider, "modelId": model_id })
            }
            None => serde_json::json!({ "type": "set_model", "modelId": model }),
        };
        self.send_cmd(cmd)
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        // A fresh cache answers instantly — the RPC itself costs the CLI's
        // whole MCP-server boot (~20s), which made every picker refresh feel
        // dead and usually hit the old 20s timeout.
        if let Some((at, cached)) = self.inner.lock().unwrap().models_cache.clone() {
            if at.elapsed() < MODELS_CACHE_TTL && !cached.is_empty() {
                return Ok(cached);
            }
        }
        // Real round-trip: send get_available_models and await the response.
        // Note: the first command after spawn can take several seconds while
        // a-coder-cli loads its MCP servers, hence the generous timeout.
        let (tx, rx) = oneshot::channel();
        {
            let mut inner = self.inner.lock().unwrap();
            if !inner.started {
                return Err(HarnessError::NotStarted);
            }
            inner.pending_models = Some(tx);
        }
        self.send_cmd(serde_json::json!({ "type": "get_available_models" }))?;

        match tokio::time::timeout(MODELS_RPC_TIMEOUT, rx).await {
            Ok(Ok(models)) if !models.is_empty() => {
                self.inner.lock().unwrap().models_cache =
                    Some((std::time::Instant::now(), models.clone()));
                Ok(models)
            }
            // Timeout/empty: fall back WITHOUT caching, so a later refresh can
            // still pick up the real catalog once the CLI is warm.
            _ => Ok(fallback_models()),
        }
    }

    async fn answer_approval(
        &self,
        request_id: &str,
        approved: bool,
        value: Option<String>,
    ) -> Result<(), HarnessError> {
        // Build the correct extension_ui_response for the dialog method that
        // issued the request (docs/rpc.md §Extension UI Responses):
        //   confirm      -> {"id", "confirmed": true}       / {"id", "cancelled": true}
        //   select       -> {"id", "value": first option}   / {"id", "cancelled": true}
        //   input/editor -> {"id", "value": edited text}    / {"id", "cancelled": true}
        let req = {
            let mut inner = self.inner.lock().unwrap();
            if !inner.started {
                return Err(HarnessError::NotStarted);
            }
            inner.pending_ui.remove(request_id)
        };

        let cmd = match (req, approved) {
            (Some(r), true) => {
                let method = r
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("confirm");
                match method {
                    "confirm" => {
                        serde_json::json!({ "type": "extension_ui_response", "id": request_id, "confirmed": true })
                    }
                    "select" => {
                        let first = r
                            .get("options")
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.first())
                            .and_then(|v| v.as_str())
                            .unwrap_or("Allow");
                        serde_json::json!({ "type": "extension_ui_response", "id": request_id, "value": first })
                    }
                    _ => {
                        // input / editor — approve, carrying the user's edited
                        // text when the dialog's Edit flow produced one.
                        let v = value.unwrap_or_default();
                        serde_json::json!({ "type": "extension_ui_response", "id": request_id, "value": v })
                    }
                }
            }
            (_, false) => {
                serde_json::json!({ "type": "extension_ui_response", "id": request_id, "cancelled": true })
            }
            (None, true) => {
                // Unknown/expired request — best effort confirm-true.
                serde_json::json!({ "type": "extension_ui_response", "id": request_id, "confirmed": true })
            }
        };

        // The process may have ended the dialog on its own (timeout); sending
        // the response anyway is harmless.
        let inner = self.inner.lock().unwrap();
        if !inner.started {
            return Err(HarnessError::NotStarted);
        }
        let mut stdin = inner.stdin.lock().unwrap();
        let child_stdin = stdin.as_mut().ok_or(HarnessError::NotStarted)?;
        writeln!(child_stdin, "{}", cmd)
            .map_err(|e| HarnessError::Process(format!("write approval answer: {e}")))?;
        Ok(())
    }

    fn subscribe(&self) -> Receiver<HarnessEvent> {
        let (tx, rx) = mpsc::channel(256);
        let mut inner = self.inner.lock().unwrap();
        inner.subscribers.retain(|s| !s.is_closed());
        inner.subscribers.push(tx);
        rx
    }

    async fn stop(&self) -> Result<(), HarnessError> {
        // Take the child out of Inner and kill the ENTIRE process tree. On
        // Windows a-coder-cli is `cmd /c shim.cmd -> node.exe`; killing only
        // cmd.exe would orphan node with our stdout pipe still held open.
        let child = {
            let mut inner = self.inner.lock().unwrap();
            inner.stopping = true;
            inner.started = false;
            inner.generation += 1; // orphan this process's stdout reader
            *inner.stdin.lock().unwrap() = None; // closing stdin lets it exit too
            let mut handle = inner.child_handle.lock().unwrap();
            handle.take()
        };
        if let Some(child) = child {
            let pid = child.id();
            tracing::info!("a-coder-cli stop(): killing pid {pid}");
            if cfg!(windows) {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .output();
            } else {
                let _ = std::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .output();
            }
            // Dropping the Child detaches it; the tree kill above handles exit.
        }
        Ok(())
    }
}

// --- stdout handling ---------------------------------------------------------

/// Process one parsed JSONL line from a-coder-cli stdout.
fn handle_stdout_line(inner: &Arc<std::sync::Mutex<Inner>>, json: &serde_json::Value) {
    let event_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match event_type {
        // Command ack — surface failures; resolve the models round-trip.
        "response" => {
            let success = json
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let command = json.get("command").and_then(|v| v.as_str()).unwrap_or("");
            if !success {
                let err = json
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("command failed")
                    .to_string();
                broadcast(inner, HarnessEvent::Error(format!("{command}: {err}")));
                return;
            }
            if command == "get_available_models" {
                let models = json
                    .pointer("/data/models")
                    .and_then(|v| v.as_array())
                    .map(|arr| parse_models(arr))
                    .unwrap_or_default();
                let tx = inner.lock().unwrap().pending_models.take();
                if let Some(tx) = tx {
                    let _ = tx.send(models);
                }
            }
        }

        // Extension UI sub-protocol: dialogs become ApprovalRequests;
        // fire-and-forget methods (notify, setStatus, ...) are skipped.
        "extension_ui_request" => {
            let method = json.get("method").and_then(|v| v.as_str()).unwrap_or("");
            let is_dialog = matches!(method, "select" | "confirm" | "input" | "editor");
            if !is_dialog {
                return;
            }
            let id = json
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            inner
                .lock()
                .unwrap()
                .pending_ui
                .insert(id.clone(), json.clone());
            broadcast(
                inner,
                HarnessEvent::ApprovalRequest {
                    id,
                    kind: method.to_string(),
                    payload: json.clone(),
                },
            );
        }

        _ => {
            let default_model = inner.lock().unwrap().model.clone();
            if let Some(ev) = parse_agent_event(json, &default_model) {
                broadcast(inner, ev);
            }
        }
    }
}

/// Broadcast an event to every live subscriber (drops closed channels).
fn broadcast(inner: &Arc<std::sync::Mutex<Inner>>, event: HarnessEvent) {
    let mut g = inner.lock().unwrap();
    g.subscribers.retain(|tx| !tx.is_closed());
    for tx in &g.subscribers {
        let _ = tx.try_send(event.clone());
    }
}

/// Pure mapping of agent event JSONL → HarnessEvent per docs/rpc.md.
fn parse_agent_event(json: &serde_json::Value, default_model: &str) -> Option<HarnessEvent> {
    let event_type = json.get("type").and_then(|v| v.as_str())?;

    match event_type {
        "agent_start" => Some(HarnessEvent::AgentStart {
            model: default_model.to_string(),
        }),

        "agent_end" => {
            // Derive success from the last assistant message's stopReason
            // ("stop"/"length"/"toolUse" = success; "error"/"aborted" = not).
            let success = json
                .get("messages")
                .and_then(|v| v.as_array())
                .and_then(|msgs| {
                    msgs.iter()
                        .rev()
                        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
                })
                .and_then(|m| m.get("stopReason").and_then(|s| s.as_str()))
                .map(|r| !matches!(r, "error" | "aborted"))
                .unwrap_or(true);
            Some(HarnessEvent::AgentEnd {
                success,
                message: None,
            })
        }

        "message_update" => {
            let delta = json.get("assistantMessageEvent")?;
            match delta.get("type").and_then(|v| v.as_str())? {
                "text_delta" => Some(HarnessEvent::TextDelta(
                    delta
                        .get("delta")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                )),
                "thinking_delta" => Some(HarnessEvent::ThinkingDelta(
                    delta
                        .get("delta")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                )),
                "error" => Some(HarnessEvent::Error(
                    delta
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("stream error")
                        .to_string(),
                )),
                _ => None, // start/text_start/text_end/toolcall_* etc. — not surfaced
            }
        }

        "tool_execution_start" => Some(HarnessEvent::ToolStart {
            tool_id: json
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            name: json
                .get("toolName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            args: json.get("args").cloned().unwrap_or(serde_json::Value::Null),
        }),

        "tool_execution_update" => Some(HarnessEvent::ToolUpdate {
            tool_id: json
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            partial: content_text(json.get("partialResult")),
        }),

        "tool_execution_end" => Some(HarnessEvent::ToolEnd {
            tool_id: json
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            result: {
                let t = content_text(json.get("result"));
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            },
            is_error: json
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }),

        "queue_update" => Some(HarnessEvent::QueueUpdate {
            steer: json
                .get("steering")
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false),
            follow_up: json
                .get("followUp")
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false),
        }),

        "auto_retry_start" => Some(HarnessEvent::Retry {
            attempt: json.get("attempt").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            reason: json
                .get("errorMessage")
                .and_then(|v| v.as_str())
                .unwrap_or("transient error")
                .to_string(),
        }),

        "extension_error" => Some(HarnessEvent::Error(
            json.get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("extension error")
                .to_string(),
        )),

        // turn_start/turn_end/message_start/message_end/compaction_*/auto_retry_end —
        // intentionally not surfaced (no UI mapping yet).
        _ => None,
    }
}

/// Extract concatenated text from an RPC content array
/// (`{"content":[{"type":"text","text":...}, ...]}`).
fn content_text(v: Option<&serde_json::Value>) -> String {
    let Some(items) = v.and_then(|c| c.get("content")).and_then(|c| c.as_array()) else {
        return String::new();
    };
    let mut out = String::new();
    for item in items {
        if item.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            }
        }
    }
    out
}

/// Parse rpc Model objects into ModelInfo (id becomes "provider/modelId" so
/// set_model can round-trip it).
fn parse_models(models: &[serde_json::Value]) -> Vec<ModelInfo> {
    models
        .iter()
        .filter_map(|m| {
            let model_id = m.get("id").and_then(|v| v.as_str())?;
            let provider = m.get("provider").and_then(|v| v.as_str());
            let id = match provider {
                Some(p) if !p.is_empty() => format!("{p}/{model_id}"),
                _ => model_id.to_string(),
            };
            Some(ModelInfo {
                id,
                name: m.get("name").and_then(|v| v.as_str()).map(String::from),
                kind: "chat".to_string(),
            })
        })
        .collect()
}

/// Static fallback when the models round-trip fails or times out.
fn fallback_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "anthropic/claude-sonnet-4-5".into(),
            name: Some("Claude Sonnet 4.5".into()),
            kind: "chat".into(),
        },
        ModelInfo {
            id: "openai/gpt-5".into(),
            name: Some("GPT-5".into()),
            kind: "chat".into(),
        },
        ModelInfo {
            id: "google/gemini-2.5-pro".into(),
            name: Some("Gemini 2.5 Pro".into()),
            kind: "chat".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> serde_json::Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn aacoder_id_is_stable() {
        let h = AaaCoderCliHarness::new();
        assert_eq!(h.id(), "a-coder-cli");
    }

    #[test]
    fn aacoder_capabilities_are_full() {
        let h = AaaCoderCliHarness::new();
        let c = h.capabilities();
        assert!(c.steer && c.abort && c.list_models && c.approvals && c.persistent);
    }

    #[test]
    fn parse_message_update_text_delta() {
        // Exact shape from docs/rpc.md.
        let json = v(
            r#"{"type":"message_update","message":{},"assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hello "}}"#,
        );
        match parse_agent_event(&json, "m") {
            Some(HarnessEvent::TextDelta(t)) => assert_eq!(t, "Hello "),
            other => panic!("expected TextDelta, got {other:?}"),
        }
    }

    #[test]
    fn parse_message_update_thinking_delta() {
        let json = v(
            r#"{"type":"message_update","message":{},"assistantMessageEvent":{"type":"thinking_delta","delta":"hm..."}}"#,
        );
        match parse_agent_event(&json, "m") {
            Some(HarnessEvent::ThinkingDelta(t)) => assert_eq!(t, "hm..."),
            other => panic!("expected ThinkingDelta, got {other:?}"),
        }
    }

    #[test]
    fn parse_message_update_other_deltas_skipped() {
        let json = v(
            r#"{"type":"message_update","message":{},"assistantMessageEvent":{"type":"text_start","contentIndex":0}}"#,
        );
        assert!(parse_agent_event(&json, "m").is_none());
    }

    #[test]
    fn parse_tool_execution_lifecycle() {
        let start = v(
            r#"{"type":"tool_execution_start","toolCallId":"call_1","toolName":"bash","args":{"command":"ls"}}"#,
        );
        match parse_agent_event(&start, "m") {
            Some(HarnessEvent::ToolStart {
                tool_id,
                name,
                args,
            }) => {
                assert_eq!(tool_id, "call_1");
                assert_eq!(name, "bash");
                assert_eq!(args["command"], "ls");
            }
            other => panic!("expected ToolStart, got {other:?}"),
        }

        let update = v(
            r#"{"type":"tool_execution_update","toolCallId":"call_1","partialResult":{"content":[{"type":"text","text":"partial out"}]}}"#,
        );
        match parse_agent_event(&update, "m") {
            Some(HarnessEvent::ToolUpdate { tool_id, partial }) => {
                assert_eq!(tool_id, "call_1");
                assert_eq!(partial, "partial out");
            }
            other => panic!("expected ToolUpdate, got {other:?}"),
        }

        let end = v(
            r#"{"type":"tool_execution_end","toolCallId":"call_1","result":{"content":[{"type":"text","text":"total 48"}]},"isError":false}"#,
        );
        match parse_agent_event(&end, "m") {
            Some(HarnessEvent::ToolEnd {
                tool_id,
                result,
                is_error,
            }) => {
                assert_eq!(tool_id, "call_1");
                assert_eq!(result.as_deref(), Some("total 48"));
                assert!(!is_error);
            }
            other => panic!("expected ToolEnd, got {other:?}"),
        }
    }

    #[test]
    fn parse_queue_update_arrays() {
        let json = v(r#"{"type":"queue_update","steering":["focus"],"followUp":[]}"#);
        match parse_agent_event(&json, "m") {
            Some(HarnessEvent::QueueUpdate { steer, follow_up }) => {
                assert!(steer);
                assert!(!follow_up);
            }
            other => panic!("expected QueueUpdate, got {other:?}"),
        }
    }

    #[test]
    fn parse_auto_retry_start() {
        let json = v(
            r#"{"type":"auto_retry_start","attempt":1,"maxAttempts":3,"delayMs":2000,"errorMessage":"529 overloaded"}"#,
        );
        match parse_agent_event(&json, "m") {
            Some(HarnessEvent::Retry { attempt, reason }) => {
                assert_eq!(attempt, 1);
                assert_eq!(reason, "529 overloaded");
            }
            other => panic!("expected Retry, got {other:?}"),
        }
    }

    #[test]
    fn parse_agent_start_uses_ctx_model() {
        let json = v(r#"{"type":"agent_start"}"#);
        match parse_agent_event(&json, "anthropic/claude-sonnet-4-5") {
            Some(HarnessEvent::AgentStart { model }) => {
                assert_eq!(model, "anthropic/claude-sonnet-4-5")
            }
            other => panic!("expected AgentStart, got {other:?}"),
        }
    }

    #[test]
    fn parse_agent_end_success_from_stop_reason() {
        let ok = v(r#"{"type":"agent_end","messages":[{"role":"assistant","stopReason":"stop"}]}"#);
        assert!(matches!(
            parse_agent_event(&ok, "m"),
            Some(HarnessEvent::AgentEnd { success: true, .. })
        ));

        let aborted =
            v(r#"{"type":"agent_end","messages":[{"role":"assistant","stopReason":"aborted"}]}"#);
        assert!(matches!(
            parse_agent_event(&aborted, "m"),
            Some(HarnessEvent::AgentEnd { success: false, .. })
        ));

        let empty = v(r#"{"type":"agent_end","messages":[]}"#);
        assert!(matches!(
            parse_agent_event(&empty, "m"),
            Some(HarnessEvent::AgentEnd { success: true, .. })
        ));
    }

    #[test]
    fn parse_extension_error() {
        let json = v(
            r#"{"type":"extension_error","extensionPath":"/x.ts","event":"tool_call","error":"boom"}"#,
        );
        match parse_agent_event(&json, "m") {
            Some(HarnessEvent::Error(e)) => assert_eq!(e, "boom"),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn parse_unknown_event_skipped() {
        assert!(parse_agent_event(&v(r#"{"type":"turn_start"}"#), "m").is_none());
        assert!(parse_agent_event(
            &v(r#"{"type":"compaction_start","reason":"threshold"}"#),
            "m"
        )
        .is_none());
    }

    #[test]
    fn parse_models_uses_provider_prefix() {
        let models = vec![
            v(r#"{"id":"claude-sonnet-4-5","name":"Claude Sonnet 4.5","provider":"anthropic"}"#),
            v(r#"{"id":"local-llama","name":"Llama"}"#),
        ];
        let parsed = parse_models(&models);
        assert_eq!(parsed[0].id, "anthropic/claude-sonnet-4-5");
        assert_eq!(parsed[1].id, "local-llama");
    }

    #[test]
    fn subscribe_returns_live_receiver() {
        let h = AaaCoderCliHarness::new();
        let mut rx = h.subscribe();
        {
            let inner = Arc::clone(&h.inner);
            broadcast(&inner, HarnessEvent::TextDelta("hi".into()));
        }
        // try_recv works because the subscriber sender is registered.
        match rx.try_recv() {
            Ok(HarnessEvent::TextDelta(t)) => assert_eq!(t, "hi"),
            other => panic!("expected TextDelta from broadcast, got {other:?}"),
        }
    }

    /// Live round-trip against a real `a-coder-cli --mode rpc` process.
    /// No LLM calls: only the get_available_models command/response cycle.
    /// Run explicitly: cargo test live_rpc_models_round_trip -- --ignored
    /// multi_thread (not the default current-thread): the adapter's stdout
    /// reader blocks in read_line inside a spawned task — on a single-worker
    /// runtime it starves the test future and the whole test deadlocks
    /// (same lesson as e2e_prompt_completes_against_stub below).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires a-coder-cli installed on PATH"]
    async fn live_rpc_models_round_trip() {
        if !is_available() {
            eprintln!("skipping: a-coder-cli not on PATH");
            return;
        }
        eprintln!("[1] binary available");
        let h = AaaCoderCliHarness::new();
        let tmp = std::env::temp_dir().join(format!("amaara-rpc-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let ctx = HarnessCtx {
            project_dir: tmp.clone(),
            model: String::new(),
            source: "cloud".into(),
            control_url: None,
            control_token: None,
        };
        h.start(&ctx).await.expect("rpc process should start");
        eprintln!("[2] started");
        let models = h.available_models().await.expect("models round-trip");
        eprintln!("[3] models returned");
        assert!(!models.is_empty(), "expected at least one configured model");
        eprintln!(
            "live models: {:?}",
            models.iter().map(|m| m.id.clone()).collect::<Vec<_>>()
        );
        h.stop().await.unwrap();
        eprintln!("[4] stopped");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

/// Headless end-to-end (Phase 15): drive the real adapter against the
/// `tools/harness-stubs/a-coder-cli` stub speaking the documented rpc
/// protocol, and assert a full prompt → agent_start → text deltas →
/// agent_end cycle plus a get_available_models round-trip. Runs in CI —
/// no LLM, no network, plain Node.
#[cfg(test)]
mod e2e_stub {
    use super::*;
    use std::time::Duration;

    fn stub_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tools")
            .join("harness-stubs")
    }

    fn stub_shim() -> std::path::PathBuf {
        if cfg!(windows) {
            stub_dir().join("a-coder-cli.cmd")
        } else {
            stub_dir().join("a-coder-cli")
        }
    }

    // multi_thread: the adapter's stdout reader does blocking read_line in a
    // spawned task, which would starve a current-thread runtime.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_prompt_completes_against_stub() {
        let shim = stub_shim();
        assert!(shim.exists(), "stub shim missing: {}", shim.display());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755));
        }
        // Route the adapter's which("a-coder-cli") at the stub.
        std::env::set_var("AMAARA_AACODER_BIN", &shim);

        let h = AaaCoderCliHarness::new();
        let tmp = std::env::temp_dir().join(format!("amaara-e2e-aacoder-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let ctx = HarnessCtx {
            project_dir: tmp.clone(),
            model: "stub/stub-model".into(),
            source: "cloud".into(),
            control_url: None,
            control_token: None,
        };
        h.start(&ctx).await.expect("stub should start");

        // Models round-trip over the same framing.
        let models = h.available_models().await.expect("models round-trip");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "stub/stub-model");

        // Prompt cycle: collect events until AgentEnd.
        let mut rx = h.subscribe();
        h.prompt("say hi", crate::harness::PromptMode::Normal)
            .await
            .expect("prompt should send");

        let mut saw_start = false;
        let mut text = String::new();
        let mut ended: Option<bool> = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while ended.is_none() {
            let ev = tokio::time::timeout_at(deadline, rx.recv())
                .await
                .expect("timed out waiting for agent_end")
                .expect("event channel closed");
            match ev {
                HarnessEvent::AgentStart { .. } => saw_start = true,
                HarnessEvent::TextDelta(t) => text.push_str(&t),
                HarnessEvent::AgentEnd { success, .. } => ended = Some(success),
                _ => {}
            }
        }

        h.stop().await.unwrap();
        std::env::remove_var("AMAARA_AACODER_BIN");
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(saw_start, "no AgentStart event");
        assert_eq!(text, "stub reply", "unexpected streamed text");
        assert_eq!(ended, Some(true), "agent should end successfully");
    }
}

#[cfg(test)]
mod extension_staging {
    use super::*;

    #[test]
    fn stages_tool_bridge_extension_idempotently() {
        let dir = std::env::temp_dir().join(format!("amaara-ext-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        stage_project_extension(&dir);
        let file = dir
            .join(".a-coder-cli")
            .join("extensions")
            .join("amaara-studio.ts");
        let staged = std::fs::read_to_string(&file).expect("extension should be staged");
        assert_eq!(staged, STUDIO_EXTENSION_TS);
        assert!(staged.contains("registerTool") && staged.contains("render_to_video"));

        // Second stage with identical content must not churn the file
        // (mtime-agnostic assertion: content stays byte-identical).
        stage_project_extension(&dir);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), STUDIO_EXTENSION_TS);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod models_cache_tests {
    use super::*;

    #[test]
    fn fresh_cache_answers_without_a_live_round_trip() {
        let h = AaaCoderCliHarness::new();
        // Not started, no stdin — the RPC path would fail with NotStarted.
        // A fresh cache must short-circuit and return the cached catalog.
        let cached = vec![ModelInfo {
            id: "ollama-cloud/nemotron-3-super".into(),
            name: Some("Ollama Cloud: nemotron-3-super".into()),
            kind: "chat".into(),
        }];
        h.inner.lock().unwrap().models_cache = Some((std::time::Instant::now(), cached.clone()));
        let models = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(h.available_models())
            .expect("cache hit");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "ollama-cloud/nemotron-3-super");
    }

    #[test]
    fn stale_cache_does_not_short_circuit() {
        let h = AaaCoderCliHarness::new();
        // Expired cache: the method must fall through to the started check
        // (NotStarted here) rather than serving stale data forever.
        h.inner.lock().unwrap().models_cache = Some((
            std::time::Instant::now() - MODELS_CACHE_TTL - Duration::from_secs(1),
            vec![ModelInfo {
                id: "old".into(),
                name: None,
                kind: "chat".into(),
            }],
        ));
        let res = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(h.available_models());
        assert!(matches!(res, Err(HarnessError::NotStarted)));
    }
}
