//! Antigravity harness adapter (Phase 13).
//!
//! `agy -p --output-format stream-json` per turn (one process per turn; cache cwd + session id across turns);
//! `Capabilities.steer = false` (degrades to abort + re-prompt); MCP via `~/.claude/mcp-servers.json`.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver};

use crate::harness::{
    event::{HarnessEvent, ModelInfo},
    Capabilities, HarnessCtx, HarnessError, PromptMode, Harness as HarnessTrait,
};

/// Antigravity harness implementation.
pub struct AntigravityHarness {
    id: String,
    capabilities: Capabilities,
}

impl AntigravityHarness {
    pub fn new() -> Self {
        AntigravityHarness {
            id: "antigravity".to_string(),
            // Antigravity print mode has no mid-stream steer
            capabilities: Capabilities {
                steer: false,
                abort: true,
                list_models: true,
                approvals: true,
                persistent: false,
            },
        }
    }
}

#[async_trait]
impl HarnessTrait for AntigravityHarness {
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
        // Antigravity has no mid-stream steer; degrade to abort + re-prompt
        Err(HarnessError::NoSteer)
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn set_model(&self, _model: &str) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        Ok(vec![])
    }

    fn subscribe(&self) -> Receiver<HarnessEvent> {
        let (_tx, rx) = mpsc::channel(10);
        rx
    }

    async fn stop(&self) -> Result<(), HarnessError> {
        Ok(())
    }
}

impl Default for AntigravityHarness {
    fn default() -> Self {
        Self::new()
    }
}
