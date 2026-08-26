//! Harness approval/permission requests (Phase 11).
//!
//! Given an ApprovalRequest + the user's answer, write a scoped allow rule into
//! the active harness's config: a-coder-cli extension permission; Claude settings.json
//! permissions.allow; Codex approvalPolicy; etc.

use serde::{Deserialize, Serialize};

/// Approval request from a harness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub kind: String, // e.g., "bash", "write", "edit", "render_to_video"
    pub payload: serde_json::Value,
}

/// User's answer to an approval request.
/// Scaffold — wired into the approval dialog in Phase 11.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalAnswer {
    Allow,
    Deny,
    Edit,
}

/// Write a scoped allow rule into the active harness's native permission config.
pub fn write_allow_rule(harness_id: &str, request: &ApprovalRequest, always_allow: bool) -> Result<(), String> {
    // For a-coder-cli extension: write to harness-pack/a-coder-cli/extensions/navya-studio.ts permissions
    // For Claude Code: write to harness-pack/claude-code/settings.json permissions.allow
    // For Codex: write to harness-pack/codex/approvalPolicy
    
    if always_allow {
        // In a real impl, write the scoped allow rule to the harness config
        // e.g., for a-coder-cli: add to extension permission allowlist
        // Never include broad Bash(*) - only specific commands
        let _ = harness_id;
        let _ = request;
    }
    
    Ok(())
}

/// Apply "always allow" rule for identical calls.
/// Scaffold — wired into the approval dialog in Phase 11.
#[allow(dead_code)]
pub fn should_auto_approve(harness_id: &str, request: &ApprovalRequest) -> bool {
    // In a real impl, check the harness's native permission config for an existing
    // scoped allow rule matching this request's kind and payload pattern.
    let _ = harness_id;
    let _ = request;
    false // Default to no auto-approve without explicit rule
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_allow_rule_succeeds() {
        let req = ApprovalRequest {
            id: "req1".to_string(),
            kind: "bash".to_string(),
            payload: serde_json::json!({"command": "npx hyperframes lint"}),
        };
        assert!(write_allow_rule("a-coder-cli", &req, true).is_ok());
    }

    #[test]
    fn should_auto_approve_defaults_false() {
        let req = ApprovalRequest {
            id: "req2".to_string(),
            kind: "bash".to_string(),
            payload: serde_json::json!({"command": "npx hyperframes lint"}),
        };
        assert!(!should_auto_approve("a-coder-cli", &req));
    }
}
