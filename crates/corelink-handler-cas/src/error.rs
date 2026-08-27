//! `CasHandlerError` taxonomy.

use thiserror::Error;

/// Error taxonomy for the CAS handler surface.
///
/// All variants are reachable via `#[non_exhaustive]`; callers MUST
/// use a wildcard arm (CoreLink codex §9.3).
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CasHandlerError {
    /// Requested object does not exist for the given `(tenant, hash)`.
    #[error("cas object not found: tenant={tenant} hash={hash}")]
    NotFound {
        /// Tenant the lookup was scoped to.
        tenant: String,
        /// Content hash requested.
        hash: String,
    },

    /// The supplied content hash does not match the bytes given on
    /// write (or returned on read with verify enabled). Emits
    /// `Sli::CorrectnessCas` failure observation BEFORE returning so
    /// the zero-budget correctness SLO has a non-zero numerator on
    /// every miss.
    #[error("cas hash mismatch: claimed={claimed} actual={actual}")]
    HashMismatch {
        /// Hash the caller claimed for the bytes.
        claimed: String,
        /// Hash actually computed from the bytes (here represented
        /// in the canonical lowercase hex form).
        actual: String,
    },

    /// Caller attempted to access another tenant's object. Fail-CLOSED
    /// + audit emit BEFORE the response.
    #[error("cas cross-tenant access denied: caller={caller} requested_tenant={requested_tenant}")]
    CrossTenantDenied {
        /// Tenant the caller authenticated as.
        caller: String,
        /// Tenant the URL/path requested.
        requested_tenant: String,
    },

    /// Audit emit failed BEFORE the mutation could be applied. The
    /// fail-CLOSED ordering returns this error and the state is
    /// unchanged.
    #[error("cas audit emit failed: {0}")]
    AuditFailed(String),

    /// The stored object is larger than the read path is willing to buffer
    /// (B-051 / ADR-S34-002).
    ///
    /// The read path materialises a whole object in memory, so peak heap is
    /// `concurrent_reads x object_size`. The per-tenant permit
    /// (`CAS_READ_CONCURRENCY_LIMIT`) bounds the first factor; this bounds the
    /// second. Distinct from [`Self::Internal`] so the route can answer 413
    /// rather than 500: the request is well-formed and the object is intact —
    /// it is the SIZE that is refused, and retrying will not change that.
    #[error("cas object too large to serve: {actual_bytes} bytes exceeds the {limit_bytes}-byte read ceiling")]
    ObjectTooLarge {
        /// Size of the stored object, from the storage layer's content length.
        actual_bytes: u64,
        /// The ceiling that refused it.
        limit_bytes: u64,
    },

    /// Internal state-machine inconsistency (lock poisoning, etc.).
    #[error("internal cas-handler error: {0}")]
    Internal(String),
}
