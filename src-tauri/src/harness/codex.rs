//! OpenAI Codex harness adapter (Phase 13).
//!
//! Drives `codex app-server` over stdio JSON-RPC:
//! - `thread/start` to begin a persistent thread
//! - `command/exec` (or `thread/run`) to send user prompts
//! - `dynamicTools` exposes the Navya MCP tools
//! - `approvalPolicy` routes tool approvals through the UI
//!
//! The adapter writes a per-project `codex.json` config that registers the
//! Navya MCP server as an MCP provider and sets the approval policy to
//! `request`.

use async_trait::async_trait;
use std::{
    collections::HashMap,
    io::BufRead,
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{mpsc, mpsc::Receiver};

use crate::harness::{
    common::{self, ChildState},
    event::{HarnessEvent, ModelInfo},
    registry,
    Capabilities, HarnessCtx, HarnessError, Harness as HarnessTrait, PromptMode,
};

/// Codex harness implementation.
pub struct CodexHarness {
    id: String,
    capabilities: Capabilities,
    inner: Arc<std::sync::Mutex<Inner>>,
}

struct Inner {
    state: ChildState,
    stdin: std::sync::Mutex<Option<std::process::ChildStdin>>,
    child_handle: std::sync::Mutex<Option<std::process::Child>>,
    /// JSON-RPC request id counter.
    rpc_id: u64,
    /// Pending approvals (id -> request).
    pending_approvals: HashMap<String, serde_json::Value>,
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
            inner: Arc::new(std::sync::Mutex::new(Inner {
                state: ChildState::new(PathBuf::new()),
                stdin: std::sync::Mutex::new(None),
                child_handle: std::sync::Mutex::new(None),
                rpc_id: 0,
                pending_approvals: HashMap::new(),
            })),
        }
    }

    /// Send a JSON-RPC message to the app-server stdin.
    fn send_rpc(&self, method: &str, params: serde_json::Value) -> Result<u64, HarnessError> {
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
        let mut stdin = inner.stdin.lock().unwrap();
        let child_stdin = stdin.as_mut().ok_or(HarnessError::NotStarted)?;
        common::send_json_line(child_stdin, &msg)?;
        Ok(id)
    }

    /// Write the per-project Codex config that registers the Navya MCP server.
    fn write_project_config(
        &self,
        project_dir: &std::path::Path,
        control_url: Option<&str>,
        control_token: Option<&str>,
    ) -> Result<(), HarnessError> {
        let mcp_dir = common::mcp_workspace_dir();
        let server_py = mcp_dir.join("navya_mcp").join("server.py");
        let server_py_abs = server_py.to_string_lossy().to_string();

        let config = serde_json::json!({
            "providerEntries": [
                {
                    "name": "Navya Cloud",
                    "type": "openai-compatible",
                    "baseUrl": control_url.unwrap_or("http://127.0.0.1:8080")
                }
            ],
            "mcpServers": {
                "navya-studio-tools": {
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
                        "NAVYA_CONTROL_URL": control_url.unwrap_or("http://127.0.0.1:8080"),
                        "NAVYA_CONTROL_TOKEN": control_token.unwrap_or("")
                    }
                }
            },
            "approvalPolicy": "request"
        });

        let path = project_dir.join("codex.json");
        common::write_config(&path, &serde_json::to_string_pretty(&config).unwrap_or_default())
    }
}

impl Default for CodexHarness {
    fn default() -> Self {
        Self::new()
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

    async fn start(&self, ctx: &HarnessCtx) -> Result<(), HarnessError> {
        let need_restart = {
            let inner = self.inner.lock().unwrap();
            inner.state.started && inner.state.project_dir != ctx.project_dir
        };
        if need_restart {
            self.stop().await?;
        }

        let Some(binary) = registry::descriptor_for(&self.id).and_then(|d| registry::detect(d).path) else {
            return Err(HarnessError::Process("codex binary not found on PATH".into()));
        };

        self.write_project_config(&ctx.project_dir, ctx.control_url.as_deref(), ctx.control_token.as_deref())?;

        let config_path = ctx.project_dir.join("codex.json");
        let args = vec![
            "app-server".to_string(),
            "--config".to_string(), config_path.to_string_lossy().to_string(),
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
                was_started && !g.state.stopping
            };
            if notify {
                let mut g = inner.lock().unwrap();
                common::broadcast(&mut g.state.subscribers, HarnessEvent::Error("Codex server exited".into()));
            }
        });

        // Start a thread with the project context.
        let _ = self.send_rpc("thread/start", serde_json::json!({"cwd": ctx.project_dir.to_string_lossy()}));

