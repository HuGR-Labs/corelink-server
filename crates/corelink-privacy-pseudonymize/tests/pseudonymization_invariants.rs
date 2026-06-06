//! Pseudonymization invariant property tests for
//! `corelink-privacy-pseudonymize` (WI-S11-002 §10.2 T-2.3).
//!
//! Coverage:
//!
//! - `prop_pseudonymize_deterministic` — same `(subject_id,
//!   erasure_salt)` always produces the same 32-byte digest. Replay-safe
//!   invariant for forensic re-correlation.
//! - `prop_pseudonymize_unique_per_salt` — different salts → different
//!   digests with ≥ 250 bits min-entropy (collision probability < 2^-128
//!   over 10k samples).
//! - `prop_pseudonymize_unique_per_subject` — different subjects with
//!   the same salt → different digests.
//! - `prop_marker_present_on_hash_construction` — every
//!   [`PseudonymizationMarker`] constructed via `from_hash` carries the
//!   canonical `pii_redacted=true` marker (WI AC-002 invariant).
//! - `prop_verify_round_trip_holds` — `verify_pseudonym(id, salt,
//!   pseudonymize_subject_id(id, salt))` is always true. The forensic
//!   re-correlation contract.
//! - `prop_verify_rejects_random_pairs` — `verify_pseudonym(other_id,
//!   salt, pseudonymize_subject_id(id, salt))` is always false when
//!   `other_id != id`.
//! - `prop_hex_length_64_chars` — every digest renders as exactly 64
//!   lowercase hex chars.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use corelink_privacy_pseudonymize::{
    pseudonymize, pseudonymize_subject_id, verify_pseudonym, PseudonymHash, PseudonymizationMarker,
    ERASURE_SALT_LEN, PII_REDACTED_MARKER_KEY, PII_REDACTED_MARKER_VALUE, PSEUDONYM_HEX_LEN,
};
use proptest::prelude::*;
use uuid::Uuid;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for PR gate; nightly job overrides to 100k.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn salt_strategy() -> impl Strategy<Value = [u8; ERASURE_SALT_LEN]> {
    proptest::collection::vec(any::<u8>(), ERASURE_SALT_LEN..=ERASURE_SALT_LEN).prop_map(|v| {
        let mut a = [0u8; ERASURE_SALT_LEN];
        a.copy_from_slice(&v);
        a
    })
}

fn subject_strategy() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 1..=64)
}

fn uuid_strategy() -> impl Strategy<Value = Uuid> {
    any::<[u8; 16]>().prop_map(Uuid::from_bytes)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        max_shrink_iters: 8_192,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_pseudonymize_deterministic(
        subject in subject_strategy(),
        salt in salt_strategy(),
    ) {
        let a = pseudonymize(&subject, &salt);
        let b = pseudonymize(&subject, &salt);
        prop_assert_eq!(a, b);
    }

    #[test]
    fn prop_pseudonymize_unique_per_salt(
        subject in subject_strategy(),
        salt_a in salt_strategy(),
        salt_b in salt_strategy(),
    ) {
        prop_assume!(salt_a != salt_b);
        let a = pseudonymize(&subject, &salt_a);
        let b = pseudonymize(&subject, &salt_b);
        let differ = a != b;
        prop_assert!(differ);
    }

    #[test]
    fn prop_pseudonymize_unique_per_subject(
        subject_a in subject_strategy(),
        subject_b in subject_strategy(),
        salt in salt_strategy(),
    ) {
        prop_assume!(subject_a != subject_b);
        let a = pseudonymize(&subject_a, &salt);
        let b = pseudonymize(&subject_b, &salt);
        let differ = a != b;
        prop_assert!(differ);
    }

    #[test]
    fn prop_marker_present_on_hash_construction(
        subject in subject_strategy(),
        salt in salt_strategy(),
    ) {
        let hash = pseudonymize(&subject, &salt);
        let marker = PseudonymizationMarker::from_hash(hash);
        prop_assert_eq!(marker.pii_redacted.as_str(), PII_REDACTED_MARKER_VALUE);
        prop_assert_eq!(marker.pseudonym.len(), PSEUDONYM_HEX_LEN);
    }

    #[test]
    fn prop_verify_round_trip_holds(
        id in uuid_strategy(),
        salt in salt_strategy(),
    ) {
        let hash = pseudonymize_subject_id(id, &salt);
        let ok = verify_pseudonym(id, &salt, &hash);
        prop_assert!(ok);
    }

    #[test]
    fn prop_verify_rejects_random_pairs(
        id_a in uuid_strategy(),
        id_b in uuid_strategy(),
        salt in salt_strategy(),
    ) {
        prop_assume!(id_a != id_b);
        let hash_a = pseudonymize_subject_id(id_a, &salt);
        let ok = verify_pseudonym(id_b, &salt, &hash_a);
        let rejected = !ok;
        prop_assert!(rejected);
    }

    #[test]
    fn prop_hex_length_64_chars(
        subject in subject_strategy(),
        salt in salt_strategy(),
    ) {
        let hash = pseudonymize(&subject, &salt);
        let hex = hash.to_hex();
        prop_assert_eq!(hex.len(), PSEUDONYM_HEX_LEN);
        let lowercase = hex.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase());
        prop_assert!(lowercase);
    }
}

#[test]
fn marker_constants_pinned() {
    assert_eq!(PII_REDACTED_MARKER_KEY, "pii_redacted");
    assert_eq!(PII_REDACTED_MARKER_VALUE, "true");
}

#[test]
fn pseudonym_hash_from_bytes_round_trips() {
    let bytes = [13u8; 32];
    let h = PseudonymHash::from_bytes(bytes);
    assert_eq!(h.as_bytes(), &bytes);
}

#[test]
fn marker_serializes_to_canonical_json() {
    let bytes = [0u8; 32];
    let h = PseudonymHash::from_bytes(bytes);
    let m = PseudonymizationMarker::from_hash(h);
    let s = serde_json::to_string(&m).unwrap();
    assert!(s.contains("\"pii_redacted\":\"true\""));
    assert!(s.contains(&"00".repeat(32)));
}
