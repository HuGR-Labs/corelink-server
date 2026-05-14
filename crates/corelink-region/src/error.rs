//! Error taxonomy for `corelink-region` — WI-S14-001.

use thiserror::Error;

/// Error taxonomy for region operations.
///
/// `#[non_exhaustive]` per project convention (new variants added without
/// breaking downstream match arms).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RegionError {
    /// Unknown region identifier supplied.
    #[error("unknown region: {0}")]
    UnknownRegion(String),

    /// DO jurisdiction validation failed (CRITICAL: Schrems II risk).
    #[error("DO jurisdiction mismatch for region {region}: expected {expected}, got {actual}")]
    JurisdictionMismatch {
        /// Region name.
        region: String,
        /// Expected jurisdiction.
        expected: String,
        /// Actual jurisdiction from Cloudflare API.
        actual: String,
    },

    /// Audit sink failed — fail-CLOSED per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER.
    #[error("audit sink failed: {0}")]
    AuditFailed(String),

    /// Migration step failed.
    #[error("migration step failed for tenant {tenant_id}: {reason}")]
    MigrationStepFailed {
        /// Tenant ID that failed.
        tenant_id: String,
        /// Reason for failure.
        reason: String,
    },

    /// Rollback path failed.
    #[error("rollback failed: {0}")]
    RollbackFailed(String),

    /// R2 location verification failed — actual location does not match hint.
    #[error("R2 location mismatch for bucket {bucket}: expected hint {expected}, actual {actual}")]
    R2LocationMismatch {
        /// Bucket name.
        bucket: String,
        /// Expected hint.
        expected: String,
        /// Actual reported location.
        actual: String,
    },

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
}
