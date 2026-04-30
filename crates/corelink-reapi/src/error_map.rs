//! Error → `error_taxonomy.md` code + gRPC status code mapping.
//!
//! Every concrete error variant from `corelink-hash`, `corelink-meta`, and
//! `corelink-worker` must map deterministically to:
//!
//! 1. A stable `error_taxonomy.md` code (`COR_*`) — the SDK exception class.
//! 2. A gRPC status code numeric (`tonic::Code` value as `i32`) — the wire
//!    surface seen by Bazel/Buck2 clients.
//! 3. A short, **non-PII** developer-facing message — written into the
//!    `google.rpc.Status.message` field for batch per-blob entries OR the
//!    outer tonic `Status::with_metadata` payload for top-level RPC errors.
//!
//! All three are bound together here so future drift (e.g. error_taxonomy
//! changes 503 → 502 for `COR_SERVICE_DEGRADED`) only needs one edit + a
//! test diff.
//!
//! ## Canonical gRPC code mapping (REAPI v2.12.0 §error model)
//!
//! Per WI-S01-005 §2 + §9.6:
//!
//! - `INVALID_ARGUMENT` (3) — proto-level malformed inputs (bad digest hex,
//!   data length disagreement with declared `size_bytes`).
//! - `UNAUTHENTICATED` (16) — PAT missing / invalid.
//! - `PERMISSION_DENIED` (7) — PAT scope insufficient.
//! - `RESOURCE_EXHAUSTED` (8) — `BatchUpdateBlobsRequest` aggregate exceeds
//!   `MaxBatchTotalSizeBytes` (4 MiB) OR `ByteStream::Write` body exceeds
//!   `max_cas_blob_size_bytes` (5 MiB). REAPI canonical: NOT `OUT_OF_RANGE`
//!   (which is read-past-EOF semantics).
//! - `ABORTED` (10) — body BLAKE3 mismatch (cache poisoning attempt).
//! - `INTERNAL` (13) — unexpected backend / programmer errors.
//! - `UNAVAILABLE` (14) — transient backend failures recommending retry.

use corelink_hash::HashMismatch;
use corelink_meta::MetaError;
use corelink_worker::storage::error::R2Error;

/// `error_taxonomy.md` code for `AuthStubError::PatInvalid`.
pub const COR_AUTH_PAT_INVALID: &str = "COR_AUTH_PAT_INVALID";
/// `error_taxonomy.md` code for `AuthStubError::ScopeInsufficient`.
pub const COR_AUTH_SCOPE_INSUFFICIENT: &str = "COR_AUTH_SCOPE_INSUFFICIENT";
/// `error_taxonomy.md` code for a single blob exceeding 5 MiB ByteStream cap.
pub const COR_CAS_BLOB_TOO_LARGE: &str = "COR_CAS_BLOB_TOO_LARGE";
/// `error_taxonomy.md` code for a `BatchUpdateBlobsRequest` aggregate
/// exceeding the 4 MiB `MaxBatchTotalSizeBytes` cap. Registered in the
/// canonical `error_taxonomy.md §3.1` table at v0.2.0; gRPC mapping is
/// `RESOURCE_EXHAUSTED` (8); HTTP equivalent 413.
pub const COR_CAS_BATCH_TOO_LARGE: &str = "COR_CAS_BATCH_TOO_LARGE";
/// `error_taxonomy.md` code for an unparseable `ByteStream::Write`
/// `resource_name`. Registered in `error_taxonomy.md §3.1` at v0.2.0;
/// gRPC mapping `INVALID_ARGUMENT` (3); HTTP 400.
pub const COR_CAS_BAD_RESOURCE_NAME: &str = "COR_CAS_BAD_RESOURCE_NAME";
/// `error_taxonomy.md` code for a `BatchUpdateBlobsRequest.digest_function`
/// the server does not advertise. Registered in `error_taxonomy.md §3.1`
/// at v0.2.0; gRPC mapping `INVALID_ARGUMENT` (3); HTTP 400.
pub const COR_CAS_DIGEST_FUNCTION_UNSUPPORTED: &str = "COR_CAS_DIGEST_FUNCTION_UNSUPPORTED";
/// `error_taxonomy.md` code for a malformed digest in
/// `BatchUpdateBlobsRequest.Request.digest`. Registered in
/// `error_taxonomy.md §3.1` at v0.2.0; gRPC mapping `INVALID_ARGUMENT` (3);
/// HTTP 400. Distinct from `COR_CAS_DIGEST_MISMATCH` (the BLAKE3-of-body vs
/// declared-digest disagreement, gRPC ABORTED 10).
pub const COR_CAS_BAD_DIGEST: &str = "COR_CAS_BAD_DIGEST";
/// `error_taxonomy.md` code for unexpected internal failures.
pub const COR_INTERNAL: &str = "COR_INTERNAL";
/// `error_taxonomy.md` code for transient degradation; hint client to retry.
pub const COR_SERVICE_DEGRADED: &str = "COR_SERVICE_DEGRADED";
/// `error_taxonomy.md` code for transient failures; alias retained for
/// caller convenience. Maps to the same `COR_SERVICE_DEGRADED` taxonomy
/// entry — keep callsite intent explicit ("transient") even though the
/// taxonomy code is shared.
pub const COR_TRANSIENT: &str = "COR_SERVICE_DEGRADED";

