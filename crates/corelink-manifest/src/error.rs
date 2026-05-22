//! Canonical error taxonomy for the manifest crate (WI-S05-005 §1).
//!
//! `ManifestError` is the single error type surfaced by the builder /
//! verifier APIs. The verifier's [`VerifyError`] is a discriminated
//! union over `ManifestError` (structure failures), [`SigError`] (sig
//! failures, delegated to [`corelink-ac::sig::SigError`] verbatim so the
//! HKDF surface stays single-source-of-truth) and the streaming-only
//! per-chunk-mismatch arm.
//!
//! Every variant carries a short canonical reason string suitable for
//! audit emission; the variants are `#[non_exhaustive]` so additional
//! variants (e.g. compression sentinel arms in S-12) can be added
//! without an SemVer break.

use corelink_ac_core::sig::SigError;
use thiserror::Error;

use crate::bounds::{MAX_CHUNK_SIZE_BYTES, MAX_CHUNKS_PER_BLOB, MAX_TOTAL_SIZE_BYTES};

/// Errors surfaced by [`crate::ManifestBuilder`] and the structural
/// path of [`crate::ManifestVerifier`]. Sig-only failures route through
/// [`SigError`] inside [`VerifyError::Sig`] so a future Ed25519 backend
/// (ADR-0021 §A) can be added without touching this taxonomy.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ManifestError {
    /// Manifest version unsupported (only `1` accepted at v1.0).
    #[error("manifest version unsupported: {0}")]
    VersionUnsupported(u8),

    /// Chunk count exceeds the bounded-parser cap.
    #[error("chunk_count {found} exceeds bound {}", MAX_CHUNKS_PER_BLOB)]
    ChunkCountExceeded {
        /// Offending count.
        found: u32,
    },

    /// Total reassembled size exceeds the bounded-parser cap.
    #[error("total_size_bytes {found} exceeds bound {}", MAX_TOTAL_SIZE_BYTES)]
    TotalSizeExceeded {
        /// Offending total size.
        found: u64,
    },

    /// A single chunk size exceeds the per-chunk bound.
    #[error("chunk size {size_bytes} at index {index} exceeds bound {}", MAX_CHUNK_SIZE_BYTES)]
    ChunkSizeExceeded {
        /// Offending index.
        index: u32,
        /// Offending size.
        size_bytes: u32,
    },

    /// A chunk has zero size — illegal per spec (chunks are 1..=4 MiB;
    /// the final partial chunk MAY be 1..=4 MiB but never zero — a zero
    /// chunk is a builder bug or an adversarial forge).
    #[error("chunk size zero at index {0}")]
    ChunkSizeZero(u32),

    /// Chunk index out-of-order — the manifest carries a non-sequential
    /// `chunks[i].index` series. Builder enforces strictly ascending
    /// `0..chunk_count`.
    #[error("chunk index out of order at slot {slot}: expected {expected}, got {found}")]
    ChunkIndexOutOfOrder {
        /// Position in the chunks list (`0`-indexed).
        slot: u32,
        /// Expected canonical index (`slot` cast to `u32`).
        expected: u32,
        /// What was actually there.
        found: u32,
    },

    /// `chunks.len()` disagrees with the declared `chunk_count`.
    #[error("chunk_count {declared} disagrees with chunks.len() {actual}")]
    ChunkCountMismatch {
        /// Declared `chunk_count` field.
        declared: u32,
        /// Actual chunks list length.
        actual: u32,
    },

    /// `total_size_bytes` disagrees with the sum of `chunks[i].size_bytes`.
    #[error("total_size_bytes {declared} disagrees with sum of chunk sizes {actual}")]
    TotalSizeMismatch {
        /// Declared `total_size_bytes`.
        declared: u64,
        /// Sum of `chunks[i].size_bytes`.
        actual: u64,
    },

    /// Recomputed Merkle root disagrees with the claimed `merkle_root`.
    /// Constant-time comparison failure path. Audit-emitted as
    /// `corelink.manifest.root_mismatch` SEV-1.
    #[error("merkle root mismatch")]
    RootMismatch,

    /// `chunker_algo` field carries an unrecognized discriminant.
    #[error("chunker_algo unknown: {0}")]
    ChunkerAlgoUnknown(u8),

    /// Empty manifest (`chunk_count == 0`). The protocol forbids zero
    /// chunks — a manifest must reference at least one chunk (the
    /// "single small blob" path uses single-blob storage in S-01, not
    /// the manifest).
    #[error("manifest must reference at least one chunk")]
    Empty,
}

