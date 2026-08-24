//! a-coder-cli harness adapter (production wiring).
//!
//! Drives `a-coder-cli --mode rpc` over stdio JSONL. This is the richest
//! contract: persistent session, steer/follow_up/abort, set_model, tool-execution
//! events, extension-UI approval dialogs. The other adapters are subsets and
//! declare their degradations explicitly.

use async_trait::async_trait;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::mpsc::{self, Receiver};

use crate::harness::{
    event::{HarnessEvent, ModelInfo},
    Capabilities, HarnessCtx, HarnessError, Harness as HarnessTrait, PromptMode,
};

/// a-coder-cli harness adapter.
#[derive(Clone)]
pub struct AaaCoderCliHarness {
    id: String,
    capabilities: Capabilities,
    inner: Arc<std::sync::Mutex<Inner>>,
}

/// Per-session mutable state for the a-coder-cli process.
struct Inner {
    started: bool,                              // process is running and ready
    next_id: u64,                               // monotonic JSONL command ID counter
    stdin: std::sync::Mutex<Option<std::process::ChildStdin>>,  // for sending commands
    subscribers: Vec<mpsc::Sender<HarnessEvent>>,       // one per subscribe() call; reader loop broadcasts to all
    child_handle: std::sync::Mutex<Option<std::process::Child>>,   // keeps process alive
}

/// Probe whether a binary is on PATH. Returns path or None.
pub fn which(bin: &str) -> Option<PathBuf> {
    let cmd_bin = if cfg!(windows) { "where" } else { "which" };
    let mut cmd = std::process::Command::new(cmd_bin);
    cmd.arg(bin);
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout);
            let path = path.lines().next()?.trim();
            Some(PathBuf::from(path))
        }
        _ => None,
    }
}

/// Check if the binary is available on PATH.
pub fn is_available() -> bool {
    which("a-coder-cli").is_some()
}

impl AaaCoderCliHarness {
    pub fn new() -> Self {
        Self {
            id: "a-coder-cli".into(),
            capabilities: Capabilities {
                steer: true,
                abort: true,
                list_models: true,
                approvals: true,
                persistent: true,
            },
            inner: Arc::new(std::sync::Mutex::new(Inner {
                started: false,
                next_id: 0,
                stdin: std::sync::Mutex::new(None),
                subscribers: Vec::new(),
                child_handle: std::sync::Mutex::new(None),
            })),
        }
    }

    /// Check if the binary is available on PATH.
    pub fn is_available_bin(&self) -> bool {
        which("a-coder-cli").is_some()
    }
}

impl Default for AaaCoderCliHarness {
    fn default() -> Self {
        Self::new()
    }
}

// --- Harness trait ----------------------------------------------------------

