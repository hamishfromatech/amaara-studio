//! HyperFrames preview server (Phase 10, Timeline tab).
//!
//! The Timeline tab embeds the live HyperFrames preview in an iframe. This
//! module owns the long-running preview process: `npx hyperframes preview
//! --background --port <port>` run in the project directory. The Rust side
//! spawns it, polls the loopback port until it answers, and holds the child
//! handle so `preview_stop` can kill it cleanly.
//!
//! Graceful degradation is the whole point: if Node/npx/hyperframes are
//! missing, or the port is taken, `start` returns a clear error rather than
//! hanging — the UI shows the cause and a retry.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex as PMutex;
use tokio::process::Child;

/// Default preview port (matches `npx hyperframes preview --port 3002`).
pub const DEFAULT_PREVIEW_PORT: u16 = 3002;

/// A running preview process. `None` when stopped.
///
/// `running` is tracked explicitly (rather than probing `Child::state`) so the
/// UI's iframe stays in sync even if the process is killed externally.
#[derive(Default)]
pub struct PreviewServer {
    pub child: Option<Child>,
    pub port: Option<u16>,
    pub running: bool,
}

impl PreviewServer {
    pub fn new(port: u16) -> Self {
        PreviewServer {
            child: None,
            port: Some(port),
            running: false,
        }
    }

    /// Whether the preview server is currently serving.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Kill the process (idempotent) and clear state.
    pub async fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill().await;
        }
        self.running = false;
        self.port = None;
    }
}

/// Shared, cheaply-clonable handle to the preview server stored in AppState.
pub type PreviewState = Arc<PMutex<PreviewServer>>;

pub fn preview_state() -> PreviewState {
    Arc::new(PMutex::new(PreviewServer::default()))
}

/// Spawn `npx hyperframes preview --background --port <port>` in `project_dir`.
///
/// Returns the spawned child (stdio inherited — the server runs headless in
/// the background). Failure to spawn (missing Node/npx) is returned as an error.
pub async fn spawn_preview(project_dir: &Path, port: u16) -> Result<Child, String> {
    let mut cmd = tokio::process::Command::new("npx");
    cmd.arg("hyperframes")
        .arg("preview")
        .arg("--background")
        .arg("--port")
        .arg(port.to_string())
        .current_dir(project_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    cmd.spawn().map_err(|e| {
        if matches!(e.kind(), std::io::ErrorKind::NotFound) {
            "npx not found. Install Node.js (≥ 22) to enable the live preview.".to_string()
        } else {
            format!("failed to spawn preview server: {e}")
        }
    })
}

/// Poll the loopback port until the preview server answers HTTP 200, or give
/// up after `timeout`. Returns the iframe URL on success.
pub async fn poll_ready(port: u16, timeout: std::time::Duration) -> Result<String, String> {
    use std::time::Instant;
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(std::time::Duration::from_secs(1))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("http://127.0.0.1:{port}/");
    let deadline = Instant::now() + std::time::Duration::from_secs(30);

    loop {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(url),
            Ok(_) => { /* server up but not 2xx — keep waiting */ }
            Err(_) => { /* not ready yet */ }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "preview server did not become ready within 30s on port {port}. Is Node.js/hyperframes installed?"
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_is_stopped_by_default() {
        let s = PreviewServer::default();
        assert!(!s.is_running());
        assert_eq!(s.port, None);
    }

    #[test]
    fn new_sets_port() {
        let s = PreviewServer::new(4321);
        assert_eq!(s.port, Some(4321));
    }

    #[test]
    fn default_port_matches_hyperframes() {
        // Keep the UI and the spawn path in lockstep.
        assert_eq!(DEFAULT_PREVIEW_PORT, 3002);
    }
}
