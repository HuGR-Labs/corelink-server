//! `corelink-chunker` — content-defined chunker for the CoreLink CAS
//! multipart pipeline (WI-S05-002 + ADR-0022).
//!
//! Two concrete algorithms, both deterministic, both producing the
//! same [`Chunk`] envelope (BLAKE3-256 digest inline + offset + bytes
//! borrowed from a chunker-internal staging buffer):
//!
//! | Algorithm | Default? | Behaviour |
//! |---|---|---|
//! | [`ChunkerAlgorithm::Fixed2MiB`] | yes | Emit a chunk every `2 MiB` of input + a final partial via [`Chunker::finalize`]. Cheapest path; no rolling-hash state. |
//! | [`ChunkerAlgorithm::FastCDC2MiB`] | opt-in | FastCDC content-defined chunking (Xia et al., USENIX ATC 2016) with bounds `min = 1 MiB` / `avg = 2 MiB` / `max = 4 MiB` and the canonical Gear-hash mask seeds (`MASK_S` = `0x0000_d9f0_0353_0000`, `MASK_L` = `0x0000_d900_0353_0000`). Better dedup ratio on payloads with shifted byte ranges (Docker layers, ML-model deltas). |
//!
//! Both impls share the bounded-parser discipline from [`bounds`] —
//! [`bounds::MAX_BLOB_SIZE`] = `160 GiB` and
//! [`bounds::MAX_CHUNKS_PER_BLOB`] = `81 920` are hard limits enforced
//! at every `feed` so a crafted oversize stream rejects early without
//! allocating, satisfying `INV-MULTIPART-BOUNDED-PARSER`.
//!
//! # Quickstart
//!
//! ```
//! use corelink_cas::chunker::{Chunker, ChunkerConfig, ChunkerKind, ChunkerStep};
//!
//! let mut chunker = ChunkerKind::new(ChunkerConfig::default())
//!     .expect("default config is always valid");
//!
//! let payload = vec![0u8; 5 * 1024 * 1024];
//! let mut cursor = 0;
//! let mut chunks_seen = 0;
//! while cursor < payload.len() {
//!     let slice = &payload[cursor..];
//!     match chunker.feed(slice) {
//!         ChunkerStep::Chunk { chunk: _, consumed } => {
//!             cursor += consumed;
//!             chunks_seen += 1;
//!         }
//!         ChunkerStep::NeedMore { consumed } => {
//!             cursor += consumed;
//!             // would request more bytes from the caller in a real pipeline
//!         }
//!         ChunkerStep::Error(_) => break,
//!     }
//! }
//! if let Some(_final_chunk) = chunker.finalize() {
//!     chunks_seen += 1;
//! }
//! assert_eq!(chunks_seen, 3); // 2 + 2 + 1 MiB
//! ```
//!
//! # API stability
//!
//! [`Chunk`], [`ChunkerAlgorithm`], [`ChunkerConfig`], [`ChunkerKind`],
//! [`ChunkerStep`] all carry `#[non_exhaustive]` so additive enum
//! variants and struct fields don't break downstream callers across
//! post-v1 releases (see ADR-0039 + WI-S05-002 §6.1.11). Mask seeds
//! are pinned via [`bounds`] constants — drift here is an
//! `INV-MULTIPART-CHUNK-DETERMINISTIC` regression and is gated on CI
//! by [`tests/chunker_canonical_vectors.rs`](../tests/chunker_canonical_vectors.rs).
//!
//! # wasm32 cleanliness
//!
//! No `tokio` / `std::sync::Mutex` / `std::time::*` / `getrandom` in
//! this crate's `src/`. The same compiled artifact runs inside the
//! Cloudflare Workers WASM bundle and the host-side test harness;
//! WI-S05-002 §1.4 throughput target ≥ 200 MB/s WASM is met by the
//! `blake3` SIMD + portable Gear-hash combo.

#![forbid(unsafe_code)]

pub mod bounds;
pub mod error;
pub mod fastcdc;
pub mod fixed;
pub mod kind;

pub use error::ChunkerError;
pub use kind::{Chunker, ChunkerKind, ChunkerStep};

