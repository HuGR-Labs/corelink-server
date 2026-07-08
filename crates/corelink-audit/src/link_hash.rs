//! Chain-integrity primitives — [`ContentHash`], [`compute_content_hash`],
//! [`link_chain_hash`].
//!
//! ## Canonical formulae
//!
//! - **Content hash** (per-event):
//!
//!   ```text
//!   content_hash = SHA-256( JCS-canonicalize( AuthEvent ) )
//!   ```
//!
//!   The producer canonicalizes via [RFC 8785][jcs] (implemented by
//!   `serde_jcs`) and digests the resulting UTF-8 byte stream. The
//!   16-byte digest is rendered as 64 lowercase hex chars; this is the
//!   stable identity of the event for the chain.
//!
//! - **Chain link** (between consecutive events):
//!
//!   ```text
//!   chain_hash_n = SHA-256( prev_chain_hash || content_hash_n )
//!   ```
//!
//!   The chain processor (S-09) reads the persisted `content_hash`
//!   string off the audit row and computes the new `chain_hash` by
//!   concatenating the two hex strings (UTF-8 bytes — the inputs are
//!   already canonical) before SHA-256. **The chain processor never
//!   re-canonicalizes the event** — re-running JCS at chain time
//!   would risk a `serde_jcs` version drift between producer and
//!   consumer corrupting every link in the chain.
//!
//! - **Genesis link**: for the first event in a chain, `prev_chain_hash`
//!   is the canonical zero string (64 hex zeros — `"0000…0"`); this
//!   matches the "genesis block" pattern from the [audit_immutability.tla]
//!   spec.
//!
//! [jcs]: https://www.rfc-editor.org/rfc/rfc8785
//! [audit_immutability.tla]: https://github.com/HumanGuardrail/corelink-server/blob/main/specs/tla/audit_immutability.tla
//!
//! ## Determinism property (INV-AUDIT-CHAIN-HASH-DETERMINISTIC)
//!
//! [`compute_content_hash`] is byte-pure: the same `AuthEvent` value
//! produces the same `ContentHash` across all platforms, all serde
//! versions compatible with `serde_jcs = "0.2"`, and all process
//! invocations. This is asserted by the property test
//! `prop_content_hash_deterministic` (10 000 iterations) and by the
//! cross-platform canonical-vector test in
//! `tests/canonical_vectors.rs`.
//!
//! Any change that breaks determinism (a new field of type
//! `f64::NAN`, a `HashMap` of unsorted keys, etc.) is a P0 bug and
//! must land alongside an explicit ADR-0033 changelog row.

use core::fmt;
use std::ops::Deref;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::AuditError;

/// Length of the canonical content-hash hex string (SHA-256 → 32 bytes
/// → 64 hex). Constant.
pub const CONTENT_HASH_HEX_LEN: usize = 64;

/// Per-event content hash. Always 64-char lowercase hex (SHA-256 of
/// the JCS-canonicalized event payload).
///
/// Constructed exclusively by [`compute_content_hash`] — there is no
/// public `From<String>` because an unverified input could be in upper
/// case, the wrong length, or contain non-hex chars and corrupt the
/// chain.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ContentHash(String);

impl ContentHash {
    /// Borrow the canonical 64-char lowercase hex string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Deref for ContentHash {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Per-row chain hash linking the event into the audit chain.
///
/// Computed by [`link_chain_hash`] from `(prev_chain_hash, content_hash)`.
/// The genesis link uses [`ChainHash::genesis`] which is the canonical
/// 64-zero hex string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ChainHash(String);

impl ChainHash {
    /// The canonical genesis chain hash (64 zero hex chars).
    #[must_use]
    pub fn genesis() -> Self {
        Self("0".repeat(CONTENT_HASH_HEX_LEN))
    }

    /// Borrow the canonical 64-char lowercase hex string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChainHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Deref for ChainHash {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Compute the per-event content hash.
///
/// Steps:
///
/// 1. Serialize `event` via [RFC 8785 JCS][jcs] (`serde_jcs::to_vec`).
/// 2. SHA-256 the resulting UTF-8 byte stream.
/// 3. Render the 32-byte digest as 64 lowercase hex chars.
///
/// The function is generic over any `Serialize` type so the same
/// primitive can be reused for non-`AuthEvent` payloads in S-09 chain
/// fixtures.
///
/// # Errors
///
/// - [`AuditError::Serialization`] / [`AuditError::Canonicalization`]
///   if `serde_jcs` rejects the value (e.g. NaN floats, non-string
///   map keys, etc.). Should be unreachable for canonical
///   [`crate::AuthEvent`] inputs which are JSON-compatible by
///   construction.
///
/// [jcs]: https://www.rfc-editor.org/rfc/rfc8785
pub fn compute_content_hash<T: Serialize>(event: &T) -> Result<ContentHash, AuditError> {
    let canonical = serde_jcs::to_vec(event).map_err(AuditError::Serialization)?;
    let digest = Sha256::digest(&canonical);
    Ok(ContentHash(hex::encode(digest)))
}

/// Compute the chain link hash from `(prev_chain_hash, content_hash)`.
///
/// Both inputs are canonical 64-char lowercase hex strings. The
/// concatenation is UTF-8 byte-equivalent to ASCII byte-concat of the
/// two hex strings (no separator, no length prefix — the producer +
/// consumer agree on the canonical 64+64 = 128 char input).
///
/// The output is the new chain hash (also 64-char lowercase hex).
#[must_use]
pub fn link_chain_hash(prev_chain_hash: &ChainHash, content_hash: &ContentHash) -> ChainHash {
    // Concat as UTF-8 bytes. Both inputs are pure ASCII (lowercase hex),
    // so byte-concat == char-concat.
    let mut buf = Vec::with_capacity(CONTENT_HASH_HEX_LEN * 2);
    buf.extend_from_slice(prev_chain_hash.as_str().as_bytes());
    buf.extend_from_slice(content_hash.as_str().as_bytes());
    let digest = Sha256::digest(&buf);
    ChainHash(hex::encode(digest))
}

/// Verify a chain link — given the previous chain hash, the event's
/// content hash, and the claimed new chain hash, confirm `claimed`
/// matches the canonical formula.
///
/// Returns `true` iff the link is valid. Used by the chain validator
/// (WI-S09-004 forward); exposed here so the producer's tests can
/// also exercise the formula without depending on the chain crate.
#[must_use]
pub fn verify_chain_link(
    prev_chain_hash: &ChainHash,
    content_hash: &ContentHash,
    claimed: &ChainHash,
) -> bool {
    let computed = link_chain_hash(prev_chain_hash, content_hash);
    // Constant-time compare — chain hashes are not secret, but a
    // timing oracle on chain validation would still be weird; cheap
    // to harden anyway.
    use subtle::ConstantTimeEq;
    let a = computed.as_str().as_bytes();
    let b = claimed.as_str().as_bytes();
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).unwrap_u8() == 1
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test module — assertions panic by design"
)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Sample {
        b: u32,
        a: &'static str,
    }

