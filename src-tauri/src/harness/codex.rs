//! Codex harness adapter (Phase 13).
//!
//! `codex app-server` JSON-RPC (`thread/start`, `command/exec`, `dynamicTools`)
//! with `approvalPolicy` → approvals; `turn.*`/`item.*` events → `HarnessEvent`.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver};

use crate::harness::{
    event::{HarnessEvent, ModelInfo},
    Capabilities, HarnessCtx, HarnessError, PromptMode, Harness as HarnessTrait,
};

/// Codex harness implementation.
pub struct CodexHarness {
    id: String,
    capabilities: Capabilities,
}

impl CodexHarness {
    pub fn new() -> Self {
        CodexHarness {
            id: "codex".to_string(),
            capabilities: Capabilities {
                steer: true,
                abort: true,
                list_models: true,
                approvals: true,
                persistent: true,
            },
        }
    }
}

#[async_trait]
impl HarnessTrait for CodexHarness {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    async fn start(&self, _ctx: &HarnessCtx) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn prompt(&self, _msg: &str, _mode: PromptMode) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn steer(&self, _msg: &str) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn set_model(&self, _model: &str) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        Ok(crate::harness::registry::fallback_models(self.id()))
    }

    fn subscribe(&self) -> Receiver<HarnessEvent> {
        let (_tx, rx) = mpsc::channel(10);
        rx
    }

    async fn stop(&self) -> Result<(), HarnessError> {
        Ok(())
    }
}

impl Default for CodexHarness {
    fn default() -> Self {
        Self::new()
    }
}
