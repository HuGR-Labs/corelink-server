//! [`ConsentLedgerError`] — canonical error taxonomy for the Consent Ledger.

use thiserror::Error;

/// Canonical error taxonomy for the Consent Ledger (WI-S11-003).
///
/// `#[non_exhaustive]` ensures callers handle future arms gracefully
/// (per S-08 P1-1 lesson).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConsentLedgerError {
    /// Audit emit failed (fail-CLOSED; state was NOT mutated).
    #[error("audit emit failed: {0}")]
    Audit(String),

    /// Consent store operation failed.
    #[error("store error: {0}")]
    Store(String),

    /// Cascade sink failed to enqueue.
    #[error("cascade sink error: {0}")]
    Cascade(String),

    /// HMAC signing or verification failed.
    #[error("hmac error: {0}")]
    Hmac(String),

    /// Locale mismatch — Accept-Language does not match payload.locale
    /// (CTRL-PRIV-CONSENT-005 strict enforcement → 422).
    #[error("locale mismatch: {0}")]
    LocaleMismatch(String),

    /// Purpose is not revocable (basis ≠ consent).
    #[error("purpose not revocable: {0}")]
    NotRevocable(String),

    /// Notice version stale — major bump detected; re-consent required.
    #[error("notice version stale: current={current}, submitted={submitted}")]
    NoticeVersionStale {
        /// Current canonical major version.
        current: u64,
        /// Major version in the submitted proof.
        submitted: u64,
    },

    /// Record not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Invalid input (generic).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Internal unexpected error.
    #[error("internal: {0}")]
    Internal(String),
}
