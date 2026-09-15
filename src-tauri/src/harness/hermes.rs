//! Hermes Agent harness adapter (Phase 13).
//!
//! Drives Hermes `tui_gateway/entry.py` over stdio JSON-RPC. The gateway emits
//! `gateway.ready` first, then accepts dispatches. Hermes has native MCP support
//! (`ctx.call_mcp`) so the adapter only needs to wire the Amaara MCP server into
//! its config and translate normalized Amaara commands to JSON-RPC dispatches.
//!
//! The adapter writes `hermes-config.yaml` (or `hermes-config.json`) into the
//! project directory pointing at the Amaara MCP server.

use async_trait::async_trait;
use std::{collections::HashMap, io::BufRead, path::PathBuf, sync::Arc};
use tokio::sync::{mpsc, mpsc::Receiver};

use crate::harness::{
    common::{self, ChildState},
    event::{HarnessEvent, ModelInfo},
    registry, Capabilities, Harness as HarnessTrait, HarnessCtx, HarnessError, PromptMode,
};

/// Hermes harness implementation.
pub struct HermesHarness {
    id: String,
    capabilities: Capabilities,
    inner: Arc<std::sync::Mutex<Inner>>,
}

struct Inner {
    state: ChildState,
    stdin: std::sync::Mutex<Option<std::process::ChildStdin>>,
    child_handle: std::sync::Mutex<Option<std::process::Child>>,
    rpc_id: u64,
    /// Pending gateway approvals.
    pending_approvals: HashMap<String, serde_json::Value>,
    ready: bool,
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
            inner: Arc::new(std::sync::Mutex::new(Inner {
                state: ChildState::new(PathBuf::new()),
                stdin: std::sync::Mutex::new(None),
                child_handle: std::sync::Mutex::new(None),
                rpc_id: 0,
                pending_approvals: HashMap::new(),
                ready: false,
            })),
        }
    }

    /// Send a JSON-RPC dispatch to the gateway stdin.
    fn send_rpc(&self, method: &str, params: serde_json::Value) -> Result<u64, HarnessError> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.state.started || !inner.ready {
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
        let mut stdin = inner.stdin.lock().unwrap();
        let child_stdin = stdin.as_mut().ok_or(HarnessError::NotStarted)?;
        common::send_json_line(child_stdin, &msg)?;
        Ok(id)
    }

    /// Write Hermes MCP config into the project directory.
    fn write_project_config(
        &self,
        project_dir: &std::path::Path,
        control_url: Option<&str>,
        control_token: Option<&str>,
    ) -> Result<(), HarnessError> {
        let mcp_dir = common::mcp_workspace_dir();
        let server_py = mcp_dir.join("amaara_mcp").join("server.py");
        let server_py_abs = server_py.to_string_lossy().to_string();

        let config = serde_json::json!({
            "mcp": {
                "servers": [
                    {
                        "name": "amaara-studio-tools",
                        "command": "uv",
                        "args": [
                            "run",
                            "--with", "fastmcp",
                            "--with", "httpx",
                            "fastmcp",
                            "run",
                            server_py_abs
                        ],
                        "env": {
                            "AMAARA_CONTROL_URL": control_url.unwrap_or("http://127.0.0.1:8080"),
                            "AMAARA_CONTROL_TOKEN": control_token.unwrap_or("")
                        }
                    }
                ]
            }
        });

        let path = project_dir.join("hermes-config.yaml");
        // Hermes accepts YAML or JSON; write JSON for simplicity (valid YAML 1.2).
        common::write_config(
            &path,
            &serde_json::to_string_pretty(&config).unwrap_or_default(),
        )
    }
}

