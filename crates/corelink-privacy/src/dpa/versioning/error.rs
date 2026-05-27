//! Error taxonomy for the DPA versioning lifecycle.

use thiserror::Error;

/// Closed taxonomy of DPA versioning errors.
///
/// `#[non_exhaustive]` per CoreLink anti-pattern policy.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum DpaVersioningError {
    /// `new <= old` — regression or no-op publish.
    #[error("dpa version regression or no-op: old={old} new={new}")]
    InvalidBump {
        /// Old version rendered.
        old: String,
        /// New version rendered.
        new: String,
    },

    /// Re-acceptance attempted with a version that is not the latest
    /// published. Latest-wins canonical (version-skip prevention).
    #[error("re-acceptance version mismatch: presented={presented} latest={latest}")]
    VersionMismatch {
        /// Version the tenant submitted.
        presented: String,
        /// Latest published version.
        latest: String,
    },

    /// Re-acceptance attempted but no pending bump for this tenant.
    #[error("no pending re-acceptance for tenant")]
    NoPendingReacceptance,

    /// Store layer failed (D1 mirror surface).
    #[error("dpa store error: {0}")]
    Store(String),

    /// Email broadcast sink failed (SES mirror surface).
    #[error("dpa broadcast error: {0}")]
    Broadcast(String),

    /// Concurrent-modification / lock contention.
    #[error("dpa state lock poisoned")]
    LockPoisoned,
}
