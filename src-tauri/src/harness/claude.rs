//! Claude Code harness adapter (Phase 12).
//!
//! Drives `claude -p --output-format stream-json --verbose
//! --include-partial-messages --input-format stream-json --permission-mode
//! manual` over stdio. Protocol shapes below were captured live from Claude
//! Code 2.1.228 (see tests + tools/harness-stubs/claude-code*) — do not
//! "simplify" them back to invented shapes; every one here is verified.
//!
//! **Input (stdin, NDJSON):**
//!   user turn:    {"type":"user","message":{"role":"user","content":[{"type":"text","text":...}]}}
//!   interrupt:    {"type":"control_request","control_request":{"request_id":<client-uuid>,"request":{"subtype":"interrupt"}}}
//!   permission:   {"type":"control_response","control_response":{"request_id":...,"response":{"subtype":"allow","updatedInput":...} | {"subtype":"deny"}}}
//!
//! **Output (stdout, NDJSON):**
//!   {"type":"system","subtype":"init",...,"model":...}
//!   {"type":"system","subtype":"status"|"thinking_tokens",...}            (skipped)
//!   {"type":"stream_event","event":{type:"message_start"|"content_block_start"|"content_block_delta"|"content_block_stop"|"message_delta"|"message_stop",...}}
//!     text deltas:  event.delta = {"type":"text_delta","text":...}
//!     thinking:     event.delta = {"type":"thinking_delta","thinking":...}
//!   {"type":"assistant","message":{...,"content":[{"type":"tool_use","id","name","input"},...]}}
//!   {"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id","content","is_error"}]}}
//!   {"type":"control_request","control_request":{"request_id":...,"request":{"subtype":"can_use_tool","tool_name":...,"input":...}}}  → permission prompt
//!   {"is_error":false,"duration_api_ms":...,"num_turns":N,"stop_reason":...}  (final result line — NO "type" field)
//!
//! The adapter writes the per-project `.claude/settings.json` so Claude picks
//! up the Amaara MCP tool server (amaara-mcp) with the live control-server URL/token.

use async_trait::async_trait;
use std::{collections::HashMap, io::BufRead, sync::Arc};
use tokio::sync::{mpsc, mpsc::Receiver};

