//! PII redaction integration tests — assert the type-system + hash
//! derivation surface prevents raw PII from landing in canonical
//! event bytes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    reason = "test code — assertions panic by design"
)]

use corelink_audit::{
    redact_pat, AuditError, EmailHash, PatIdHash, PrincipalIdHash, REDACTED_PAT_PLACEHOLDER,
};

#[test]
fn redact_pat_macro_returns_canonical_placeholder() {
    let pat_plaintext = "corelink_pat_live_xxx.secret_yyy.sig_zzz";
    let redacted = redact_pat!(pat_plaintext);
    assert_eq!(redacted, REDACTED_PAT_PLACEHOLDER);
    assert_eq!(redacted, "[REDACTED-PAT]");
}

#[test]
fn principal_id_hash_no_constructor_for_raw_input() {
    // The only public constructor is `derive`. There is no
    // `From<String>` / `new(&str)` — verified at compile time.
    let raw = "user_2NkX8a3Bq";
    let hash = PrincipalIdHash::derive(raw).expect("derive");
    // The hash is 16 hex chars; the raw string is 13 chars and
    // contains underscore — clearly distinct.
    assert_eq!(hash.as_str().len(), 16);
    assert!(!hash.as_str().contains('_'));
    assert_ne!(hash.as_str(), raw);
}

#[test]
fn empty_inputs_rejected_at_construction() {
    let cases: Vec<(Result<(), AuditError>, &'static str)> = vec![
        (PrincipalIdHash::derive("").map(|_| ()), "principal_id"),
        (PatIdHash::derive("").map(|_| ()), "pat_id"),
        (EmailHash::derive("").map(|_| ()), "email"),
    ];
    for (res, expected_kind) in cases {
        match res {
            Err(AuditError::EmptyHashInput { kind }) => assert_eq!(kind, expected_kind),
            Ok(()) => panic!("must reject empty for kind={expected_kind}"),
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }
}

#[test]
fn email_hash_does_not_leak_raw_email() {
    let raw = "alice+sneaky@example.com";
    let hash = EmailHash::derive(raw).expect("derive");
    assert_eq!(hash.as_str().len(), 16);
    assert!(!hash.as_str().contains('@'));
    assert!(!hash.as_str().contains("alice"));
    let serialized = serde_jcs::to_vec(&hash).expect("jcs");
    let s = String::from_utf8(serialized).expect("utf8");
    assert!(!s.contains(raw));
    assert!(!s.contains("alice"));
    assert!(!s.contains("@example.com"));
}

#[test]
fn pat_id_hash_does_not_leak_raw_pat() {
    let raw = "corelink_pat_live_xxx";
    let hash = PatIdHash::derive(raw).expect("derive");
    assert_eq!(hash.as_str().len(), 16);
    assert!(!hash.as_str().contains("corelink"));
    let serialized = serde_jcs::to_vec(&hash).expect("jcs");
    let s = String::from_utf8(serialized).expect("utf8");
    assert!(!s.contains(raw));
    assert!(!s.contains("corelink_pat"));
}

#[test]
fn hash_derivation_is_one_way_deterministic() {
    // Same input ⇒ same hash.
    let a1 = PrincipalIdHash::derive("user_x").expect("derive");
    let a2 = PrincipalIdHash::derive("user_x").expect("derive");
    assert_eq!(a1, a2);
    // Different input ⇒ different hash (collision-free at this scale).
    let b = PrincipalIdHash::derive("user_y").expect("derive");
    assert_ne!(a1, b);
    // Hex-only output.
    for ch in a1.as_str().chars() {
        assert!(
            ch.is_ascii_hexdigit() && (!ch.is_alphabetic() || ch.is_lowercase()),
            "non-canonical hex char: {ch}"
        );
    }
}
