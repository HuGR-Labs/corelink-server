//! `corelink-privacy-pseudonymize` — canonical pseudonymization helper
//! for the CoreLink privacy pipeline (WI-S11-002).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! cryptographic primitive** that every pseudonymized backend in
//! `corelink-privacy-erasure-worker` invokes plus the verify
//! (re-derive) surface used by forensic re-correlation. Production
//! wiring at WI-S11-008 binds the `PseudonymVerifier` trait to the
//! customer-controlled erasure_salt vault (BYOK KMS deferred to S-14
//! per ADR-S11-003 interim).
//!
//! Specifically, the crate ships:
//!
//! 1. [`PseudonymHash`] — typed 32-byte SHA-256 output wrapper +
//!    canonical 64-char lowercase hex rendering.
//! 2. [`pseudonymize`] — `sha256(subject_id || erasure_salt)` canonical
//!    helper. Per WI-S11-002 §9.3 DD-002: SHA-256 is the FIPS 180-4
//!    regulatory-defensible choice over BLAKE3 (NIST not yet approved
//!    BLAKE3 for regulatory contexts; Privacy Officer signature on
//!    ADR-S11-003 interim documents the trade-off).
//! 3. [`pseudonymize_subject_id`] — wrapper that takes a [`uuid::Uuid`]
//!    subject id + 32-byte salt + returns the canonical [`PseudonymHash`].
//! 4. [`verify_pseudonym`] — re-derive helper used by forensic
//!    re-correlation: holding the customer-controlled erasure_salt + a
//!    candidate subject_id + a previously-recorded pseudonym, returns
//!    true if the pseudonym matches the canonical re-derived value
//!    (constant-time comparison via [`subtle::ConstantTimeEq`] so a
//!    timing-side-channel can never leak a partial match).
//! 5. [`PII_REDACTED_MARKER_KEY`] + [`PII_REDACTED_MARKER_VALUE`] —
//!    canonical marker constants. The pseudonymized backend writers
//!    insert `{ "pii_redacted": "true" }` (alongside the pseudonym) so
//!    the 24h verification job sweep can assert 100% marker presence
//!    per WI-S11-002 AC-002.
//! 6. [`PseudonymizationMarker`] — typed marker shape for serde
//!    interoperability with the audit chain envelope + report.
//!
//! # GDPR / WP29 alignment
//!
//! GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 (endorsed by EDPB
//! anonymization techniques) recognises pseudonymization as the canonical
//! escape valve when a competing legal obligation (e.g. SOC 2 audit log
//! immutability, LGPD Art. 16 fiscal retention 5y, R2 Object Lock 7y)
//! prevents physical erasure. The canonical pseudonymization rule is:
//!
//! ```text
//! pseudonym(subject_id, erasure_salt) = sha256(subject_id || erasure_salt)
//! ```
//!
//! The `erasure_salt` is per-tenant + per-DSR scope (forward secrecy
//! invariant per WI-S11-002 §9.3 DD-003); production wiring binds the
//! salt to a customer-controlled vault (interim D1 vault per
//! ADR-S11-003; full BYOK KMS deferred to S-14).
//!
//! # Anti-patterns avoided
//!
//! - Global salt reuse (cross-DSR correlation; forward secrecy
//!   violation; GDPR Recital 26 violation).
//! - BLAKE3 without ADR rationale (not FIPS 180-4 approved for
//!   regulatory contexts).
//! - Plaintext salt at-rest (CTRL-CRYPTO-002 violation; production
//!   wiring AES-256-GCM encrypts the D1 vault interim per ADR-S11-003).
//! - Non-constant-time pseudonym verification (timing side-channel
//!   leak; mitigated via [`subtle::ConstantTimeEq`]).
//!
//! # `wasm32-unknown-unknown` compatibility
//!
//! This crate compiles clean to `wasm32-unknown-unknown` (the canonical
//! Cloudflare Worker target). The SHA-256 implementation is via the
//! `sha2` crate which is wasm32-OK; no `ring` / no C toolchain
//! dependency. Verified per WI-S11-002 verification commands.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

/// Canonical pseudonym hex-string length: 64 lowercase chars (32-byte
/// SHA-256 output rendered as lowercase hex).
pub const PSEUDONYM_HEX_LEN: usize = 64;

