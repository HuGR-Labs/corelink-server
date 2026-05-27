//! Canonical [`DedupError`] taxonomy surfaced by every fallible API in
//! the crate.
//!
//! The taxonomy is `#[non_exhaustive]` per S-04 / S-05 / S-06 lesson —
//! the variant set grows additively across S-07 follow-on WIs (eviction
//! worker, quota middleware, DASH-DEDUP dashboard) without breaking
//! downstream callers. "Digest is missing" is NOT modeled as an error
//! — it is the happy-path outcome (the entire purpose of FindMissingBlobs);
//! this enum carries only **transport / programmer / auth** failures.

use thiserror::Error;

use crate::dedup::audit::DedupAuditSinkError;
use crate::dedup::metrics::DedupMetricsObserverError;

/// Canonical errors surfaced by [`crate::dedup::index::DedupIndex`] +
/// [`crate::dedup::write::record_chunk_write`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DedupError {
    /// Cross-tenant dedup query attempted while
    /// `dedup.cross_tenant.enabled = false` (the default per
    /// CTRL-ISO-005). Flipping the flag to `true` requires an ADR +
    /// BYOE (S-14) + Privacy Lead signoff per spec contract §10.s07.4.
    /// Maps to `PERMISSION_DENIED` (gRPC code 7; HTTP 403) at the
    /// handler boundary.
    #[error(
        "cross-tenant dedup attempted (CTRL-ISO-005 violation; \
         dedup.cross_tenant.enabled=false)"
    )]
    CrossTenantBlocked,

    /// Caller supplied a batch size > [`crate::dedup::index::MAX_FIND_MISSING_BATCH_SIZE`].
    /// Programmer wiring error — the handler MUST chunk inputs > 250
    /// before reaching the trait surface (Lote 10.5bis P0 lesson; D1
    /// `IN`-list cap).
    #[error("batch size {got} exceeds canonical ceiling {limit}")]
    BatchTooLarge {
        /// Caller-requested batch size.
        got: usize,
        /// Canonical ceiling.
        limit: usize,
    },

    /// Chunk-digest hex string violated the canonical 64-char lower-hex
    /// shape (BLAKE3-256). Programmer wiring error; mapped to 5xx by
    /// handler.
    #[error("invalid chunk_digest shape: {reason}")]
    InvalidChunkDigest {
        /// Short reason code (e.g. `"length != 64"` /
        /// `"non-hex character"`).
        reason: &'static str,
    },

    /// Index backend transport failure (D1 / fake). Maps to
    /// `UNAVAILABLE` (gRPC code 14; HTTP 503) at the handler boundary.
    #[error("dedup index backend error: {0}")]
    IndexBackend(String),

    /// Audit sink error. Fail-closed envelope: caller MUST surface this
    /// as 503 to preserve the `audit_emit ⇔ handler` atomicity contract
    /// (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    #[error("dedup audit sink error: {0}")]
    Audit(#[from] DedupAuditSinkError),

    /// Metrics emit failure. Caller may downgrade to log-and-continue
    /// per WI §14.s07.001.6 (production wiring uses a synchronous
    /// counter trait; failure is rare but explicit).
    #[error("dedup metrics observer error: {0}")]
    Metrics(#[from] DedupMetricsObserverError),
}
