//! `CasHandlerError` taxonomy.

use thiserror::Error;
use uuid::Uuid;

/// What the write implementation can prove about a failed mutation.
///
/// Decorators that reserve quota before delegating a write use this outcome to
/// decide whether a reservation may be released.  Only [`Self::NotWritten`]
/// permits that release.  [`Self::Committed`] and [`Self::Pending`] retain the
/// liability: the former is already durable, while the latter requires durable
/// reconciliation before its effect can be determined. [`Self::Unknown`] is a
/// legacy compatibility result and may not settle a reservation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MutationEffect {
    /// The implementation proved no durable storage mutation occurred.
    NotWritten,
    /// The storage mutation committed, although a later operation failed.
    Committed,
    /// Storage acknowledgement or compensation is ambiguous and is owned by a
    /// durable reconciliation intent.
    Pending {
        /// Identifier of the reconciliation intent that owns the ambiguity.
        intent_id: Uuid,
    },
    /// A legacy implementation returned an error without reporting a durable
    /// effect or persisting a reconciliation intent.
    Unknown,
}

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

/// A CAS write error together with the proved durable-mutation effect.
///
/// This is additive to [`CasHandlerError`]: callers that only need the legacy
/// error may continue using [`crate::handler::CasWriteHandler::write`], while
/// accounting and storage decorators call the effect-aware companion method.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("{cause}")]
#[non_exhaustive]
pub struct CasWriteFailure {
    /// Original handler failure for HTTP/error mapping.
    #[source]
    pub cause: CasHandlerError,
    /// Durable effect proved by the implementation.
    pub effect: MutationEffect,
}

impl CasWriteFailure {
    /// Construct a failure that proved no storage write occurred.
    #[must_use]
    pub fn not_written(cause: CasHandlerError) -> Self {
        Self {
            cause,
            effect: MutationEffect::NotWritten,
        }
    }

    /// Construct a failure after the storage write durably committed.
    #[must_use]
    pub fn committed(cause: CasHandlerError) -> Self {
        Self {
            cause,
            effect: MutationEffect::Committed,
        }
    }

    /// Construct a failure whose storage effect needs reconciliation.
    #[must_use]
    pub fn pending(cause: CasHandlerError, intent_id: Uuid) -> Self {
        Self {
            cause,
            effect: MutationEffect::Pending { intent_id },
        }
    }

    /// Construct a legacy failure without a proved durable-mutation effect.
    #[must_use]
    pub fn unknown(cause: CasHandlerError) -> Self {
        Self {
            cause,
            effect: MutationEffect::Unknown,
        }
    }
}
