//! Local LLM sidecar via llama.cpp `llama-server` (Phase 9).
//
// Local-chat sidecar support is built but not yet wired into the app state
// (sd-server handles images today; llama-server wiring is the next phase).
// Keep the implementation compiled + unit-tested without failing the
// `-D warnings` dead-code gate until it is connected.
#![allow(dead_code)]

use serde::Deserialize;
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::timeout;
use tracing::{info, warn};

use crate::errors::{ErrorCode, StudioError};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LlamaConfig {
    pub binary_path: Option<PathBuf>,
    pub model_path: Option<PathBuf>,
    pub host: String,
    pub port: u16,
    pub context_size: i32,
    pub gpu_layers: i32,
    pub threads: i32,
    pub use_jinja: bool,
    pub extra_args: Vec<String>,
}

impl LlamaConfig {
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

#[derive(Clone, Debug, Default)]
pub struct LlamaServer {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug, Default)]
struct Inner {
    config: LlamaConfig,
    child: Option<Child>,
    supports_tools: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PropsResponse {
    #[serde(default)]
    chat_template_caps: ChatTemplateCaps,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ChatTemplateCaps {
    #[serde(default)]
    supports_tools: bool,
    #[serde(default)]
    supports_tool_calls: bool,
}

impl LlamaServer {
    #[must_use]
    pub fn new(config: LlamaConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                config,
                child: None,
                supports_tools: None,
            })),
        }
    }

    pub async fn ensure_running(&self, config: &LlamaConfig) -> Result<(), StudioError> {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.child.is_some() && inner.config != *config {
                info!("llama-server config changed; restarting");
                let _ = Self::stop_locked(&mut inner);
            }
            inner.config = config.clone();
        }
        if self.is_healthy().await {
            return Ok(());
        }
        self.start(config).await
    }

    pub async fn start(&self, config: &LlamaConfig) -> Result<(), StudioError> {
        let binary = config.binary_path.as_ref().ok_or_else(|| {
            StudioError::retryable(
                ErrorCode::SidecarCrash,
                "llama-server binary path not set".into(),
            )
        })?;
        let model = config.model_path.as_ref().ok_or_else(|| {
            StudioError::retryable(
                ErrorCode::SidecarCrash,
                "llama.cpp model path not set".into(),
            )
        })?;
        if !binary.exists() {
            return Err(StudioError::retryable(
                ErrorCode::SidecarCrash,
                format!("llama-server binary not found: {}", binary.display()),
            ));
        }
        if !model.exists() {
            return Err(StudioError::retryable(
                ErrorCode::SidecarCrash,
                format!("llama.cpp model not found: {}", model.display()),
            ));
        }

        let mut args = vec![
            "--model".to_string(),
            model.to_string_lossy().to_string(),
            "--host".to_string(),
            config.host.clone(),
            "--port".to_string(),
            config.port.to_string(),
            "--ctx-size".to_string(),
            config.context_size.to_string(),
            "--n-gpu-layers".to_string(),
            config.gpu_layers.to_string(),
            "--threads".to_string(),
            config.threads.to_string(),
            "--metrics".to_string(),
            "--props".to_string(),
            "--no-webui".to_string(),
        ];
        if config.use_jinja {
            args.push("--jinja".to_string());
        }
        for extra in &config.extra_args {
            args.push(extra.clone());
        }

        let child = Command::new(binary)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                StudioError::retryable(
                    ErrorCode::SidecarCrash,
                    format!("failed to spawn llama-server: {e}"),
                )
            })?;

        {
            let mut inner = self.inner.lock().unwrap();
            inner.child = Some(child);
        }

        let health_url = format!("{}/health", config.base_url());
        match timeout(Duration::from_secs(60), Self::wait_health(&health_url)).await {
            Ok(Ok(())) => {
                info!("llama-server healthy at {}", config.base_url());
                self.probe_tool_support(config).await;
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                let _ = self.stop();
                Err(StudioError::retryable(
                    ErrorCode::SidecarCrash,
                    "llama-server did not become healthy within 60s".into(),
                ))
            }
        }
    }

    pub fn stop(&self) -> Result<(), StudioError> {
        let mut inner = self.inner.lock().unwrap();
        Self::stop_locked(&mut inner)
    }

    fn stop_locked(inner: &mut Inner) -> Result<(), StudioError> {
        if let Some(mut child) = inner.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        inner.supports_tools = None;
        Ok(())
    }

    #[must_use]
    pub async fn is_healthy(&self) -> bool {
        let url = {
            let inner = self.inner.lock().unwrap();
            if inner.child.is_none() {
                return false;
            }
            format!("{}/health", inner.config.base_url())
        };
        Self::ping(&url).await
    }

    #[must_use]
    pub fn supports_tools(&self) -> Option<bool> {
        let inner = self.inner.lock().unwrap();
        inner.supports_tools
    }

    async fn wait_health(url: &str) -> Result<(), StudioError> {
        for _ in 0..600 {
            if Self::ping(url).await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(StudioError::retryable(
            ErrorCode::SidecarCrash,
            "llama-server health check timed out".into(),
        ))
    }

    async fn ping(url: &str) -> bool {
        reqwest::get(url)
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn probe_tool_support(&self, config: &LlamaConfig) {
        let url = format!("{}/props", config.base_url());
        let supports = match reqwest::get(&url).await {
            Ok(resp) => match resp.json::<PropsResponse>().await {
                Ok(props) => Some(
                    props.chat_template_caps.supports_tools
                        || props.chat_template_caps.supports_tool_calls,
                ),
                Err(_) => None,
            },
            Err(_) => None,
        };
        {
            let mut inner = self.inner.lock().unwrap();
            inner.supports_tools = supports;
        }
        match supports {
            Some(false) => warn!(
                "llama-server loaded model does not report tool-call support; local agent may fail"
            ),
            Some(true) => info!("llama-server model reports tool-call support"),
            None => warn!("could not determine llama-server tool-call support from /props"),
        }
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llama_config_base_url() {
        let cfg = LlamaConfig {
            host: "127.0.0.1".into(),
            port: 8080,
            ..Default::default()
        };
        assert_eq!(cfg.base_url(), "http://127.0.0.1:8080");
    }

    #[test]
    fn props_response_parses() {
        let json = serde_json::json!({"chat_template_caps": {"supports_tools": true, "supports_tool_calls": true}});
        let props: PropsResponse = serde_json::from_value(json).unwrap();
        assert!(props.chat_template_caps.supports_tools);
        assert!(props.chat_template_caps.supports_tool_calls);
    }
}
