//! Canonical newtypes that travel across the SplitBlob/SpliceBlob
//! handler trait boundary (WI-S05-001 §1, §6.1).
//!
//! Mirrors the shape pattern already canonical for `reapi::ac::types` —
//! every digest is a typed BLAKE3 wrapper so the trait surfaces refuse
//! to compile against an opaque `[u8; 32]` (no accidental cross-domain
//! confusion between blob, chunk, and manifest digests).

use core::fmt;

use corelink_hash::Digest;
use uuid::Uuid;

/// Canonical maximum chunks a single blob can decompose into. Mirrors
/// `corelink-manifest` (forward) per sprint contract §5.1 P1-SR5-001.
pub const MAX_CHUNKS_PER_BLOB: u32 = 81920;

/// REAPI v2 `Digest` shape for the original (pre-split) blob — `(hash,
/// size_bytes)` 2-tuple identity. Mirrors the AC handler's
/// [`ActionDigest`](crate::reapi::ac::ActionDigest) shape so the SplitBlob
/// flow preserves REAPI v2 wire compatibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlobDigest {
    /// 32-byte BLAKE3 hash of the blob's bytes.
    hash: Digest,
    /// Declared body size in bytes (REAPI v2 `Digest.size_bytes`). Held
    /// as `u64` because chunked blobs are explicitly authorized for
    /// large sizes (sprint contract §5.1 — up to 160 GiB single-session
    /// multipart).
    size_bytes: u64,
}

impl BlobDigest {
    /// Construct a fresh [`BlobDigest`].
    #[must_use]
    pub const fn new(hash: Digest, size_bytes: u64) -> Self {
        Self { hash, size_bytes }
    }

    /// Borrow the BLAKE3 hash.
    #[must_use]
    pub const fn hash(&self) -> &Digest {
        &self.hash
    }

    /// Declared blob size in bytes.
    #[must_use]
    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Borrow the canonical 64-char lowercase hex of the hash.
    #[must_use]
    pub fn hash_hex(&self) -> String {
        self.hash.to_hex()
    }
}

impl fmt::Display for BlobDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blake3:{}/{}", self.hash.to_hex(), self.size_bytes)
    }
}

/// Per-chunk content-addressed digest — BLAKE3 over the chunk bytes.
/// Distinct newtype from [`BlobDigest`] so cross-domain confusion (e.g.
/// fetching a blob digest as a chunk) is a compile error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkDigest(Digest);

impl ChunkDigest {
    /// Wrap a pre-computed digest.
    #[must_use]
    pub const fn from_digest(d: Digest) -> Self {
        Self(d)
    }

    /// Compute the canonical [`ChunkDigest`] over chunk bytes.
    #[must_use]
    pub fn compute(chunk_bytes: &[u8]) -> Self {
        Self(Digest::compute(chunk_bytes))
    }

    /// Borrow the inner [`Digest`].
    #[must_use]
    pub const fn as_digest(&self) -> &Digest {
        &self.0
    }

    /// Canonical 64-char lowercase hex form.
    #[must_use]
    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }
}

impl fmt::Display for ChunkDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_hex())
    }
}

/// Manifest-tree root digest — BLAKE3 over the canonical manifest
/// envelope's chunk-digest sequence (delegate WI-S05-005). Distinct
/// newtype from [`BlobDigest`] / [`ChunkDigest`] so downstream call
/// sites cannot accidentally splice a blob using a chunk digest as the
/// manifest key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManifestDigest(Digest);

impl ManifestDigest {
    /// Wrap a pre-computed digest.
    #[must_use]
    pub const fn from_digest(d: Digest) -> Self {
        Self(d)
    }

