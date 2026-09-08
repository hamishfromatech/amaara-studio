//! SD.cpp sidecar + local image gen (Phase 8/14).
//!
//! Local image generation through stable-diffusion.cpp `sd-server`, started
//! lazily on the first local image request:
//!
//! 1. `sidecar::bootstrap::ensure_sd_server_binary` verifies (or downloads)
//!    the binary against its `.sha256` sidecar (staged by
//!    `scripts/build-sidecars.*`).
//! 2. A model is discovered in the configured `sd_models_dir` (`*.gguf`,
//!    `*.safetensors`, `*.ckpt`) — the server loads one pipeline at startup
//!    and cannot run without weights. Missing model → honest, actionable
//!    error instead of a broken server.
//! 3. `sd-server --diffusion-model <model> --listen-ip 127.0.0.1
//!    --listen-port 7860` spawns with its output forwarded into the sidecar
//!    log drawer; readiness polls `/sdcpp/v1/capabilities`.
//! 4. Generation uses the native async API (`POST /sdcpp/v1/img_gen` →
//!    `GET /sdcpp/v1/jobs/{id}`) and returns the PNG as a data URL.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Default sd-server bind port (avoids llama-server's 8080).
pub const DEFAULT_SD_PORT: u16 = 7860;

/// GPU build flavor selected at BUNDLE time (not runtime).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SdGpuBackend {
    Cuda,
    Vulkan,
    #[default]
    Cpu,
}

impl SdGpuBackend {
    /// Flavor token used in release-asset URLs (bootstrap download path).
    pub fn flavor(self) -> &'static str {
        match self {
            SdGpuBackend::Cuda => "cuda",
            SdGpuBackend::Vulkan => "vulkan",
            SdGpuBackend::Cpu => "cpu",
        }
    }
}

/// sd-server lifecycle state.
#[derive(Debug, Clone)]
pub struct SdServerState {
    pub status: SdStatus,
    pub models_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdStatus {
    Stopped,
    Starting,
    Running(String), // base URL
    Error(String),
}

impl Default for SdServerState {
    fn default() -> Self {
        SdServerState {
            status: SdStatus::Stopped,
            models_dir: "models".to_string(),
        }
    }
}

/// Receives a sidecar child's output streams ("stdout" / "error") so the
/// caller can route them into the log drawer without this module touching
/// tauri types.
pub type OutputForwarder = dyn Fn(&str, Box<dyn tokio::io::AsyncRead + Send + Unpin>) + Send + Sync;

/// Lazy sd-server supervisor: spawn on first local image request, kill on stop.
#[derive(Default)]
pub struct SdServerSupervisor {
    state: Arc<Mutex<SdServerState>>,
    child: Arc<Mutex<Option<tokio::process::Child>>>,
}

impl SdServerSupervisor {
    pub fn new() -> Self {
        SdServerSupervisor::default()
    }