/// Canonical 32-byte erasure salt size. Per WI-S11-002 §9.3 DD-003:
/// per-tenant + per-DSR scope (NOT global). Forward secrecy: even if
/// a single salt leaks, prior + future DSRs remain unrecoverable.
pub const ERASURE_SALT_LEN: usize = 32;

/// Canonical PII-redacted marker key. Pseudonymized backend writers
/// insert `{ Self::PII_REDACTED_MARKER_KEY: Self::PII_REDACTED_MARKER_VALUE }`
/// alongside the substituted pseudonym so the 24h verification job
/// sweep asserts 100% marker presence per WI-S11-002 AC-002.
pub const PII_REDACTED_MARKER_KEY: &str = "pii_redacted";

/// Canonical PII-redacted marker value (string `"true"`; the JSON
/// schema for pseudonymized rows uses string-typed booleans for
/// cross-backend interop with stores that lack native boolean
/// columns — D1 numeric, R2 metadata header).
pub const PII_REDACTED_MARKER_VALUE: &str = "true";

/// Canonical typed marker shape used by the audit chain envelope + the
/// 24h verification report. Serializes to JSON as
/// `{ "pii_redacted": "true", "pseudonym": "<64-char hex>" }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PseudonymizationMarker {
    /// Canonical PII-redacted marker (always the string `"true"`;
    /// pinned via [`PII_REDACTED_MARKER_VALUE`] so cross-component
    /// regression tests can assert presence verbatim).
    pub pii_redacted: String,
    /// Canonical 64-char lowercase hex pseudonym
    /// (`sha256(subject_id || erasure_salt)`).
    pub pseudonym: String,
}

impl PseudonymizationMarker {
    /// Construct a fresh marker from a [`PseudonymHash`].
    #[must_use]
    pub fn from_hash(hash: PseudonymHash) -> Self {
        Self {
            pii_redacted: PII_REDACTED_MARKER_VALUE.to_string(),
            pseudonym: hash.to_hex(),
        }
    }
}

/// Canonical 32-byte SHA-256 pseudonym output. Wraps the digest so the
/// trait surface guarantees `(subject_id, erasure_salt) → 32 bytes`
/// without leaking the underlying [u8; 32] representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PseudonymHash([u8; 32]);

impl PseudonymHash {
    /// Construct from raw 32 bytes (used by deserialization paths).
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying 32-byte digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Render as canonical 64-char lowercase hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl core::fmt::Display for PseudonymHash {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Canonical pseudonymization primitive:
/// `sha256(subject_id_bytes || erasure_salt)`.
///
/// Per WI-S11-002 §9.3 DD-002: SHA-256 (FIPS 180-4) is the
/// regulatory-defensible choice (Privacy Officer signature on
/// ADR-S11-003 documents the trade-off vs BLAKE3 which is NIST-pending).
///
/// The output is deterministic: replaying the same `(subject_id_bytes,
/// erasure_salt)` always produces the same 32-byte digest. This is the
/// load-bearing invariant for forensic re-correlation in a court-ordered
/// production hold (the customer holds the salt under BYOK; producing
/// the salt + this surface re-derives the pseudonym + asserts whether
/// the audit row corresponds to a given subject).
#[must_use]
pub fn pseudonymize(
    subject_id_bytes: &[u8],
    erasure_salt: &[u8; ERASURE_SALT_LEN],
) -> PseudonymHash {
    let mut hasher = Sha256::new();
    hasher.update(subject_id_bytes);
    hasher.update(erasure_salt);
    let digest = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&digest);
    PseudonymHash(bytes)
}

/// Canonical pseudonymization helper for [`uuid::Uuid`] subject ids
/// (the canonical PAT principal post-authn shape across the CoreLink
/// codebase).
///
/// Equivalent to `pseudonymize(subject_id.as_bytes(), erasure_salt)`.
#[must_use]
pub fn pseudonymize_subject_id(
    subject_id: Uuid,
    erasure_salt: &[u8; ERASURE_SALT_LEN],
) -> PseudonymHash {
    pseudonymize(subject_id.as_bytes(), erasure_salt)
}

/// Verify a candidate `(subject_id, erasure_salt)` pair re-derives the
/// supplied [`PseudonymHash`]. Used by forensic re-correlation flows:
/// the customer (BYOK key holder; ADR-S11-003 interim D1 vault path)
/// produces the salt under court order, this surface proves the
/// audit-row pseudonym corresponds to the disclosed subject_id.
///
/// Comparison is constant-time via [`subtle::ConstantTimeEq`] so a
/// partial-match timing side channel cannot leak prefix bytes.
#[must_use]
pub fn verify_pseudonym(
    candidate_subject_id: Uuid,
    erasure_salt: &[u8; ERASURE_SALT_LEN],
    expected: &PseudonymHash,
) -> bool {
    let recomputed = pseudonymize_subject_id(candidate_subject_id, erasure_salt);
    recomputed.as_bytes().ct_eq(expected.as_bytes()).into()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    fn canonical_salt() -> [u8; 32] {
        let mut s = [0u8; 32];
        for (i, b) in s.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(11);
        }
        s
    }

