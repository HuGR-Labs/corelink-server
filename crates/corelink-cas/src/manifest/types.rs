//! Public canonical types for the manifest crate (WI-S05-005 §1).
//!
//! Every public type is `#[non_exhaustive]` — the manifest wire format
//! is locked at v1 by ADR-0041 (forward) but additive evolution to v2+
//! is anticipated; pinning `#[non_exhaustive]` keeps the SemVer
//! discipline so additional fields don't break downstream callers.

use uuid::Uuid;

use crate::manifest::bounds::{MANIFEST_PREIMAGE_LEN, MANIFEST_SIG_LEN};

/// Length in bytes of the canonical 32-byte BLAKE3-256 digest used for
/// the blob digest, the per-chunk digests, and the manifest root.
pub const DIGEST_LEN: usize = 32;

/// Canonical manifest version emitted at v1.0. Higher versions are
/// reserved for future ADRs.
pub const MANIFEST_VERSION_V1: u8 = 1;

/// Sealed-manifest version (canonical reference; v1 at first GA per
/// ADR-0041).
pub const CURRENT_MANIFEST_VERSION: u8 = MANIFEST_VERSION_V1;

/// Chunker algorithm discriminant pinned into the canonical preimage.
///
/// Wire byte values are stable: `Fixed2MiB = 1`, `FastCdc2MiB = 2`.
/// `0` is reserved as the never-issued sentinel (a manifest signed
/// with `0` is rejected per the bounded parser).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ChunkerAlgorithm {
    /// Fixed-size 2 MiB chunker (default; matches
    /// `corelink-chunker::FixedChunker`).
    Fixed2MiB,
    /// FastCDC content-defined chunker (opt-in; matches
    /// `corelink-chunker::FastCdcChunker`).
    FastCdc2MiB,
}

impl ChunkerAlgorithm {
    /// Canonical wire byte. The byte value is pinned by ADR-0041
    /// (forward) and MUST NOT change between minor versions.
    #[must_use]
    pub const fn wire_byte(self) -> u8 {
        match self {
            Self::Fixed2MiB => 1,
            Self::FastCdc2MiB => 2,
        }
    }

    /// Parse a wire byte; rejects unknown discriminants + the reserved
    /// `0` sentinel.
    ///
    /// # Errors
    ///
    /// Returns the original byte for caller-side error mapping.
    pub const fn from_wire_byte(b: u8) -> Result<Self, u8> {
        match b {
            1 => Ok(Self::Fixed2MiB),
            2 => Ok(Self::FastCdc2MiB),
            other => Err(other),
        }
    }
}

/// Per-chunk reference inside the manifest. Mirrors the `manifest_chunks`
/// row shape from `corelink-multipart-schema` (`(tenant_id, blob_digest,
/// chunk_index, chunk_digest)`) plus the chunk's `size_bytes` (which
/// lives on the `chunks` row in D1 and is materialised here so the
/// streaming verify can perform the size check without an extra D1
/// roundtrip).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ChunkRef {
    /// 0-indexed chunk position. Strictly ascending in the canonical
    /// chunks list; the order is semantic (concatenation produces the
    /// blob bytes).
    pub index: u32,
    /// BLAKE3-256 hash of the chunk bytes.
    pub digest: [u8; DIGEST_LEN],
    /// Chunk size in bytes (`1..=MAX_CHUNK_SIZE_BYTES`).
    pub size_bytes: u32,
}

impl ChunkRef {
    /// Construct a fresh [`ChunkRef`].
    #[must_use]
    pub const fn new(index: u32, digest: [u8; DIGEST_LEN], size_bytes: u32) -> Self {
        Self {
            index,
            digest,
            size_bytes,
        }
    }
}

