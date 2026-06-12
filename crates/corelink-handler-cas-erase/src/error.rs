//! `CasEraseError` taxonomy for the per-hash CAS erase surface.

use thiserror::Error;

/// Error taxonomy for the CAS erase handler. `#[non_exhaustive]`.
///
/// Maps to HTTP status codes at the container wiring layer
/// (`routes::cas_erase`):
///
/// - [`CasEraseError::CrossTenantDenied`] → 403 Forbidden.
/// - [`CasEraseError::InvalidDigest`] → 400 Bad Request.
/// - [`CasEraseError::Transport`] → 500 Internal Server Error (the underlying
///   R2/D1 detail is logged, never echoed to the caller).
///
/// A *successful* erase — including an idempotent re-erase of an already-erased
/// hash — is NOT an error; it is reported via
/// [`crate::handler::EraseOutcome`].
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CasEraseError {
    /// The caller's authenticated tenant does not match the `:tenant` in the
    /// erase request path. Fail-CLOSED BEFORE any storage access — mirrors the
    /// read/write CAS routes' cross-tenant guard.
    #[error("cas-erase cross-tenant denied: caller={caller} requested_tenant={requested_tenant}")]
    CrossTenantDenied {
        /// Caller's authenticated tenant.
        caller: String,
        /// Tenant in the URL/path.
        requested_tenant: String,
    },

    /// The supplied content digest is not a well-formed CAS hash. The erase is
    /// refused BEFORE addressing storage so a malformed key can never widen a
    /// LIST/DELETE prefix.
    #[error("cas-erase invalid digest: {digest}")]
    InvalidDigest {
        /// The rejected digest (already validated to be log-safe by the
        /// [`crate::handler::validate_digest`] charset check).
        digest: String,
    },

    /// The underlying R2 delete or D1 tombstone transport failed; state may be
    /// partially mutated. The caller MUST treat this as a retryable failure —
    /// the operation is idempotent, so a retry is safe.
    #[error("cas-erase transport error: {0}")]
    Transport(String),
}
