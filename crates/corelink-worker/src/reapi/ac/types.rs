//! Canonical shapes that travel across the AC handler trait boundary
//! (WI-S04-001 §1 + §6.1).
//!
//! These mirror the REAPI v2 `ActionResult` proto fields the handler
//! consumes; the full proto codec (with prost) lives in the
//! `corelink-reapi` host-server feature alongside the gRPC service
//! wrapper. We only expose the **subset** the pure-logic handler reads
//! / writes, which is enough to satisfy every Gherkin scenario in WI §8.

use core::fmt;

use corelink_hash::Digest;

/// REAPI v2 `Digest` shape — `(hash, size_bytes)` 2-tuple identity.
///
/// Per WI-S02-002 lesson 2 the REAPI digest identity is **both** the
/// 32-byte BLAKE3 hash AND the declared `size_bytes`; a handler that
/// keys lookups by hash alone silently maps `(H, wrong_size)` to
/// "present" when `(H, real_size)` exists. The AC handler therefore
/// preserves both fields; the canonical [`AcKey`](super::AcKey) builds
/// from `(tenant_id, action_digest.hash)` (size is opaque metadata, not
/// part of the PK) but the audit envelope + sig payload + REAPI wire
/// echo carry both fields per spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActionDigest {
    /// 32-byte BLAKE3 hash of the canonical Action proto.
    pub hash: Digest,
    /// Declared body size in bytes (REAPI v2 `Digest.size_bytes`).
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

/// REAPI v2 `OutputFile` digest reference — referenced output blob.
///
/// The full REAPI `OutputFile` proto carries `path` + `digest` +
/// `is_executable` flags; the *aliveness check* the handler performs
/// only consumes the digest. `path` + `is_executable` round-trip via
/// [`ActionResult::raw_proto_bytes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutputFileDigest {
    /// 32-byte BLAKE3 of the output blob bytes.
    pub digest: Digest,
    /// Output blob size in bytes (REAPI v2 `Digest.size_bytes`).
    pub size_bytes: i64,
}

impl OutputFileDigest {
    /// Construct a fresh [`OutputFileDigest`].
    #[must_use]
    pub const fn new(digest: Digest, size_bytes: i64) -> Self {
        Self { digest, size_bytes }
    }
}

/// REAPI v2 `OutputDirectory` digest reference. Distinct from
/// [`OutputFileDigest`] so the handler can tag audit events with the
/// originating slot kind on outputs-missing failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutputDirectoryDigest {
    /// 32-byte BLAKE3 of the canonical Tree proto.
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

/// Pure-logic projection of REAPI v2 `ActionResult` proto for the
/// CoreLink AC handler.
///
/// Field set covers everything the handler needs to enforce
/// `INV-AC-OUTPUTS-VALID`, sign + verify the AC envelope, and round-
/// trip the proto echo on idempotent UPDATEs. The `raw_proto_bytes`
/// field is the canonical serialized REAPI form retained verbatim so
/// the gRPC wrapper in `corelink-reapi` can echo the original bytes
/// without re-encoding (avoids any CE 1.0 retry-stability footgun
/// analogous to WI-S01-005 lesson 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionResult {
    /// `OutputFile` digest references — every digest must be alive in
    /// `blob_meta` at UPDATE time per `INV-AC-OUTPUTS-VALID`.
    pub output_files: Vec<OutputFileDigest>,
    /// `OutputDirectory` (Tree proto) digest references — same
    /// aliveness requirement.
    pub output_directories: Vec<OutputDirectoryDigest>,
    /// `exit_code`. Round-tripped only.
    pub exit_code: i32,
    /// Canonical REAPI v2 ActionResult proto bytes — held verbatim so
    /// the handler echoes the original form on idempotent re-update
    /// (REAPI conformance) and so the audit envelope can hash the same
    /// canonical bytes the client sent.
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
    /// then directories. Exposed so the handler's outputs-aliveness
    /// check + audit envelope can iterate without cloning.
    pub fn iter_output_digests(&self) -> impl Iterator<Item = Digest> + '_ {
        self.output_files
            .iter()
            .map(|o| o.digest)
            .chain(self.output_directories.iter().map(|o| o.digest))
    }
}

/// Canonical 32-byte BLAKE3 of the REAPI canonical `ActionResult` proto
/// bytes — opaque integrity binding kept on the `ac_meta` row + sig
/// envelope (per ADR-0037 / Lote 10.4-tris P0-R5-003).
///
/// Computed by the handler via
/// `corelink_hash::Digest::compute(result.raw_proto_bytes)` — the
/// same BLAKE3-256 already used everywhere in CAS (`corelink-hash`
/// WI-S01-002). Carries no cryptographic security claim by itself
/// (per ADR-0037 Lote 10.4-tris fix); the binding is via the HKDF
/// envelope sig (CTRL-AC-002), this is the index column for
/// idempotent re-update detection (UPDATE same digest with same
/// result_hash = no-op vs UPDATE same digest with mismatched
/// result_hash = 409 `COR_AC_RESULT_HASH_MISMATCH`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResultHash(Digest);

impl ResultHash {
    /// Compute the canonical [`ResultHash`] over `result.raw_proto_bytes`.
    #[must_use]
    pub fn compute(result: &ActionResult) -> Self {
        Self(Digest::compute(&result.raw_proto_bytes))
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

impl fmt::Display for ResultHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_hex())
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
    fn action_digest_display_includes_blake3_prefix() {
        let h = Digest::compute(b"action-bytes");
        let ad = ActionDigest::new(h, 1234);
        let s = format!("{ad}");
        assert!(s.starts_with("blake3:"));
        assert!(s.ends_with("/1234"));
    }

    #[test]
    fn action_result_output_count_sums_files_and_dirs() {
        let h = Digest::compute(b"x");
        let r = ActionResult::new(
            vec![
                OutputFileDigest::new(h, 1),
                OutputFileDigest::new(h, 2),
                OutputFileDigest::new(h, 3),
            ],
            vec![OutputDirectoryDigest::new(h, 4)],
            0,
            b"{}".to_vec(),
        );
        assert_eq!(r.output_count(), 4);
    }

    #[test]
    fn iter_output_digests_files_first() {
        let h_a = Digest::compute(b"a");
        let h_b = Digest::compute(b"b");
        let r = ActionResult::new(
            vec![OutputFileDigest::new(h_a, 1)],
            vec![OutputDirectoryDigest::new(h_b, 2)],
            0,
            Vec::new(),
        );
        let collected: Vec<Digest> = r.iter_output_digests().collect();
        assert_eq!(collected, vec![h_a, h_b]);
    }

    #[test]
    fn result_hash_is_blake3_of_raw_bytes() {
        let bytes = b"canonical-action-result".to_vec();
        let r = ActionResult::new(Vec::new(), Vec::new(), 0, bytes.clone());
        let rh = ResultHash::compute(&r);
        assert_eq!(rh.as_digest(), &Digest::compute(&bytes));
    }

    #[test]
    fn result_hash_changes_on_byte_flip() {
        let r1 = ActionResult::new(Vec::new(), Vec::new(), 0, b"hello".to_vec());
        let r2 = ActionResult::new(Vec::new(), Vec::new(), 0, b"hellp".to_vec());
        assert_ne!(ResultHash::compute(&r1), ResultHash::compute(&r2));
    }
}
