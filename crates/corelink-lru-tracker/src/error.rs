//! Canonical [`LruError`] taxonomy surfaced by every fallible API in
//! the crate.
//!
//! The taxonomy is `#[non_exhaustive]` per S-04 / S-05 / S-06 / S-07
//! lesson — the variant set grows additively across S-07 follow-on WIs
//! without breaking downstream callers.

use thiserror::Error;

use corelink_eviction::BlobMetaError;

use crate::audit::LruAuditSinkError;
use crate::metrics::LruMetricsObserverError;

/// Canonical errors surfaced by [`crate::tracker::LruTracker`] +
/// [`crate::tracker::InMemoryLruTracker`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LruError {
    /// Audit sink error. Fail-closed envelope: caller MUST surface this
    /// as 503 (or equivalent) to preserve the `audit_emit ⇔ flush`
    /// atomicity contract (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). The
    /// flush rolls back the per-batch UPDATE on this error per WI §6.1
    /// fail-closed envelope.
    #[error("lru audit sink error: {0}")]
    Audit(#[from] LruAuditSinkError),

    /// Metrics emit failure. Caller may downgrade to log-and-continue
    /// per the production wiring (S-09 metrics fan-out is sync but rare).
    #[error("lru metrics observer error: {0}")]
    Metrics(#[from] LruMetricsObserverError),

    /// `blob_meta` backend error surfaced via
    /// `corelink-eviction::blob_meta::BlobMetaError` from the conditional
    /// monotone UPDATE at flush.
    #[error("blob_meta store error: {0}")]
    BlobMeta(#[from] BlobMetaError),

    /// Programmer wiring error — config invariants violated at
    /// construction (caught by `LruConfig::with_overrides`).
    #[error("invalid lru config: {reason}")]
    InvalidConfig {
        /// Short reason code.
        reason: &'static str,
    },

    /// Backend transport failure.
    #[error("lru backend error: {0}")]
    Backend(String),
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn invalid_config_format_carries_reason() {
        let e = LruError::InvalidConfig {
            reason: "queue_size_max == 0",
        };
        let s = format!("{e}");
        assert!(s.contains("queue_size_max == 0"));
    }

    #[test]
    fn from_audit_sink_error_round_trips() {
        let inner = LruAuditSinkError::Store("inner".to_string());
        let outer: LruError = inner.into();
        assert!(matches!(outer, LruError::Audit(_)));
    }

    #[test]
    fn from_metrics_observer_error_round_trips() {
        let inner = LruMetricsObserverError::Backend("inner".to_string());
        let outer: LruError = inner.into();
        assert!(matches!(outer, LruError::Metrics(_)));
    }

    #[test]
    fn from_blob_meta_error_round_trips() {
        let inner = BlobMetaError::Backend("inner".to_string());
        let outer: LruError = inner.into();
        assert!(matches!(outer, LruError::BlobMeta(_)));
    }
}
