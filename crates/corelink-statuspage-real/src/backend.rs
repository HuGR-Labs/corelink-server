//! Canonical [`StatuspageBackend`] trait.
//!
//! Every implementation — production HTTP wiring + in-memory fake +
//! any future provider variant — satisfies the same trait surface so
//! the privacy-erasure-worker publish job is structurally
//! provider-agnostic.

use thiserror::Error;

use crate::audit::StatuspageAuditError;
use crate::report::DsrCompletionReport;

/// Successful publish outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishOutcome {
    /// Statuspage page ID that received the data-point.
    pub page_id: String,
    /// Statuspage metric ID that received the data-point.
    pub metric_id: String,
    /// Final HTTP status (always 2xx on success).
    pub status: u16,
    /// Number of attempts taken (1 = first-shot).
    pub attempts: u32,
}

/// Top-level error type.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum StatuspageClientError {
    /// Local rate-limiter denied the publish. The caller MUST wait
    /// `retry_after_ms` before retrying. Audit emit already fired with
    /// outcome `RateLimited`.
    #[error("statuspage rate-limited: retry after {retry_after_ms} ms (jitter ≤ {jitter_ms})")]
    RateLimited {
        /// Wait at least this many ms before retrying.
        retry_after_ms: u64,
        /// Suggested jitter ceiling.
        jitter_ms: u64,
    },
    /// Statuspage rejected the API key. Audit emit already fired with
    /// outcome `AuthFailed`.
    #[error("statuspage auth failed (status {status})")]
    AuthFailed {
        /// HTTP status returned by Statuspage (401 / 403).
        status: u16,
    },
    /// Transport failure exhausted retries.
    #[error("statuspage transport failure after {attempts} attempts: {reason}")]
    TransportExhausted {
        /// Attempts made.
        attempts: u32,
        /// Final reason string.
        reason: String,
    },
    /// Permanent (non-retryable) rejection other than auth.
    #[error("statuspage rejected (status {status}): {reason}")]
    PermanentReject {
        /// HTTP status.
        status: u16,
        /// Reason / body excerpt.
        reason: String,
    },
    /// Audit emit failed — caller MUST treat the dispatch as a no-op.
    #[error("statuspage audit emit failed: {0}")]
    AuditFailed(#[from] StatuspageAuditError),
}

/// Canonical Statuspage backend trait.
pub trait StatuspageBackend: core::fmt::Debug + Send + Sync {
    /// Publish a 24h-rolling DSR completion stats data-point to the
    /// configured Statuspage page + metric.
    ///
    /// # Errors
    ///
    /// Returns any [`StatuspageClientError`] variant. The audit
    /// envelope is fail-CLOSED: if audit emit fails the error is
    /// surfaced via [`StatuspageClientError::AuditFailed`] and the
    /// caller MUST NOT treat the publish as successful.
    fn publish_dsr_metric(
        &self,
        report: &DsrCompletionReport,
        now_epoch_ms: u64,
    ) -> Result<PublishOutcome, StatuspageClientError>;
}
