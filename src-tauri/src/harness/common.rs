//! Shared harness-adapter utilities (Phase 12+).
//!
//! Each harness adapter needs the same low-level primitives:
//! - spawn a CLI binary in the project directory
//! - inject the control-server URL/token into the environment
//! - kill the entire process tree on stop (Windows `taskkill /F /T`, Unix `-TERM`)
//! - write the harness-pack config file with the live control-server URL/token
//! - broadcast events to subscribers
//!
//! This module extracts those primitives so each adapter file focuses on its
//! native wire protocol (stream-json, JSON-RPC, WebSocket, ...) instead of
//! duplicating child-process plumbing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

use tokio::sync::mpsc;

use crate::harness::event::HarnessEvent;
use crate::harness::HarnessError;

/// Inject `AMAARA_CONTROL_URL` and `AMAARA_CONTROL_TOKEN` into a `Command`.
pub fn inject_control_env(
    cmd: &mut Command,
    control_url: Option<&str>,
    control_token: Option<&str>,
) {
    if let Some(url) = control_url {
        cmd.env("AMAARA_CONTROL_URL", url);
    }
    if let Some(token) = control_token {
        cmd.env("AMAARA_CONTROL_TOKEN", token);
    }
}

/// Spawn `prog` with `args` in `cwd`, piping stdin/stdout and discarding stderr.
/// Injects the control-server env vars before spawning. Returns the child and
/// the piped stdin so the adapter can write native commands.
pub fn spawn_command(
    prog: &Path,
    args: &[String],
    cwd: &Path,
    control_url: Option<&str>,
    control_token: Option<&str>,
) -> Result<(Child, ChildStdin), HarnessError> {
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    inject_control_env(&mut cmd, control_url, control_token);
    // No flashing console window on Windows (creation flag inherited from
    // Command automatically if spawned from a GUI app; explicit for robustness).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| HarnessError::Process(format!("failed to spawn harness: {e}")))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| HarnessError::Process("harness stdin pipe missing".into()))?;
    Ok((child, stdin))
}

/// Kill the entire process tree rooted at `pid`.
/// Windows: `taskkill /F /T /PID <pid>`. Unix: `kill -TERM <pid>`.
pub fn kill_process_tree(pid: u32) {
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output();
    } else {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output();
    }
}

/// Resolve the absolute path to the amaara-mcp workspace directory.
/// Looks first for `AMAARA_MCP_DIR` env var, then the bundled relative path
/// `<src-tauri>/../amaara-mcp` (works both in dev and in an installed app when
/// the workspace is copied next to the binary).
pub fn mcp_workspace_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("AMAARA_MCP_DIR") {
        return PathBuf::from(dir);
    }
    // Default: one level up from the compiled binary's directory.
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("."))
        .parent()
        .unwrap_or(Path::new("."))
        .join("..")
        .join("amaara-mcp")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("amaara-mcp"))
}

/// Write a text config file, creating parent directories as needed.
pub fn write_config(path: &Path, content: &str) -> Result<(), HarnessError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            HarnessError::Process(format!("creating config dir {}: {e}", parent.display()))
        })?;
    }
    std::fs::write(path, content)
        .map_err(|e| HarnessError::Process(format!("writing config {}: {e}", path.display())))
}

/// Broadcast an event to every live subscriber, dropping closed channels.
pub fn broadcast(subscribers: &mut Vec<mpsc::Sender<HarnessEvent>>, event: HarnessEvent) {
    subscribers.retain(|tx| !tx.is_closed());
    for tx in subscribers {
        let _ = tx.try_send(event.clone());
    }
}

/// Write one JSONL line to a harness's stdin.
pub fn send_json_line(
    stdin: &mut ChildStdin,
    value: &serde_json::Value,
) -> Result<(), HarnessError> {
    let line = format!(
        "{}\n",
        serde_json::to_string(value).map_err(|e| HarnessError::Process(e.to_string()))?
    );
    stdin
        .write_all(line.as_bytes())
        .map_err(|e| HarnessError::Process(format!("write to harness stdin: {e}")))?;
    stdin
        .flush()
        .map_err(|e| HarnessError::Process(format!("flush harness stdin: {e}")))
}

/// A small bundle of child-process state used by every stdio-based adapter.
pub struct ChildState {
    pub started: bool,
    pub stopping: bool,
    pub project_dir: PathBuf,
    pub subscribers: Vec<mpsc::Sender<HarnessEvent>>,
}

impl ChildState {
    pub fn new(project_dir: PathBuf) -> Self {
        Self {
            started: false,
            stopping: false,
            project_dir,
            subscribers: Vec::new(),
        }
    }
}