/// Sealed manifest envelope. Returned by [`crate::manifest::ManifestBuilder::build`]
/// and consumed by [`crate::manifest::ManifestVerifier::verify_full`] /
/// [`crate::manifest::ManifestVerifier::verify_structure`]. The streaming verifier
/// consumes the envelope MINUS its `chunks` field — the streaming surface
/// reads chunks one at a time from a [`crate::manifest::ChunkRefSource`] (the D1
/// `manifest_chunks` projection in production).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Manifest {
    /// Manifest schema version. Always `1` at v1.0.
    pub version: u8,
    /// Tenant scoping — UUIDv7 minted host-side.
    pub tenant_id: Uuid,
    /// BLAKE3-256 hash of the reassembled blob bytes (the canonical
    /// blob digest the SplitBlob handler computed).
    pub blob_digest: [u8; DIGEST_LEN],
    /// BLAKE3-256 root of the chunk-digest Merkle tree.
    pub merkle_root: [u8; DIGEST_LEN],
    /// Number of chunks bound by the manifest. Equal to `chunks.len()`
    /// — pinned via [`crate::manifest::error::ManifestError::ChunkCountMismatch`]
    /// when divergent.
    pub chunk_count: u32,
    /// Sum of `chunks[i].size_bytes`.
    pub total_size_bytes: u64,
    /// Canonical-ordered chunk references (ascending `index`,
    /// `0..chunk_count`).
    pub chunks: Vec<ChunkRef>,
    /// Wall-clock instant the manifest was sealed (unix ms).
    pub created_at_ms: u64,
    /// HKDF-SHA256 + BLAKE3 keyed-hash signature over the canonical
    /// preimage. Length = [`MANIFEST_SIG_LEN`].
    pub sig: [u8; MANIFEST_SIG_LEN],
    /// HKDF salt — TDK rotation version. `0` is the reserved sentinel
    /// per ADR-0021 §Sentinel.
    pub sig_key_id: u32,
    /// Chunker algorithm pinned into the canonical preimage so the
    /// `(blob_digest, manifest_root, chunker_algo)` triple is bound by
    /// the sig (a manifest computed under FastCDC cannot be replayed as
    /// a Fixed2MiB manifest).
    pub chunker_algo: ChunkerAlgorithm,
}

impl Manifest {
    /// Compute the canonical 102-byte preimage that the manifest
    /// signature authenticates. The byte layout is locked by WI-S05-005
    /// §6.1.5:
    ///
    /// ```text
    /// version_le_u8                 (1 byte)
    /// tenant_id_uuid                (16 bytes; raw big-endian UUID bytes)
    /// blob_digest                   (32 bytes; BLAKE3-256)
    /// merkle_root                   (32 bytes; BLAKE3-256)
    /// chunk_count_le_u32            (4 bytes)
    /// total_size_bytes_le_u64       (8 bytes)
    /// created_at_ms_le_u64          (8 bytes)
    /// chunker_algo_le_u8            (1 byte; canonical wire byte)
    /// ─────────────────────────────────── 102 bytes
    /// ```
    ///
    /// Why every field is signed: an attacker who captures a manifest
    /// `(merkle_root_X, chunk_count=25, total_size_bytes=50_MiB)` could
    /// otherwise forge a manifest with the same `merkle_root_X` but
    /// `chunk_count=80_000, total_size_bytes=160_GiB` — bounded parser
    /// would accept (within bounds) and the sig would still verify. By
    /// committing chunk_count + total_size_bytes (and chunker_algo) into
    /// the canonical preimage, the sig binds the bound-relevant
    /// metadata.
    ///
    /// `chunks: Vec<ChunkRef>` is NOT in the canonical preimage:
    /// `merkle_root` already commits to the chunk set via the tree
    /// binding. `sig` / `sig_key_id` are NOT in the canonical preimage
    /// either — those are the sig output, not its input.
    #[must_use]
    pub fn canonical_bytes(&self) -> [u8; MANIFEST_PREIMAGE_LEN] {
        let mut out = [0u8; MANIFEST_PREIMAGE_LEN];
        out[0] = self.version;
        // UUID raw bytes are 16 bytes big-endian; matches the wire
        // shape used by `corelink-ac::sig::canonical::compose`.
        let tenant_bytes = self.tenant_id.into_bytes();
        out[1..17].copy_from_slice(&tenant_bytes);
        out[17..49].copy_from_slice(&self.blob_digest);
        out[49..81].copy_from_slice(&self.merkle_root);
        out[81..85].copy_from_slice(&self.chunk_count.to_le_bytes());
        out[85..93].copy_from_slice(&self.total_size_bytes.to_le_bytes());
        out[93..101].copy_from_slice(&self.created_at_ms.to_le_bytes());
        out[101] = self.chunker_algo.wire_byte();
        out
    }
}

/// Canonical chunk-stream entry consumed by [`crate::manifest::ManifestBuilder::build`].
/// Mirrors the `corelink-chunker::Chunk` shape from upstream WI-S05-002.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkInput {
    /// BLAKE3-256 hash of the chunk bytes.
    pub digest: [u8; DIGEST_LEN],
    /// Chunk size in bytes.
    pub size_bytes: u32,
}

impl ChunkInput {
    /// Construct a fresh [`ChunkInput`].
    #[must_use]
    pub const fn new(digest: [u8; DIGEST_LEN], size_bytes: u32) -> Self {
        Self { digest, size_bytes }
    }
}