use crate::harness::{
    common::{self, ChildState},
    event::{HarnessEvent, ModelInfo},
    registry, Capabilities, Harness as HarnessTrait, HarnessCtx, HarnessError, PromptMode,
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
    /// Pending permission (can_use_tool) requests awaiting a user answer,
    /// keyed by request_id — kept so the answer can carry updatedInput.
    pending_permissions: HashMap<String, serde_json::Value>,
    /// Tool ids already announced (assistant messages repeat tool_use blocks
    /// that content_block_start already surfaced — never emit twice).
    emitted_tools: std::collections::HashSet<String>,
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
                state: ChildState::new(std::path::PathBuf::new()),
                stdin: std::sync::Mutex::new(None),
                child_handle: std::sync::Mutex::new(None),
                pending_permissions: HashMap::new(),
                emitted_tools: std::collections::HashSet::new(),
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

    /// Write the per-project `.claude/settings.json` that wires the Amaara MCP
    /// server and a permission policy. User-authored keys in an existing file
    /// are preserved; only `permissions` + `mcpServers` are studio-owned.
    fn write_project_config(
        &self,
        project_dir: &std::path::Path,
        control_url: Option<&str>,
        control_token: Option<&str>,
    ) -> Result<(), HarnessError> {
        let settings_dir = project_dir.join(".claude");
        let settings_path = settings_dir.join("settings.json");

        let mcp_dir = common::mcp_workspace_dir();
        let server_py = mcp_dir.join("amaara_mcp").join("server.py");
        let server_py_abs = server_py.to_string_lossy().to_string();

        let mut settings = serde_json::json!({
            "permissions": {
                "allow": ["Read", "Bash", "Write", "Edit", "Glob", "Grep"],
                "deny": ["WebFetch", "WebSearch"]
            },
            "mcpServers": {
                "amaara-studio-tools": {
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
            &settings_path,
            &serde_json::to_string_pretty(&settings).unwrap_or_default(),
        )
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
                "claude binary not found on PATH".into(),
            ));
        };

        self.write_project_config(
            &ctx.project_dir,
            ctx.control_url.as_deref(),
            ctx.control_token.as_deref(),
        )?;

        // Spawn Claude in persistent bidirectional stream-json mode.
        // Permission prompts that the user's settings don't pre-allow arrive
        // as `can_use_tool` control requests and are surfaced through the
        // studio's native ApprovalDialog (Phase 11). We must NOT pass
        // --dangerously-skip-permissions — that would silently bypass the
        // approval flow entirely.
        let args = vec![
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--permission-mode".to_string(),
            "manual".to_string(),
            "--include-partial-messages".to_string(),
            "--input-format".to_string(),
            "stream-json".to_string(),
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
            inner.emitted_tools.clear();
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
                    &mut g.state.subscribers,
                    HarnessEvent::Error("Claude Code process exited".into()),
                );
            }
        });

        Ok(())
    }

    async fn prompt(&self, msg: &str, mode: PromptMode) -> Result<(), HarnessError> {
        // Real stream-json user turn (captured from Claude Code 2.1.228):
        // {"type":"user","message":{"role":"user","content":[{"type":"text","text":...}]}}.
        // The previously-sent {"type":"user_message"} shape is not part of the
        // protocol — the CLI ignores it and the turn never starts.
        if matches!(mode, PromptMode::Steer | PromptMode::FollowUp) {
            // Steer in Claude is an interrupt followed by a new user message.
            self.send_cmd(control_request_interrupt())?;
        }
        self.send_cmd(serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": [ { "type": "text", "text": msg } ] }
        }))
    }

    async fn steer(&self, msg: &str) -> Result<(), HarnessError> {
        self.send_cmd(control_request_interrupt())?;
        self.send_cmd(serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": [ { "type": "text", "text": msg } ] }
        }))
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        // SDK control protocol: interrupt the current turn.
        self.send_cmd(control_request_interrupt())
    }

    async fn set_model(&self, _model: &str) -> Result<(), HarnessError> {
        // Claude Code model selection is handled via `--model` on spawn or via
        // settings.json `model`. The adapter restarts pick the session model
        // up from the HarnessCtx; mid-session switching is not exposed by the
        // stream-json control protocol we rely on.
        Ok(())
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        // Claude has no live model RPC over this transport. Surface the static
        // descriptor catalog until a live listing is wired.
        Ok(registry::fallback_models(self.id()))
    }

    async fn answer_approval(
        &self,
        request_id: &str,
        approved: bool,
        value: Option<String>,
    ) -> Result<(), HarnessError> {
        // Claude's stream-json permission flow: can_use_tool control_request →
        // control_response. An edited value replaces the relevant input field
        // (Bash-like tools take `command`).
        let pending_input = {
            let mut inner = self.inner.lock().unwrap();
            inner.pending_permissions.remove(request_id)
        };
        let response = if approved {
            let mut updated = serde_json::json!({ "subtype": "allow" });
            if let (Some(v), Some(req)) = (value, pending_input.as_ref()) {
                let input = req
                    .pointer("/request/input")
                    .and_then(|i| i.as_object())
                    .cloned()
                    .unwrap_or_default();
                if input.contains_key("command") {
                    let mut with_cmd = input.clone();
                    with_cmd.insert("command".into(), serde_json::Value::String(v));
                    updated["updatedInput"] = serde_json::Value::Object(with_cmd);
                } else if !input.is_empty() {
                    updated["updatedInput"] = serde_json::Value::Object(input);
                }
            }
            serde_json::json!({ "type": "control_response", "control_response": {
                "request_id": request_id, "response": updated } })
        } else {
            serde_json::json!({ "type": "control_response", "control_response": {
                "request_id": request_id, "response": { "subtype": "deny" } } })
        };
        self.send_cmd(response)
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

/// Client-generated interrupt request (SDK control protocol).
fn control_request_interrupt() -> serde_json::Value {
    let id = format!(
        "interrupt-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    serde_json::json!({
        "type": "control_request",
        "control_request": { "request_id": id, "request": { "subtype": "interrupt" } }
    })
}

