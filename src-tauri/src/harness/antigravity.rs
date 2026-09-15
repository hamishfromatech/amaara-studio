//! Antigravity CLI harness adapter (Phase 13).
//!
//! `agy -p "<prompt>" --output-format stream-json` is one-shot per prompt
//! (no documented persistent stdin stream). The adapter therefore spawns a
//! fresh process on every `prompt()` call, caches the project directory, and
//! streams the resulting NDJSON events back to the UI.
//!
//! Because there is no mid-stream steering, `steer()` degrades to `abort` +
//! re-prompt (the Antigravity capability matrix declares `steer: false`).
//! Tools come from the Amaara MCP server injected into `~/.claude/mcp-servers.json`.

use async_trait::async_trait;
use std::{io::BufRead, path::PathBuf, sync::Arc};
use tokio::sync::{mpsc, mpsc::Receiver};

use crate::harness::{
    common::{self, ChildState},
    event::{HarnessEvent, ModelInfo},
    registry, Capabilities, Harness as HarnessTrait, HarnessCtx, HarnessError, PromptMode,
};

/// Antigravity harness implementation.
pub struct AntigravityHarness {
    id: String,
    capabilities: Capabilities,
    inner: Arc<std::sync::Mutex<Inner>>,
}

struct Inner {
    state: ChildState,
    /// Active one-shot process handle for abort().
    child_handle: std::sync::Mutex<Option<std::process::Child>>,
}

impl AntigravityHarness {
    pub fn new() -> Self {
        AntigravityHarness {
            id: "antigravity".to_string(),
            // Antigravity print mode has no mid-stream steer and no persistence.
            capabilities: Capabilities {
                steer: false,
                abort: true,
                list_models: true,
                approvals: true,
                persistent: false,
            },
            inner: Arc::new(std::sync::Mutex::new(Inner {
                state: ChildState::new(PathBuf::new()),
                child_handle: std::sync::Mutex::new(None),
            })),
        }
    }

    /// Write the Claude-Code-compatible `~/.claude/mcp-servers.json` fragment
    /// that points the Antigravity CLI at the Amaara MCP server.
    fn write_mcp_config(
        &self,
        control_url: Option<&str>,
        control_token: Option<&str>,
    ) -> Result<(), HarnessError> {
        let mcp_dir = common::mcp_workspace_dir();
        let server_py = mcp_dir.join("amaara_mcp").join("server.py");
        let server_py_abs = server_py.to_string_lossy().to_string();

        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .ok_or_else(|| HarnessError::Process("cannot resolve home directory".into()))?;
        let config_dir = PathBuf::from(home).join(".claude");
        let config_path = config_dir.join("mcp-servers.json");

        let mut existing = if config_path.exists() {
            std::fs::read_to_string(&config_path)
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default()
        } else {
            serde_json::Map::new()
        };

        existing.insert(
            "amaara-studio-tools".to_string(),
            serde_json::json!({
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
            }),
        );

        common::write_config(
            &config_path,
            &serde_json::to_string_pretty(&existing).unwrap_or_default(),
        )
    }

    /// Spawn `agy -p msg --output-format stream-json` and pump events.
    async fn spawn_turn(&self, msg: &str, ctx: &HarnessCtx) -> Result<(), HarnessError> {
        let Some(binary) =
            registry::descriptor_for(&self.id).and_then(|d| registry::detect(d).path)
        else {
            return Err(HarnessError::Process("agy binary not found on PATH".into()));
        };

        self.write_mcp_config(ctx.control_url.as_deref(), ctx.control_token.as_deref())?;

        let args = vec![
            "-p".to_string(),
            msg.to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];

        let (mut child, _stdin) = common::spawn_command(
            &binary,
            &args,
            &ctx.project_dir,
            ctx.control_url.as_deref(),
            ctx.control_token.as_deref(),
        )?;

        let stdout = child.stdout.take().expect("stdout pipe should be open");
        {
            let inner = self.inner.lock().unwrap();
            inner.child_handle.lock().unwrap().replace(child);
        }

        let inner = Arc::clone(&self.inner);
        let project_dir = ctx.project_dir.clone();
        tokio::spawn(async move {
            let mut reader = std::io::BufReader::new(stdout);
            let mut buf = String::new();
            let mut started = false;
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
                        if !started {
                            started = true;
                            let mut g = inner.lock().unwrap();
                            common::broadcast(
                                &mut g.state.subscribers,
                                HarnessEvent::AgentStart {
                                    model: "default".to_string(),
                                },
                            );
                        }
                        if let Some(ev) = parse_stream_json(&json) {
                            let mut g = inner.lock().unwrap();
                            common::broadcast(&mut g.state.subscribers, ev);
                        }
                    }
                    Err(_) => break,
                }
            }
            let mut g = inner.lock().unwrap();
            common::broadcast(
                &mut g.state.subscribers,
                HarnessEvent::AgentEnd {
                    success: true,
                    message: None,
                },
            );
            g.state.project_dir = project_dir;
            g.child_handle.lock().unwrap().take();
        });

        Ok(())
    }
}