/// gRPC code (canonical numeric) for INVALID_ARGUMENT. Mirrors `tonic::Code`
/// on the wire; we re-export the integer so the pure-logic crate compiles
/// without `tonic` (wasm32 build).
pub const GRPC_INVALID_ARGUMENT: i32 = 3;
/// gRPC code for ABORTED.
pub const GRPC_ABORTED: i32 = 10;
/// gRPC code for RESOURCE_EXHAUSTED.
pub const GRPC_RESOURCE_EXHAUSTED: i32 = 8;
/// gRPC code for INTERNAL.
pub const GRPC_INTERNAL: i32 = 13;
/// gRPC code for UNAUTHENTICATED.
pub const GRPC_UNAUTHENTICATED: i32 = 16;
/// gRPC code for PERMISSION_DENIED.
pub const GRPC_PERMISSION_DENIED: i32 = 7;
/// gRPC code for UNAVAILABLE.
pub const GRPC_UNAVAILABLE: i32 = 14;

/// Mapping triple: error_taxonomy code, gRPC numeric code, dev-facing message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorMapping {
    /// Stable `error_taxonomy.md` code (`COR_*`).
    pub taxonomy_code: &'static str,
    /// Canonical gRPC numeric code.
    pub grpc_code: i32,
    /// Short developer-facing message (no PII, no body bytes).
    pub message: &'static str,
}

/// Mapping for [`HashMismatch`].
pub trait HashErrorMapping {
    /// The canonical [`ErrorMapping`] for this error.
    fn mapping(&self) -> ErrorMapping;
}

impl HashErrorMapping for HashMismatch {
    fn mapping(&self) -> ErrorMapping {
        ErrorMapping {
            taxonomy_code: corelink_hash::COR_CAS_DIGEST_MISMATCH,
            grpc_code: GRPC_ABORTED,
            message: "digest mismatch: BLAKE3 of body does not match declared digest",
        }
    }
}

/// Mapping for [`MetaError`].
pub trait MetaErrorMapping {
    /// The canonical [`ErrorMapping`] for this error.
    fn mapping(&self) -> ErrorMapping;
}

impl MetaErrorMapping for MetaError {
    fn mapping(&self) -> ErrorMapping {
        match self {
            // Backend transport / driver fault → 503 retryable. Maps to
            // tonic `UNAVAILABLE` so canonical clients honour the gRPC retry
            // policy.
            Self::Backend(_) => ErrorMapping {
                taxonomy_code: COR_SERVICE_DEGRADED,
                grpc_code: GRPC_UNAVAILABLE,
                message: "metadata backend transient failure",
            },
            // NotFound / Tombstoned / RefcountUnderflow are programmer
            // errors in the WI-S01-005 write path (commit_put never sees a
            // tombstoned row before an INSERT OR IGNORE). Map to INTERNAL
            // so SDK clients surface the codepath as a bug, not as a
            // recoverable failure.
            Self::NotFound | Self::Tombstoned | Self::RefcountUnderflow => ErrorMapping {
                taxonomy_code: COR_INTERNAL,
                grpc_code: GRPC_INTERNAL,
                message: "metadata invariant violation in CAS write path",
            },
            Self::AuditIdempotencyConflict => ErrorMapping {
                taxonomy_code: COR_INTERNAL,
                grpc_code: GRPC_INTERNAL,
                message:
                    "audit_outbox idempotency conflict (request_id reuse with different payload)",
            },
        }
    }
}