    #[test]
    fn pseudonymize_is_deterministic() {
        let subject = b"subject-42";
        let salt = canonical_salt();
        let a = pseudonymize(subject, &salt);
        let b = pseudonymize(subject, &salt);
        assert_eq!(a, b);
    }

    #[test]
    fn pseudonymize_changes_with_salt() {
        let subject = b"subject-42";
        let mut salt = canonical_salt();
        let a = pseudonymize(subject, &salt);
        salt[0] ^= 0xff;
        let b = pseudonymize(subject, &salt);
        assert_ne!(a, b);
    }

    #[test]
    fn pseudonymize_changes_with_subject() {
        let salt = canonical_salt();
        let a = pseudonymize(b"alpha", &salt);
        let b = pseudonymize(b"beta", &salt);
        assert_ne!(a, b);
    }

    #[test]
    fn hex_output_is_64_lowercase() {
        let salt = canonical_salt();
        let hash = pseudonymize(b"x", &salt);
        let hex = hash.to_hex();
        assert_eq!(hex.len(), PSEUDONYM_HEX_LEN);
        assert!(hex
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
    }

    #[test]
    fn marker_values_pinned() {
        assert_eq!(PII_REDACTED_MARKER_KEY, "pii_redacted");
        assert_eq!(PII_REDACTED_MARKER_VALUE, "true");
    }

    #[test]
    fn marker_roundtrip_serde() {
        let salt = canonical_salt();
        let hash = pseudonymize(b"alice", &salt);
        let m = PseudonymizationMarker::from_hash(hash);
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("pii_redacted"));
        assert!(json.contains("\"true\""));
        assert!(json.contains(&hash.to_hex()));
    }

    fn fixed_uuid(seed: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = seed.wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }

    #[test]
    fn pseudonymize_subject_id_uuid_matches_bytes() {
        let salt = canonical_salt();
        let id = fixed_uuid(1);
        let by_bytes = pseudonymize(id.as_bytes(), &salt);
        let by_uuid = pseudonymize_subject_id(id, &salt);
        assert_eq!(by_bytes, by_uuid);
    }

    #[test]
    fn verify_pseudonym_accepts_match() {
        let salt = canonical_salt();
        let id = fixed_uuid(2);
        let hash = pseudonymize_subject_id(id, &salt);
        assert!(verify_pseudonym(id, &salt, &hash));
    }

    #[test]
    fn verify_pseudonym_rejects_mismatch_subject() {
        let salt = canonical_salt();
        let id_a = fixed_uuid(3);
        let id_b = fixed_uuid(4);
        let hash_a = pseudonymize_subject_id(id_a, &salt);
        assert!(!verify_pseudonym(id_b, &salt, &hash_a));
    }

    #[test]
    fn verify_pseudonym_rejects_mismatch_salt() {
        let salt_a = canonical_salt();
        let mut salt_b = salt_a;
        salt_b[5] ^= 0xff;
        let id = fixed_uuid(5);
        let hash_a = pseudonymize_subject_id(id, &salt_a);
        assert!(!verify_pseudonym(id, &salt_b, &hash_a));
    }

    #[test]
    fn from_bytes_roundtrip() {
        let bytes = [7u8; 32];
        let h = PseudonymHash::from_bytes(bytes);
        assert_eq!(h.as_bytes(), &bytes);
    }
}
