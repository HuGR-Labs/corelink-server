//! Canonical bounds for the R2 multipart adapter (WI-S05-003 + ADR-0022).
//!
//! Every constant here is load-bearing — drift is an
//! `INV-MULTIPART-BOUNDED-PARSER` regression and the canonical-vector
//! tests in `tests/prop_r2_multipart.rs` will reject the build.
//!
//! References:
//! - R2 Multipart API limits:
//!   <https://developers.cloudflare.com/r2/api/s3/multipart-uploads/>
//! - ADR-0022 (chunk size = 2 MiB content unit; multipart part size =
//!   16 MiB API unit; max single multipart blob ≈ 156.25 GiB).

/// R2 hard limit on the number of parts in a single multipart upload
/// (S3-compatible). Crossing this maps to
/// [`crate::MultipartError::MaxPartsExceeded`].
pub const R2_MAX_PARTS_PER_UPLOAD: u32 = 10_000;

/// R2 hard maximum part size (5 GiB; S3-compatible).
pub const R2_MAX_PART_SIZE_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// R2 minimum part size for non-final parts (5 MiB; S3-compatible).
/// Final parts are exempt — see
/// [`crate::adapter::MultipartAdapter::upload_part`].
pub const R2_MIN_PART_SIZE_BYTES: u64 = 5 * 1024 * 1024;

/// Canonical part size used by the CoreLink multipart pipeline
/// (16 MiB) per ADR-0022. Each part packs ≈ 8 chunks of 2 MiB content.
pub const CANONICAL_PART_SIZE_BYTES: u64 = 16 * 1024 * 1024;

/// Default per-tenant `upload_part` concurrency budget. Tunable per
/// tier in S-13 (`tier.parallel_parts`); GA default is 8.
pub const DEFAULT_PER_TENANT_PARALLEL_PARTS: usize = 8;

/// Default sweeper max-age for [`crate::MultipartAdapter::list_orphans`]
/// (7 days, in seconds).
pub const DEFAULT_ORPHAN_MAX_AGE_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Chunk-bucket family literal: `chunk-<region>` per WI §1 invariant 1.
///
/// Used by [`crate::object_key::compose`] to route chunk uploads to
/// the canonical R2 bucket.
pub const CHUNK_BUCKET_PREFIX: &str = "chunk-";

/// Manifest-bucket family literal: `manifest-<region>` per WI §1 invariant 1.
///
/// Used by [`crate::object_key::compose`] to route manifest uploads
/// to the canonical R2 bucket.
pub const MANIFEST_BUCKET_PREFIX: &str = "manifest-";

/// Maximum single-session multipart blob size — `CANONICAL_PART_SIZE_BYTES
/// × R2_MAX_PARTS_PER_UPLOAD` = 16 MiB × 10_000 ≈ 156.25 GiB. The sprint
/// contract phrases this as "160 GiB max single multipart" which is the
/// rounded-up advertised bound; the exact byte count is what the parser
/// gates on.
pub const MAX_SINGLE_SESSION_BLOB_BYTES: u64 =
    CANONICAL_PART_SIZE_BYTES * (R2_MAX_PARTS_PER_UPLOAD as u64);

/// Compile-time bound proof: the canonical 16 MiB part × 10_000 parts
/// equals exactly 160_000 × 1 MiB. Drift here breaks the ADR-0022
/// invariant + the wire-shape used by the stitched-flow router
/// (WI-S05-006).
const _ASSERT_MAX_BLOB: () = {
    let max_blob_bytes = MAX_SINGLE_SESSION_BLOB_BYTES;
    // 160_000 × 1 MiB = 16 MiB × 10_000.
    let expected = 160_000_u64 * 1024 * 1024;
    assert!(
        max_blob_bytes == expected,
        "canonical part × max parts must equal 160_000 MiB (≈ 156.25 GiB) — ADR-0022"
    );
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r2_hard_limits_match_spec() {
        assert_eq!(R2_MAX_PARTS_PER_UPLOAD, 10_000);
        assert_eq!(R2_MAX_PART_SIZE_BYTES, 5 * 1024 * 1024 * 1024);
        assert_eq!(R2_MIN_PART_SIZE_BYTES, 5 * 1024 * 1024);
    }

    #[test]
    fn canonical_part_size_is_16_mib() {
        assert_eq!(CANONICAL_PART_SIZE_BYTES, 16 * 1024 * 1024);
    }

    #[test]
    fn default_concurrency_is_eight() {
        assert_eq!(DEFAULT_PER_TENANT_PARALLEL_PARTS, 8);
    }

    #[test]
    fn default_orphan_max_age_is_seven_days() {
        assert_eq!(DEFAULT_ORPHAN_MAX_AGE_SECONDS, 7 * 86_400);
    }

    #[test]
    fn bucket_prefixes_match_spec() {
        assert_eq!(CHUNK_BUCKET_PREFIX, "chunk-");
        assert_eq!(MANIFEST_BUCKET_PREFIX, "manifest-");
    }

    #[test]
    fn max_blob_bytes_is_160_000_mib() {
        assert_eq!(MAX_SINGLE_SESSION_BLOB_BYTES, 160_000 * 1024 * 1024);
    }
}