/// Mapping for [`R2Error`]. The R2 adapter already exposes a `taxonomy_code()`
/// helper for some variants; this trait surface keeps the mapping centralized
/// so the handler doesn't need to know about the adapter's internal helper.
pub trait R2ErrorMapping {
    /// The canonical [`ErrorMapping`] for this error.
    fn mapping(&self) -> ErrorMapping;
}

impl R2ErrorMapping for R2Error {
    fn mapping(&self) -> ErrorMapping {
        match self {
            R2Error::BlobTooLarge { .. } => ErrorMapping {
                taxonomy_code: COR_CAS_BLOB_TOO_LARGE,
                grpc_code: GRPC_RESOURCE_EXHAUSTED,
                message: "blob exceeds single-blob 5 MiB cap (use ByteStream::Write or multipart)",
            },
            R2Error::NotFound => ErrorMapping {
                taxonomy_code: COR_INTERNAL,
                grpc_code: GRPC_INTERNAL,
                message: "R2 reported NotFound on write path (programmer error)",
            },
            R2Error::RegionMismatch { .. } => ErrorMapping {
                taxonomy_code: COR_INTERNAL,
                grpc_code: GRPC_INTERNAL,
                message: "TenantCtx region mismatch with R2 adapter region",
            },
            R2Error::Backend(_) => ErrorMapping {
                taxonomy_code: COR_SERVICE_DEGRADED,
                grpc_code: GRPC_UNAVAILABLE,
                message: "R2 transient backend failure",
            },
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn hash_mismatch_maps_to_aborted() {
        let m = HashMismatch.mapping();
        assert_eq!(m.taxonomy_code, "COR_CAS_DIGEST_MISMATCH");
        assert_eq!(m.grpc_code, GRPC_ABORTED);
        assert!(m.message.contains("digest mismatch"));
    }

    #[test]
    fn meta_backend_maps_to_unavailable_503() {
        let e = MetaError::Backend("D1 timeout".to_owned());
        let m = e.mapping();
        assert_eq!(m.taxonomy_code, "COR_SERVICE_DEGRADED");
        assert_eq!(m.grpc_code, GRPC_UNAVAILABLE);
    }

    #[test]
    fn meta_notfound_maps_to_internal_in_write_path() {
        let m = MetaError::NotFound.mapping();
        assert_eq!(m.taxonomy_code, "COR_INTERNAL");
        assert_eq!(m.grpc_code, GRPC_INTERNAL);
    }

    #[test]
    fn meta_audit_conflict_maps_to_internal() {
        let m = MetaError::AuditIdempotencyConflict.mapping();
        assert_eq!(m.taxonomy_code, "COR_INTERNAL");
        assert_eq!(m.grpc_code, GRPC_INTERNAL);
    }

    #[test]
    fn r2_blob_too_large_maps_to_resource_exhausted() {
        let m = R2Error::BlobTooLarge {
            size: 6 * 1024 * 1024,
            limit: 5 * 1024 * 1024,
        }
        .mapping();
        assert_eq!(m.taxonomy_code, "COR_CAS_BLOB_TOO_LARGE");
        assert_eq!(m.grpc_code, GRPC_RESOURCE_EXHAUSTED);
    }

    #[test]
    fn r2_backend_maps_to_unavailable() {
        let m = R2Error::Backend("R2 5xx".to_owned()).mapping();
        assert_eq!(m.taxonomy_code, "COR_SERVICE_DEGRADED");
        assert_eq!(m.grpc_code, GRPC_UNAVAILABLE);
    }
}
