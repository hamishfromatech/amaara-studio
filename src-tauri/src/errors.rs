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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
