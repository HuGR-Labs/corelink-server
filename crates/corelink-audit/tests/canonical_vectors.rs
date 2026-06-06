//! Canonical vectors — pin algorithmic invariants across refactors.
//!
//! These tests are **not** property tests; they capture canonical
//! shapes documented in `WI-S03-007 §1 + §6.1.2 + §10.5.3` so that
//! any future refactor that drifts the on-the-wire surface fails
//! immediately.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    reason = "test code — assertions panic by design"
)]

use corelink_audit::{
    compute_content_hash, link_chain_hash, AuthEvent, AuthEventData, AuthEventType, ChainHash,
    Emitter, InMemoryEmitter, PrincipalIdHash, RegionTag, RequestId, RetentionHint, TenantId,
    TokenKind,
};
use uuid::Uuid;

/// 33 canonical event-type strings — the on-the-wire identity. Any
/// rename breaks the chain consumer dispatch in S-09.
#[test]
fn canonical_event_type_strings_pinned() {
    let expected: &[&str] = &[
        "auth.token.issued",
        "auth.token.validated",
        "auth.token.revoked",
        "auth.token.expired",
        "auth.session.created",
        "auth.session.revoked",
        "auth.anomaly.token_replay_detected",
        "auth.denied.scope",
        "auth.denied.invalid",
        "auth.denied.signature_invalid",
        "auth.denied.expired",
        "auth.denied.scope_insufficient",
        "auth.denied.not_found",
        "auth.denied.malformed",
        "auth.denied.revoked",
        "auth.denied.rate_limit",
        "auth.tenant.provisioned",
        "auth.tenant.deleted",
        "auth.account.created",
        "auth.account.deleted",
        "auth.membership.added",
        "auth.membership.removed",
        "auth.membership.role_changed",
        "auth.webauthn.registered",
        "auth.webauthn.authenticated",
        "auth.webauthn.deleted",
        "auth.webauthn.sign_count_regression",
        "auth.webauthn.origin_attack_attempt",
        "auth.webauthn.new_device_used",
        "auth.pat.scope_escalated",
        "auth.admin_op.webauthn_authenticated",
        "auth.admin_op.mass_revoke",
        "auth.anomaly.cross_region_burst",
    ];
    let actual: Vec<&str> = AuthEventType::canonical().map(|t| t.as_str()).collect();
    assert_eq!(actual.len(), 33);
    assert_eq!(expected.len(), 33);
    assert_eq!(actual, expected);
}

