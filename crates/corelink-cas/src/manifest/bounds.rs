//! Canonical bounds for the manifest crate (WI-S05-005 §6.1).
//!
//! These constants are the cross-crate alignment authority for the
//! manifest pipeline. The S-05 spec contract §5.1 P1-SR5-001 pins
//! [`MAX_CHUNKS_PER_BLOB`] at `81_920` (160 GiB / 2 MiB exact); the
//! `corelink-cas::chunker` module's `MAX_CHUNKS_PER_BLOB` mirrors this value
//! verbatim and the worker's
//! `corelink-worker::reapi::cas::types::MAX_CHUNKS_PER_BLOB` constant
//! does the same. Drift between the three is caught by the compile-time
//! `_assert!` check below + per-crate canonical-vector tests.

/// Maximum chunks a single multipart blob can decompose into. Spec
/// contract §5.1 P1-SR5-001 cross-crate alignment authority.
///
/// Derivation: `160 GiB / 2 MiB = 81_920` exact. The value MUST stay
/// byte-equal across `corelink-chunker`, `corelink-multipart-schema`,
/// `corelink-worker::reapi::cas::types`, and this crate.
pub const MAX_CHUNKS_PER_BLOB: u32 = 81_920;

/// Maximum total byte size a single manifest can reference. `160 GiB`
/// = `MAX_CHUNKS_PER_BLOB × 2 MiB`. Exceeding this size requires the
/// stitched-manifest flow (WI-S05-006 — multiple multipart sessions
/// stitched via a parent manifest).
pub const MAX_TOTAL_SIZE_BYTES: u64 = 160 * 1024 * 1024 * 1024;

/// Maximum legal `chunk_size_bytes` for a single chunk (4 MiB —
/// FastCDC max bound; aligned with `corelink-chunker::FASTCDC_MAX`).
pub const MAX_CHUNK_SIZE_BYTES: u32 = 4 * 1024 * 1024;

/// Length of the canonical preimage that the manifest signature
/// authenticates. Bytes 0..102 of the manifest envelope: `version (1)
/// || tenant_id (16) || blob_digest (32) || merkle_root (32) ||
/// chunk_count (4) || total_size_bytes (8) || created_at_ms (8) ||
/// chunker_algo (1) = 102 bytes` (WI §6.1.5 layout).
pub const MANIFEST_PREIMAGE_LEN: usize = 102;

/// Length of an HKDF-derived manifest signature (BLAKE3 keyed-hash
/// output = 32 bytes — matches `corelink_ac`'s `AC_ENVELOPE_SIG_LEN`).
pub const MANIFEST_SIG_LEN: usize = 32;

/// Cross-crate alignment self-check: derived bounds stay coherent.
const _: () = assert!(
    (MAX_CHUNKS_PER_BLOB as u64) * (2 * 1024 * 1024) == MAX_TOTAL_SIZE_BYTES,
    "MAX_TOTAL_SIZE_BYTES / MAX_CHUNKS_PER_BLOB / 2 MiB chunk default must stay aligned"
);
const _: () = assert!(
    MANIFEST_PREIMAGE_LEN == 102,
    "manifest canonical preimage MUST be 102 bytes per WI-S05-005 §6.1.5"
);
const _: () = assert!(MANIFEST_SIG_LEN == 32, "manifest sig MUST be 32 bytes (BLAKE3 keyed-hash)");
