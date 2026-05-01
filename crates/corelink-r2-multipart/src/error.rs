//! Error taxonomy surfaced by the R2 multipart adapter (WI-S05-003 §1).
//!
//! Every variant has a stable `audit_code` that lines up with the
//! `COR_MULTIPART_*` codes pinned in `error_taxonomy.md`. The handler
//! that consumes this crate maps each variant to an HTTP status +
//! audit event per the WI §1 mapping table.

use thiserror::Error;

/// Fault classes produced by [`crate::adapter::MultipartAdapter`].
///
/// `#[non_exhaustive]` so additive variants can land in post-v1
/// patch releases without breaking downstream callers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MultipartError {
    /// `upload_id` not found or expired (404 + `COR_MULTIPART_TIMEOUT`).
    #[error("upload_id `{upload_id}` not found or expired")]
    UploadIdNotFound {
        /// Diagnostic — the unrecognised upload id.
        upload_id: String,
    },

    /// `upload_id` exists but is bound to a different `tenant_id`
    /// than the call carries (cross-tenant replay; rejected).
    /// Maps to 404 / 403 depending on handler policy; the canonical
    /// audit code is `COR_MULTIPART_CROSS_TENANT`. The session is
    /// left intact so the legitimate tenant can finish their work.
    #[error("upload_id `{upload_id}` is bound to another tenant; cross-tenant access rejected")]
    CrossTenantUpload {
        /// Diagnostic — the upload id whose binding mismatched.
        upload_id: String,
    },

    /// Part number sat outside `1..=R2_MAX_PARTS_PER_UPLOAD`. Maps to
    /// 413 + `COR_MULTIPART_BLOB_TOO_LARGE` (handler may route to the
    /// stitched multi-session flow per WI-S05-006).
    #[error("part number {part_number} exceeds R2 max {max} parts per upload")]
    MaxPartsExceeded {
        /// The offending part number.
        part_number: u32,
        /// The hard limit (10_000).
        max: u32,
    },

    /// Part body exceeded R2's per-part hard maximum (5 GiB). Maps to
    /// 413.
    #[error("part {part_number} size {size_bytes} exceeds R2 max part size {limit_bytes}")]
    PartTooLarge {
        /// Offending part.
        part_number: u32,
        /// Submitted size.
        size_bytes: u64,
        /// R2 hard limit.
        limit_bytes: u64,
    },

    /// Complete-multipart was issued with a part list that diverges
    /// from the parts the adapter recorded — either a missing part
    /// number, or an ETag that doesn't match what the
    /// adapter computed/returned for that part. Maps to 422 +
    /// `COR_MULTIPART_PART_MISSING`.
    #[error(
        "part {part_number} missing or ETag mismatch (expected `{expected}`, got `{actual}`)"
    )]
    PartMissing {
        /// Offending part.
        part_number: u32,
        /// ETag the adapter recorded at `upload_part` time.
        expected: String,
        /// ETag the caller supplied to `complete`.
        actual: String,
    },

    /// `complete` was called but no part has been recorded for the
    /// session — the empty-part-list case. Maps to 422.
    #[error("complete called with no parts uploaded for session `{upload_id}`")]
    EmptyPartList {
        /// The session id.
        upload_id: String,
    },

    /// `complete` was called with the same part number declared
    /// twice — duplicates would silently shadow each other in the
    /// committed object. Maps to 422.
    #[error("complete called with duplicate part number {part_number}")]
    DuplicatePartNumber {
        /// Offending part.
        part_number: u32,
    },

    /// Per-tenant concurrency permit exhausted. Maps to 429 +
    /// `COR_MULTIPART_CONCURRENCY_LIMITED`. Surfaced from the
    /// non-blocking `try_*` API only — the awaiting `upload_part`
    /// path queues on the semaphore for back-pressure.
    #[error("per-tenant concurrency limit reached for tenant `{tenant_id}` (limit {limit})")]
    ConcurrencyLimitReached {
        /// The tenant whose budget tripped.
        tenant_id: String,
        /// The configured budget.
        limit: usize,
    },

    /// Backend (R2 SDK / network / authz) failure — opaque
    /// diagnostic. Maps to 503 + `COR_MULTIPART_BACKEND_UNAVAILABLE`.
    #[error("R2 backend unavailable: {0}")]
    Backend(String),

    /// Path composition rejected the inputs (empty digest, bad
    /// tenant prefix, …). Programmer-error class; maps to 500. The
    /// adapter never trusts client-provided path components — this
    /// arm fires only when an upstream caller violates the
    /// pure-logic constructor's preconditions.
    #[error("invalid object key composition: {reason}")]
    InvalidObjectKey {
        /// Human-readable reason.
        reason: String,
    },
}