use self::bounds::{
    FASTCDC_DEFAULT_AVG, FASTCDC_DEFAULT_MAX, FASTCDC_DEFAULT_MIN, FASTCDC_MASK_L, FASTCDC_MASK_S,
    FIXED_DEFAULT_CHUNK_SIZE,
};

/// Algorithm dispatch for [`ChunkerKind::new`]. `#[non_exhaustive]`
/// per ADR-0039 forward-compat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChunkerAlgorithm {
    /// Fixed-size chunks of [`bounds::FIXED_DEFAULT_CHUNK_SIZE`]
    /// bytes (default `2 MiB`) + final partial. Cheapest path —
    /// stateless beyond the cumulative offset counter.
    Fixed2MiB,

    /// FastCDC content-defined chunking (opt-in). Uses Gear-hash
    /// rolling fingerprint with canonical mask seeds + min / avg /
    /// max bounds for deterministic, content-aware boundaries that
    /// dedupe across shifted payloads.
    FastCDC2MiB,
}

/// Canonical chunker configuration. Construct via
/// [`ChunkerConfig::default`] (canonical defaults pinned in
/// [`bounds`]); custom configs MUST validate via
/// [`ChunkerConfig::validate`] before they reach
/// [`ChunkerKind::new`].
///
/// `#[non_exhaustive]` so additive fields don't break downstream
/// callers.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ChunkerConfig {
    /// Algorithm dispatch.
    pub algorithm: ChunkerAlgorithm,
    /// Fixed-chunker target chunk size (only honoured by
    /// [`ChunkerAlgorithm::Fixed2MiB`]).
    pub fixed_chunk_size: usize,
    /// FastCDC minimum chunk size — chunks shorter than this are
    /// suppressed (no boundary yielded even if the rolling hash
    /// would otherwise fire).
    pub fastcdc_min: usize,
    /// FastCDC target average chunk size — drives mask selection
    /// (small mask before the average; large mask after).
    pub fastcdc_avg: usize,
    /// FastCDC maximum chunk size — when reached, a forced
    /// boundary fires regardless of the rolling hash.
    pub fastcdc_max: usize,
    /// Gear-hash mask applied while window length < `fastcdc_avg`
    /// (stricter; harder to fire). Pin to the canonical
    /// [`bounds::FASTCDC_MASK_S`] for cross-implementation
    /// determinism unless an ADR ratifies a new value.
    pub fastcdc_mask_s: u64,
    /// Gear-hash mask applied once window length ≥ `fastcdc_avg`
    /// (looser; easier to fire). Pin to
    /// [`bounds::FASTCDC_MASK_L`].
    pub fastcdc_mask_l: u64,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            algorithm: ChunkerAlgorithm::Fixed2MiB,
            fixed_chunk_size: FIXED_DEFAULT_CHUNK_SIZE,
            fastcdc_min: FASTCDC_DEFAULT_MIN,
            fastcdc_avg: FASTCDC_DEFAULT_AVG,
            fastcdc_max: FASTCDC_DEFAULT_MAX,
            fastcdc_mask_s: FASTCDC_MASK_S,
            fastcdc_mask_l: FASTCDC_MASK_L,
        }
    }
}

impl ChunkerConfig {
    /// Convenience builder for [`ChunkerAlgorithm::FastCDC2MiB`]
    /// preserving every other canonical default.
    #[must_use]
    pub fn fastcdc_default() -> Self {
        Self {
            algorithm: ChunkerAlgorithm::FastCDC2MiB,
            ..Self::default()
        }
    }

