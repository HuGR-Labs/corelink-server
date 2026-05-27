//! API-layer error taxonomy for the config admin handlers (WI-S13-001).

use thiserror::Error;

use corelink_config_do::ConfigError;

/// HTTP API errors for the config admin endpoints.
///
/// Mapped to HTTP status codes by the handler layer.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ApiError {
    /// Admin role check failed (HTTP 403).
    #[error("admin role required")]
    NotAdmin,

    /// MFA session too old or implausibly future (HTTP 401).
    #[error("MFA stale: mfa_ts={mfa_ts_ms}ms, now={now_ms}ms, window={window_ms}ms")]
    MfaStale {
        /// MFA completion timestamp (Unix ms).
        mfa_ts_ms: u64,
        /// Request handling timestamp (Unix ms).
        now_ms: u64,
        /// Freshness window (ms).
        window_ms: u64,
    },

    /// Rollback requires dual-approval header (HTTP 403).
    #[error("dual approval required for rollback (X-Dual-Approver missing)")]
    DualApprovalMissing,

    /// Config store error (HTTP 409/410/400/500 depending on variant).
    #[error("config store error: {0}")]
    Config(#[from] ConfigError),

    /// Request body deserialization failed (HTTP 400).
    #[error("invalid request body: {0}")]
    BadRequest(String),
}

impl ApiError {
    /// HTTP status code for this error.
    #[must_use]
    pub fn status_code(&self) -> u16 {
        match self {
            Self::NotAdmin | Self::DualApprovalMissing => 403,
            Self::MfaStale { .. } => 401,
            Self::BadRequest(_) => 400,
            Self::Config(ConfigError::VersionConflict { .. }) => 409,
            Self::Config(ConfigError::SchemaInvalid(_)) => 400,
            Self::Config(ConfigError::VersionExpired(_)) => 410,
            Self::Config(ConfigError::VersionUnknown(_)) => 404,
            Self::Config(_) => 500,
        }
    }
}
