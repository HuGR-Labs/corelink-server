//! Canonical wire shapes for the Action Cache envelope (WI-S04-003 §1).
//!
//! These types mirror the REAPI v2 `ActionResult` proto subset the
//! Merkle codec consumes, plus the CoreLink-specific [`AcEnvelope`]
//! surface (with `version` / `tenant_id` / `merkle_root` / `result_hash`
//! / sig fields). The worker handler's `ActionResult` type
//! (`corelink-worker::reapi::ac::types::ActionResult`) is the same
//! field set with handler-internal helpers; a thin
//! [`From`]-style adapter at the integration seam moves between the
//! two without copying the digest set.

use core::fmt;

use corelink_hash::Digest;
use serde::{Deserialize, Serialize};

/// Serde adapter rendering a [`Digest`] as a 64-char lowercase hex
/// string in JSON. Inverse rejects any input that does not parse via
/// `Digest::from_hex` — preserves the canonical encoding.
pub mod digest_serde {
    use corelink_hash::Digest;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize a [`Digest`] as a 64-char lowercase hex string.
    ///
    /// # Errors
    ///
    /// Surface any error produced by the serializer.
    pub fn serialize<S>(d: &Digest, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.serialize_str(&d.to_hex())
    }

    /// Deserialize a [`Digest`] from a 64-char lowercase hex string.
    ///
    /// # Errors
    ///
    /// Surface a serde error if the input is not a valid hex digest.
    pub fn deserialize<'de, D>(de: D) -> Result<Digest, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(de)?;
        Digest::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// Serde adapter rendering a `[u8; 32]` as a 64-char lowercase hex
/// string. Used for the envelope's `merkle_root` + `result_hash`
/// fields so the JSON wire form is stable + human-readable.
pub mod hex32_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize a 32-byte array as 64-char lowercase hex.
    ///
    /// # Errors
    ///
    /// Surface any error produced by the serializer.
    pub fn serialize<S>(bytes: &[u8; 32], ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.serialize_str(&hex::encode(bytes))
    }

    /// Deserialize a 64-char lowercase hex string into a 32-byte array.
    ///
    /// # Errors
    ///
    /// Surface a serde error if the input is not a valid 64-char hex
    /// string.
    pub fn deserialize<'de, D>(de: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(de)?;
        let raw = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let arr: [u8; 32] = raw
            .as_slice()
            .try_into()
            .map_err(|_| serde::de::Error::custom("hex32: expected 32-byte input"))?;
        Ok(arr)
    }
}

/// REAPI v2 `Digest` shape — `(hash, size_bytes)` 2-tuple identity.
///
/// Per WI-S02-002 lesson 2, REAPI digest identity is **both** the
/// 32-byte BLAKE3 hash AND the declared `size_bytes`. We keep both
/// fields in the wire shape but the Merkle tree binds only the
/// hash bytes (the size is metadata; rejecting `(H, wrong_size)`
/// is the handler's `(tenant_id, action_digest, size)` cross-check
/// concern, not a Merkle invariant).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionDigest {
    /// 32-byte BLAKE3 hash of the canonical Action proto.
    #[serde(with = "digest_serde")]
    pub hash: Digest,
    /// Declared body size in bytes.
    pub size_bytes: i64,
}

impl ActionDigest {
    /// Construct a fresh [`ActionDigest`].
    #[must_use]
    pub const fn new(hash: Digest, size_bytes: i64) -> Self {
        Self { hash, size_bytes }
    }

    /// Borrow the canonical 64-char lowercase hex of the hash.
    #[must_use]
    pub fn hash_hex(&self) -> String {
        self.hash.to_hex()
    }
}

impl fmt::Display for ActionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blake3:{}/{}", self.hash.to_hex(), self.size_bytes)
    }
}

/// REAPI v2 `OutputFile` digest reference. The CoreLink Merkle codec
/// only consumes the `digest` field; `path` + `is_executable` etc. are
/// round-tripped via the embedded canonical `raw_proto_bytes` if the
/// caller needs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutputFileDigest {
    /// 32-byte BLAKE3 of the output blob bytes.
    #[serde(with = "digest_serde")]
    pub digest: Digest,
    /// Output blob size in bytes.
    pub size_bytes: i64,
}

impl OutputFileDigest {
    /// Construct a fresh [`OutputFileDigest`].
    #[must_use]
    pub const fn new(digest: Digest, size_bytes: i64) -> Self {
        Self { digest, size_bytes }
    }
}

/// REAPI v2 `OutputDirectory` digest reference (Tree proto).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutputDirectoryDigest {
    /// 32-byte BLAKE3 of the canonical Tree proto.
    #[serde(with = "digest_serde")]
    pub digest: Digest,
    /// Tree proto size in bytes.
    pub size_bytes: i64,
}

impl OutputDirectoryDigest {
    /// Construct a fresh [`OutputDirectoryDigest`].
    #[must_use]
    pub const fn new(digest: Digest, size_bytes: i64) -> Self {
        Self { digest, size_bytes }
    }
}

/// Pure projection of the REAPI v2 `ActionResult` proto fields the
/// CoreLink Merkle codec binds.
///
/// `raw_proto_bytes` is the canonical REAPI serialization retained
/// verbatim so the handler can echo the original bytes back on
/// idempotent UPDATEs without round-tripping through prost. The
/// Merkle root is computed exclusively from the `output_files` +
/// `output_directories` digest set (lex-sorted by digest bytes per
/// ADR-0037) — `exit_code` and `raw_proto_bytes` are NOT bound by the
/// tree itself; the surrounding `AcEnvelope` `result_hash` (= BLAKE3
/// of `merkle_root`) plus the HKDF sig of the canonical envelope
/// preimage delivers the binding for the surrounding fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResult {
    /// `OutputFile` digest references — every digest must be alive in
    /// `blob_meta` at UPDATE time per `INV-AC-OUTPUTS-VALID`.
    pub output_files: Vec<OutputFileDigest>,
    /// `OutputDirectory` (Tree proto) digest references.
    pub output_directories: Vec<OutputDirectoryDigest>,
    /// `exit_code`. Round-tripped only.
    pub exit_code: i32,
    /// Canonical REAPI v2 ActionResult proto bytes — held verbatim so
    /// the handler echoes the original form on idempotent re-update
    /// (REAPI conformance).
    pub raw_proto_bytes: Vec<u8>,
}

