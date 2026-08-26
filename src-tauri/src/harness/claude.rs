//! Claude Code harness adapter (Phase 12).
//!
//! Drives `claude -p --output-format stream-json --verbose --include-partial-messages
//! --input-format stream-json` over stdio. This is the second harness (after
//! a-coder-cli) that proves the studio is harness-agnostic.
//!
//! Stream-json protocol:
//! - Input (stdin): NDJSON user messages + control messages:
//!     {"type":"user_message","text":"..."}
//!     {"type":"interrupt"}
//!     {"type":"permission_response","permission_response":{"id":"...","response":"allow"}}
//! - Output (stdout): NDJSON events:
//!     {"type":"system","subtype":"init",...}
//!     {"type":"stream_event","event":{"type":"message_start",...}}
//!     {"type":"stream_event","event":{"type":"text_delta","delta":"..."}}
//!     {"type":"stream_event","event":{"type":"tool_use",...}}
//!     {"type":"stream_event","event":{"type":"tool_result",...}}
//!     {"type":"stream_event","event":{"type":"message_stop",...}}
//!     {"type":"permission_request","permission_request":{...}}
//!
//! The adapter writes the per-project `.claude/settings.json` so Claude picks up
//! the Navya MCP tool server (navya-mcp) with the live control-server URL/token.

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

/// Claude Code harness implementation.
pub struct ClaudeCodeHarness {
    id: String,
    capabilities: Capabilities,
    inner: Arc<std::sync::Mutex<Inner>>,
}

struct Inner {
    state: ChildState,
    stdin: std::sync::Mutex<Option<std::process::ChildStdin>>,
    child_handle: std::sync::Mutex<Option<std::process::Child>>,
    /// Pending permission requests awaiting a user answer.
    pending_permissions: HashMap<String, serde_json::Value>,
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
            inner: Arc::new(std::sync::Mutex::new(Inner {
                state: ChildState::new(PathBuf::new()),
                stdin: std::sync::Mutex::new(None),
                child_handle: std::sync::Mutex::new(None),
                pending_permissions: HashMap::new(),
            })),
        }
    }

    /// Send a JSONL command line to the process stdin.
    fn send_cmd(&self, cmd: serde_json::Value) -> Result<(), HarnessError> {
        let inner = self.inner.lock().unwrap();
        if !inner.state.started {
            return Err(HarnessError::NotStarted);
        }
        let mut stdin = inner.stdin.lock().unwrap();
        let child_stdin = stdin.as_mut().ok_or(HarnessError::NotStarted)?;
        common::send_json_line(child_stdin, &cmd)
    }

    /// Write the per-project `.claude/settings.json` that wires the Navya MCP
    /// server and a permissive-but-safe permission policy.
    fn write_project_config(
        &self,
        project_dir: &std::path::Path,
        control_url: Option<&str>,
        control_token: Option<&str>,
    ) -> Result<(), HarnessError> {
        let settings_dir = project_dir.join(".claude");
        let settings_path = settings_dir.join("settings.json");

        let mcp_dir = common::mcp_workspace_dir();
        let server_py = mcp_dir.join("navya_mcp").join("server.py");
        let server_py_abs = server_py.to_string_lossy().to_string();

        let mut settings = serde_json::json!({
            "permissions": {
                "allow": ["Read", "Bash", "Write", "Edit", "Glob", "Grep"],
                "deny": ["WebFetch", "WebSearch"]
            },
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
            }
        });

        // Preserve any existing user settings.json so we don't clobber their
        // entire Claude config on project open.
        if settings_path.exists() {
            if let Ok(raw) = std::fs::read_to_string(&settings_path) {
                if let Ok(existing) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(existing_obj) = existing.as_object() {
                        for (k, v) in existing_obj {
                            if k != "mcpServers" && k != "permissions" {
                                settings[k] = v.clone();
                            }
                        }
                    }
                }
            }
        }

        common::write_config(
            &settings_path, &serde_json::to_string_pretty(&settings).unwrap_or_default())
    }
}

