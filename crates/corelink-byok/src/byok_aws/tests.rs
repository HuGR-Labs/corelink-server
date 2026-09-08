use super::*;
use serde_json::json;

#[test]
fn aad_canonicalize_rejects_non_object() {
    let err = canonicalize_aad_to_string_map(&json!("hello")).unwrap_err();
    match err {
        BYOKError::EnvelopeError(_) => {}
        other => panic!("expected EnvelopeError, got {other:?}"),
    }
}

#[test]
fn aad_canonicalize_rejects_non_string_values() {
    let err = canonicalize_aad_to_string_map(&json!({"x": 42})).unwrap_err();
    match err {
        BYOKError::EnvelopeError(_) => {}
        other => panic!("expected EnvelopeError, got {other:?}"),
    }
}

#[test]
fn aad_canonicalize_is_key_order_independent() {
    let a = json!({"tenant_id": "t-1", "blob_hash": "sha256:abc"});
    let b = json!({"blob_hash": "sha256:abc", "tenant_id": "t-1"});
    let (_, bytes_a) = canonicalize_aad_to_string_map(&a).unwrap();
    let (_, bytes_b) = canonicalize_aad_to_string_map(&b).unwrap();
    assert_eq!(bytes_a, bytes_b);
    // Sanity: bytes start with `{` and contain sorted keys.
    let s = std::str::from_utf8(&bytes_a).unwrap();
    assert!(s.starts_with('{'));
    assert!(s.find("\"blob_hash\"").unwrap() < s.find("\"tenant_id\"").unwrap());
}

#[test]
fn aad_canonicalize_is_value_change_sensitive() {
    let a = json!({"tenant_id": "t-1"});
    let b = json!({"tenant_id": "t-2"});
    let (_, bytes_a) = canonicalize_aad_to_string_map(&a).unwrap();
    let (_, bytes_b) = canonicalize_aad_to_string_map(&b).unwrap();
    assert_ne!(bytes_a, bytes_b);
}

#[test]
fn aad_fingerprint_is_length_sensitive() {
    let short = aad_fingerprint(b"abc");
    let long = aad_fingerprint(b"abcabcabc");
    assert_ne!(short, long);
}

#[test]
fn aad_fingerprint_is_content_sensitive() {
    let a = aad_fingerprint(b"abcdefgh");
    let b = aad_fingerprint(b"abcdefgi");
    assert_ne!(a, b);
}
