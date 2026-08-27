//! OpenClaw harness adapter (Phase 13).
//!
//! OpenClaw is gateway-centric: a WebSocket gateway runs at `ws://127.0.0.1:18789`
//! and exposes JSON-RPC calls such as `tools.invoke` with `confirm: "request"`
//! or `confirm: "report"` approval modes. The adapter connects to that gateway,
//! wires the Navya MCP server indirectly through the gateway's own MCP bridge,
//! and translates normalized Navya commands to JSON-RPC messages.
//!
//! In a dev environment the gateway is assumed to be started separately (or via
//! `openclaw mcp serve` for the MCP bridge). This adapter connects to the
//! gateway URL configured in `harness-pack/openclaw/config.json`.

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{mpsc, mpsc::Receiver};
use tokio_tungstenite::connect_async;

use crate::harness::{
    common::{self, ChildState},
    event::{HarnessEvent, ModelInfo},
    registry,
    Capabilities, HarnessCtx, HarnessError, Harness as HarnessTrait, PromptMode,
};

/// OpenClaw harness implementation.
pub struct OpenClawHarness {
    id: String,
    capabilities: Capabilities,
    inner: Arc<std::sync::Mutex<Inner>>,
    /// Gateway URL from the descriptor registry (or env override).
    gateway_url: String,
}

struct Inner {
    state: ChildState,
    /// Outbound message sender to the WS task.
    tx: Option<mpsc::Sender<serde_json::Value>>,
    /// Pending tool-invocation approvals.
    pending_approvals: HashMap<String, serde_json::Value>,
    /// Request id counter.
    rpc_id: u64,
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
            inner: Arc::new(std::sync::Mutex::new(Inner {
                state: ChildState::new(PathBuf::new()),
                tx: None,
                pending_approvals: HashMap::new(),
                rpc_id: 0,
            })),
            gateway_url: std::env::var("OPENCLAW_GATEWAY_URL")
                .unwrap_or_else(|_| "ws://127.0.0.1:18789".to_string()),
        }
    }

    /// Send a JSON-RPC request to the gateway.
    fn send_rpc(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), HarnessError> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.state.started {
            return Err(HarnessError::NotStarted);
        }
        inner.rpc_id += 1;
        let id = inner.rpc_id;
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if let Some(tx) = inner.tx.as_ref() {
            let _ = tx.try_send(msg);
            Ok(())
        } else {
            Err(HarnessError::Process("OpenClaw gateway sender not ready".into()))
        }
    }
}

impl Default for OpenClawHarness {
    fn default() -> Self {
        Self::new()
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

    async fn start(&self, ctx: &HarnessCtx) -> Result<(), HarnessError> {
        let need_restart = {
            let inner = self.inner.lock().unwrap();
            inner.state.started && inner.state.project_dir != ctx.project_dir
        };
        if need_restart {
            self.stop().await?;
        }

        // OpenClaw gateway is assumed to be running. Connect via WS.
        let url = self.gateway_url.clone();
        let (ws_stream, _) = connect_async(&url)
            .await
            .map_err(|e| HarnessError::Process(format!("OpenClaw gateway connect failed: {e}")))?;
        let (mut write, mut read) = ws_stream.split();
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(256);

        {
            let mut inner = self.inner.lock().unwrap();
            inner.state.started = true;
            inner.state.stopping = false;
            inner.state.project_dir = ctx.project_dir.clone();
            inner.tx = Some(tx);
        }

        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            // Outbound writer task.
            let writer = tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    if let Ok(text) = serde_json::to_string(&msg) {
                        if write.send(tokio_tungstenite::tungstenite::Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                }
            });

            // Inbound reader task.
            while let Some(Ok(msg)) = read.next().await {
                let text = match msg {
                    tokio_tungstenite::tungstenite::Message::Text(t) => t.to_string(),
                    tokio_tungstenite::tungstenite::Message::Binary(b) => String::from_utf8_lossy(&b).to_string(),
                    _ => continue,
                };
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    handle_message(&inner, &json);
                }
            }

