//! Canonical error taxonomy for [`crate::Chunker`] implementations.
//!
//! `#[non_exhaustive]` so additive variants don't break downstream
//! callers across post-v1 releases (WI-S05-002 §6.1.11, §17).

use thiserror::Error;

/// Error returned by [`crate::Chunker`] implementations + the
/// [`crate::ChunkerConfig::validate`] surface.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChunkerError {
    /// Cumulative input exceeded [`crate::bounds::MAX_BLOB_SIZE`].
    /// The chunker rejects further input early without allocating
    /// — see WI-S05-002 §2.3 (R-3) bounded-parser invariant.
    #[error(
        "input exceeds maximum blob size: {found} > {max} bytes (single multipart cap; stitched flow upstream)"
    )]
    BlobTooLarge {
        /// Cumulative bytes the chunker has been fed so far.
        found: u64,
        /// Hard limit (160 GiB).
        max: u64,
    },

    /// The chunker would cross [`crate::bounds::MAX_CHUNKS_PER_BLOB`]
    /// if it emitted another chunk. Cross-crate aligned with
    /// `corelink-worker::reapi::cas::types::MAX_CHUNKS_PER_BLOB` per
    /// spec contract S-05 §5.1 P1-SR5-001.
    #[error(
        "blob decomposed into too many chunks: {found} > {max} (cross-crate cap)"
    )]
    TooManyChunks {
        /// Number of chunks already emitted (or about to be emitted).
        found: u64,
        /// Hard limit (81 920).
        max: u32,
    },

    /// `ChunkerConfig` for [`crate::ChunkerAlgorithm::FastCDC2MiB`]
    /// violated the `min ≤ avg ≤ max` invariant required by the
    /// FastCDC algorithm (Xia 2016 §3.4).
    #[error("FastCDC config invalid: min={min} avg={avg} max={max} (require min ≤ avg ≤ max, all > 0)")]
    FastCdcConfigInvalid {
        /// Configured minimum chunk size.
        min: usize,
        /// Configured target average chunk size.
        avg: usize,
        /// Configured maximum chunk size.
        max: usize,
    },

    /// Fixed chunk size invalid (zero or wider than `MAX_BLOB_SIZE`).
    #[error("fixed chunk size invalid: {size} (must be > 0 and ≤ MAX_BLOB_SIZE)")]
    FixedChunkSizeInvalid {
        /// Configured fixed chunk size.
        size: usize,
    },
}

impl ChunkerError {
    /// Short canonical identifier for audit dashboards / logs.
    /// Mirrors the `audit_code` short-id contract used by
    /// `corelink-ac` / `corelink-meta` so downstream pipelines can
    /// filter by error class without parsing display strings.
    #[must_use]
    pub const fn audit_code(&self) -> &'static str {
        match self {
            Self::BlobTooLarge { .. } => "blob_too_large",
            Self::TooManyChunks { .. } => "too_many_chunks",
            Self::FastCdcConfigInvalid { .. } => "fastcdc_config_invalid",
            Self::FixedChunkSizeInvalid { .. } => "fixed_chunk_size_invalid",
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "test code; panic on assertion failure is the contract"
    )]

    use super::*;

    #[test]
    fn audit_codes_are_canonical() {
        assert_eq!(
            ChunkerError::BlobTooLarge {
                found: 200,
                max: 160
            }
            .audit_code(),
            "blob_too_large"
        );
        assert_eq!(
            ChunkerError::TooManyChunks {
                found: 100_000,
                max: 81_920,
            }
            .audit_code(),
            "too_many_chunks"
        );
        assert_eq!(
            ChunkerError::FastCdcConfigInvalid {
                min: 0,
                avg: 0,
                max: 0
            }
            .audit_code(),
            "fastcdc_config_invalid"
        );
        assert_eq!(
            ChunkerError::FixedChunkSizeInvalid { size: 0 }.audit_code(),
            "fixed_chunk_size_invalid"
        );
    }

    #[test]
    fn debug_renders_helpful_message() {
        let e = ChunkerError::BlobTooLarge {
            found: 200_000,
            max: 160_000,
        };
        let rendered = format!("{e}");
        assert!(rendered.contains("200000"));
        assert!(rendered.contains("160000"));
    }
}