    /// Idempotently ensure the server is up; returns the base URL.
    ///
    /// `bin` is the configured `sd_binary_path` (relative paths resolve
    /// against the manifest dir); `models_dir` the configured
    /// `sd_models_dir`. Progress lines go to `on_progress`; when
    /// `attach_output` is given it receives ("stdout"/"error", stream) pairs
    /// to forward (the command layer routes them into the sidecar log
    /// drawer). Deliberately takes no `tauri::AppHandle` — referencing tauri
    /// here keeps its windowing code alive in test binaries, whose comctl32
    /// v6 import then fails to load (no app manifest on test executables).
    pub async fn ensure_running(
        &self,
        bin: &Path,
        models_dir: &Path,
        flavor: SdGpuBackend,
        attach_output: Option<&OutputForwarder>,
        on_progress: &mut (dyn FnMut(&str) + Send),
    ) -> Result<String, String> {
        {
            let state = self.state.lock().await;
            if let SdStatus::Running(url) = &state.status {
                return Ok(url.clone());
            }
        }

        // 1. Binary: verify checksums (or download-on-first-run).
        on_progress("checking sd-server binary");
        let resolved = resolve_binary(bin)
            .ok_or_else(|| "sd-server binary not found. Point Settings → sd_binary_path at the staged binary or set NAVYA_SD_RELEASE_BASE.".to_string())?;
        crate::sidecar::bootstrap::ensure_sd_server_binary(
            &resolved,
            flavor.flavor(),
            on_progress,
        )
        .await
        .map_err(|e| format!("sd-server bootstrap: {e}"))?;

        // 2. Model: the server loads one pipeline at startup — without
        //    weights it has nothing to serve. Fail with guidance.
        let model = find_model(models_dir).ok_or_else(|| {
            format!(
                "no diffusion model found in {} (looked for *.gguf, *.safetensors, *.ckpt). Point Settings → sd_models_dir at a stable-diffusion.cpp model.",
                models_dir.display()
            )
        })?;
        on_progress(&format!("using model {}", model.display()));

        let url = format!("http://127.0.0.1:{DEFAULT_SD_PORT}");
        {
            let mut state = self.state.lock().await;
            state.status = SdStatus::Starting;
            state.models_dir = models_dir.to_string_lossy().to_string();
        }

        // 3. Spawn. stdout/stderr stream into the sidecar log drawer.
        let mut cmd = tokio::process::Command::new(&resolved);
        cmd.arg("--diffusion-model")
            .arg(&model)
            .arg("--listen-ip")
            .arg("127.0.0.1")
            .arg("--listen-port")
            .arg(DEFAULT_SD_PORT.to_string())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        on_progress(&format!("spawning {}", resolved.display()));
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn sd-server: {e}"))?;
        if let Some(fwd) = attach_output {
            if let Some(out) = child.stdout.take() {
                fwd("stdout", Box::new(out));
            }
            if let Some(err) = child.stderr.take() {
                fwd("error", Box::new(err));
            }
        }
        *self.child.lock().await = Some(child);

        // 4. Readiness: a 2xx on /sdcpp/v1/capabilities means the HTTP server
        //    is up; model load time is bounded by the generous timeout.
        on_progress("waiting for sd-server to load the model (this can take a while)");
        if let Err(err) = poll_ready(&url, std::time::Duration::from_secs(180)).await {
            // Spawned but never became ready — kill the child and reset state
            // so the next attempt starts clean (leaving the dead child in
            // self.child leaked the process and stuck the status on Starting).
            if let Some(mut c) = self.child.lock().await.take() {
                let _ = c.kill().await;
            }
            let mut state = self.state.lock().await;
            state.status = SdStatus::Error(err.clone());
            return Err(err);
        }

        {
            let mut state = self.state.lock().await;
            state.status = SdStatus::Running(url.clone());
        }
        on_progress(&format!("sd-server ready at {url}"));
        Ok(url)
    }

    /// Stop the server (kills the child process). Idempotent.
    pub async fn stop(&self) {
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
        }
        let mut state = self.state.lock().await;
        state.status = SdStatus::Stopped;
    }

    /// Current status.
    pub async fn status(&self) -> SdStatus {
        self.state.lock().await.status.clone()
    }

    /// Generate one PNG for `prompt` via the native async API. Returns a
    /// `data:image/png;base64,…` URL usable directly in <img> tags.
    pub async fn generate(&self, prompt: &str) -> Result<String, String> {
        let url = {
            let state = self.state.lock().await;
            match &state.status {
                SdStatus::Running(url) => url.clone(),
                _ => {
                    return Err(
                        "sd-server not running. Pick Source=Local in the TopBar to start it."
                            .to_string(),
                    );
                }
            }
        };
        generate_via_http(&url, prompt).await
    }
}

/// Resolve the sd-server binary: explicit config path (relative → manifest
/// dir) → triple-staged binary next to the manifest → PATH.
fn resolve_binary(configured: &Path) -> Option<PathBuf> {
    if configured.is_absolute() {
        return configured.exists().then(|| configured.to_path_buf());
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let joined = manifest_dir.join(configured);
    if joined.exists() {
        return Some(joined);
    }
    // Bundled externalBin name (sd-server-<triple>[.exe]) staged by
    // scripts/build-sidecars.*.
    let triple = crate::sidecar::bootstrap::host_triple();
    let ext = if cfg!(target_os = "windows") { ".exe" } else { "" };
    let staged = manifest_dir
        .join("binaries")
        .join(format!("sd-server-{triple}{ext}"));
    if staged.exists() {
        return Some(staged);
    }
    crate::harness::registry::which_path("sd-server")
}

/// First diffusion-weights file in `models_dir` (shallow search).
fn find_model(models_dir: &Path) -> Option<PathBuf> {
    const EXTS: [&str; 3] = ["gguf", "safetensors", "ckpt"];
    let entries = std::fs::read_dir(models_dir).ok()?;
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .map(|x| EXTS.contains(&x.to_lowercase().as_str()))
                .unwrap_or(false)
        })
}

