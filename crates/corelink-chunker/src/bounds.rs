//! Canonical bounded-parser constants for [`crate::Chunker`]
//! implementations (WI-S05-002 §6.1.1, §9.6, §9.7; spec contract
//! S-05 §5.1 P1-SR5-001).
//!
//! Every constant in this module is load-bearing — drift here is the
//! kind of cross-crate alignment regression Lote 10.5-tris flagged
//! (P1-SR5-001 / P0-SR5-003): the worker handler
//! (`corelink-worker::reapi::cas::types::MAX_CHUNKS_PER_BLOB`) and the
//! upstream manifest builder (`corelink-manifest`, WI-S05-005) MUST
//! observe the same numerical values byte-for-byte. CI must include a
//! constant-equal assertion when the manifest crate lands so the
//! cross-crate seam stays in lock-step.

/// Maximum size in bytes of a single multipart blob. Hard upper bound
/// on what any [`crate::Chunker`] is allowed to consume in one
/// session. Beyond this the caller MUST split into multiple multipart
/// sessions and stitch via the upstream manifest layer (out-of-scope
/// for WI-S05-002; ships in WI-S05-006).
///
/// Numerical value: **160 GiB** = `10_000` R2 multipart parts ×
/// `16 MiB` per part (R2 multipart hard limits per ADR-0022 decision
/// matrix).
pub const MAX_BLOB_SIZE: u64 = 160 * 1024 * 1024 * 1024;

/// Maximum number of chunks that a single blob may decompose into.
/// Cross-crate aligned with
/// `corelink-worker::reapi::cas::types::MAX_CHUNKS_PER_BLOB` per
/// spec contract S-05 §5.1 P1-SR5-001 (Lote 10.5-tris fix raising
/// 80_000 → 81_920 so 160 GiB / 2 MiB = 81920 holds exactly).
///
/// Numerical value: **81 920** = `MAX_BLOB_SIZE / FIXED_DEFAULT_CHUNK_SIZE`.
pub const MAX_CHUNKS_PER_BLOB: u32 = 81_920;

/// Default fixed-size chunk granularity for [`crate::ChunkerAlgorithm::Fixed2MiB`].
/// Sprint contract S-05 §9.2 anchors the choice.
pub const FIXED_DEFAULT_CHUNK_SIZE: usize = 2 * 1024 * 1024;

/// FastCDC default minimum chunk size (Xia 2016, recalibrated for
/// CoreLink's 2 MiB target average per WI §1.5).
pub const FASTCDC_DEFAULT_MIN: usize = 1024 * 1024;

/// FastCDC default target average chunk size.
pub const FASTCDC_DEFAULT_AVG: usize = 2 * 1024 * 1024;

/// FastCDC default maximum chunk size — caps the worst-case anchor
/// search before the chunker forces a cut.
pub const FASTCDC_DEFAULT_MAX: usize = 4 * 1024 * 1024;

/// FastCDC small mask (`Mask_S` in Xia 2016 §3.4) — applied while the
/// rolling-hash window is still under `FASTCDC_DEFAULT_AVG`. Bias:
/// **stricter** mask → boundary harder to hit → average chunk size
/// drifts toward the upper bound.
///
/// Bit value canonical per WI §1 source listing
/// (`0x0000_d9f0_0353_0000`); identical seed shape used by the
/// upstream `fastcdc` Rust crate so cross-implementation determinism
/// holds.
pub const FASTCDC_MASK_S: u64 = 0x0000_d9f0_0353_0000;

/// FastCDC large mask (`Mask_L` in Xia 2016 §3.4) — applied once the
/// rolling-hash window has crossed `FASTCDC_DEFAULT_AVG`. Bias:
/// **looser** mask → boundary easier to hit → average chunk size is
/// kept close to the target.
pub const FASTCDC_MASK_L: u64 = 0x0000_d900_0353_0000;

/// Compile-time sanity: the canonical fixed default × the canonical
/// chunk-count cap recovers the canonical blob ceiling exactly. Any
/// future change to one of the three constants without updating the
/// other two breaks the build.
const _: () = assert!(
    (FIXED_DEFAULT_CHUNK_SIZE as u64) * (MAX_CHUNKS_PER_BLOB as u64) == MAX_BLOB_SIZE,
    "MAX_BLOB_SIZE / FIXED_DEFAULT_CHUNK_SIZE / MAX_CHUNKS_PER_BLOB must stay aligned"
);
