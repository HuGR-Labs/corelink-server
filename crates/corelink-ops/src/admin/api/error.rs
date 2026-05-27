//! Admin API error taxonomy (WI-S13-002).

use corelink_dual_approval::DualApprovalError;

/// Admin API handler error — wraps dual-approval + dispatch errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AdminApiError {
    /// Dual-approval gate rejection (maps to HTTP 403 or 401).
    #[error("dual-approval rejected: {0}")]
    DualApprovalRejected(#[from] DualApprovalError),

    /// Schema validation failed (malformed request body).
    #[error("schema validation error: {0}")]
    SchemaValidation(String),

    /// Op dispatch failed (downstream error from op handler).
    #[error("op dispatch error: {0}")]
    DispatchError(String),

    /// Audit emission failed (fail-CLOSED: op not executed).
    #[error("audit emission failure (fail-closed): {0}")]
    AuditFailure(String),
}

impl AdminApiError {
    /// HTTP status code for this error.
    pub fn http_status(&self) -> u16 {
        match self {
            Self::DualApprovalRejected(DualApprovalError::MfaStale { .. }) => 401,
            Self::DualApprovalRejected(_) => 403,
            Self::SchemaValidation(_) => 400,
            Self::AuditFailure(_) => 503,
            Self::DispatchError(_) => 500,
        }
    }
}
