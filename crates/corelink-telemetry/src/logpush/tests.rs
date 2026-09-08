use super::*;
use serde_json::json;

#[test]
fn email_redacted() {
    let r = InMemoryPiiRedactor::new();
    let out = r.redact("contact alice@example.com today");
    assert!(out.redacted.contains("<EMAIL_REDACTED>"));
    assert!(!out.redacted.contains("alice@example.com"));
    assert_eq!(out.email_hits, 1);
}

#[test]
fn ipv4_redacted() {
    let r = InMemoryPiiRedactor::new();
    let out = r.redact("client 192.168.1.42 connected");
    assert!(out.redacted.contains("<IP_REDACTED>"));
    assert!(!out.redacted.contains("192.168.1.42"));
    assert_eq!(out.ip_hits, 1);
}

#[test]
fn ipv4_invalid_octet_passes_through() {
    let r = InMemoryPiiRedactor::new();
    // 999 > 255 — not a valid IPv4.
    let out = r.redact("999.0.0.1 not an ip");
    assert!(out.redacted.contains("999"));
    assert_eq!(out.ip_hits, 0);
}

#[test]
fn ipv6_redacted() {
    let r = InMemoryPiiRedactor::new();
    let out = r.redact("from 2001:db8::1 routed");
    assert!(out.redacted.contains("<IP_REDACTED>"));
    assert!(!out.redacted.contains("2001:db8::1"));
    assert_eq!(out.ip_hits, 1);
}

#[test]
fn bearer_token_redacted() {
    let r = InMemoryPiiRedactor::new();
    let out = r.redact("Authorization: Bearer abcdef0123456789ZYXW");
    assert!(out.redacted.contains("<TOKEN_REDACTED>"));
    assert!(!out.redacted.contains("abcdef0123456789ZYXW"));
    assert_eq!(out.token_hits, 1);
}

#[test]
fn jwt_redacted() {
    let r = InMemoryPiiRedactor::new();
    let jwt =
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NSJ9.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let out = r.redact(&format!("token {jwt} expired"));
    assert!(out.redacted.contains("<TOKEN_REDACTED>"));
    assert!(!out.redacted.contains(jwt));
    assert_eq!(out.token_hits, 1);
}

#[test]
fn pan_redacted_visa_test_number() {
    let r = InMemoryPiiRedactor::new();
    // Standard Visa test PAN (4111-1111-1111-1111; Luhn-valid).
    let out = r.redact("card 4111 1111 1111 1111 captured");
    assert!(out.redacted.contains("<PAN_REDACTED>"));
    assert_eq!(out.pan_hits, 1);
}

#[test]
fn pan_invalid_luhn_passes_through() {
    let r = InMemoryPiiRedactor::new();
    // 16 digit run that is NOT Luhn-valid.
    let out = r.redact("transaction 1234567890123456 logged");
    assert!(out.redacted.contains("1234567890123456"));
    assert_eq!(out.pan_hits, 0);
}

#[test]
fn cpf_redacted_canonical() {
    let r = InMemoryPiiRedactor::new();
    // Canonical valid CPF (random; passes mod-11 test).
    let out = r.redact("doc 529.982.247-25 verified");
    assert!(out.redacted.contains("<CPF_REDACTED>"));
    assert_eq!(out.cpf_cnpj_hits, 1);
}

#[test]
fn cnpj_redacted_canonical() {
    let r = InMemoryPiiRedactor::new();
    // Canonical valid CNPJ.
    let out = r.redact("empresa 11.222.333/0001-81 ativa");
    assert!(out.redacted.contains("<CNPJ_REDACTED>"));
    assert_eq!(out.cpf_cnpj_hits, 1);
}

#[test]
fn cpf_repeated_digits_rejected() {
    let r = InMemoryPiiRedactor::new();
    let out = r.redact("doc 111.111.111-11 invalid");
    assert!(out.redacted.contains("111.111.111-11"));
    assert_eq!(out.cpf_cnpj_hits, 0);
}