    /// Replace the algorithm; returns `self` for fluent chaining.
    /// `#[non_exhaustive]` on the struct prevents callers from
    /// constructing the config inline outside this crate, so this
    /// builder is the canonical way to flip the algorithm field.
    #[must_use]
    pub fn with_algorithm(mut self, algorithm: ChunkerAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Replace the fixed-chunker target chunk size.
    #[must_use]
    pub fn with_fixed_chunk_size(mut self, size: usize) -> Self {
        self.fixed_chunk_size = size;
        self
    }

    /// Replace the FastCDC `(min, avg, max)` triple in one call.
    #[must_use]
    pub fn with_fastcdc_bounds(mut self, min: usize, avg: usize, max: usize) -> Self {
        self.fastcdc_min = min;
        self.fastcdc_avg = avg;
        self.fastcdc_max = max;
        self
    }

    /// Replace the FastCDC mask seeds. Use with caution — drift
    /// from the canonical [`bounds::FASTCDC_MASK_S`] /
    /// [`bounds::FASTCDC_MASK_L`] breaks
    /// `INV-MULTIPART-CHUNK-DETERMINISTIC` cross-implementation.
    #[must_use]
    pub fn with_fastcdc_masks(mut self, mask_s: u64, mask_l: u64) -> Self {
        self.fastcdc_mask_s = mask_s;
        self.fastcdc_mask_l = mask_l;
        self
    }

    /// Validate the bounds + mask + sizes invariants.
    ///
    /// - `Fixed2MiB`: `fixed_chunk_size ∈ (0, MAX_BLOB_SIZE]`.
    /// - `FastCDC2MiB`: `0 < min ≤ avg ≤ max ≤ MAX_BLOB_SIZE` and
    ///   the bounds fit inside [`bounds::MAX_BLOB_SIZE`].
    ///
    /// Mask seeds are NOT validated (any `u64` is a legal mask in
    /// the FastCDC paper); call sites that must guarantee canonical
    /// determinism MUST cross-check against [`bounds::FASTCDC_MASK_S`]
    /// / [`bounds::FASTCDC_MASK_L`] explicitly.
    pub fn validate(&self) -> Result<(), ChunkerError> {
        match self.algorithm {
            ChunkerAlgorithm::Fixed2MiB => {
                if self.fixed_chunk_size == 0
                    || self.fixed_chunk_size as u64 > bounds::MAX_BLOB_SIZE
                {
                    return Err(ChunkerError::FixedChunkSizeInvalid {
                        size: self.fixed_chunk_size,
                    });
                }
                Ok(())
            }
            ChunkerAlgorithm::FastCDC2MiB => {
                let valid = self.fastcdc_min > 0
                    && self.fastcdc_min <= self.fastcdc_avg
                    && self.fastcdc_avg <= self.fastcdc_max
                    && (self.fastcdc_max as u64) <= bounds::MAX_BLOB_SIZE;
                if !valid {
                    return Err(ChunkerError::FastCdcConfigInvalid {
                        min: self.fastcdc_min,
                        avg: self.fastcdc_avg,
                        max: self.fastcdc_max,
                    });
                }
                Ok(())
            }
        }
    }
}

/// A single chunk emitted by a [`Chunker`] / [`ChunkerKind`].
///
/// `bytes` borrows from the chunker's internal staging buffer; the
/// `'a` lifetime is tied to the `&mut self` borrow through which
/// the chunk was emitted. Storing a `Chunk` past the next mutating
/// call is a borrow-check error — see [`Chunk::to_owned`] for the
/// owned-copy helper.
#[derive(Debug, Clone)]
pub struct Chunk<'a> {
    /// Chunk bytes (lives inside the chunker's staging buffer).
    pub bytes: &'a [u8],
    /// BLAKE3-256 digest of `bytes`, computed inline during
    /// emission.
    pub digest: [u8; 32],
    /// Offset of this chunk's first byte within the entire blob.
    /// Strictly monotonic across `feed` / `finalize` calls.
    pub offset_in_blob: u64,
    /// `bytes.len()` mirrored as a `usize` for ergonomics.
    pub size_bytes: usize,
}

impl Chunk<'_> {
    /// Heap-owned copy of this chunk so the caller can store it
    /// past the next chunker mutation. Allocates `size_bytes`
    /// once.
    #[must_use]
    pub fn to_owned(&self) -> OwnedChunk {
        OwnedChunk {
            bytes: self.bytes.to_vec(),
            digest: self.digest,
            offset_in_blob: self.offset_in_blob,
            size_bytes: self.size_bytes,
        }
    }

    /// Lowercase 64-char hex of the BLAKE3 digest.
    #[must_use]
    pub fn digest_hex(&self) -> String {
        hex::encode(self.digest)
    }
}

/// Heap-owned snapshot of a [`Chunk`] for callers that need to
/// store the chunk past the next chunker mutation.
#[derive(Debug, Clone)]
pub struct OwnedChunk {
    /// Owned chunk bytes.
    pub bytes: Vec<u8>,
    /// BLAKE3-256 digest.
    pub digest: [u8; 32],
    /// Offset of the first byte within the blob.
    pub offset_in_blob: u64,
    /// `bytes.len()`.
    pub size_bytes: usize,
}