fn handle_stdout_line(inner: &Arc<std::sync::Mutex<Inner>>, json: &serde_json::Value) {
    // The per-turn result line carries no "type" field at all — skip it (the
    // stream's message_stop already drives AgentEnd).
    if json.get("type").is_none() && json.get("is_error").is_some() {
        return;
    }
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
                g.emitted_tools.clear();
                common::broadcast(&mut g.state.subscribers, HarnessEvent::AgentStart { model });
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
        // Complete assistant message: authoritative tool_use blocks with full
        // input (content_block_start carries an empty input that fills in via
        // input_json_delta chunks we'd have to accumulate — the assistant
        // message lands right before execution and always has the args).
        "assistant" => {
            let blocks = json
                .pointer("/message/content")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            let mut starts = Vec::new();
            for block in blocks.iter() {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    starts.push((id, block.clone()));
                }
            }
            if starts.is_empty() {
                return;
            }
            let mut g = inner.lock().unwrap();
            for (id, block) in starts {
                if id.is_empty() || !g.emitted_tools.insert(id.clone()) {
                    continue;
                }
                common::broadcast(
                    &mut g.state.subscribers,
                    HarnessEvent::ToolStart {
                        tool_id: id,
                        name: block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        args: block
                            .get("input")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null),
                    },
                );
            }
        }
        // Tool results arrive as top-level user-role messages.
        "user" => {
            let items = json
                .pointer("/message/content")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            let mut ends = Vec::new();
            for item in items.iter() {
                if item.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                    ends.push(item.clone());
                }
            }
            if ends.is_empty() {
                return;
            }
            let mut g = inner.lock().unwrap();
            for tool_result in ends {
                let id = tool_result
                    .get("tool_use_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let is_error = tool_result
                    .get("is_error")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let content = tool_result.get("content");
                let text = match content {
                    Some(serde_json::Value::String(s)) => Some(s.clone()),
                    Some(other) => content_text(other),
                    None => None,
                };
                common::broadcast(
                    &mut g.state.subscribers,
                    HarnessEvent::ToolEnd {
                        tool_id: id,
                        result: text,
                        is_error,
                    },
                );
            }
        }
        // Permission prompts (only for tools not pre-allowed by settings).
        "control_request" => {
            let req = json.get("control_request");
            let subtype = req
                .and_then(|r| r.pointer("/request/subtype"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if subtype != "can_use_tool" {
                return; // interrupt acks etc. — nothing to surface
            }
            let id = req
                .and_then(|r| r.get("request_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tool_name = req
                .and_then(|r| r.pointer("/request/tool_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("tool")
                .to_string();
            let mut g = inner.lock().unwrap();
            g.pending_permissions.insert(id.clone(), json.clone());
            common::broadcast(
                &mut g.state.subscribers,
                HarnessEvent::ApprovalRequest {
                    id,
                    kind: tool_name,
                    payload: json.clone(),
                },
            );
        }
        // Acknowledgements of OUR control requests — nothing to surface.
        "control_response" => {}
        _ => {}
    }
}

fn parse_stream_event(event: &serde_json::Value) -> Option<HarnessEvent> {
    let event_type = event.get("type").and_then(|v| v.as_str())?;
    match event_type {
        // content_block_delta carries the real deltas:
        //   text:     {"type":"text_delta","text":...}
        //   thinking: {"type":"thinking_delta","thinking":...}
        "content_block_delta" => {
            let delta = event.get("delta")?;
            match delta.get("type").and_then(|v| v.as_str())? {
                "text_delta" => Some(HarnessEvent::TextDelta(
                    delta
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                )),
                "thinking_delta" => Some(HarnessEvent::ThinkingDelta(
                    delta
                        .get("thinking")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                )),
                _ => None, // input_json_delta / signature_delta — not surfaced
            }
        }
        "message_delta" => {
            // Carries the turn's stop_reason; message_stop follows immediately.
            None
        }
        "message_stop" => {
            // End of the assistant turn. Success is refined by the final
            // result line; here we optimistically mark the turn complete.
            Some(HarnessEvent::AgentEnd {
                success: true,
                message: None,
            })
        }
        "error" => Some(HarnessEvent::Error(
            event
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Claude error")
                .to_string(),
        )),
        _ => None, // message_start / content_block_start / content_block_stop
    }
}

fn content_text(v: &serde_json::Value) -> Option<String> {
    if let Some(text) = v.as_str() {
        return Some(text.to_string());
    }
    if let Some(items) = v.as_array() {
        let mut out = String::new();
        for item in items {
            if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            }
        }
        return if out.is_empty() { None } else { Some(out) };
    }
    if let Some(items) = v.get("content").and_then(|c| c.as_array()) {
        return content_text(&serde_json::Value::Array(items.clone()));
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

    // --- shapes captured live from Claude Code 2.1.228 ----------------------

    #[test]
    fn parse_text_delta_from_content_block_delta() {
        let json = serde_json::json!({
            "type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": "ok"}
        });
        match parse_stream_event(&json) {
            Some(HarnessEvent::TextDelta(t)) => assert_eq!(t, "ok"),
            other => panic!("expected TextDelta, got {other:?}"),
        }
    }

    #[test]
    fn parse_thinking_delta_from_content_block_delta() {
        let json = serde_json::json!({
            "type": "stream_event_event_only",
        });
        // thinking delta shape (from live capture)
        let ev = serde_json::json!({
            "type": "content_block_delta", "index": 0,
            "delta": {"type": "thinking_delta", "thinking": "The user wants"}
        });
        let _ = json;
        match parse_stream_event(&ev) {
            Some(HarnessEvent::ThinkingDelta(t)) => assert_eq!(t, "The user wants"),
            other => panic!("expected ThinkingDelta, got {other:?}"),
        }
    }

    #[test]
    fn message_stop_yields_agent_end() {
        let ev = parse_stream_event(&serde_json::json!({"type": "message_stop"}));
        assert!(matches!(
            ev,
            Some(HarnessEvent::AgentEnd { success: true, .. })
        ));
    }

    #[test]
    fn tool_result_from_user_event() {
        let h = ClaudeCodeHarness::new();
        let mut rx = h.subscribe();
        let inner = Arc::clone(&h.inner);
        let json = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"tool_use_id": "tu_9", "type": "tool_result",
                 "content": "studio-probe-42\n", "is_error": false}
            ]}
        });
        handle_stdout_line(&inner, &json);
        match rx.try_recv() {
            Ok(HarnessEvent::ToolEnd {
                tool_id,
                result,
                is_error,
            }) => {
                assert_eq!(tool_id, "tu_9");
                assert_eq!(result.as_deref(), Some("studio-probe-42\n"));
                assert!(!is_error);
            }
            other => panic!("expected ToolEnd, got {other:?}"),
        }
    }

    #[test]
    fn tool_start_from_assistant_message_with_full_args() {
        let h = ClaudeCodeHarness::new();
        let mut rx = h.subscribe();
        let inner = Arc::clone(&h.inner);
        let json = serde_json::json!({
            "type": "assistant",
            "message": {"id": "m1", "role": "assistant",
                "content": [{"type": "tool_use", "id": "call_1", "name": "Bash",
                             "input": {"command": "echo studio-probe-123"}}]}
        });
        handle_stdout_line(&inner, &json);
        match rx.try_recv() {
            Ok(HarnessEvent::ToolStart {
                tool_id,
                name,
                args,
            }) => {
                assert_eq!(tool_id, "call_1");
                assert_eq!(name, "Bash");
                assert_eq!(args["command"], "echo studio-probe-123");
            }
            other => panic!("expected ToolStart, got {other:?}"),
        }
        // The same tool_use id must never emit twice.
        handle_stdout_line(&inner, &json);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn can_use_tool_becomes_approval_request() {
        let h = ClaudeCodeHarness::new();
        let mut rx = h.subscribe();
        let inner = Arc::clone(&h.inner);
        let json = serde_json::json!({
            "type": "control_request",
            "control_request": {"request_id": "req-7",
                "request": {"subtype": "can_use_tool", "tool_name": "WebFetch",
                            "input": {"url": "https://example.com"}}}
        });
        handle_stdout_line(&inner, &json);
        match rx.try_recv() {
            Ok(HarnessEvent::ApprovalRequest { id, kind, payload }) => {
                assert_eq!(id, "req-7");
                assert_eq!(kind, "WebFetch");
                assert_eq!(
                    payload
                        .pointer("/control_request/request/tool_name")
                        .and_then(|v| v.as_str()),
                    Some("WebFetch")
                );
            }
            other => panic!("expected ApprovalRequest, got {other:?}"),
        }
        // The result line (no "type") must not panic or emit.
        let inner2 = Arc::clone(&h.inner);
        let no_type =
            serde_json::json!({"is_error": false, "num_turns": 1, "stop_reason": "end_turn"});
        handle_stdout_line(&inner2, &no_type);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn system_init_broadcasts_agent_start() {
        let h = ClaudeCodeHarness::new();
        let mut rx = h.subscribe();
        let inner = Arc::clone(&h.inner);
        let json =
            serde_json::json!({"type":"system","subtype":"init","model":"claude-sonnet-4-5"});
        handle_stdout_line(&inner, &json);
        assert!(
            matches!(rx.try_recv(), Ok(HarnessEvent::AgentStart { model }) if model == "claude-sonnet-4-5")
        );
    }
}
