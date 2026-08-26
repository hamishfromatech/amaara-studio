//! Error model & telemetry (Phase 15).
//!
//! Every sidecar/harness/network failure → a typed StudioEvent::Error with a user action
//! (retry / open logs / check settings), never a silent hang. Define error codes in this module.

use serde::{Deserialize, Serialize};

/// Error codes for studio operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorCode {
    /// Sidecar process failed to start or crashed.
    SidecarCrash,
    /// Harness adapter connection lost or timed out.
    HarnessTimeout,
    /// Control server HTTP request failed.
    ControlServerError,
    /// Render sidecar job failed or was cancelled.
    RenderFailed,
    /// sd-server health probe failed.
    SdServerUnavailable,
    /// llama-server health probe failed.
    LlamaServerUnavailable,
    /// Navya Cloud API error (rate limit, auth failure, etc.).
    NavyaApiError,
}

impl ErrorCode {
    /// Machine-readable identifier for the error channel / logs.
    pub fn code_name(&self) -> &'static str {
        match self {
            ErrorCode::SidecarCrash => "SidecarCrash",
            ErrorCode::HarnessTimeout => "HarnessTimeout",
            ErrorCode::ControlServerError => "ControlServerError",
            ErrorCode::RenderFailed => "RenderFailed",
            ErrorCode::SdServerUnavailable => "SdServerUnavailable",
            ErrorCode::LlamaServerUnavailable => "LlamaServerUnavailable",
            ErrorCode::NavyaApiError => "NavyaApiError",
        }
    }

    /// Human-readable description for status strips / debug output.
    pub fn label(&self) -> &'static str {
        match self {
            ErrorCode::SidecarCrash => "sidecar crashed",
            ErrorCode::HarnessTimeout => "harness timed out",
            ErrorCode::ControlServerError => "control server error",
            ErrorCode::RenderFailed => "render failed",
            ErrorCode::SdServerUnavailable => "sd-server unavailable",
            ErrorCode::LlamaServerUnavailable => "llama-server unavailable",
            ErrorCode::NavyaApiError => "navya api error",
        }
    }
}

impl From<String> for StudioError {
    fn from(message: String) -> Self {
        StudioError {
            code: ErrorCode::SidecarCrash,
            message,
            user_action: UserAction::CheckSettings,
        }
    }
}

impl From<&str> for StudioError {
    fn from(message: &str) -> Self {
        StudioError::from(message.to_string())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioError {
    pub code: ErrorCode,
    pub message: String,
    pub user_action: UserAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserAction {
    Retry,
    OpenLogs,
    CheckSettings,
    ContactSupport,
}

impl StudioError {
    /// Create a retryable error.
    pub fn retryable(code: ErrorCode, message: String) -> Self {
        StudioError {
            code,
            message,
            user_action: UserAction::Retry,
        }
    }

    /// Create an error requiring logs inspection.
    pub fn with_logs(code: ErrorCode, message: String) -> Self {
        StudioError {
            code,
            message,
            user_action: UserAction::OpenLogs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_error_has_retry_action() {
        let err = StudioError::retryable(ErrorCode::HarnessTimeout, "Connection lost".to_string());
        assert!(matches!(err.user_action, UserAction::Retry));
    }

    #[test]
    fn with_logs_error_has_open_logs_action() {
        let err = StudioError::with_logs(ErrorCode::SidecarCrash, "Process crashed".to_string());
        assert!(matches!(err.user_action, UserAction::OpenLogs));
    }

    #[test]
    fn error_codes_have_stable_identifiers() {
        assert_eq!(ErrorCode::SidecarCrash.code_name(), "SidecarCrash");
        assert_eq!(ErrorCode::NavyaApiError.label(), "navya api error");
    }

    #[test]
    fn from_string_defaults_to_sidecar_crash() {
        let err: StudioError = "boom".into();
        assert_eq!(err.code, ErrorCode::SidecarCrash);
        assert_eq!(err.message, "boom");
    }

    #[test]
    fn studio_error_serializes_with_code_and_action() {
        let err = StudioError::retryable(ErrorCode::HarnessTimeout, "no answer".to_string());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("HarnessTimeout"));
        assert!(json.contains("Retry"));
        let back: StudioError = serde_json::from_str(&json).unwrap();
        assert_eq!(back.code, ErrorCode::HarnessTimeout);
        assert_eq!(back.user_action, UserAction::Retry);
    }
}
