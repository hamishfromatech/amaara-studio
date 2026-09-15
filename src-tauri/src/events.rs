//! The only webview event channel (Phase 1).
//!
//! Rust emits `StudioEvent`s to the React UI over a Tauri custom event named
//! `"studio://event"`, carrying this enum serialized as JSON. Every later phase
//! funnels into these four categories rather than inventing new channels:
//!   Harness — agent chat / tool-streaming events (a-coder-cli contract)
//!   Render  — render-queue lifecycle + progress
//!   Sidecar — harness/render/sd sidecar status + logs
//!   Project — project create/switch/update
//!   Preview — HyperFrames preview server lifecycle (Timeline tab)
//!
//! See `ARCHITECTURE.md` §4 for the concrete request loop.

use serde::{Deserialize, Serialize};

use crate::errors::StudioError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum StudioEvent {
    Harness(HarnessEvent),
    Render(RenderEvent),
    Sidecar(SidecarEvent),
    Project(ProjectEvent),
    Preview(PreviewEvent),
    /// A studio operation failed (sidecar/harness/render/network). Carries a
    /// typed error with a suggested user action so the UI can offer a concrete
    /// next step instead of hanging silently (Phase 15 error model).
    Error(StudioError),
}

// --- Harness ---------------------------------------------------------------

/// Normalized a-coder-cli `docs/rpc.md` event surface. Other adapters (Phase 12+)
/// translate their native streams into these same variants — the UI never sees
/// harness-specific shapes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HarnessEvent {
    AgentStart {
        model: String,
    },
    AgentEnd {
        success: bool,
        #[serde(default)]
        message: Option<String>,
    },
    TextDelta(String),
    ThinkingDelta(String),
    ToolStart {
        tool_id: String,
        name: String,
        args: serde_json::Value,
    },
    ToolUpdate {
        tool_id: String,
        partial: String,
    },
    ToolEnd {
        tool_id: String,
        #[serde(default)]
        result: Option<String>,
        is_error: bool,
    },
    ApprovalRequest {
        id: String,
        kind: String,
        payload: serde_json::Value,
    },
    QueueUpdate {
        steer: bool,
        follow_up: bool,
    },
    Retry {
        attempt: u32,
        reason: String,
    },
    Error(String),
}

// --- Render ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderEvent {
    Started {
        job_id: String,
        target: String,
        quality: String,
    },
    Progress {
        job_id: String,
        stage: String,
        frame: u64,
        total_frames: Option<u64>,
    },
    Completed {
        job_id: String,
        output_path: String,
        #[serde(default)]
        duration_ms: Option<i64>,
    },
    Failed {
        job_id: String,
        error: String,
    },
    Cancelled {
        job_id: String,
    },
}

// --- Sidecar ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SidecarEvent {
    Starting {
        name: String,
    },
    Ready {
        name: String,
    },
    Exit {
        name: String,
        code: i32,
    },
    /// One line appended to a sidecar log drawer (design.md status strip).
    LogLine {
        name: String,
        level: String,
        message: String,
    },
}

// --- Project ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProjectEvent {
    Created {
        project_id: String,
        name: String,
    },
    Switched {
        project_id: String,
        name: String,
    },
    Updated {
        project_id: String,
        changed_at: i64,
    },
    AssetAdded {
        project_id: String,
        asset_id: String,
        path: String,
    },
    Deleted {
        project_id: String,
    },
}

/// HyperFrames preview-server lifecycle (Timeline tab, Phase 10). The server
/// is `npx hyperframes preview --background` bound to a loopback port; the UI
/// embeds its URL in an iframe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreviewEvent {
    /// Server is up; `url` is the iframe source, `port` the bound port.
    Started { url: String, port: u16 },
    /// Server was stopped (project switch / tab leave / stop button).
    Stopped,
    /// Server failed to start (missing Node/npx/hyperframes, port in use, …).
    Failed { error: String },
}

impl PreviewEvent {
    fn describe(&self) -> String {
        match self {
            PreviewEvent::Started { port, .. } => format!("up port={port}"),
            PreviewEvent::Stopped => "stopped".into(),
            PreviewEvent::Failed { error } => format!("fail:{error}"),
        }
    }
}

impl StudioEvent {
    /// A short human label for the status strip / debug output.
    pub fn describe(&self) -> String {
        match self {
            StudioEvent::Harness(e) => e.describe(),
            StudioEvent::Render(e) => e.describe(),
            StudioEvent::Sidecar(e) => format!("sidecar: {}", e.describe()),
            StudioEvent::Project(e) => e.describe(),
            StudioEvent::Preview(e) => format!("preview: {}", e.describe()),
            StudioEvent::Error(e) => format!("error:{}:{}", e.code.code_name(), e.message),
        }
    }
}