impl Default for ClaudeCodeHarness {
    fn default() -> Self {
        Self::new()
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

    async fn start(&self, ctx: &HarnessCtx) -> Result<(), HarnessError> {
        // Idempotent: if already running in the same project dir, return Ok.
        let need_restart = {
            let inner = self.inner.lock().unwrap();
            inner.state.started && inner.state.project_dir != ctx.project_dir
        };
        if need_restart {
            self.stop().await?;
        }

        let Some(binary) = registry::descriptor_for(&self.id).and_then(|d| registry::detect(d).path) else {
            return Err(HarnessError::Process("claude binary not found on PATH".into()));
        };

        self.write_project_config(
            &ctx.project_dir, ctx.control_url.as_deref(), ctx.control_token.as_deref())?;

        // Spawn Claude in persistent bidirectional mode.
        // `--dangerously-skip-permissions` is used so the first end-to-end pass
        // doesn't hang waiting for approvals; Phase 11 can switch to
        // `--permission-mode manual` and forward `permission_request` events.
        let args = vec![
            "-p".to_string(),
            "--output-format".to_string(), "stream-json".to_string(),
            "--verbose".to_string(),
            "--include-partial-messages".to_string(),
            "--input-format".to_string(), "stream-json".to_string(),
            "--dangerously-skip-permissions".to_string(),
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
                common::broadcast(
                    &mut g.state.subscribers, HarnessEvent::Error("Claude Code process exited".into()));
            }
        });

        Ok(())
    }

    async fn prompt(&self, msg: &str, mode: PromptMode) -> Result<(), HarnessError> {
        match mode {
            PromptMode::Steer | PromptMode::FollowUp => {
                // Steer in Claude is an interrupt followed by a user message.
                self.send_cmd(serde_json::json!({ "type": "interrupt" }))?;
            }
            PromptMode::Normal => {}
        }
        self.send_cmd(serde_json::json!({ "type": "user_message", "text": msg }))
    }

    async fn steer(&self, msg: &str) -> Result<(), HarnessError> {
        self.send_cmd(serde_json::json!({ "type": "interrupt" }))?;
        self.send_cmd(serde_json::json!({ "type": "user_message", "text": msg }))
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        self.send_cmd(serde_json::json!({ "type": "interrupt" }))
    }

    async fn set_model(&self, _model: &str) -> Result<(), HarnessError> {
        // Claude Code model selection is handled via `--model` on spawn or via
        // settings.json. A real implementation would restart with the new model
        // or write it into `.claude/settings.json` under `model`.
        Ok(())
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        // Claude has no live model RPC like a-coder-cli. Surface the static
        // descriptor catalog until a live listing is wired.
        Ok(registry::fallback_models(self.id()))
    }

    async fn answer_approval(&self, request_id: &str, approved: bool) -> Result<(), HarnessError> {
        let response = if approved { "allow" } else { "deny" };
        // Remove the pending request; if it's gone, still send the response.
        {
            let mut inner = self.inner.lock().unwrap();
            inner.pending_permissions.remove(request_id);
        }
        self.send_cmd(serde_json::json!({
            "type": "permission_response",
            "permission_response": { "id": request_id, "response": response }
        }))
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
    let top_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match top_type {
        "system" => {
            let subtype = json.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
            if subtype == "init" {
                let model = json
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default")
                    .to_string();
                let mut g = inner.lock().unwrap();
                common::broadcast(
                    &mut g.state.subscribers, HarnessEvent::AgentStart { model });
            }
        }
        "stream_event" => {
            if let Some(event) = json.get("event") {
                if let Some(ev) = parse_stream_event(event) {
                    let mut g = inner.lock().unwrap();
                    common::broadcast(&mut g.state.subscribers, ev);
                }
            }
        }
        "permission_request" => {
            if let Some(req) = json.get("permission_request") {
                let id = req.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let _name = req.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let kind = req.get("type").and_then(|v| v.as_str()).unwrap_or("confirm").to_string();
                {
                    let mut g = inner.lock().unwrap();
                    g.pending_permissions.insert(id.clone(), req.clone());
                }
                let mut g = inner.lock().unwrap();
                common::broadcast(
                    &mut g.state.subscribers,
                    HarnessEvent::ApprovalRequest { id, kind, payload: req.clone() },
                );
            }
        }
        _ => {}
    }
}