    #[test]
    fn content_hash_jcs_canonical_orders_keys() {
        // The struct declares `b` before `a`; JCS canonicalizes by
        // sorting keys lexicographically, so the canonical bytes will
        // start with `{"a":` not `{"b":`.
        let s = Sample { b: 7, a: "x" };
        let canonical = serde_jcs::to_vec(&s).expect("jcs");
        let canonical_str = String::from_utf8(canonical).expect("utf8");
        assert!(
            canonical_str.starts_with("{\"a\":"),
            "JCS did not sort keys lexicographically: {canonical_str}"
        );
    }

    #[test]
    fn content_hash_byte_equal_on_repeated_compute() {
        let s = Sample { b: 7, a: "x" };
        let h1 = compute_content_hash(&s).expect("hash");
        let h2 = compute_content_hash(&s).expect("hash");
        assert_eq!(h1, h2);
        assert_eq!(h1.as_str().len(), CONTENT_HASH_HEX_LEN);
    }

    #[test]
    fn content_hash_diverges_on_field_change() {
        let a = Sample { b: 1, a: "x" };
        let b = Sample { b: 2, a: "x" };
        assert_ne!(
            compute_content_hash(&a).expect("hash"),
            compute_content_hash(&b).expect("hash")
        );
    }

    #[test]
    fn genesis_chain_hash_is_64_zeros() {
        assert_eq!(
            ChainHash::genesis().as_str(),
            "0000000000000000000000000000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn link_chain_hash_deterministic() {
        let prev = ChainHash::genesis();
        let content = compute_content_hash(&Sample { b: 1, a: "x" }).expect("hash");
        let h1 = link_chain_hash(&prev, &content);
        let h2 = link_chain_hash(&prev, &content);
        assert_eq!(h1, h2);
        assert_eq!(h1.as_str().len(), CONTENT_HASH_HEX_LEN);
    }

    #[test]
    fn link_chain_hash_chain_extends_correctly() {
        let prev = ChainHash::genesis();
        let c1 = compute_content_hash(&Sample { b: 1, a: "x" }).expect("hash");
        let h1 = link_chain_hash(&prev, &c1);
        let c2 = compute_content_hash(&Sample { b: 2, a: "y" }).expect("hash");
        let h2 = link_chain_hash(&h1, &c2);
        // Each link must be distinct from the previous, and h2 must
        // depend on h1 (changing prev would change h2).
        assert_ne!(h1, h2);
        let h2_genesis = link_chain_hash(&ChainHash::genesis(), &c2);
        assert_ne!(h2, h2_genesis);
    }

    #[test]
    fn verify_chain_link_accepts_correct_link() {
        let prev = ChainHash::genesis();
        let c = compute_content_hash(&Sample { b: 1, a: "x" }).expect("hash");
        let h = link_chain_hash(&prev, &c);
        assert!(verify_chain_link(&prev, &c, &h));
    }

    #[test]
    fn verify_chain_link_rejects_tampered_link() {
        let prev = ChainHash::genesis();
        let c = compute_content_hash(&Sample { b: 1, a: "x" }).expect("hash");
        let h = link_chain_hash(&prev, &c);
        // Tamper: flip first hex char.
        let tampered_str = format!("1{}", &h.as_str()[1..]);
        let tampered = ChainHash(tampered_str);
        assert!(!verify_chain_link(&prev, &c, &tampered));
    }

    #[test]
    fn rfc8785_a3_test_vector_smoke() {
        // RFC 8785 Annex A.3 (numbers): {"numbers":[333333333.3333333,1E30,4.5]}
        // canonicalizes to a known string. We don't depend on that exact
        // string here (the canonicalizer is the audited `serde_jcs`); we
        // assert the determinism property + non-empty digest.
        #[derive(Serialize)]
        struct V {
            numbers: Vec<f64>,
        }
        let v = V {
            numbers: vec![333_333_333.333_333_3, 1e30, 4.5],
        };
        let h1 = compute_content_hash(&v).expect("hash");
        let h2 = compute_content_hash(&v).expect("hash");
        assert_eq!(h1, h2);
        assert_eq!(h1.as_str().len(), CONTENT_HASH_HEX_LEN);
    }
}
