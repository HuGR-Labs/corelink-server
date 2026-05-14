//! Error taxonomy for the DO config-singleton (WI-S13-001).

use thiserror::Error;

/// All errors returned by [`crate::ConfigSingletonStore`].
///
/// `#[non_exhaustive]` ensures new variants do not break existing
/// exhaustive match arms in downstream crates.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// CAS version mismatch — caller should fetch current and retry.
    #[error("version conflict (expected {expected}, current {current})")]
    VersionConflict {
        /// Version the caller claimed was current.
        expected: u64,
        /// Actual current version.
        current: u64,
    },

    /// Payload failed schema or domain validation.
    #[error("schema validation failed: {0}")]
    SchemaInvalid(String),

    /// Rollback target version not found in the 90d retention window.
    #[error("rollback target version {0} not in 90d retention window")]
    VersionExpired(u64),

    /// Rollback target version does not exist in history.
    #[error("rollback target version {0} unknown")]
    VersionUnknown(u64),

    /// Propagation to edge workers did not complete within SLO.
    #[error("propagation timeout (>{0}s)")]
    PropagationTimeout(u32),

    /// Storage backend returned an error.
    #[error("storage backend error: {0}")]
    Backend(String),

    /// JSON serialization / deserialization failure.
    #[error("serialization error: {0}")]
    Serialization(String),
}