impl OwnedChunk {
    /// Lowercase 64-char hex of the BLAKE3 digest.
    #[must_use]
    pub fn digest_hex(&self) -> String {
        hex::encode(self.digest)
    }

    /// Borrowed view back to a [`Chunk`] (lifetime tied to `&self`).
    #[must_use]
    pub fn as_chunk(&self) -> Chunk<'_> {
        Chunk {
            bytes: &self.bytes,
            digest: self.digest,
            offset_in_blob: self.offset_in_blob,
            size_bytes: self.size_bytes,
        }
    }
}

/// Crate version sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod lib_tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "test code; panic on assertion failure is the contract"
    )]

    use super::*;

    #[test]
    fn default_config_validates() {
        ChunkerConfig::default().validate().expect("default valid");
    }

    #[test]
    fn fastcdc_default_validates() {
        ChunkerConfig::fastcdc_default().validate().expect("fastcdc default valid");
    }

    #[test]
    fn fixed_invalid_zero_size() {
        let cfg = ChunkerConfig {
            fixed_chunk_size: 0,
            ..ChunkerConfig::default()
        };
        let err = cfg.validate().expect_err("zero size invalid");
        assert!(matches!(err, ChunkerError::FixedChunkSizeInvalid { size: 0 }));
    }

    #[test]
    fn fixed_invalid_too_large() {
        let cfg = ChunkerConfig {
            fixed_chunk_size: usize::try_from(bounds::MAX_BLOB_SIZE)
                .ok()
                .and_then(|v| v.checked_add(1))
                .unwrap_or(usize::MAX),
            ..ChunkerConfig::default()
        };
        let err = cfg.validate().expect_err("oversized invalid");
        assert!(matches!(err, ChunkerError::FixedChunkSizeInvalid { .. }));
    }

    #[test]
    fn fastcdc_invalid_min_zero() {
        let cfg = ChunkerConfig {
            algorithm: ChunkerAlgorithm::FastCDC2MiB,
            fastcdc_min: 0,
            ..ChunkerConfig::default()
        };
        let err = cfg.validate().expect_err("zero min invalid");
        assert!(matches!(err, ChunkerError::FastCdcConfigInvalid { .. }));
    }

    #[test]
    fn fastcdc_invalid_min_gt_avg() {
        let cfg = ChunkerConfig {
            algorithm: ChunkerAlgorithm::FastCDC2MiB,
            fastcdc_min: 4 * 1024 * 1024,
            fastcdc_avg: 2 * 1024 * 1024,
            fastcdc_max: 4 * 1024 * 1024,
            ..ChunkerConfig::default()
        };
        let err = cfg.validate().expect_err("min>avg invalid");
        assert!(matches!(err, ChunkerError::FastCdcConfigInvalid { .. }));
    }

    #[test]
    fn fastcdc_invalid_avg_gt_max() {
        let cfg = ChunkerConfig {
            algorithm: ChunkerAlgorithm::FastCDC2MiB,
            fastcdc_min: 1024,
            fastcdc_avg: 8 * 1024,
            fastcdc_max: 4 * 1024,
            ..ChunkerConfig::default()
        };
        let err = cfg.validate().expect_err("avg>max invalid");
        assert!(matches!(err, ChunkerError::FastCdcConfigInvalid { .. }));
    }

    #[test]
    fn chunk_to_owned_preserves_fields() {
        let bytes = vec![1u8, 2, 3, 4, 5];
        let digest = [42u8; 32];
        let chunk = Chunk {
            bytes: &bytes,
            digest,
            offset_in_blob: 100,
            size_bytes: 5,
        };
        let owned = chunk.to_owned();
        assert_eq!(owned.bytes, vec![1u8, 2, 3, 4, 5]);
        assert_eq!(owned.digest, digest);
        assert_eq!(owned.offset_in_blob, 100);
        assert_eq!(owned.size_bytes, 5);
        assert_eq!(owned.digest_hex(), chunk.digest_hex());
        let borrowed = owned.as_chunk();
        assert_eq!(borrowed.bytes, &[1u8, 2, 3, 4, 5]);
    }

    #[test]
    fn version_matches_cargo_pkg_version() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
