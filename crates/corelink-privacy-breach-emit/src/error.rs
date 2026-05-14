//! Error taxonomy for `corelink-privacy-breach-emit`.
//!
//! All public error enums are `#[non_exhaustive]` per the canonical
//! Lote 10.9-quinquies NEW-P0-2 pattern (mirrors `ErasureWorkerError`,
//! `DsrRequestError`, `BillingEmitError`).

use thiserror::Error;

/// Errors from the [`crate::audit_emit::BreachAuditSink`] trait.
///
/// `#[non_exhaustive]` reserves additive growth for follow-on WIs
/// (e.g., a future SIEM-specific sink error variant).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BreachAuditSinkError {
    /// Backend storage failure (R2 put failed, mutex poisoned, etc.).
    #[error("breach audit sink store error: {0}")]
    Store(String),
}

/// Top-level errors for the breach emit pipeline.
///
/// `#[non_exhaustive]` reserves additive growth.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BreachEmitError {
    /// The [`crate::audit_emit::BreachAuditSink`] rejected the emit.
    /// Per WI-S11-006 §9.3 DD: dispatch proceeds despite this error;
    /// SEV-1 alert fires; manual re-emit queued.
    #[error("breach audit emit failed: {0}")]
    AuditEmit(#[from] BreachAuditSinkError),

    /// Payload serialization error (serde_json).
    #[error("breach event serialization failed: {0}")]
    Serialization(String),

    /// Configuration error (missing breach_id, empty jurisdictions for SEV-1, etc.).
    #[error("breach emit config error: {0}")]
    Config(String),
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn audit_sink_error_display() {
        let e = BreachAuditSinkError::Store("R2 put failed".to_string());
        let s = e.to_string();
        assert!(s.contains("R2 put failed"));
    }

    #[test]
    fn breach_emit_error_from_audit_sink() {
        let inner = BreachAuditSinkError::Store("mutex poisoned".to_string());
        let outer: BreachEmitError = inner.into();
        let s = outer.to_string();
        assert!(s.contains("breach audit emit failed"));
    }
}