            let _ = writer.await;
            let notify = {
                let mut g = inner.lock().unwrap();
                let was_started = g.state.started;
                g.state.started = false;
                g.tx = None;
                was_started && !g.state.stopping
            };
            if notify {
                let mut g = inner.lock().unwrap();
                common::broadcast(
                    &mut g.state.subscribers, HarnessEvent::Error("OpenClaw gateway disconnected".into()));
            }
        });

        Ok(())
    }

    async fn prompt(&self, msg: &str, _mode: PromptMode) -> Result<(), HarnessError> {
        self.send_rpc(
            "tools.invoke",
            serde_json::json!({
                "tool": "prompt",
                "args": { "text": msg },
                "confirm": "report"
            }),
        )
    }

    async fn steer(&self, msg: &str) -> Result<(), HarnessError> {
        self.send_rpc(
            "tools.invoke",
            serde_json::json!({
                "tool": "steer",
                "args": { "text": msg },
                "confirm": "report"
            }),
        )
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        self.send_rpc(
            "session.cancel",
            serde_json::json!({}),
        )
    }

    async fn set_model(&self, _model: &str) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        Ok(registry::fallback_models(self.id()))
    }

    async fn answer_approval(
        &self,
        request_id: &str,
        approved: bool,
    ) -> Result<(), HarnessError> {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.pending_approvals.remove(request_id);
        }
        self.send_rpc(
            "tools.respond",
            serde_json::json!({
                "id": request_id,
                "approved": approved
            }),
        )
    }

    fn subscribe(&self) -> Receiver<HarnessEvent> {
        let (tx, rx) = mpsc::channel(256);
        let mut inner = self.inner.lock().unwrap();
        inner.state.subscribers.retain(|s| !s.is_closed());
        inner.state.subscribers.push(tx);
        rx
    }

    async fn stop(&self) -> Result<(), HarnessError> {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.state.stopping = true;
            inner.state.started = false;
            inner.tx = None;
        }
        Ok(())
    }
}

fn handle_message(inner: &Arc<std::sync::Mutex<Inner>>, json: &serde_json::Value) {
    if let Some(method) = json.get("method").and_then(|v| v.as_str()) {
        let params = json.get("params").cloned().unwrap_or(serde_json::Value::Null);
        match method {
            "agent.start" => {
                let mut g = inner.lock().unwrap();
                common::broadcast(
                    &mut g.state.subscribers, HarnessEvent::AgentStart { model: "default".to_string() });
            }
            "agent.text_delta" => {
                if let Some(delta) = params.get("delta").and_then(|v| v.as_str()) {
                    let mut g = inner.lock().unwrap();
                    common::broadcast(&mut g.state.subscribers, HarnessEvent::TextDelta(delta.to_string()));
                }
            }
            "agent.thinking_delta" => {
                if let Some(delta) = params.get("delta").and_then(|v| v.as_str()) {
                    let mut g = inner.lock().unwrap();
                    common::broadcast(&mut g.state.subscribers, HarnessEvent::ThinkingDelta(delta.to_string()));
                }
            }
            "agent.tool_call" => {
                if let Some(tool) = params.get("tool_call") {
                    let id = tool.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let args = tool.get("arguments").cloned().unwrap_or(serde_json::Value::Null);
                    let mut g = inner.lock().unwrap();
                    common::broadcast(
                        &mut g.state.subscribers, HarnessEvent::ToolStart { tool_id: id, name, args });
                }
            }
            "agent.tool_result" => {
                if let Some(result) = params.get("tool_result") {
                    let id = result.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let content = result.get("content").and_then(content_text);
                    let is_error = result.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                    let mut g = inner.lock().unwrap();
                    common::broadcast(
                        &mut g.state.subscribers, HarnessEvent::ToolEnd { tool_id: id, result: content, is_error });
                }
            }
            "agent.end" => {
                let mut g = inner.lock().unwrap();
                common::broadcast(
                    &mut g.state.subscribers, HarnessEvent::AgentEnd { success: true, message: None });
            }
            "agent.error" => {
                if let Some(err) = params.get("error").and_then(|v| v.as_str()) {
                    let mut g = inner.lock().unwrap();
                    common::broadcast(&mut g.state.subscribers, HarnessEvent::Error(err.to_string()));
                }
            }
            "approval.request" => {
                if let Some(req) = params.get("request") {
                    let id = req.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let kind = req.get("kind").and_then(|v| v.as_str()).unwrap_or("confirm").to_string();
                    {
                        let mut g = inner.lock().unwrap();
                        g.pending_approvals.insert(id.clone(), req.clone());
                    }
                    let mut g = inner.lock().unwrap();
                    common::broadcast(
                        &mut g.state.subscribers, HarnessEvent::ApprovalRequest { id, kind, payload: req.clone() });
                }
            }
            _ => {}
        }
    }
}

fn content_text(v: &serde_json::Value) -> Option<String> {
    if let Some(text) = v.as_str() {
        return Some(text.to_string());
    }
    if let Some(items) = v.as_array() {
        let mut out = String::new();
        for item in items {
            if let Some(t) = item.as_str() {
                out.push_str(t);
            }
        }
        return if out.is_empty() { None } else { Some(out) };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openclaw_harness_has_correct_id() {
        let harness = OpenClawHarness::new();
        assert_eq!(harness.id(), "openclaw");
    }

    #[tokio::test]
    async fn openclaw_harness_capabilities_are_full() {
        let harness = OpenClawHarness::new();
        let caps = harness.capabilities();
        assert!(caps.steer);
        assert!(caps.abort);
        assert!(caps.list_models);
        assert!(caps.approvals);
        assert!(caps.persistent);
    }
}