impl Default for HermesHarness {
    fn default() -> Self {
        Self::new()
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

    async fn start(&self, ctx: &HarnessCtx) -> Result<(), HarnessError> {
        let need_restart = {
            let inner = self.inner.lock().unwrap();
            inner.state.started && inner.state.project_dir != ctx.project_dir
        };
        if need_restart {
            self.stop().await?;
        }

        // Already running in this project — nothing to do (idempotent start;
        // send_prompt calls start on every turn).
        {
            let inner = self.inner.lock().unwrap();
            if inner.state.started && inner.state.project_dir == ctx.project_dir {
                return Ok(());
            }
        }

        let Some(binary) =
            registry::descriptor_for(&self.id).and_then(|d| registry::detect(d).path)
        else {
            return Err(HarnessError::Process(
                "hermes binary not found on PATH".into(),
            ));
        };

        self.write_project_config(
            &ctx.project_dir,
            ctx.control_url.as_deref(),
            ctx.control_token.as_deref(),
        )?;

        let args = vec![
            "--config".to_string(),
            ctx.project_dir
                .join("hermes-config.yaml")
                .to_string_lossy()
                .to_string(),
        ];

        let (mut child, stdin) = common::spawn_command(
            &binary,
            &args,
            &ctx.project_dir,
            ctx.control_url.as_deref(),
            ctx.control_token.as_deref(),
        )?;

        let stdout = child.stdout.take().expect("stdout pipe should be open");

        {
            let mut inner = self.inner.lock().unwrap();
            inner.state.started = true;
            inner.state.stopping = false;
            inner.state.project_dir = ctx.project_dir.clone();
            *inner.stdin.lock().unwrap() = Some(stdin);
            inner.child_handle.lock().unwrap().replace(child);
        }

        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            let mut reader = std::io::BufReader::new(stdout);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => {
                        let line = buf.trim_end_matches(['\n', '\r']);
                        if line.is_empty() {
                            continue;
                        }
                        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
                            continue;
                        };
                        handle_stdout_line(&inner, &json);
                    }
                    Err(_) => break,
                }
            }
            let notify = {
                let mut g = inner.lock().unwrap();
                let was_started = g.state.started;
                g.state.started = false;
                g.ready = false;
                was_started && !g.state.stopping
            };
            if notify {
                let mut g = inner.lock().unwrap();
                common::broadcast(
                    &mut g.state.subscribers,
                    HarnessEvent::Error("Hermes gateway exited".into()),
                );
            }
        });

        Ok(())
    }

    async fn prompt(&self, msg: &str, _mode: PromptMode) -> Result<(), HarnessError> {
        self.send_rpc(
            "gateway.dispatch",
            serde_json::json!({"type": "user_message", "text": msg}),
        )?;
        Ok(())
    }

    async fn steer(&self, msg: &str) -> Result<(), HarnessError> {
        // Hermes steering: send an interrupt then a corrected user message.
        self.send_rpc("gateway.dispatch", serde_json::json!({"type": "interrupt"}))?;
        self.send_rpc(
            "gateway.dispatch",
            serde_json::json!({"type": "user_message", "text": msg}),
        )?;
        Ok(())
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        self.send_rpc("gateway.dispatch", serde_json::json!({"type": "abort"}))?;
        Ok(())
    }

    async fn set_model(&self, _model: &str) -> Result<(), HarnessError> {
        // Hermes model selection is typically plugin/provider config.
        Ok(())
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        Ok(registry::fallback_models(self.id()))
    }

    async fn answer_approval(
        &self,
        request_id: &str,
        approved: bool,
        _value: Option<String>,
    ) -> Result<(), HarnessError> {
        // Hermes approval RPC is boolean-only; an edited value is not
        // representable, so it is ignored (the user can re-run after edit).
        {
            let mut inner = self.inner.lock().unwrap();
            inner.pending_approvals.remove(request_id);
        }
        self.send_rpc(
            "gateway.dispatch",
            serde_json::json!({
                "type": "approval_response",
                "approval_response": { "id": request_id, "approved": approved }
            }),
        )?;
        Ok(())
    }

    fn subscribe(&self) -> Receiver<HarnessEvent> {
        let (tx, rx) = mpsc::channel(256);
        let mut inner = self.inner.lock().unwrap();
        inner.state.subscribers.retain(|s| !s.is_closed());
        inner.state.subscribers.push(tx);
        rx
    }

    async fn stop(&self) -> Result<(), HarnessError> {
        let child = {
            let mut inner = self.inner.lock().unwrap();
            inner.state.stopping = true;
            inner.state.started = false;
            inner.ready = false;
            *inner.stdin.lock().unwrap() = None;
            let mut handle = inner.child_handle.lock().unwrap();
            handle.take()
        };
        if let Some(child) = child {
            common::kill_process_tree(child.id());
        }
        Ok(())
    }
}