#[test]
fn redaction_idempotent() {
    let r = InMemoryPiiRedactor::new();
    let input = "alice@example.com from 192.168.1.42 with Bearer abcdef0123456789ZYXW";
    let pass1 = r.redact(input);
    let pass2 = r.redact(&pass1.redacted);
    assert_eq!(pass1.redacted, pass2.redacted);
    // Second pass MUST find zero hits (idempotency).
    assert_eq!(pass2.email_hits, 0);
    assert_eq!(pass2.ip_hits, 0);
    assert_eq!(pass2.token_hits, 0);
}

#[test]
fn redaction_preserves_non_pii() {
    let r = InMemoryPiiRedactor::new();
    let input = "GET /v1/cas/put status=200 duration_ms=42 region=iad";
    let out = r.redact(input);
    assert_eq!(out.redacted, input);
    assert_eq!(out.total_hits(), 0);
}

#[test]
fn redact_json_walks_string_leaves() {
    let r = InMemoryPiiRedactor::new();
    let mut v = json!({
        "user": "alice@example.com",
        "client_ip": "192.168.1.42",
        "status": 200,
        "nested": {
            "auth": "Bearer abcdef0123456789ZYXW"
        },
        "tags": ["alice@example.com", "ok"]
    });
    let outcome = r.redact_json(&mut v);
    let s = v.to_string();
    assert!(!s.contains("alice@example.com"));
    assert!(!s.contains("192.168.1.42"));
    assert!(!s.contains("abcdef0123456789ZYXW"));
    assert!(s.contains("\"status\":200"));
    assert!(outcome.email_hits == 2);
    assert!(outcome.ip_hits == 1);
    assert!(outcome.token_hits == 1);
}

#[test]
fn pattern_kind_canonical_strings() {
    let v = canonical_pii_pattern_kinds();
    assert_eq!(v.len(), 5);
    let mut set = std::collections::HashSet::new();
    for k in v {
        assert!(set.insert(k.as_str()));
    }
    assert_eq!(set.len(), 5);
}

#[test]
fn placeholders_pinned() {
    assert_eq!(EMAIL_PLACEHOLDER, "<EMAIL_REDACTED>");
    assert_eq!(IP_PLACEHOLDER, "<IP_REDACTED>");
    assert_eq!(TOKEN_PLACEHOLDER, "<TOKEN_REDACTED>");
    assert_eq!(PAN_PLACEHOLDER, "<PAN_REDACTED>");
    assert_eq!(CPF_PLACEHOLDER, "<CPF_REDACTED>");
    assert_eq!(CNPJ_PLACEHOLDER, "<CNPJ_REDACTED>");
}

#[test]
fn counters_accumulate_across_passes() {
    let r = InMemoryPiiRedactor::new();
    r.redact("alice@example.com");
    r.redact("bob@example.com");
    let (email, _, _, _, _) = r.snapshot_counters();
    assert_eq!(email, 2);
}

#[test]
fn cloned_redactor_shares_counters() {
    let r1 = InMemoryPiiRedactor::new();
    let r2 = r1.clone();
    r1.redact("alice@example.com");
    let (email, _, _, _, _) = r2.snapshot_counters();
    assert_eq!(email, 1);
}

#[test]
fn placeholders_not_rematch_themselves() {
    let r = InMemoryPiiRedactor::new();
    let s = "<EMAIL_REDACTED>";
    let out = r.redact(s);
    assert_eq!(out.redacted, s);
    assert_eq!(out.total_hits(), 0);
}

#[test]
fn outcome_total_hits_sums_correctly() {
    let r = InMemoryPiiRedactor::new();
    let out = r.redact("alice@example.com 192.168.1.42 Bearer abcdef0123456789ZYXW");
    assert_eq!(out.total_hits(), 3);
}