impl ActionResult {
    /// Construct an [`ActionResult`] from its canonical fields.
    #[must_use]
    pub fn new(
        output_files: Vec<OutputFileDigest>,
        output_directories: Vec<OutputDirectoryDigest>,
        exit_code: i32,
        raw_proto_bytes: Vec<u8>,
    ) -> Self {
        Self {
            output_files,
            output_directories,
            exit_code,
            raw_proto_bytes,
        }
    }

    /// Aggregate count of output blobs (files + directories).
    #[must_use]
    pub fn output_count(&self) -> usize {
        self.output_files.len() + self.output_directories.len()
    }

    /// Borrow every output digest in canonical order — files first,
    /// then directories. Used by the outputs-aliveness check.
    pub fn iter_output_digests(&self) -> impl Iterator<Item = Digest> + '_ {
        self.output_files
            .iter()
            .map(|o| o.digest)
            .chain(self.output_directories.iter().map(|o| o.digest))
    }
}

/// CoreLink AC envelope canonical version byte (v1).
pub const AC_ENVELOPE_VERSION: u8 = 1;

/// Length of the BLAKE3 Merkle root in bytes.
pub const MERKLE_ROOT_LEN: usize = 32;

/// Length of the BLAKE3-of-merkle-root index column (32 bytes raw,
/// `2 * MERKLE_ROOT_LEN` chars when hex-encoded).
pub const RESULT_HASH_LEN: usize = 32;

/// CoreLink Action Cache envelope (WI-S04-003 §1 + ADR-0037).
///
/// Serializes to / deserializes from JSON via [`crate::ac_core::codec`]; the
/// Merkle root is the canonical authority for the output binding,
/// `result_hash` is the index-column derivation
/// (`BLAKE3(merkle_root)`) per ADR-0037 §Decision. Forward-compat
/// fields are admitted via `#[non_exhaustive]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AcEnvelope {
    /// Envelope version. `v1` is the only canonical value at S-04 GA.
    pub version: u8,
    /// Owning tenant id (canonical hyphenated UUID v7).
    pub tenant_id: String,
    /// REAPI v2 ActionDigest identity.
    pub action_digest: ActionDigest,
    /// REAPI v2 ActionResult.
    pub result: ActionResult,
    /// 32-byte BLAKE3 Merkle root over the result's output digests.
    #[serde(with = "hex32_serde")]
    pub merkle_root: [u8; MERKLE_ROOT_LEN],
    /// 32-byte `BLAKE3(merkle_root)` index column (per ADR-0037).
    #[serde(with = "hex32_serde")]
    pub result_hash: [u8; RESULT_HASH_LEN],
    /// Wall-clock millis at envelope construction (Unix epoch).
    pub created_at_ms: u64,
    /// HKDF-SHA256 signature bytes (delegate WI-S04-004 supplies
    /// the real signer; this crate ships the verifier trait surface).
    /// Empty for envelopes built with the no-op signer in tests.
    pub sig: Vec<u8>,
    /// HKDF tenant-key version (must be `> 0`; `0` is the reserved
    /// sentinel per WI-S04-004 P0-R5-001).
    pub sig_key_id: u32,
}

impl AcEnvelope {
    /// Construct a fresh envelope with `version = v1`. Public for
    /// tests + integration adapters; the canonical builder is
    /// [`crate::ac_core::merkle::build`].
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "envelope is canonical 8-field shape per ADR-0037; constructor mirrors the wire surface 1:1"
    )]
    pub const fn new(
        tenant_id: String,
        action_digest: ActionDigest,
        result: ActionResult,
        merkle_root: [u8; MERKLE_ROOT_LEN],
        result_hash: [u8; RESULT_HASH_LEN],
        created_at_ms: u64,
        sig: Vec<u8>,
        sig_key_id: u32,
    ) -> Self {
        Self {
            version: AC_ENVELOPE_VERSION,
            tenant_id,
            action_digest,
            result,
            merkle_root,
            result_hash,
            created_at_ms,
            sig,
            sig_key_id,
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
    fn action_digest_display_format() {
        let h = Digest::compute(b"action");
        let ad = ActionDigest::new(h, 42);
        let s = format!("{ad}");
        assert!(s.starts_with("blake3:"));
        assert!(s.ends_with("/42"));
    }

    #[test]
    fn output_count_sums_files_and_directories() {
        let h = Digest::compute(b"x");
        let r = ActionResult::new(
            vec![
                OutputFileDigest::new(h, 1),
                OutputFileDigest::new(h, 2),
            ],
            vec![OutputDirectoryDigest::new(h, 3)],
            0,
            Vec::new(),
        );
        assert_eq!(r.output_count(), 3);
        let collected: Vec<Digest> = r.iter_output_digests().collect();
        assert_eq!(collected.len(), 3);
    }

    #[test]
    fn envelope_constants() {
        assert_eq!(AC_ENVELOPE_VERSION, 1);
        assert_eq!(MERKLE_ROOT_LEN, 32);
        assert_eq!(RESULT_HASH_LEN, 32);
    }
}
