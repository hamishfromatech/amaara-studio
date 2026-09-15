//! Harness events (Phase 4).
//!
//! Normalized event surface from any harness adapter. The UI renders these
//! exclusively — it never sees harness-specific shapes.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: Option<String>,
    #[serde(rename = "kind")]
    pub kind: String, // chat|image|video|...
}

impl HarnessEvent {
    /// A short human label for the status strip / debug output.
    pub fn describe(&self) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_event_serializes() {
        let ev = HarnessEvent::ToolEnd {
            tool_id: "tc1".into(),
            result: Some("ok".into()),
            is_error: false,
        };
        let json = serde_json::to_string(&ev).unwrap();
        let back: HarnessEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&back).unwrap());
    }

    #[test]
    fn describe_is_stable() {
        assert_eq!(
            HarnessEvent::ToolStart {
                tool_id: "a".into(),
                name: "write".into(),
                args: serde_json::json!({})
            }
            .describe(),
            "tool:start[write]"
        );
    }
}
