//! Harness approval/permission requests (Phase 11).
//!
//! Given an ApprovalRequest + the user's answer, write a scoped allow rule into
//! the active harness's config: a-coder-cli extension permission; Claude settings.json
//! permissions.allow; Codex approvalPolicy; etc.

use serde::{Deserialize, Serialize};
use std::path::Path;

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
///
/// Implemented for Claude Code (appends the tool name to the project's
/// `.claude/settings.json` `permissions.allow`). Harnesses whose policy file we
/// don't manage return an error — callers surface it, but the user's answer is
/// still relayed to the harness (persistence must never block an approval).
pub fn write_allow_rule(
    harness_id: &str,
    project_dir: &Path,
    request: &ApprovalRequest,
    always_allow: bool,
) -> Result<(), String> {
    if !always_allow {
        return Ok(());
    }
    match harness_id {
        "claude-code" => write_claude_allow(project_dir, request),
        // a-coder-cli enforces permissions inside its extension; codex, hermes
        // and openclaw have no writable policy file we manage yet.
        other => Err(format!(
            "\"always allow\" persistence is not implemented for {other}; you'll be asked again next time"
        )),
    }
}

/// Append the requested tool as a coarse allow rule to the project's Claude
/// settings.json (`permissions.allow: ["Bash", ...]`). The dialog promises
/// "Always allow this {kind} request" — the coarse tool-name rule is the
/// closest native Claude Code equivalent.
fn write_claude_allow(project_dir: &Path, request: &ApprovalRequest) -> Result<(), String> {
    let payload = request
        .payload
        .as_object()
        .ok_or("approval payload is not an object")?;
    let tool = payload
        .get("name")
        .or_else(|| payload.get("tool"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("approval payload has no tool name")?;

    let settings_path = project_dir.join(".claude").join("settings.json");
    let mut settings: serde_json::Value = if settings_path.exists() {
        let raw = std::fs::read_to_string(&settings_path)
            .map_err(|e| format!("reading {}: {e}", settings_path.display()))?;
        serde_json::from_str(&raw)
            .map_err(|e| format!("parsing {}: {e}", settings_path.display()))?
    } else {
        serde_json::json!({})
    };

    let obj = settings
        .as_object_mut()
        .ok_or("settings.json is not an object")?;
    let perms = obj
        .entry("permissions")
        .or_insert_with(|| serde_json::json!({}));
    let perms_obj = perms
        .as_object_mut()
        .ok_or("permissions is not an object")?;
    let allow = perms_obj
        .entry("allow")
        .or_insert_with(|| serde_json::json!([]));
    let allow_arr = allow
        .as_array_mut()
        .ok_or("permissions.allow is not an array")?;
    if !allow_arr.iter().any(|v| v.as_str() == Some(tool)) {
        allow_arr.push(serde_json::json!(tool));
    }

    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).unwrap_or_else(|_| "{}".to_string()),
    )
    .map_err(|e| format!("writing {}: {e}", settings_path.display()))?;
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
        // always_allow=false is always a no-op success.
        assert!(write_allow_rule("a-coder-cli", std::path::Path::new("."), &req, false).is_ok());
        // Harnesses without a writable policy honestly say so instead of
        // silently pretending the rule was persisted.
        let err =
            write_allow_rule("a-coder-cli", std::path::Path::new("."), &req, true).unwrap_err();
        assert!(err.contains("a-coder-cli") && err.contains("not implemented"));
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

    #[test]
    fn claude_allow_rule_persists_tool_name() {
        let dir = std::env::temp_dir().join(format!("amaara-allow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let req = ApprovalRequest {
            id: "r1".to_string(),
            kind: "bash".to_string(),
            payload: serde_json::json!({"id": "toolu_1", "name": "Bash", "input": {"command": "ls"}}),
        };
        write_allow_rule("claude-code", &dir, &req, true).expect("rule write");
        let raw = std::fs::read_to_string(dir.join(".claude").join("settings.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["permissions"]["allow"][0], "Bash");
        // A second allow of the same tool must not duplicate the entry.
        write_allow_rule("claude-code", &dir, &req, true).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join(".claude").join("settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(v["permissions"]["allow"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn always_allow_false_is_a_noop() {
        let dir = std::env::temp_dir().join(format!("amaara-allow-noop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let req = ApprovalRequest {
            id: "r2".to_string(),
            kind: "bash".to_string(),
            payload: serde_json::json!({"name": "Bash"}),
        };
        write_allow_rule("claude-code", &dir, &req, false).unwrap();
        assert!(!dir.join(".claude").join("settings.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_harness_reports_not_implemented() {
        let dir = std::env::temp_dir().join(format!("amaara-allow-unsup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let req = ApprovalRequest {
            id: "r3".to_string(),
            kind: "bash".to_string(),
            payload: serde_json::json!({"name": "Bash"}),
        };
        let err = write_allow_rule("codex", &dir, &req, true).unwrap_err();
        assert!(err.contains("codex"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