/// Canonical CloudEvents 1.0 envelope shape for `auth.token.validated`
/// — pinned bytes via JCS canonicalization. Drift in field
/// ordering / camelCase / type-tag would change `content_hash`
/// and break the chain.
#[test]
fn canonical_envelope_shape_token_validated() {
    let event = AuthEvent::with_id(
        Uuid::from_u128(0x0193_8af0_1234_5678_9abc_def0_1234_5678),
        AuthEventType::TokenValidated,
        "corelink://wnam/auth/middleware",
        TenantId::from_uuid(Uuid::from_u128(0x0193_8af0_aaaa_bbbb_cccc_dddd_eeee_ffff)),
        PrincipalIdHash::derive("user_2NkX8a3Bq").expect("derive"),
        RegionTag::Wnam,
        RequestId::new("req_abc123"),
        RetentionHint::Team90d,
        1_700_000_000_000,
        AuthEventData::TokenValidated {
            token_kind: TokenKind::Pat,
            scope_bitset: 0b0000_0001,
        },
    );

    let canonical = serde_jcs::to_vec(&event).expect("jcs");
    let s = String::from_utf8(canonical).expect("utf8");

    // CloudEvents mandatory fields
    assert!(s.contains(r#""specversion":"1.0""#));
    assert!(s.contains(r#""type":"auth.token.validated""#));
    assert!(s.contains(r#""datacontenttype":"application/json""#));
    assert!(s.contains(r#""source":"corelink://wnam/auth/middleware""#));
    assert!(s.contains(r#""time_unix_ms":1700000000000"#));

    // CoreLink extensions
    assert!(s.contains(r#""region":"wnam""#));
    assert!(s.contains(r#""request_id":"req_abc123""#));
    assert!(s.contains(r#""retention_hint":"team90d""#));

    // Data payload
    assert!(s.contains(r#""data":{"#));
    assert!(s.contains(r#""token_kind":"pat""#));
    assert!(s.contains(r#""scope_bitset":1"#));

    // The principal_id_hash should be the 16-char hex prefix.
    assert!(s.contains(r#""principal_id_hash":""#));
    let phash = PrincipalIdHash::derive("user_2NkX8a3Bq").expect("derive");
    assert!(s.contains(phash.as_str()));

    // Raw principal_id MUST NOT appear.
    assert!(!s.contains("user_2NkX8a3Bq"), "raw principal leaked: {s}");
}

/// Content hash byte-equal across two fresh `compute_content_hash` calls.
#[test]
fn canonical_content_hash_byte_equal_across_calls() {
    let event = AuthEvent::with_id(
        Uuid::from_u128(0x0193_8af0_aaaa_bbbb_cccc_dddd_eeee_0000),
        AuthEventType::TokenValidated,
        "corelink://wnam/auth/middleware",
        TenantId::from_uuid(Uuid::nil()),
        PrincipalIdHash::derive("u").expect("derive"),
        RegionTag::Wnam,
        RequestId::new("r"),
        RetentionHint::Team90d,
        0,
        AuthEventData::TokenValidated {
            token_kind: TokenKind::Pat,
            scope_bitset: 0,
        },
    );
    let h1 = compute_content_hash(&event).expect("hash");
    let h2 = compute_content_hash(&event).expect("hash");
    assert_eq!(h1, h2);
    assert_eq!(h1.as_str().len(), 64);
}

/// Chain link extends a 3-event chain — every link unique + each
/// link reproducible.
#[test]
fn chain_links_extend_3_events() {
    let make = |label: &str| {
        AuthEvent::new(
            AuthEventType::TokenValidated,
            "corelink://wnam/auth/middleware",
            TenantId::from_uuid(Uuid::nil()),
            PrincipalIdHash::derive(label).expect("derive"),
            RegionTag::Wnam,
            RequestId::new(label),
            RetentionHint::Team90d,
            1_700_000_000_000,
            AuthEventData::TokenValidated {
                token_kind: TokenKind::Pat,
                scope_bitset: 0,
            },
        )
    };
    let e1 = make("u_1");
    let e2 = make("u_2");
    let e3 = make("u_3");

    let c1 = compute_content_hash(&e1).expect("hash");
    let c2 = compute_content_hash(&e2).expect("hash");
    let c3 = compute_content_hash(&e3).expect("hash");

    let h0 = ChainHash::genesis();
    let h1 = link_chain_hash(&h0, &c1);
    let h2 = link_chain_hash(&h1, &c2);
    let h3 = link_chain_hash(&h2, &c3);

    // Each link is distinct.
    assert_ne!(h0, h1);
    assert_ne!(h1, h2);
    assert_ne!(h2, h3);

    // Re-running the same link is byte-equal.
    let h1_again = link_chain_hash(&h0, &c1);
    assert_eq!(h1, h1_again);
}

/// SEV-1 fan-out set is exactly 6 events. A deviation would change
/// the dual-emitter routing in S-09 chain wiring.
#[test]
fn sev1_fanout_set_canonical() {
    let sev1: Vec<&str> = AuthEventType::canonical()
        .filter(|t| t.is_sev1())
        .map(|t| t.as_str())
        .collect();
    let expected: Vec<&str> = vec![
        "auth.anomaly.token_replay_detected",
        "auth.webauthn.sign_count_regression",
        "auth.webauthn.origin_attack_attempt",
        "auth.pat.scope_escalated",
        "auth.admin_op.mass_revoke",
        "auth.anomaly.cross_region_burst",
    ];
    let mut sev1_sorted = sev1.clone();
    sev1_sorted.sort_unstable();
    let mut expected_sorted = expected.clone();
    expected_sorted.sort_unstable();
    assert_eq!(
        sev1_sorted, expected_sorted,
        "SEV-1 set drift: {sev1:?} vs {expected:?}"
    );
}

/// In-memory emitter captures every event in order.
#[test]
fn in_memory_emitter_canonical_order() {
    let emitter = InMemoryEmitter::new();
    for (i, t) in AuthEventType::canonical().enumerate() {
        let data = corelink_audit::events::synthetic_data_for(t).expect("synthetic");
        let event = AuthEvent::new(
            t,
            "corelink://wnam/auth/middleware",
            TenantId::from_uuid(Uuid::nil()),
            PrincipalIdHash::derive(&format!("u_{i}")).expect("derive"),
            RegionTag::Wnam,
            RequestId::new(format!("req_{i}")),
            RetentionHint::Team90d,
            1_700_000_000_000_i64.wrapping_add(i as i64),
            data,
        );
        emitter.emit(event).expect("emit");
    }
    let snapshot = emitter.snapshot();
    assert_eq!(snapshot.len(), 33);
    for (i, e) in snapshot.iter().enumerate() {
        let expected_t = AuthEventType::canonical().nth(i).expect("nth");
        assert_eq!(e.event_type, expected_t);
    }
}
