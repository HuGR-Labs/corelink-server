//! Error taxonomy for the metadata store.
//!
//! Each variant carries a stable canonical string code aligned with the
//! `error_taxonomy.md` registry so the upstream REAPI handler (WI-S01-005)
//! can map directly to gRPC status codes without any per-call switch.

use thiserror::Error;

/// Canonical error returned by every [`crate::MetaStore`] method.
#[derive(Debug, Error)]
pub enum MetaError {
    /// Backend transport / driver fault. Wraps an opaque diagnostic from the
    /// underlying D1 (or in-memory) implementation. Maps upstream to
    /// `COR_SERVICE_DEGRADED`.
    #[error("metadata backend error: {0}")]
    Backend(String),

    /// A `commit_decrement` was issued against a row that does not exist.
    /// Programmer error — the REAPI handler must call `commit_put` first.
    #[error("blob_meta row not found for tenant_id+digest")]
    NotFound,

    /// A `commit_decrement` was issued against an already-tombstoned row.
    /// Tombstoned rows are read-only until the GC sweep physically deletes
    /// them (S-06).
    #[error("blob_meta row is tombstoned and cannot be decremented")]
    Tombstoned,

    /// A `commit_decrement` would drive `refcount` below zero. Captured as
    /// an explicit invariant violation (`refcount >= 0` CHECK constraint)
    /// rather than relying on the SQL layer to reject it; lets the REAPI
    /// handler return a typed error to the audit chain.
    #[error("refcount underflow: cannot decrement refcount below zero")]
    RefcountUnderflow,

    /// `(request_id, event_type)` already present in `audit_outbox` AND the
    /// stored `payload_json` differs from the new attempt. The idempotent
    /// retry path is: `(request_id, event_type)` exists with the *same*
    /// payload → no-op success. A *different* payload signals client confusion
    /// (request_id reuse across distinct events), which we surface explicitly.
    #[error("audit_outbox idempotency conflict: same (request_id, event_type), different payload")]
    AuditIdempotencyConflict,
}

impl MetaError {
    /// Stable canonical error code matching `error_taxonomy.md`.
    ///
    /// Used by the REAPI handler in WI-S01-005 to derive the gRPC status
    /// without re-pattern-matching on the variant string.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Backend(_) => "COR_SERVICE_DEGRADED",
            Self::NotFound => "COR_META_BLOB_NOT_FOUND",
            Self::Tombstoned => "COR_META_TOMBSTONED",
            Self::RefcountUnderflow => "COR_META_REFCOUNT_UNDERFLOW",
            Self::AuditIdempotencyConflict => "COR_AUDIT_IDEMPOTENCY_CONFLICT",
        }
    }
}
