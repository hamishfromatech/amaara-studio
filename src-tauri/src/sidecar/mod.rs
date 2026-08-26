//! Sidecar supervision: spawn, supervise, and cleanly stop child processes
//! (Phase 2). The lifecycle logic is a pure state machine so it can be unit
//! tested without a running Tauri app; runtime event emission goes through the
//! single `"studio://event"` channel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::events;

pub mod llama;

pub mod spec {
    //! The three sidecars the app supervises (harness / render / sd-server).
    use super::*;

    /// A supervised child process definition. `bin` may be an absolute path or a
    /// name resolved via env override / PATH (BUILD-GAPS risk #34).
    #[derive(Debug, Clone)]
    pub struct SidecarSpec {
        pub name: String,
        pub bin: String,
        pub args: Vec<String>,
        pub env: HashMap<String, String>,
    }

    impl SidecarSpec {
        pub fn new(name: &str) -> Self {
            let mut spec = SidecarSpec {
                name: name.to_string(),
                bin: name.to_string(), // real binary name (resolved via PATH)
                args: vec![],
                env: HashMap::new(),
            };
            spec.resolve_bin();
            spec
        }

        /// Resolve the binary path from a per-name `NAVYA_SIDECAR_<NAME>_BIN`
        /// env var, then PATH via `which`. Returns the resolved name even if not
        /// found on PATH — validation happens later in `validate()`.
        pub fn resolve_bin(&mut self) {
            if let Ok(p) = std::env::var(format!("NAVYA_SIDECAR_{}_BIN", self.name.to_uppercase().replace("-", "_"))) {
                self.bin = p;
                return;
            }
            // Try PATH lookup; if missing keep the bare name so validate() reports it.
            if let Some(p) = which_path(&self.bin) {
                self.bin = p.to_string_lossy().to_string();
            }
        }

        /// Resolve to an absolute path when possible.
        pub fn resolve_path(&self) -> Result<PathBuf, SidecarError> {
            let full = if Path::new(&self.bin).is_absolute() {
                PathBuf::from(&self.bin)
            } else {
                // Try PATH lookup one more time
                which_path(&self.bin).unwrap_or_else(|| PathBuf::from(&self.bin))
            };
            if full.exists() {
                Ok(full)
            } else {
                Err(SidecarError::NotFound { name: self.name.clone(), bin: self.bin.clone() })
            }
        }

