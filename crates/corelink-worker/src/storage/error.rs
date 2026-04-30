//! Storage error taxonomy (WI-S01-003 §6.1.6).
//!
//! Maps backend faults onto the canonical CoreLink error codes from
//! `error_taxonomy.md`:
//!
//! - [`COR_CAS_BLOB_NOT_FOUND`] (HTTP 404) ← R2 GET miss (and cross-tenant
//!   prefix mismatch, which is indistinguishable to the caller — by design,
//!   per ADR-0028 the 404 is uniform and closes the cross-tenant
//!   enumeration oracle).
//! - [`COR_CAS_BLOB_TOO_LARGE`] (HTTP 413) ← body exceeded the single-blob
//!   5 MiB cap (WI title constraint). Multipart upload (WI-S05-003) is the
//!   recovery path.
//! - [`COR_INTERNAL`] (HTTP 500) ← `RegionMismatch` (the dispatcher gave us
//!   a `TenantCtx` for a region we are not pinned to; this is a programmer
//!   error in the caller layer, never a client-induced fault, so it
//!   surfaces as a generic internal error rather than a client-validation
//!   code — clients cannot drive this path).
//! - [`COR_SERVICE_DEGRADED`] (HTTP 503) ← any backend 5xx / transport
//!   fault.
//!
//! `If-None-Match: *` rejection (R2 412 Precondition Failed) is **not** an
//! error — duplicate writes are idempotent per INV-CAS-IDEMPOTENCY and surface
//! as `Ok(PutOutcome::Duplicate)` from the writer. The metric label
//! `result="conflict_duplicate"` (WI-S01-003 §6.1.5) is emitted by the writer
//! at that point.
//!
//! All four codes referenced here are in the canonical taxonomy table at
//! `specs/03_architecture/error_taxonomy.md` (lines 105, 107, 216, 218 at
//! the time of writing). The "no new code without entry here" rule from the
//! taxonomy preamble is satisfied by the fact that this crate does not
//! introduce a single new code.

use thiserror::Error;

use crate::Region;

/// Canonical CoreLink error code emitted on R2 GET miss or any cross-tenant
/// prefix-mismatch read.
pub const COR_CAS_BLOB_NOT_FOUND: &str = "COR_CAS_BLOB_NOT_FOUND";

/// Canonical CoreLink error code for blobs exceeding the single-blob 5 MiB
/// upper bound (HTTP 413). Multipart upload is the recovery path.
pub const COR_CAS_BLOB_TOO_LARGE: &str = "COR_CAS_BLOB_TOO_LARGE";

/// Canonical CoreLink error code for an internal server error
/// (HTTP 500, `error_taxonomy.md` line 218 at time of writing). Used for
/// the writer-side `RegionMismatch` programmer-error variant — clients
/// cannot drive this code; it indicates a dispatcher / wiring bug.
pub const COR_INTERNAL: &str = "COR_INTERNAL";

/// Canonical CoreLink error code emitted on transient backend faults
/// (R2 5xx, transport-layer hiccups). HTTP mapping: 503.
pub const COR_SERVICE_DEGRADED: &str = "COR_SERVICE_DEGRADED";

/// Storage-layer error surface for [`crate::storage::r2`].
///
/// Every variant carries a stable error code (see consts above) so the
/// REAPI handler / Worker control plane can emit the canonical
/// `error_taxonomy.md` mapping without re-classifying.
#[derive(Debug, Error)]
pub enum R2Error {
    /// Object not present at the canonical key. Returned on R2 GET miss
    /// **and** on a cross-tenant read where Tenant B asks for a digest stored
    /// under Tenant A's prefix (the key under B's prefix doesn't exist;
    /// reader sees the same NotFound as a real miss).
    #[error("blob not found ({COR_CAS_BLOB_NOT_FOUND})")]
    NotFound,

    /// Body exceeded the single-blob upper bound (WI-S01-003 title:
    /// "≤ 5 MiB"). Larger blobs are routed via the multipart writer in
    /// WI-S05-003.
    #[error("blob exceeds single-blob limit ({size} > {limit} bytes; {COR_CAS_BLOB_TOO_LARGE})")]
    BlobTooLarge {
        /// Observed body size in bytes.
        size: usize,
        /// Configured upper bound (5 MiB by default).
        limit: usize,
    },

    /// The dispatcher handed the writer/reader a `TenantCtx` whose region
    /// does not match this adapter's region binding. This is a programmer
    /// error in the caller (auth middleware / region router), never a
    /// client-induced fault — the adapter refuses rather than silently
    /// writing to / reading from the wrong R2 bucket. Surfaces upstream as
    /// `COR_INTERNAL` (HTTP 500) since clients cannot drive this code.
    #[error("region mismatch (adapter={adapter}, ctx={ctx}; {COR_INTERNAL})")]
    RegionMismatch {
        /// The region this writer or reader is bound to.
        adapter: Region,
        /// The region carried by the `TenantCtx` the caller passed in.
        ctx: Region,
    },

    /// Backend (R2 binding or test fake) returned a transient fault.
    /// The wrapped string is the backend's own diagnostic; callers map this
    /// to `COR_SERVICE_DEGRADED` (HTTP 503, exponential backoff).
    #[error("backend fault ({COR_SERVICE_DEGRADED}): {0}")]
    Backend(String),
}

impl R2Error {
    /// Canonical error code from `error_taxonomy.md` for this variant.
    #[must_use]
    pub fn taxonomy_code(&self) -> &'static str {
        match self {
            Self::NotFound => COR_CAS_BLOB_NOT_FOUND,
            Self::BlobTooLarge { .. } => COR_CAS_BLOB_TOO_LARGE,
            Self::RegionMismatch { .. } => COR_INTERNAL,
            Self::Backend(_) => COR_SERVICE_DEGRADED,
        }
    }
}
