//! Error types for WI-S13-004 drift consumer.

use thiserror::Error;

/// Canonical error taxonomy for drift consumer operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DriftConsumerError {
    /// Region is not in the canonical REGIONS list.
    #[error("invalid region: {0:?}")]
    InvalidRegion(String),

    /// Terraform exit code is unrecognised (not 0, 1, or 2).
    #[error("unrecognised terraform exit code: {0}")]
    UnrecognisedExitCode(i32),

    /// Audit emit failed (fail-CLOSED: blocks state mutation).
    #[error("audit emit failed: {0}")]
    AuditFailed(String),

    /// Store insert failed.
    #[error("store insert failed: {0}")]
    StoreFailed(String),

    /// Finding not found for given ID.
    #[error("finding not found: {0}")]
    NotFound(String),

    /// Attempted an operation that is security-forbidden (e.g. auto-apply).
    #[error("security violation — forbidden operation: {0}")]
    ForbiddenOperation(String),
}

/// Store-layer errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DriftStoreError {
    /// Attempted to overwrite an immutable field (append-only violation).
    #[error("append-only violation: attempted to mutate immutable field {0:?}")]
    AppendOnlyViolation(String),

    /// Internal store error.
    #[error("internal store error: {0}")]
    Internal(String),
}