impl HarnessEvent {
    fn describe(&self) -> String {
        match self {
            HarnessEvent::AgentStart { model } => format!("agent:start[{model}]"),
            HarnessEvent::AgentEnd { success, .. } => format!("agent:end(success={success})"),
            HarnessEvent::TextDelta(_) => "text:delta".into(),
            HarnessEvent::ThinkingDelta(_) => "thinking:delta".into(),
            HarnessEvent::ToolStart { name, .. } => format!("tool:start[{name}]"),
            HarnessEvent::ToolUpdate { tool_id, .. } => format!("tool:update[{tool_id}]"),
            HarnessEvent::ToolEnd { is_error, .. } => format!("tool:end(error={is_error})"),
            HarnessEvent::ApprovalRequest { kind, .. } => format!("approval:{kind}"),
            HarnessEvent::QueueUpdate { steer, follow_up } => {
                format!("queue:steer={steer},follow_up={follow_up}")
            }
            HarnessEvent::Retry { attempt, reason } => format!("retry#{attempt}:{reason}"),
            HarnessEvent::Error(e) => format!("error:{e}"),
        }
    }
}

impl RenderEvent {
    fn describe(&self) -> String {
        match self {
            RenderEvent::Started { job_id, .. } => format!("render:start[{job_id}]"),
            RenderEvent::Progress {
                frame,
                total_frames,
                ..
            } => {
                if let Some(t) = total_frames {
                    format!("render:progress{frame}/{t}")
                } else {
                    "render:progress".into()
                }
            }
            RenderEvent::Completed { job_id, .. } => format!("render:done[{job_id}]"),
            RenderEvent::Failed { job_id, error } => format!("render:fail[{job_id}]{error}"),
            RenderEvent::Cancelled { job_id } => format!("render:cancel[{job_id}]"),
        }
    }
}

impl SidecarEvent {
    fn describe(&self) -> String {
        match self {
            SidecarEvent::Starting { name } => format!("{name}:starting"),
            SidecarEvent::Ready { name } => format!("{name}:ready"),
            SidecarEvent::Exit { name, code } => format!("{name}:exit({code})"),
            SidecarEvent::LogLine { message, .. } => message.clone(),
        }
    }
}

impl ProjectEvent {
    fn describe(&self) -> String {
        match self {
            ProjectEvent::Created { project_id, name } => {
                format!("project:create[{project_id}] {name}")
            }
            ProjectEvent::Switched { project_id, name } => {
                format!("project:switch[{project_id}] {name}")
            }
            ProjectEvent::Updated { changed_at, .. } => format!("project:update@{changed_at}"),
            ProjectEvent::AssetAdded {
                project_id,
                asset_id,
                ..
            } => format!("project:asset[{project_id}:{asset_id}]"),
            ProjectEvent::Deleted { project_id } => format!("project:delete[{project_id}]"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_event_round_trips_through_json() {
        let ev = StudioEvent::Harness(HarnessEvent::ToolEnd {
            tool_id: "tc1".into(),
            result: Some("ok".into()),
            is_error: false,
        });
        let json = serde_json::to_string(&ev).unwrap();
        let back: StudioEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&back).unwrap());
    }

    #[test]
    fn every_variant_serializes() {
        // Force the compiler to touch each variant; asserts all derive Serialize.
        let _ = StudioEvent::Harness(HarnessEvent::Error("x".into()));
        let _ = StudioEvent::Render(RenderEvent::Failed {
            job_id: "r1".into(),
            error: "e".into(),
        });
        let _ = StudioEvent::Sidecar(SidecarEvent::Exit {
            name: "h".into(),
            code: 1,
        });
        let _ = StudioEvent::Project(ProjectEvent::Created {
            project_id: "p".into(),
            name: "n".into(),
        });
        let _ = StudioEvent::Preview(PreviewEvent::Started {
            url: "http://127.0.0.1:3002/".to_string(),
            port: 3002,
        });
    }

    #[test]
    fn describe_is_stable() {
        assert_eq!(
            HarnessEvent::ToolStart {
                tool_id: "a".to_string(),
                name: "write".to_string(),
                args: serde_json::json!({})
            }
            .describe(),
            "tool:start[write]"
        );
        assert_eq!(
            RenderEvent::Progress {
                job_id: "j".to_string(),
                stage: "s".into(),
                frame: 5,
                total_frames: Some(10)
            }
            .describe(),
            "render:progress5/10"
        );
    }

    #[test]
    fn error_event_round_trips_through_json() {
        let ev = StudioEvent::Error(crate::errors::StudioError {
            code: crate::errors::ErrorCode::SidecarCrash,
            message: "llama-server crashed".into(),
            user_action: crate::errors::UserAction::OpenLogs,
        });
        let json = serde_json::to_string(&ev).unwrap();
        let back: StudioEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&back).unwrap());
        assert!(back.describe().starts_with("error:SidecarCrash:"));
    }
}