        Ok(())
    }

    async fn prompt(&self, msg: &str, _mode: PromptMode) -> Result<(), HarnessError> {
        // Codex uses `command/exec` (or `thread/run`) to run a user message.
        self.send_rpc("command/exec", serde_json::json!({"command": msg}))?;
        Ok(())
    }

    async fn steer(&self, msg: &str) -> Result<(), HarnessError> {
        // Steer via a follow-up command/exec on the same thread.
        self.send_rpc("command/exec", serde_json::json!({"command": msg, "steer": true}))?;
        Ok(())
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        self.send_rpc("thread/cancel", serde_json::json!({}))?;
        Ok(())
    }

    async fn set_model(&self, model: &str) -> Result<(), HarnessError> {
        // Codex model is per-thread; set on the running thread.
        self.send_rpc("thread/update", serde_json::json!({"model": model}))?;
        Ok(())
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        Ok(registry::fallback_models(self.id()))
    }

    async fn answer_approval(&self, request_id: &str, approved: bool) -> Result<(), HarnessError> {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.pending_approvals.remove(request_id);
        }
        self.send_rpc(
            "approval/respond",
            serde_json::json!({"id": request_id, "approved": approved}),
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
    // JSON-RPC responses/notifications.
    if let Some(method) = json.get("method").and_then(|v| v.as_str()) {
        let params = json.get("params").cloned().unwrap_or(serde_json::Value::Null);
        match method {
            "thread/started" => {
                let mut g = inner.lock().unwrap();
                common::broadcast(&mut g.state.subscribers, HarnessEvent::AgentStart { model: "default".to_string() });
            }
            "turn/started" => {
                let mut g = inner.lock().unwrap();
                common::broadcast(&mut g.state.subscribers, HarnessEvent::AgentStart { model: "default".to_string() });
            }
            "item/text_delta" => {
                if let Some(delta) = params.get("delta").and_then(|v| v.as_str()) {
                    let mut g = inner.lock().unwrap();
                    common::broadcast(&mut g.state.subscribers, HarnessEvent::TextDelta(delta.to_string()));
                }
            }
            "item/tool_call" => {
                if let Some(tool) = params.get("tool_call") {
                    let id = tool.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let args = tool.get("arguments").cloned().unwrap_or(serde_json::Value::Null);
                    let mut g = inner.lock().unwrap();
                    common::broadcast(&mut g.state.subscribers, HarnessEvent::ToolStart { tool_id: id, name, args });
                }
            }
            "item/tool_result" => {
                if let Some(result) = params.get("tool_result") {
                    let id = result.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let content = result.get("content").and_then(content_text);
                    let is_error = result.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                    let mut g = inner.lock().unwrap();
                    common::broadcast(&mut g.state.subscribers, HarnessEvent::ToolEnd { tool_id: id, result: content, is_error });
                }
            }
            "turn/completed" => {
                let mut g = inner.lock().unwrap();
                common::broadcast(&mut g.state.subscribers, HarnessEvent::AgentEnd { success: true, message: None });
            }
            "turn/failed" => {
                let err = params.get("error").and_then(|v| v.as_str()).unwrap_or("turn failed").to_string();
                let mut g = inner.lock().unwrap();
                common::broadcast(&mut g.state.subscribers, HarnessEvent::AgentEnd { success: false, message: Some(err.clone()) });
                common::broadcast(&mut g.state.subscribers, HarnessEvent::Error(err));
            }
            "approval/request" => {
                if let Some(req) = params.get("request") {
                    let id = req.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let kind = req.get("type").and_then(|v| v.as_str()).unwrap_or("confirm").to_string();
                    {
                        let mut g = inner.lock().unwrap();
                        g.pending_approvals.insert(id.clone(), req.clone());
                    }
                    let mut g = inner.lock().unwrap();
                    common::broadcast(&mut g.state.subscribers, HarnessEvent::ApprovalRequest { id, kind, payload: req.clone() });
                }
            }
            _ => {}
        }
    } else if let Some(result) = json.get("result") {
        // Handle responses to our own requests (e.g. thread/start ack).
        let _ = result;
    } else if let Some(err) = json.get("error") {
        let msg = err.get("message").and_then(|v| v.as_str()).unwrap_or("JSON-RPC error").to_string();
        let mut g = inner.lock().unwrap();
        common::broadcast(&mut g.state.subscribers, HarnessEvent::Error(msg));
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
            } else if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
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
    fn codex_harness_has_correct_id() {
        let harness = CodexHarness::new();
        assert_eq!(harness.id(), "codex");
    }

    #[tokio::test]
    async fn codex_harness_capabilities_are_full() {
        let harness = CodexHarness::new();
        let caps = harness.capabilities();
        assert!(caps.steer);
        assert!(caps.abort);
        assert!(caps.list_models);
        assert!(caps.approvals);
        assert!(caps.persistent);
    }

    #[test]
    fn parse_tool_call_notification() {
        let h = CodexHarness::new();
        let mut rx = h.subscribe();
        let inner = Arc::clone(&h.inner);
        let json = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "item/tool_call",
            "params": {
                "tool_call": {"id":"tc1","name":"Bash","arguments":{"command":"ls"}}
            }
        });
        handle_stdout_line(&inner, &json);
        assert!(matches!(rx.try_recv(), Ok(HarnessEvent::ToolStart { tool_id, name, .. }) if tool_id == "tc1" && name == "Bash"));
    }
}