/// Poll `/sdcpp/v1/capabilities` until the server answers 2xx.
async fn poll_ready(url: &str, timeout: std::time::Duration) -> Result<(), String> {
    use std::time::Instant;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(resp) = client.get(format!("{url}/sdcpp/v1/capabilities")).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "sd-server did not become ready within {}s at {url}. Check the log drawer for startup errors (missing model? incompatible weights?).",
                timeout.as_secs()
            ));
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// POST /sdcpp/v1/img_gen, then poll GET /sdcpp/v1/jobs/{id} to completion.
/// Returns the first image as a data URL.
async fn generate_via_http(url: &str, prompt: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let body = serde_json::json!({
        "prompt": prompt,
        "negative_prompt": "",
        "width": 512,
        "height": 512,
        "steps": 20,
        "batch_count": 1,
        "output_format": "png",
    });
    let resp = client
        .post(format!("{url}/sdcpp/v1/img_gen"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("img_gen request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("img_gen failed: HTTP {}", resp.status()));
    }
    let job: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("img_gen response: {e}"))?;
    let job_id = job
        .get("job_id")
        .and_then(|v| v.as_str())
        .ok_or("img_gen response missing job_id")?
        .to_string();

    // Poll to completion (CPU generation of a 512px image can take minutes).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    loop {
        let job = client
            .get(format!("{url}/sdcpp/v1/jobs/{job_id}"))
            .send()
            .await
            .map_err(|e| format!("job poll failed: {e}"))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("job poll response: {e}"))?;
        match job.get("status").and_then(|v| v.as_str()) {
            Some("completed") => break,
            Some("failed") | Some("error") => {
                let err = job
                    .get("error")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown error".to_string());
                return Err(format!("sd-server job failed: {err}"));
            }
            _ => {}
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "sd-server job {job_id} timed out after 10 minutes"
            ));
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let b64 = job
        .pointer("/result/images/0/b64_json")
        .and_then(|v| v.as_str())
        .ok_or("completed job has no image data")?;
    Ok(format!("data:image/png;base64,{b64}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sd_status_stopped_by_default() {
        let state = SdServerState::default();
        assert_eq!(state.status, SdStatus::Stopped);
    }

    #[test]
    fn find_model_ignores_non_weight_files() {
        let tmp = std::env::temp_dir().join(format!("navya-sd-model-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("readme.txt"), "not a model").unwrap();
        assert!(find_model(&tmp).is_none());
        std::fs::write(tmp.join("model.safetensors"), b"weights").unwrap();
        let found = find_model(&tmp).expect("safetensors should be found");
        assert_eq!(found.extension().unwrap(), "safetensors");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn ensure_running_fails_honestly_without_model() {
        let sup = SdServerSupervisor::new();
        // Empty models dir: bootstrap may pass (binary staged on dev machines)
        // but model discovery must fail with actionable guidance either way.
        let empty = std::env::temp_dir().join(format!("navya-sd-empty-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        let bin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries").join("sd-server.exe");
        let mut progress: Vec<String> = Vec::new();
        let err = sup
            .ensure_running(&bin, &empty, SdGpuBackend::Cpu, None, &mut |m: &str| {
                progress.push(m.into());
            })
            .await
            .unwrap_err();
        assert!(
            err.contains("model") || err.contains("sd-server"),
            "unexpected error: {err}"
        );
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[tokio::test]
    async fn generate_requires_running() {
        let sup = SdServerSupervisor::new();
        let err = sup.generate("test prompt").await.unwrap_err();
        assert!(err.contains("not running"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn stop_is_idempotent() {
        let sup = SdServerSupervisor::new();
        sup.stop().await;
        assert_eq!(sup.status().await, SdStatus::Stopped);
    }
}