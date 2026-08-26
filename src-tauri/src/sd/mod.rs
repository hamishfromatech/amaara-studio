//! SD.cpp sidecar + local image gen (Phase 8).
//!
//! Local image generation through stable-diffusion.cpp `sd-server`, started lazily,
//! with the Cloud/Local Source toggle from design.md routed to set_generation_source.
//!
//! **Scaffold** — wired into AppState and the control server in Phase 9.
#![allow(dead_code)]

use std::sync::Arc;
use tokio::sync::Mutex;

/// GPU build flavor selected at BUNDLE time (not runtime).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdGpuBackend {
    Cuda,
    Vulkan,
    Cpu,
}

impl Default for SdGpuBackend {
    fn default() -> Self {
        SdGpuBackend::Cuda // Windows-first: CUDA is the common NVIDIA path
    }
}

/// sd-server lifecycle state.
#[derive(Debug, Clone)]
pub struct SdServerState {
    pub status: SdStatus,
    pub url: String,
    pub gpu_backend: SdGpuBackend,
    pub models_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdStatus {
    Stopped,
    Starting,
    Running(String), // URL
    Error(String),
}

impl Default for SdServerState {
    fn default() -> Self {
        SdServerState {
            status: SdStatus::Stopped,
            url: "http://127.0.0.1:7860".to_string(),
            gpu_backend: SdGpuBackend::default(),
            models_dir: "models".to_string(),
        }
    }
}

/// Lazy sd-server supervisor (Phase 8 scaffold).
/// Real implementation would spawn `sd-server --diffusion-model … --vae … --llm …`
/// with GPU backend flags (-DSD_CUDA/-GGML_VULKAN=ON) chosen at bundle time.
#[derive(Default)]
pub struct SdServerSupervisor {
    state: Arc<Mutex<SdServerState>>,
}

impl SdServerSupervisor {
    pub fn new() -> Self {
        SdServerSupervisor {
            state: Arc::new(Mutex::new(SdServerState::default())),
        }
    }

    /// Start the sd-server lazily on first local image request.
    pub async fn start(&self) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if matches!(state.status, SdStatus::Running(_) | SdStatus::Starting) {
            return Ok(());
        }

        // Honest binary detection: sd-server must be on PATH.
        if crate::sidecar::which_path("sd-server").is_none() {
            let msg = "sd-server not on PATH. Install stable-diffusion.cpp to enable local image generation.".to_string();
            state.status = SdStatus::Error(msg.clone());
            return Err(msg);
        }

        state.status = SdStatus::Starting;
        // TODO: real spawn once model paths are configurable.
        state.status = SdStatus::Running(state.url.clone());
        Ok(())
    }

    /// Stop the sd-server.
    pub async fn stop(&self) {
        let mut state = self.state.lock().await;
        state.status = SdStatus::Stopped;
    }

    /// Get current status.
    pub async fn status(&self) -> SdStatus {
        let state = self.state.lock().await;
        state.status.clone()
    }

    /// Health probe for sd-server.
    pub async fn health_probe(&self, url: &str) -> bool {
        // Mock health check — real impl would GET http://<url>/health or /sdapi/v1/health
        url.starts_with("http://") && url.ends_with(":7860")
    }

    /// Generate image via sd-server txt2img HTTP API.
    pub async fn generate(&self, prompt: &str) -> Result<String, String> {
        let status = self.status().await;
        if !matches!(status, SdStatus::Running(_)) {
            return Err("sd-server not running".to_string());
        }

        // In a real impl, POST to sd-server's txt2img endpoint:
        // POST http://127.0.0.1:7860/sdapi/v1/txt2img with {prompt, negative_prompt, ...}
        // Return saved asset path or base64 image data.

        Ok(format!("assets/img/local-{}.png", hash_prompt(prompt)))
    }
}

fn hash_prompt(s: &str) -> String {
    let mut h: u32 = 0;
    for c in s.chars() {
        h = h.wrapping_mul(31).wrapping_add(c as u32);
    }
    format!("{:08x}", h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sd_status_stopped_by_default() {
        let state = SdServerState::default();
        assert_eq!(state.status, SdStatus::Stopped);
    }

    #[tokio::test]
    async fn sd_server_supervisor_starts() {
        let sup = SdServerSupervisor::new();
        // On machines without sd-server installed, start() reports honest "not on PATH".
        match sup.start().await {
            Ok(()) => {
                let status = sup.status().await;
                assert!(matches!(status, SdStatus::Running(_)));
            }
            Err(msg) => {
                assert!(msg.contains("not on PATH"), "unexpected error: {msg}");
            }
        }
    }

    #[tokio::test]
    async fn sd_server_generate_requires_running() {
        let sup = SdServerSupervisor::new();
        // Not started yet
        let err = sup.generate("test prompt").await.unwrap_err();
        assert_eq!(err, "sd-server not running");

        // If sd-server is available, start and generate
        if sup.start().await.is_ok() {
            let path = sup.generate("black holes").await.unwrap();
            assert!(path.contains("local-"));
        }
    }
}