fn parse_stream_event(event: &serde_json::Value) -> Option<HarnessEvent> {
    let event_type = event.get("type").and_then(|v| v.as_str())?;
    match event_type {
        "text_delta" => Some(HarnessEvent::TextDelta(
            event.get("delta").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        )),
        "thinking_delta" => Some(HarnessEvent::ThinkingDelta(
            event.get("delta").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        )),
        "tool_use" => {
            let tool = event.get("tool_use")?;
            Some(HarnessEvent::ToolStart {
                tool_id: tool.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                name: tool.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                args: tool.get("input").cloned().unwrap_or(serde_json::Value::Null),
            })
        }
        "tool_result" => {
            let tool = event.get("tool_result")?;
            Some(HarnessEvent::ToolEnd {
                tool_id: tool.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                result: tool.get("content").and_then(content_text),
                is_error: tool.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false),
            })
        }
        "message_stop" => {
            let success = event
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .map(|r| !matches!(r, "error" | "aborted"))
                .unwrap_or(true);
            Some(HarnessEvent::AgentEnd { success, message: None })
        }
        "error" => Some(HarnessEvent::Error(
            event.get("message").and_then(|v| v.as_str()).unwrap_or("Claude error").to_string(),
        )),
        "message_start" | "content_block_start" | "content_block_stop" | "tool_input_delta" => None,
        _ => None,
    }
}

fn content_text(v: &serde_json::Value) -> Option<String> {
    if let Some(text) = v.as_str() {
        return Some(text.to_string());
    }
    if let Some(items) = v.get("content").and_then(|c| c.as_array()) {
        let mut out = String::new();
        for item in items {
            if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
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

    #[test]
    fn parse_text_delta() {
        let json = serde_json::json!({"type":"stream_event","event":{"type":"text_delta","delta":"hello"}});
        let ev = parse_stream_event(&json["event"]);
        assert!(matches!(ev, Some(HarnessEvent::TextDelta(t)) if t == "hello"));
    }

    #[test]
    fn parse_tool_use_and_result() {
        let use_json = serde_json::json!({"type":"stream_event","event":{"type":"tool_use","tool_use":{"id":"tu1","name":"Bash","input":{"command":"ls"}}}});
        let ev = parse_stream_event(&use_json["event"]);
        assert!(matches!(ev, Some(HarnessEvent::ToolStart { tool_id, name, .. }) if tool_id == "tu1" && name == "Bash"));

        let result_json = serde_json::json!({"type":"stream_event","event":{"type":"tool_result","tool_result":{"tool_use_id":"tu1","content":"out","is_error":false}}});
        let ev = parse_stream_event(&result_json["event"]);
        assert!(matches!(ev, Some(HarnessEvent::ToolEnd { tool_id, result: Some(r), is_error: false }) if tool_id == "tu1" && r == "out"));
    }

    #[test]
    fn system_init_broadcasts_agent_start() {
        let h = ClaudeCodeHarness::new();
        let mut rx = h.subscribe();
        let inner = Arc::clone(&h.inner);
        let json = serde_json::json!({"type":"system","subtype":"init","model":"claude-sonnet-4-5"});
        handle_stdout_line(&inner, &json);
        assert!(matches!(rx.try_recv(), Ok(HarnessEvent::AgentStart { model }) if model == "claude-sonnet-4-5"));
    }
}