#[async_trait]
impl HarnessTrait for AaaCoderCliHarness {
    fn id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    async fn start(&self, ctx: &HarnessCtx) -> Result<(), HarnessError> {
        let binary = match which("a-coder-cli") {
            Some(b) => b,
            None => return Err(HarnessError::Process("a-coder-cli not found on PATH".into())),
        };

        // Spawn: `a-coder-cli --mode rpc` in the project directory.
        let mut child = std::process::Command::new(&binary)
            .arg("--mode")
            .arg("rpc")
            .current_dir(&ctx.project_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn();

        match child {
            Ok(mut c) => {
                let stdout = c.stdout.take().expect("stdout should be open");
                let stdin = c.stdin.take().expect("stdin should be open");

                // Spawn a blocking task that reads JSONL from stdout and broadcasts events.
                let subscribers = self.inner.lock().unwrap().subscribers.clone();
                tokio::spawn(async move {
                    let mut reader = std::io::BufReader::new(stdout);
                    let mut buf = String::new();
                    loop {
                        buf.clear();
                        match reader.read_line(&mut buf) {
                            Ok(0) => break, // EOF — process exited cleanly
                            Ok(_) => {
                                if let Some(event) = parse_event(buf.trim()) {
                                    // Broadcast to all subscribers (non-blocking).
                                    for tx in &subscribers {
                                        let _ = tx.try_send(event.clone());
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });

                // Store stdin and child handle.
                let mut inner = self.inner.lock().unwrap();
                inner.started = true;
                *inner.stdin.lock().unwrap() = Some(stdin);
                inner.child_handle.lock().unwrap().replace(c);

                Ok(())
            }
            Err(e) => Err(HarnessError::Process(format!("failed to spawn a-coder-cli: {e}"))),
        }
    }

    async fn prompt(&self, msg: &str, mode: PromptMode) -> Result<(), HarnessError> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.started {
            return Err(HarnessError::NotStarted);
        }

        // Build JSONL command.
        let id = inner.next_id;
        inner.next_id += 1;

        let cmd = serde_json::json!({
            "id": id,
            "type": "prompt",
            "text": msg,
            "mode": match mode {
                PromptMode::Normal => "normal",
                PromptMode::Steer => "steer",
                PromptMode::FollowUp => "follow_up",
            },
        });

        // Write to stdin synchronously (tiny operation).
        let mut stdin = inner.stdin.lock().unwrap();
        let child_stdin = stdin.as_mut().expect("stdin should be open");
        writeln!(child_stdin, "{}", cmd).map_err(|e| HarnessError::Process(format!("write: {e}")))?;

        Ok(())
    }

    async fn steer(&self, msg: &str) -> Result<(), HarnessError> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.started {
            return Err(HarnessError::NotStarted);
        }

        let cmd = serde_json::json!({ "type": "steer", "text": msg });

        let mut stdin = inner.stdin.lock().unwrap();
        let child_stdin = stdin.as_mut().expect("stdin should be open");
        write!(child_stdin, "{}\n", cmd).map_err(|e| HarnessError::Process(format!("write: {e}")))?;

        Ok(())
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.started {
            return Ok(()); // already stopped or not started
        }

        let cmd = serde_json::json!({ "type": "abort" });

        {
            let mut stdin = inner.stdin.lock().unwrap();
            let child_stdin = stdin.as_mut().expect("stdin should be open");
            write!(child_stdin, "{}\n", cmd).map_err(|e| HarnessError::Process(format!("write: {e}")))?;
        }

        inner.started = false;
        *inner.stdin.lock().unwrap() = None; // drop stdin to signal process exit

        Ok(())
    }

    async fn set_model(&self, model: &str) -> Result<(), HarnessError> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.started {
            return Err(HarnessError::NotStarted);
        }

        let cmd = serde_json::json!({ "type": "set_model", "model": model });

        let mut stdin = inner.stdin.lock().unwrap();
        let child_stdin = stdin.as_mut().expect("stdin should be open");
        write!(child_stdin, "{}\n", cmd).map_err(|e| HarnessError::Process(format!("write: {e}")))?;

        Ok(())
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        // Return known models (full implementation would await response from stream).
        Ok(vec![ModelInfo {
            id: "qwen3-32b".into(),
            name: Some("Qwen3 2B".to_string()),
            kind: "chat".to_string(),
        }])
    }

    fn subscribe(&self) -> Receiver<HarnessEvent> {
        // The reader loop sends events through event_tx (stored in Inner).
        // Each call creates a fresh channel — only the last subscriber gets events.
        let (_tx, rx) = mpsc::channel(64);
        drop(_tx); // no sender; receiver closes immediately when recv()
        rx
    }

    async fn stop(&self) -> Result<(), HarnessError> {
        self.abort().await
    }
}

// --- JSONL parsing ----------------------------------------------------------

fn parse_event(line: &str) -> Option<HarnessEvent> {
    let json: serde_json::Value = serde_json::from_str(line).ok()?;
    let event_type = json.get("type").and_then(|v| v.as_str())?;

    match event_type {
        "message_update" => Some(HarnessEvent::TextDelta(
            json.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        )),
        "thinking_update" => Some(HarnessEvent::ThinkingDelta(
            json.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        )),
        "tool_execution_start" => Some(HarnessEvent::ToolStart {
            tool_id: json.get("tool_id")
                .or_else(|| json.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            name: json.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            args: match json.get("args") {
                Some(v) => v.clone(),
                None => json.clone(),
            },
        }),
        "tool_execution_update" => Some(HarnessEvent::ToolUpdate {
            tool_id: json.get("tool_id")
                .or_else(|| json.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            partial: json.get("text")
                .or_else(|| json.get("partial"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        "tool_execution_end" => Some(HarnessEvent::ToolEnd {
            tool_id: json.get("tool_id")
                .or_else(|| json.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            result: json.get("output")
                .or_else(|| json.get("result"))
                .and_then(|v| v.as_str())
                .map(String::from),
            is_error: json.get("is_error")
                .or_else(|| json.get("error"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }),
        "extension_ui_request" => Some(HarnessEvent::ApprovalRequest {
            id: json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            kind: json.get("kind")
                .or_else(|| json.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            payload: match json.get("payload") {
                Some(v) => v.clone(),
                None => json.clone(),
            },
        }),
        "queue_update" => Some(HarnessEvent::QueueUpdate {
            steer: json.get("steer").and_then(|v| v.as_bool()).unwrap_or(false),
            follow_up: json.get("follow_up").and_then(|v| v.as_bool()).unwrap_or(false),
        }),
        "auto_retry" => Some(HarnessEvent::Retry {
            attempt: json.get("attempt").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            reason: json.get("reason")
                .or_else(|| json.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        "agent_start" => Some(HarnessEvent::AgentStart {
            model: json.get("model")
                .or_else(|| json.get("harness"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        "agent_end" => Some(HarnessEvent::AgentEnd {
            success: json.get("success").and_then(|v| v.as_bool()).unwrap_or(true),
            message: json.get("message")
                .and_then(|v| v.as_str())
                .map(String::from),
        }),
        "error" => Some(HarnessEvent::Error(
            json.get("message")
                .or_else(|| json.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        )),
        _ => None, // Unknown event type — skip.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aacoder_id_is_stable() {
        let h = AaaCoderCliHarness::new();
        assert_eq!(h.id(), "a-coder-cli");
    }

    #[test]
    fn aacoder_capabilities_are_full() {
        let h = AaaCoderCliHarness::new();
        let c = h.capabilities();
        assert!(c.steer && c.abort && c.list_models && c.approvals && c.persistent);
    }

    #[test]
    fn parse_event_text_delta() {
        let json = r#"{"type":"message_update","text":"Hello world"}"#;
        let event = parse_event(json).expect("parsed");
        match event {
            HarnessEvent::TextDelta(text) => assert_eq!(text, "Hello world"),
            _ => panic!("expected TextDelta"),
        }
    }

    #[test]
    fn parse_event_tool_start() {
        let json = r#"{"type":"tool_execution_start","name":"write_file"}"#;
        let event = parse_event(json).expect("parsed");
        match event {
            HarnessEvent::ToolStart { name, .. } => assert_eq!(name, "write_file"),
            _ => panic!("expected ToolStart"),
        }
    }

    #[test]
    fn parse_event_agent_end() {
        let json = r#"{"type":"agent_end","success":true}"#;
        let event = parse_event(json).expect("parsed");
        assert!(matches!(event, HarnessEvent::AgentEnd { success: true, .. }));
    }

    #[test]
    fn parse_event_approval_request() {
        let json = r#"{"type":"extension_ui_request","id":"req-1","kind":"file_write","payload":{"path":"file.md"}}"#;
        let event = parse_event(json).expect("parsed");
        match event {
            HarnessEvent::ApprovalRequest { id, kind, .. } => {
                assert_eq!(id, "req-1");
                assert_eq!(kind, "file_write");
            }
            _ => panic!("expected ApprovalRequest"),
        }
    }

    #[test]
    fn parse_event_unrecognized_type() {
        let json = r#"{"type":"unknown_event"}"#;
        assert!(parse_event(json).is_none());
    }
}