        pub fn validate(&self) -> Result<(), SidecarError> {
            self.resolve_path()?;
            Ok(())
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("sidecar binary '{name}' ({bin}) not found")]
    NotFound { name: String, bin: String },
    #[error("command failed to launch: {0}")]
    Launch(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Lifecycle status. `Exited(code)` is terminal; `Running`/`Starting` are active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarStatus {
    Starting,
    Running,
    Stopped,
    Exited(i32),
}

/// Captured result of running a sidecar to completion (used by tests + lifecycle).
#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Look up a binary on PATH (Windows: `where`, Unix: `which`).
pub fn which_path(bin: &str) -> Option<PathBuf> {
    let cmd = if cfg!(windows) { "where" } else { "which" };
    let output = std::process::Command::new(cmd).arg(bin).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|s| PathBuf::from(s.trim()))
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The supervised-process state machine. Thread-safe; shared across the app via
/// `Arc`. Runtime spawning feeds [`CommandOutcome`]s here so restart/stop logic
/// stays identical whether launched through Tauri or std::process.
#[derive(Default)]
pub struct Supervisor {
    entries: HashMap<String, SupEntry>,
    /// When true, a sidecar that exits is restarted once (Gate 2 lifecycle test).
    pub restart_on_exit: bool,
}

#[derive(Debug)]
struct SupEntry {
    #[allow(dead_code)]
    spec: spec::SidecarSpec,
    status: SidecarStatus,
    started_at_ms: i64,
    /// Set after the first restart so a crash loop can't spin forever
    /// (restart-on-exit is documented as "restart once", Gate 2).
    restarted: bool,
    #[allow(dead_code)]
    log_lines: Vec<(i64, String)>, // (ts_ms, line)
}

impl Supervisor {
    pub fn new() -> Self {
        Supervisor {
            entries: HashMap::new(),
            restart_on_exit: true,
        }
    }

    /// Start a sidecar. Idempotent — starting an already-running one is a no-op.
    pub fn start(&mut self, spec: spec::SidecarSpec) -> Result<(), SidecarError> {
        let name = spec.name.clone();
        if self.entries.contains_key(&name) {
            return Ok(()); // already tracked (running or stopped)
        }
        spec.validate()?;
        self.entries.insert(
            name.clone(),
            SupEntry {
                spec,
                status: SidecarStatus::Starting,
                started_at_ms: now_ms(),
                restarted: false,
                log_lines: Vec::new(),
            },
        );
        Ok(())
    }

    pub fn stop(&mut self, name: &str) -> Option<SidecarStatus> {
        let mut entry = self.entries.remove(name)?;
        if matches!(entry.status, SidecarStatus::Running | SidecarStatus::Starting) {
            entry.status = SidecarStatus::Stopped;
        }
        Some(entry.status)
    }

    /// Restart a stopped sidecar (Gate 2: restart-on-exit behavior). No-op if it
    /// is running or unknown. Returns the new entry's id.
    pub fn restart(&mut self, name: &str) -> Result<(), SidecarError> {
        let Some(entry) = self.entries.get(name) else {
            return Err(SidecarError::NotFound {
                name: name.to_string(),
                bin: String::new(),
            });
        };
        if matches!(entry.status, SidecarStatus::Running | SidecarStatus::Starting) {
            return Ok(()); // don't double-start a live sidecar
        }
        let spec = entry.spec.clone();
        self.start(spec)?;
        Ok(())
    }

    /// Fold a child's terminal outcome into the supervisor state. Implements
    /// restart-on-exit (at most once per entry) and records the exit code for
    /// status-strip display.
    pub fn on_exit(&mut self, name: &str, outcome: CommandOutcome) {
        let Some(entry) = self.entries.get_mut(name) else {
            return;
        };
        entry.log_lines.push((now_ms(), format!("exit={}", outcome.exit_code.unwrap_or(-1))));
        let code = outcome.exit_code.unwrap_or(-1);
        let first_exit = !matches!(entry.status, SidecarStatus::Exited(_));
        if self.restart_on_exit && !entry.restarted && first_exit {
            // First exit: restart once.
            entry.restarted = true;
            entry.log_lines.clear();
            entry.status = SidecarStatus::Starting;
            entry.started_at_ms = now_ms();
        } else {
            // Disabled, already restarted, or already terminal → stay Exited.
            entry.status = SidecarStatus::Exited(code);
        }
    }

    /// Current status snapshot for the UI / tests.
    pub fn status(&self, name: &str) -> Option<SidecarStatus> {
        self.entries.get(name).map(|e| e.status.clone())
    }

    /// Number of tracked entries (used by tests).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if no entries are tracked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Emit `SidecarEvent`s for every tracked sidecar into the webview. Runtime
    /// only — reads shared state and requires a Tauri app handle.
    #[allow(unused_variables)]
    pub fn emit(&self, app_handle: &tauri::AppHandle) -> Result<(), tauri::Error> {
        use tauri::Emitter;
        for (name, entry) in self.entries.iter() {
            match &entry.status {
                SidecarStatus::Running | SidecarStatus::Starting => {
                    let _ = app_handle.emit("studio://event", events::SidecarEvent::Ready { name: name.clone() });
                }
                SidecarStatus::Exited(code) => {
                    let _ = app_handle.emit(
                        "studio://event",
                        events::SidecarEvent::Exit { name: name.clone(), code: *code },
                    );
                }
                SidecarStatus::Stopped => {}
            }
        }
        Ok(())
    }

    /// Wrap as Arc for shared ownership.
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

/// Forward a sidecar child's stdout/stderr to the UI as `SidecarEvent::LogLine`
/// lines (status-strip log drawer, Phase 15). Also prevents pipe-buffer
/// deadlock: without a reader, a chatty child can stall once its pipe fills.
///
/// The task runs detached for the life of the stream — when the child exits
/// the stream closes and the loop ends naturally.
pub fn spawn_log_forwarder(
    name: &str,
    stream: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    level: &'static str,
    app_handle: tauri::AppHandle,
) {
    use tauri::Emitter;
    use tokio::io::AsyncBufReadExt;
    let name = name.to_string();
    tokio::spawn(async move {
        let reader = tokio::io::BufReader::new(stream);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app_handle.emit(
                "studio://event",
                events::SidecarEvent::LogLine {
                    name: name.clone(),
                    level: level.to_string(),
                    message: line,
                },
            );
        }
    });
}

impl std::fmt::Debug for Supervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Supervisor")
            .field("restart_on_exit", &self.restart_on_exit)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Path to the bundled stub binaries used in tests.
    fn stubs_dir() -> std::path::PathBuf {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tools")
            .join("sidecar-stubs");
        p
    }

    fn stub_spec(name: &str, bin: &str) -> spec::SidecarSpec {
        let mut s = spec::SidecarSpec::new(name);
        s.bin = stubs_dir().join(bin).to_string_lossy().to_string();
        s.args = vec!["--role".to_string(), name.to_string()];
        s
    }

    #[test]
    fn start_is_idempotent_and_validates_path() {
        let mut sup = Supervisor::new();
        let s = stub_spec("harness", "navya-harness-stub");
        sup.start(s.clone()).unwrap();
        assert_eq!(sup.status("harness"), Some(SidecarStatus::Starting));
        // Starting again is a no-op (still the same entry).
        let _ = sup.start(s);
        assert_eq!(sup.status("harness"), Some(SidecarStatus::Starting));
    }

    #[test]
    fn restart_after_stop() {
        let mut sup = Supervisor::new();
        let s = stub_spec("render", "navya-render-stub");
        sup.start(s.clone()).unwrap();
        // stop() returns the final status (Stopped for a previously Starting/Running entry).
        // The entry is removed from the map.
        assert_eq!(sup.stop("render"), Some(SidecarStatus::Stopped));
        assert_eq!(sup.status("render"), None);
        // start() re-registers the entry with Starting status.
        sup.start(s).unwrap();
        assert_eq!(sup.status("render"), Some(SidecarStatus::Starting));
    }

    #[test]
    fn restart_on_exit_cycles_status_once() {
        let mut sup = Supervisor::new();
        let s = stub_spec("sd-server", "navya-sd-stub");
        sup.start(s).unwrap();
        // First exit with a nonzero code: supervisor cycles back to Starting.
        sup.on_exit("sd-server", CommandOutcome { exit_code: Some(1), stdout: String::new(), stderr: String::new() });
        assert_eq!(sup.status("sd-server"), Some(SidecarStatus::Starting));
        // Second exit: terminal (restart-once, no crash loop).
        sup.on_exit("sd-server", CommandOutcome { exit_code: Some(1), stdout: String::new(), stderr: String::new() });
        assert_eq!(sup.status("sd-server"), Some(SidecarStatus::Exited(1)));
    }

    #[test]
    fn no_restart_when_disabled() {
        let mut sup = Supervisor::new();
        sup.restart_on_exit = false;
        let s = stub_spec("harness", "navya-harness-stub");
        sup.start(s).unwrap();
        sup.on_exit("harness", CommandOutcome { exit_code: Some(0), stdout: String::new(), stderr: String::new() });
        assert_eq!(sup.status("harness"), Some(SidecarStatus::Exited(0)));
    }

    #[test]
    fn stop_unknown_returns_none() {
        let mut sup = Supervisor::new();
        assert!(sup.stop("missing").is_none());
    }

    #[test]
    fn missing_binary_is_reported() {
        let s = stub_spec("harness", "does-not-exist-binary");
        let err = s.validate().unwrap_err();
        assert!(matches!(err, SidecarError::NotFound { .. }));
    }
}