impl Default for AntigravityHarness {
    fn default() -> Self {
        Self::new()
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

    async fn start(&self, ctx: &HarnessCtx) -> Result<(), HarnessError> {
        // One-shot harness: start just validates MCP config and records cwd.
        self.write_mcp_config(ctx.control_url.as_deref(), ctx.control_token.as_deref())?;
        let mut inner = self.inner.lock().unwrap();
        inner.state.started = true;
        inner.state.project_dir = ctx.project_dir.clone();
        Ok(())
    }

    async fn prompt(&self, msg: &str, _mode: PromptMode) -> Result<(), HarnessError> {
        let ctx;
        {
            let inner = self.inner.lock().unwrap();
            if !inner.state.started {
                return Err(HarnessError::NotStarted);
            }
            ctx = HarnessCtx {
                project_dir: inner.state.project_dir.clone(),
                model: "default".to_string(),
                source: "cloud".to_string(),
                control_url: None,
                control_token: None,
            };
        }
        self.spawn_turn(msg, &ctx).await
    }

    async fn steer(&self, _msg: &str) -> Result<(), HarnessError> {
        // Antigravity has no mid-stream steer; degrade to abort + re-prompt.
        Err(HarnessError::NoSteer)
    }

    async fn abort(&self) -> Result<(), HarnessError> {
        let child = {
            let inner = self.inner.lock().unwrap();
            let child = inner.child_handle.lock().unwrap().take();
            child
        };
        if let Some(child) = child {
            common::kill_process_tree(child.id());
        }
        Ok(())
    }

    async fn set_model(&self, _model: &str) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn available_models(&self) -> Result<Vec<ModelInfo>, HarnessError> {
        Ok(registry::fallback_models(self.id()))
    }

    fn subscribe(&self) -> Receiver<HarnessEvent> {
        let (tx, rx) = mpsc::channel(256);
        let mut inner = self.inner.lock().unwrap();
        inner.state.subscribers.retain(|s| !s.is_closed());
        inner.state.subscribers.push(tx);
        rx
    }

    async fn stop(&self) -> Result<(), HarnessError> {
        self.abort().await?;
        let mut inner = self.inner.lock().unwrap();
        inner.state.started = false;
        Ok(())
    }
}

fn parse_stream_json(json: &serde_json::Value) -> Option<HarnessEvent> {
    let event_type = json.get("type").and_then(|v| v.as_str())?;
    match event_type {
        "step_update" => {
            let step_type = json.get("step_type").and_then(|v| v.as_str()).unwrap_or("");
            match step_type {
                "thinking" => Some(HarnessEvent::ThinkingDelta(
                    json.get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                )),
                "text" => Some(HarnessEvent::TextDelta(
                    json.get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                )),
                "tool_use" => Some(HarnessEvent::ToolStart {
                    tool_id: json
                        .get("tool_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: json
                        .get("tool_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    args: json
                        .get("tool_input")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                }),
                "tool_result" => Some(HarnessEvent::ToolEnd {
                    tool_id: json
                        .get("tool_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    result: json
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    is_error: json
                        .get("is_error")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                }),
                _ => None,
            }
        }
        "final_result" => Some(HarnessEvent::AgentEnd {
            success: !json
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            message: json
                .get("content")
                .and_then(|v| v.as_str())
                .map(String::from),
        }),
        "error" => Some(HarnessEvent::Error(
            json.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Antigravity error")
                .to_string(),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_harness_has_correct_id() {
        let harness = AntigravityHarness::new();
        assert_eq!(harness.id(), "antigravity");
        assert!(!harness.capabilities().steer);
        assert!(!harness.capabilities().persistent);
    }

    #[test]
    fn parse_step_updates() {
        let thinking =
            serde_json::json!({"type":"step_update","step_type":"thinking","content":"hmm"});
        assert!(
            matches!(parse_stream_json(&thinking), Some(HarnessEvent::ThinkingDelta(t)) if t == "hmm")
        );

        let tool = serde_json::json!({"type":"step_update","step_type":"tool_use","tool_id":"t1","tool_name":"Bash","tool_input":{"cmd":"ls"}});
        assert!(
            matches!(parse_stream_json(&tool), Some(HarnessEvent::ToolStart { tool_id, name, .. }) if tool_id == "t1" && name == "Bash")
        );
    }
}
