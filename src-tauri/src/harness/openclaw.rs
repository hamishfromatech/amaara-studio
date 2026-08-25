//! OpenClaw harness adapter (Phase 13).
//!
//! WebSocket gateway client (`ws://127.0.0.1:18789` via `openclaw` SDK or raw WS JSON-RPC),
//! `tools.invoke` with `confirm: request` → approvals.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver};

use crate::harness::{
    event::{HarnessEvent, ModelInfo},
    Capabilities, HarnessCtx, HarnessError, PromptMode, Harness as HarnessTrait,
};

/// OpenClaw harness implementation.
pub struct OpenClawHarness {
    id: String,
    capabilities: Capabilities,
}

impl OpenClawHarness {
    pub fn new() -> Self {
        OpenClawHarness {
            id: "openclaw".to_string(),
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
impl HarnessTrait for OpenClawHarness {
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

impl Default for OpenClawHarness {
    fn default() -> Self {
        Self::new()
    }
}
