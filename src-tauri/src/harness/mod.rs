//! Harness trait and registry (Phase 4).
//!
//! A pluggable adapter over six real harnesses: a-coder-cli, claude code, hermes,
// openclaw, openai codex, antigravity cli. The trait is the protocol translator —
// each impl spawns/connects the harness, translates normalized commands to the
// native protocol, and emits normalized HarnessEvent streams.

pub(crate) mod event;
pub(crate) mod approvals;
pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod hermes;
pub(crate) mod antigravity;
pub(crate) mod openclaw;

use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver};

use crate::harness::event::{HarnessEvent, ModelInfo};

/// Harness capabilities — what operations this harness supports natively.
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub steer: bool,         // mid-stream steering supported
    pub abort: bool,         // can abort current turn
    pub list_models: bool,   // can list available models
    pub approvals: bool,     // supports approval/permission requests
    pub persistent: bool,    // supports persistent sessions across turns
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            steer: true,
            abort: true,
            list_models: true,
            approvals: true,
            persistent: true,
        }
    }
}

/// The harness protocol trait. All adapters implement this to prove agnosticism.
#[async_trait::async_trait]
pub trait Harness: Send + Sync {
    fn id(&self) -> &str;
    
    /// Return the capabilities of this harness implementation.
    fn capabilities(&self) -> Capabilities;
    
    /// Start the harness with the given context (project dir, settings, etc.).
    async fn start(&self, ctx: &HarnessCtx) -> Result<(), HarnessError>;
    
    /// Send a prompt to the harness in the given mode.
    async fn prompt(&self, msg: &str, mode: PromptMode) -> Result<(), HarnessError>;
    
    /// Steer the harness mid-turn (if supported). Returns Err(NoSteer) if not supported.
    async fn steer(&self, msg: &str) -> Result<(), HarnessError>;
    
    /// Abort the current turn/process.
    async fn abort(&self) -> Result<(), HarnessError>;
    
    /// Set the active model for the harness.
    async fn set_model(&self, model: &str) -> Result<(), HarnessError>;
    
    /// List available models from the harness/provider.
    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError>;
    
    /// Subscribe to harness events (text deltas, tool calls, approvals, etc.).
    fn subscribe(&self) -> Receiver<HarnessEvent>;
    
    /// Stop the harness cleanly.
    async fn stop(&self) -> Result<(), HarnessError>;
}

/// Context passed to harness start/operations.
#[derive(Debug, Clone)]
pub struct HarnessCtx {
    pub project_dir: std::path::PathBuf,
    pub model: String,
    pub source: String, // "cloud" or "local"
}

/// Prompt modes for the harness.
#[derive(Debug, Clone, Copy)]
pub enum PromptMode {
    Normal,
    Steer,
    FollowUp,
}

/// Harness errors.
#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error("harness not started or already stopped")]
    NotStarted,
    #[error("steering is not supported by this harness")]
    NoSteer,
    #[error("harness process error: {0}")]
    Process(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Registry of active harness instances. Kept thread-safe via Arc+Mutex.
#[derive(Default)]
pub struct HarnessRegistry {
    instances: std::collections::HashMap<String, Arc<dyn Harness>>,
}

impl HarnessRegistry {
    pub fn new() -> Self {
        HarnessRegistry {
            instances: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, harness: impl 'static + Send + Sync + Harness) {
        let id = harness.id().to_string();
        self.instances.insert(id, Arc::new(harness));
    }

    pub fn list_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.instances.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Harness>> {
        self.instances.get(id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_default_is_full() {
        let caps = Capabilities::default();
        assert!(caps.steer);
        assert!(caps.abort);
        assert!(caps.list_models);
        assert!(caps.approvals);
        assert!(caps.persistent);
    }

    #[test]
    fn harness_registry_registers_and_lists() {
        // Mock harness impl for testing registry
        struct MockHarness;
        #[async_trait::async_trait]
        impl Harness for MockHarness {
            fn id(&self) -> &str { "mock-harness" }
            fn capabilities(&self) -> Capabilities { Capabilities::default() }
            async fn start(&self, _ctx: &HarnessCtx) -> Result<(), HarnessError> { Ok(()) }
            async fn prompt(&self, _msg: &str, _mode: PromptMode) -> Result<(), HarnessError> { Ok(()) }
            async fn steer(&self, _msg: &str) -> Result<(), HarnessError> { Err(HarnessError::NoSteer) }
            async fn abort(&self) -> Result<(), HarnessError> { Ok(()) }
            async fn set_model(&self, _model: &str) -> Result<(), HarnessError> { Ok(()) }
            async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> { Ok(vec![]) }
            fn subscribe(&self) -> Receiver<HarnessEvent> {
                let (_tx, rx) = mpsc::channel(10);
                rx
            }
            async fn stop(&self) -> Result<(), HarnessError> { Ok(()) }
        }

        let mut reg = HarnessRegistry::new();
        reg.register(MockHarness {});
        assert_eq!(reg.list_ids(), vec!["mock-harness".to_string()]);
        assert!(reg.get("mock-harness").is_some());
    }
}
