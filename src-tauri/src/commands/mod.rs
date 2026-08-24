//! Tauri commands for the UI (Phases 6, 10, 14).
//!
//! Each `#[tauri::command]` is the IPC entry point that the UI calls via
//! `invoke()`. Commands call into the harness, sidecar, navya, render, etc.
//! modules — they should not contain tool logic; that's in navya-tools.

pub mod timeline;

use serde::{Deserialize, Serialize};

/// Standard command response wrapping a value with optional error context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CmdResponse<T> {
    pub ok: bool,
    pub value: Option<T>,
    pub error: Option<String>,
}

impl<T> CmdResponse<T> {
    pub fn ok(value: T) -> Self {
        CmdResponse { ok: true, value: Some(value), error: None }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        CmdResponse { ok: false, value: None, error: Some(msg.into()) }
    }
}

/// Generic empty value for commands that don't return data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitValue;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_response_ok_wraps_value() {
        let r: CmdResponse<u32> = CmdResponse::ok(42);
        assert!(r.ok);
        assert_eq!(r.value, Some(42));
        assert_eq!(r.error, None);
    }

    #[test]
    fn cmd_response_err_has_no_value() {
        let r: CmdResponse<u32> = CmdResponse::err("oops");
        assert!(!r.ok);
        assert_eq!(r.value, None);
        assert_eq!(r.error, Some("oops".to_string()));
    }
}