    /// Construct from 32 raw bytes by routing through the canonical
    /// hex encoding so the manifest digest goes through the same
    /// ParseError-checked surface every other digest in the system
    /// uses. Convenience helper for property-test wiring + the
    /// handler's "lookup unknown manifest" path; production manifest
    /// digests come from the assembler's BLAKE3 build root.
    ///
    /// # Panics
    ///
    /// Never panics in practice (the hex encoding round-trip is
    /// total over `[u8; 32]`). The internal `expect` carries a
    /// `&'static` reason so the lint allow-list can audit it.
    #[must_use]
    #[allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "round-trip over a deterministically-formed 64-char [a-f0-9] string is total; index-based writes target a stack-pinned `[u8; 64]` whose bounds are statically derived from the input `[u8; 32]`"
    )]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        let mut buf = [0u8; 64];
        const NIBBLE: &[u8; 16] = b"0123456789abcdef";
        for (i, b) in bytes.iter().enumerate() {
            buf[i * 2] = NIBBLE[(b >> 4) as usize];
            buf[i * 2 + 1] = NIBBLE[(b & 0x0F) as usize];
        }
        let s = core::str::from_utf8(&buf)
            .expect("manual lowercase-hex of [u8; 32] is always valid ASCII");
        Self(Digest::from_hex(s).expect("manual lowercase-hex of [u8; 32] always parses"))
    }

    /// Borrow the inner [`Digest`].
    #[must_use]
    pub const fn as_digest(&self) -> &Digest {
        &self.0
    }

    /// Canonical 64-char lowercase hex form.
    #[must_use]
    pub fn to_hex(&self) -> String {
        self.0.to_hex()
    }
}

impl fmt::Display for ManifestDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_hex())
    }
}

/// Server-minted multipart-session id — UUIDv7 in production wiring;
/// the trait surface accepts any UUID (the in-memory fake mints
/// deterministically via UUIDv4-style construction).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Wrap a [`Uuid`].
    #[must_use]
    pub const fn new(id: Uuid) -> Self {
        Self(id)
    }

    /// Borrow the underlying [`Uuid`].
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Canonical chunk index — `u32` newtype so the index column type is
/// explicit at every call site (the sprint contract names a 32-bit
/// chunk index everywhere; the bound `MAX_CHUNKS_PER_BLOB = 81920` fits
/// comfortably).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkIndex(pub u32);

impl fmt::Display for ChunkIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
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
    fn blob_digest_display_includes_blake3_prefix() {
        let h = Digest::compute(b"blob-bytes");
        let bd = BlobDigest::new(h, 1234);
        let s = format!("{bd}");
        assert!(s.starts_with("blake3:"));
        assert!(s.ends_with("/1234"));
    }

    #[test]
    fn chunk_digest_compute_matches_blake3() {
        let cd = ChunkDigest::compute(b"chunk-bytes");
        assert_eq!(cd.as_digest(), &Digest::compute(b"chunk-bytes"));
    }

    #[test]
    fn manifest_digest_from_bytes_round_trips() {
        let raw = [42u8; 32];
        let md = ManifestDigest::from_bytes(raw);
        // Hex of the input bytes via the same canonical lowercase
        // mapping the helper uses.
        let mut expected_hex = String::with_capacity(64);
        for b in raw {
            expected_hex.push_str(&format!("{b:02x}"));
        }
        assert_eq!(md.to_hex(), expected_hex);
    }

    #[test]
    fn newtypes_are_distinct_types_at_display() {
        // The three digest newtypes wrap the same underlying BLAKE3
        // bytes, so their hex form coincides — but the BlobDigest
        // Display carries a `/<size>` suffix, distinguishing it. The
        // value of this test is mostly that the trio compiles as
        // separate types: a generic `<D: SomeTrait>` slot that wants
        // a ChunkDigest cannot accept a BlobDigest, etc. (verified
        // structurally elsewhere — the SessionStore signature takes
        // `ChunkDigest` exclusively).
        let bd = BlobDigest::new(Digest::compute(b"a"), 1);
        let cd = ChunkDigest::compute(b"a");
        let md = ManifestDigest::from_digest(Digest::compute(b"a"));
        // Inner hex coincides — by construction; cross-domain
        // confusion is prevented at the type level, not the bytes.
        assert_eq!(cd.to_hex(), md.to_hex());
        // BlobDigest carries the size suffix.
        assert!(bd.to_string().contains('/'));
        assert!(!cd.to_string().contains('/'));
        assert!(!md.to_string().contains('/'));
    }

    #[test]
    fn max_chunks_constant_matches_sprint_contract() {
        // Sprint contract §5.1 P1-SR5-001 cross-crate alignment.
        assert_eq!(MAX_CHUNKS_PER_BLOB, 81920);
    }
}
