//! Error taxonomy for `corelink-turbo-bridge`.

use thiserror::Error;

/// Errors returned by [`crate::handler::TurboArtifactHandler`] implementations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TurboBridgeError {
    /// The requested artifact was not found in the store.
    #[error("artifact not found: hash={hash:?}")]
    NotFound {
        /// The hash that was requested but not present.
        hash: String,
    },

    /// The `team_id` in the request does not match the authenticated
    /// `caller_tenant`.  Emitting a cross-tenant audit event and returning
    /// this error is mandatory BEFORE any storage access.
    #[error("cross-tenant denied: caller={caller:?} requested team_id={requested_team_id:?}")]
    CrossTenantDenied {
        /// The authenticated caller's tenant.
        caller: String,
        /// The `team_id` the caller supplied in the request.
        requested_team_id: String,
    },

    /// The hash string exceeds [`crate::MAX_HASH_LEN`] characters.
    #[error("artifact hash too long: len={len} max={max}")]
    HashTooLong {
        /// Actual length of the supplied hash string.
        len: usize,
        /// Maximum accepted length.
        max: usize,
    },

    /// An audit emit failed; the operation was aborted without mutating state
    /// (fail-CLOSED ordering).
    #[error("audit sink failed: {0}")]
    AuditFailed(String),

    /// Internal storage error (lock poisoned, I/O failure, etc.).
    #[error("internal error: {0}")]
    Internal(String),
}
