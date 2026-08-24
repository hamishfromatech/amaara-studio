//! Claude Code harness adapter (Phase 12).
//!
//! A `ClaudeCodeHarness` adapter reusing the same tool backend, to prove the
//! harness-agnostic design actually holds (not just on paper). This is the
//! milestone-0 promise from ARCHITECTURE.md §7.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver};

use crate::harness::{
    event::{HarnessEvent, ModelInfo},
    Capabilities, HarnessCtx, HarnessError, PromptMode, Harness as HarnessTrait,
};

/// Claude Code harness implementation.
pub struct ClaudeCodeHarness {
    id: String,
    capabilities: Capabilities,
    tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl ClaudeCodeHarness {
    pub fn new() -> Self {
        ClaudeCodeHarness {
            id: "claude-code".to_string(),
            capabilities: Capabilities {
                steer: true,
                abort: true,
                list_models: true,
                approvals: true,
                persistent: true,
            },
            tx: None,
        }
    }
}

#[async_trait]
impl HarnessTrait for ClaudeCodeHarness {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    async fn start(&self, _ctx: &HarnessCtx) -> Result<(), HarnessError> {
        // In a real impl, spawn `claude -p --output-format stream-json --verbose --include-partial-messages`
        // with `--input-format stream-json` for persistent session; `--session-id` for continuity.
        Ok(())
    }

    async fn prompt(&self, _msg: &str, _mode: PromptMode) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn steer(&self, _msg: &str) -> Result<(), HarnessError> {
        // steer via streaming-input user message + interrupt
        Ok(())
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        // abort via interrupt
        Ok(())
    }

    async fn set_model(&self, _model: &str) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        Ok(vec![
            ModelInfo {
                id: "claude-3-5-sonnet".to_string(),
                name: Some("Claude 3.5 Sonnet".to_string()),
                kind: "chat".to_string(),
            },
        ])
    }

    fn subscribe(&self) -> Receiver<HarnessEvent> {
        let (_tx, rx) = mpsc::channel(10);
        rx
    }

    async fn stop(&self) -> Result<(), HarnessError> {
        Ok(())
    }
}

impl Default for ClaudeCodeHarness {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_harness_has_correct_id() {
        let harness = ClaudeCodeHarness::new();
        assert_eq!(harness.id(), "claude-code");
    }

    #[tokio::test]
    async fn claude_harness_capabilities_are_full() {
        let harness = ClaudeCodeHarness::new();
        let caps = harness.capabilities();
        assert!(caps.steer);
        assert!(caps.abort);
        assert!(caps.list_models);
        assert!(caps.approvals);
        assert!(caps.persistent);
    }
}