impl MultipartError {
    /// Stable canonical short-id used by the audit dashboard split
    /// (mirrors the `audit_code` contract from `error_taxonomy.md`).
    #[must_use]
    pub fn audit_code(&self) -> &'static str {
        match self {
            Self::UploadIdNotFound { .. } => "COR_MULTIPART_TIMEOUT",
            Self::CrossTenantUpload { .. } => "COR_MULTIPART_CROSS_TENANT",
            Self::MaxPartsExceeded { .. } => "COR_MULTIPART_BLOB_TOO_LARGE",
            Self::PartTooLarge { .. } => "COR_MULTIPART_PART_TOO_LARGE",
            Self::PartMissing { .. } => "COR_MULTIPART_PART_MISSING",
            Self::EmptyPartList { .. } => "COR_MULTIPART_EMPTY_PART_LIST",
            Self::DuplicatePartNumber { .. } => "COR_MULTIPART_DUPLICATE_PART",
            Self::ConcurrencyLimitReached { .. } => "COR_MULTIPART_CONCURRENCY_LIMITED",
            Self::Backend(_) => "COR_MULTIPART_BACKEND_UNAVAILABLE",
            Self::InvalidObjectKey { .. } => "COR_MULTIPART_INVALID_KEY",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_codes_are_stable() {
        let cases: &[(MultipartError, &str)] = &[
            (
                MultipartError::UploadIdNotFound {
                    upload_id: "u".into(),
                },
                "COR_MULTIPART_TIMEOUT",
            ),
            (
                MultipartError::CrossTenantUpload {
                    upload_id: "u".into(),
                },
                "COR_MULTIPART_CROSS_TENANT",
            ),
            (
                MultipartError::MaxPartsExceeded {
                    part_number: 10_001,
                    max: 10_000,
                },
                "COR_MULTIPART_BLOB_TOO_LARGE",
            ),
            (
                MultipartError::PartTooLarge {
                    part_number: 1,
                    size_bytes: 6 * 1024 * 1024 * 1024,
                    limit_bytes: 5 * 1024 * 1024 * 1024,
                },
                "COR_MULTIPART_PART_TOO_LARGE",
            ),
            (
                MultipartError::PartMissing {
                    part_number: 2,
                    expected: "a".into(),
                    actual: "b".into(),
                },
                "COR_MULTIPART_PART_MISSING",
            ),
            (
                MultipartError::EmptyPartList {
                    upload_id: "u".into(),
                },
                "COR_MULTIPART_EMPTY_PART_LIST",
            ),
            (
                MultipartError::DuplicatePartNumber { part_number: 3 },
                "COR_MULTIPART_DUPLICATE_PART",
            ),
            (
                MultipartError::ConcurrencyLimitReached {
                    tenant_id: "t".into(),
                    limit: 8,
                },
                "COR_MULTIPART_CONCURRENCY_LIMITED",
            ),
            (
                MultipartError::Backend("boom".into()),
                "COR_MULTIPART_BACKEND_UNAVAILABLE",
            ),
            (
                MultipartError::InvalidObjectKey {
                    reason: "empty".into(),
                },
                "COR_MULTIPART_INVALID_KEY",
            ),
        ];
        for (err, code) in cases {
            assert_eq!(err.audit_code(), *code, "{err:?}");
        }
    }

    #[test]
    fn display_includes_part_number_for_part_missing() {
        let e = MultipartError::PartMissing {
            part_number: 7,
            expected: "deadbeef".into(),
            actual: "cafebabe".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("part 7"));
        assert!(s.contains("deadbeef"));
        assert!(s.contains("cafebabe"));
    }
}