fn handle_stdout_line(inner: &Arc<std::sync::Mutex<Inner>>, json: &serde_json::Value) {
    if let Some(method) = json.get("method").and_then(|v| v.as_str()) {
        match method {
            "gateway.ready" => {
                let mut g = inner.lock().unwrap();
                g.ready = true;
            }
            "agent.start" => {
                let mut g = inner.lock().unwrap();
                common::broadcast(
                    &mut g.state.subscribers,
                    HarnessEvent::AgentStart {
                        model: "default".to_string(),
                    },
                );
            }
            "agent.text_delta" => {
                if let Some(delta) = json
                    .get("params")
                    .and_then(|p| p.get("delta"))
                    .and_then(|v| v.as_str())
                {
                    let mut g = inner.lock().unwrap();
                    common::broadcast(
                        &mut g.state.subscribers,
                        HarnessEvent::TextDelta(delta.to_string()),
                    );
                }
            }
            "agent.thinking_delta" => {
                if let Some(delta) = json
                    .get("params")
                    .and_then(|p| p.get("delta"))
                    .and_then(|v| v.as_str())
                {
                    let mut g = inner.lock().unwrap();
                    common::broadcast(
                        &mut g.state.subscribers,
                        HarnessEvent::ThinkingDelta(delta.to_string()),
                    );
                }
            }
            "agent.tool_call" => {
                if let Some(tool) = json.get("params").and_then(|p| p.get("tool_call")) {
                    let id = tool
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = tool
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let args = tool
                        .get("arguments")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let mut g = inner.lock().unwrap();
                    common::broadcast(
                        &mut g.state.subscribers,
                        HarnessEvent::ToolStart {
                            tool_id: id,
                            name,
                            args,
                        },
                    );
                }
            }
            "agent.tool_result" => {
                if let Some(result) = json.get("params").and_then(|p| p.get("tool_result")) {
                    let id = result
                        .get("tool_call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let content = result.get("content").and_then(content_text);
                    let is_error = result
                        .get("is_error")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let mut g = inner.lock().unwrap();
                    common::broadcast(
                        &mut g.state.subscribers,
                        HarnessEvent::ToolEnd {
                            tool_id: id,
                            result: content,
                            is_error,
                        },
                    );
                }
            }
            "agent.end" => {
                let mut g = inner.lock().unwrap();
                common::broadcast(
                    &mut g.state.subscribers,
                    HarnessEvent::AgentEnd {
                        success: true,
                        message: None,
                    },
                );
            }
            "agent.error" => {
                if let Some(err) = json
                    .get("params")
                    .and_then(|p| p.get("error"))
                    .and_then(|v| v.as_str())
                {
                    let mut g = inner.lock().unwrap();
                    common::broadcast(
                        &mut g.state.subscribers,
                        HarnessEvent::Error(err.to_string()),
                    );
                }
            }
            "approval.request" => {
                if let Some(req) = json.get("params").and_then(|p| p.get("request")) {
                    let id = req
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let kind = req
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("confirm")
                        .to_string();
                    {
                        let mut g = inner.lock().unwrap();
                        g.pending_approvals.insert(id.clone(), req.clone());
                    }
                    let mut g = inner.lock().unwrap();
                    common::broadcast(
                        &mut g.state.subscribers,
                        HarnessEvent::ApprovalRequest {
                            id,
                            kind,
                            payload: req.clone(),
                        },
                    );
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
    fn hermes_harness_has_correct_id() {
        let harness = HermesHarness::new();
        assert_eq!(harness.id(), "hermes");
    }

    #[tokio::test]
    async fn hermes_harness_capabilities_are_full() {
        let harness = HermesHarness::new();
        let caps = harness.capabilities();
        assert!(caps.steer);
        assert!(caps.abort);
        assert!(caps.list_models);
        assert!(caps.approvals);
        assert!(caps.persistent);
    }

    #[test]
    fn parse_tool_call_and_result() {
        let h = HermesHarness::new();
        let mut rx = h.subscribe();
        let inner = Arc::clone(&h.inner);
        let call = serde_json::json!({"jsonrpc":"2.0","method":"agent.tool_call","params":{"tool_call":{"id":"tc1","name":"Bash","arguments":{"command":"ls"}}}});
        handle_stdout_line(&inner, &call);
        assert!(
            matches!(rx.try_recv(), Ok(HarnessEvent::ToolStart { tool_id, name, .. }) if tool_id == "tc1" && name == "Bash")
        );
    }
}
