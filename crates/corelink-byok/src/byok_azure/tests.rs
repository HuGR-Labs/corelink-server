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

#[test]
fn resolve_fips_host_accepts_premium_hsm() {
    let (h, s) = resolve_fips_host("https://myvault.vault.azure.net/keys/k").unwrap();
    assert_eq!(h, "myvault.vault.azure.net");
    assert_eq!(s, "vault.azure.net");
}

#[test]
fn resolve_fips_host_accepts_managed_hsm() {
    let (h, s) = resolve_fips_host("https://corp-hsm.managedhsm.azure.net/keys/k").unwrap();
    assert_eq!(h, "corp-hsm.managedhsm.azure.net");
    assert_eq!(s, "managedhsm.azure.net");
}

#[test]
fn resolve_fips_host_accepts_us_gov() {
    let (h, s) = resolve_fips_host("https://gov-vault.vault.usgovcloudapi.net/keys/k").unwrap();
    assert_eq!(h, "gov-vault.vault.usgovcloudapi.net");
    assert_eq!(s, "vault.usgovcloudapi.net");
}

#[test]
fn resolve_fips_host_rejects_non_https() {
    let err = resolve_fips_host("http://myvault.vault.azure.net/keys/k").unwrap_err();
    assert!(matches!(err, BYOKError::Provider(_)));
}

#[test]
fn resolve_fips_host_rejects_non_fips_tld() {
    let err = resolve_fips_host("https://attacker.example.com/keys/k").unwrap_err();
    assert!(matches!(err, BYOKError::Provider(_)));
}