/// Composite error type returned by the verifier surfaces.
///
/// `Structure` covers every algorithmic / bounded-parser failure;
/// `Sig` delegates to [`SigError`] verbatim; `StreamingChunkMismatch`
/// is the streaming-only mid-stream fail-fast arm
/// (`INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST`).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VerifyError {
    /// Structural / bounded-parser failure.
    #[error("manifest structure invalid: {0}")]
    Structure(#[from] ManifestError),

    /// Cripto-signature verification failure (HKDF-SHA256 + BLAKE3
    /// keyed-hash). Delegates to [`corelink-ac::sig::SigError`] so the
    /// underlying error taxonomy stays single-source-of-truth.
    #[error("manifest signature invalid: {0}")]
    Sig(#[from] SigError),

    /// Streaming progressive verify caught a per-chunk hash mismatch.
    /// The handler's signal channel MUST propagate this to the caller
    /// before the next chunk is processed (cancel_token cancellation in
    /// the worker, gRPC error trailer in the wire path).
    #[error("streaming chunk verify failed at index {index}")]
    StreamingChunkMismatch {
        /// Offending chunk index.
        index: u32,
    },

    /// Caller supplied a chunk byte buffer that disagreed with the
    /// declared `size_bytes` for that chunk. Surfaced separately from
    /// the hash mismatch arm so audit dashboards can split "wire
    /// truncation" from "tamper detected".
    #[error("streaming chunk size mismatch at index {index}: declared {declared}, observed {observed}")]
    StreamingChunkSizeMismatch {
        /// Offending chunk index.
        index: u32,
        /// Manifest-declared size.
        declared: u32,
        /// Observed bytes length.
        observed: usize,
    },

    /// The streaming source ran out of chunks before the manifest was
    /// fully consumed.
    #[error("streaming source ended early at index {index} (expected {expected_total} chunks)")]
    StreamingSourceTruncated {
        /// Last index successfully observed (or `0` if none).
        index: u32,
        /// Manifest-declared total chunks.
        expected_total: u32,
    },

    /// The streaming source delivered a chunk with an out-of-order
    /// index — the canonical streaming surface MUST deliver chunks in
    /// `0..chunk_count` ascending order. Surfacing this distinct from
    /// `StreamingChunkMismatch` keeps "tamper" vs "wiring bug"
    /// separable in audit.
    #[error("streaming chunk delivered out of order: expected {expected}, got {found}")]
    StreamingChunkIndexUnexpected {
        /// Expected canonical index.
        expected: u32,
        /// What the source actually delivered.
        found: u32,
    },
}

impl VerifyError {
    /// Short canonical audit code (≤ 32 chars) for SIEM emission. The
    /// strings are byte-stable across versions per the audit-event
    /// taxonomy contract (S-03 `corelink-audit` §32-variant enum).
    #[must_use]
    pub fn audit_code(&self) -> &'static str {
        match self {
            Self::Structure(e) => e.audit_code(),
            Self::Sig(_) => "manifest_sig_invalid",
            Self::StreamingChunkMismatch { .. } => "manifest_streaming_chunk_mismatch",
            Self::StreamingChunkSizeMismatch { .. } => "manifest_streaming_chunk_size_mismatch",
            Self::StreamingSourceTruncated { .. } => "manifest_streaming_truncated",
            Self::StreamingChunkIndexUnexpected { .. } => "manifest_streaming_chunk_unexpected",
        }
    }
}

impl ManifestError {
    /// Short canonical audit code (≤ 32 chars). Mirrors the
    /// `audit_code()` discipline established in `corelink-ac::error`.
    #[must_use]
    pub fn audit_code(&self) -> &'static str {
        match self {
            Self::VersionUnsupported(_) => "manifest_version_unsupported",
            Self::ChunkCountExceeded { .. } => "manifest_chunk_count_exceeded",
            Self::TotalSizeExceeded { .. } => "manifest_total_size_exceeded",
            Self::ChunkSizeExceeded { .. } => "manifest_chunk_size_exceeded",
            Self::ChunkSizeZero(_) => "manifest_chunk_size_zero",
            Self::ChunkIndexOutOfOrder { .. } => "manifest_chunk_index_out_of_order",
            Self::ChunkCountMismatch { .. } => "manifest_chunk_count_mismatch",
            Self::TotalSizeMismatch { .. } => "manifest_total_size_mismatch",
            Self::RootMismatch => "manifest_root_mismatch",
            Self::ChunkerAlgoUnknown(_) => "manifest_chunker_algo_unknown",
            Self::Empty => "manifest_empty",
        }
    }
}
