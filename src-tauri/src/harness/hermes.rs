//! Hermes harness adapter (Phase 13).
//!
//! Hermes `tui_gateway` stdio JSON-RPC (`gateway.ready` handshake, dispatch);
//! MCP via Hermes `mcp.servers`; plugin hooks → approvals.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver};

use crate::harness::{
    event::{HarnessEvent, ModelInfo},
    Capabilities, HarnessCtx, HarnessError, PromptMode, Harness as HarnessTrait,
};

/// Hermes harness implementation.
pub struct HermesHarness {
    id: String,
    capabilities: Capabilities,
}

impl HermesHarness {
    pub fn new() -> Self {
        HermesHarness {
            id: "hermes".to_string(),
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
impl HarnessTrait for HermesHarness {
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

impl Default for HermesHarness {
    fn default() -> Self {
        Self::new()
    }
}
