use super::*;

fn principal() -> PrincipalIdHash {
    PrincipalIdHash::derive("user_xyz").expect("derive")
}

#[test]
fn auth_event_type_canonical_count_is_33() {
    // Per WI v1.2.0 §1 + §31 changelog: the canonical taxonomy has
    // **33 distinct on-the-wire event-type strings**. Composition:
    //
    // - 4 token lifecycle (issued/validated/revoked/expired)
    // - 2 session (created/revoked)
    // - 2 legacy generic denied (scope/invalid) — kept for backwards
    //   compat; new producers prefer the granular variants below
    // - 1 generic denied (rate_limit) — has no granular sibling
    // - 6 granular denied (signature_invalid/expired/scope_insufficient/
    //   not_found/malformed/revoked) — Lote 10.3bis P0
    // - 5 tenant + membership (tenant_provisioned/tenant_deleted/
    //   account_created/account_deleted/membership_added/removed/
    //   role_changed; 5 distinct here because tenant_deleted +
    //   account_deleted are listed under "lifecycle" in §1)
    // - 6 webauthn (registered/authenticated/deleted/sign_count_regression/
    //   origin_attack_attempt/new_device_used)
    // - 1 PAT lifecycle (pat_scope_escalated)
    // - 2 admin op (webauthn_authenticated/mass_revoke)
    // - 2 anomaly (token_replay_detected/cross_region_burst)
    //
    // Total: 4+2+2+1+6+7+6+1+2+2 = 33.
    assert_eq!(AuthEventType::canonical().count(), 33);
}

#[test]
fn each_variant_has_distinct_canonical_string() {
    let mut seen = std::collections::HashSet::new();
    for v in AuthEventType::canonical() {
        let s = v.as_str();
        assert!(s.starts_with("auth."), "non-canonical prefix: {s}");
        assert!(seen.insert(s), "duplicate canonical string: {s}");
    }
    assert_eq!(seen.len(), 33);
}

#[test]
fn data_event_type_round_trips_for_every_variant() {
    // For each AuthEventType, pair with a synthetic AuthEventData
    // and assert event_type() returns the same variant.
    for t in AuthEventType::canonical() {
        let d = synthetic_data(t);
        assert_eq!(d.event_type(), t, "drift on variant: {t:?}");
    }
}

#[test]
fn sev1_set_is_exactly_six() {
    let count = AuthEventType::canonical().filter(|t| t.is_sev1()).count();
    assert_eq!(count, 6, "SEV-1 fanout set must be exactly 6 events");
}

#[test]
fn auth_event_subject_is_tenant_id_text() {
    let tenant = TenantId::from_uuid(Uuid::nil());
    let event = AuthEvent::new(
        AuthEventType::TokenValidated,
        "corelink://wnam/auth/middleware",
        tenant,
        principal(),
        RegionTag::Wnam,
        RequestId::new("req_abc"),
        RetentionHint::Team90d,
        1_700_000_000_000,
        AuthEventData::TokenValidated {
            token_kind: TokenKind::Pat,
            scope_bitset: 0b1,
        },
    );
    assert_eq!(event.subject(), tenant.to_canonical_text());
}

fn synthetic_data(t: AuthEventType) -> AuthEventData {
    let cred = WebAuthnCredentialIdHash::derive(b"cid_xyz").expect("derive");
    let pat = PatIdHash::derive("pat_xyz").expect("derive");
    let email = EmailHash::derive("user@example.com").expect("derive");
    match t {
        AuthEventType::TokenIssued => AuthEventData::TokenIssued {
            token_kind: TokenKind::Pat,
            pat_id_hash: Some(pat.clone()),
            scope_bitset: 1,
        },
        AuthEventType::TokenValidated => AuthEventData::TokenValidated {
            token_kind: TokenKind::ClerkJwt,
            scope_bitset: 1,
        },
        AuthEventType::TokenRevoked => AuthEventData::TokenRevoked {
            token_kind: TokenKind::Pat,
            pat_id_hash: Some(pat.clone()),
        },
        AuthEventType::TokenExpired => AuthEventData::TokenExpired {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::SessionCreated => AuthEventData::SessionCreated {
            session_id: SessionId::from_uuid(Uuid::nil()),
        },
        AuthEventType::SessionRevoked => AuthEventData::SessionRevoked {
            session_id: SessionId::from_uuid(Uuid::nil()),
        },
        AuthEventType::TokenReplayDetected => AuthEventData::TokenReplayDetected {
            token_kind: TokenKind::Pat,
            signal: "challenge_replay",
        },
        AuthEventType::DeniedScope => AuthEventData::DeniedScope {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedInvalid => AuthEventData::DeniedInvalid {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedSignatureInvalid => AuthEventData::DeniedSignatureInvalid {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedExpired => AuthEventData::DeniedExpired {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedScopeInsufficient => AuthEventData::DeniedScopeInsufficient {
            token_kind: TokenKind::Pat,
            required_scope_bitset: 0b10,
            actual_scope_bitset: 0b01,
        },
        AuthEventType::DeniedNotFound => AuthEventData::DeniedNotFound {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedMalformed => AuthEventData::DeniedMalformed {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedRevoked => AuthEventData::DeniedRevoked {
            token_kind: TokenKind::Pat,
        },
        AuthEventType::DeniedRateLimit => AuthEventData::DeniedRateLimit {
            token_kind: TokenKind::Pat,
            window_ms: 60_000,
        },
        AuthEventType::TenantProvisioned => AuthEventData::TenantProvisioned {
            owner_email_hash: email.clone(),
        },
        AuthEventType::TenantDeleted => AuthEventData::TenantDeleted {
            dsr_triggered: true,
        },
        AuthEventType::AccountCreated => AuthEventData::AccountCreated {
            email_hash: email.clone(),
        },
        AuthEventType::AccountDeleted => AuthEventData::AccountDeleted {
            dsr_triggered: false,
        },
        AuthEventType::MembershipAdded => AuthEventData::MembershipAdded {
            role: MembershipRole::Member,
        },
        AuthEventType::MembershipRemoved => AuthEventData::MembershipRemoved {
            role: MembershipRole::Member,
        },
        AuthEventType::MembershipRoleChanged => AuthEventData::MembershipRoleChanged {
            from_role: MembershipRole::Member,
            to_role: MembershipRole::Admin,
        },
        AuthEventType::WebauthnRegistered => AuthEventData::WebauthnRegistered {
            credential_id_hash: cred.clone(),
            aaguid_hex: "00000000000000000000000000000000".to_string(),
        },
        AuthEventType::WebauthnAuthenticated => AuthEventData::WebauthnAuthenticated {
            credential_id_hash: cred.clone(),
            sign_count: 1,
        },
        AuthEventType::WebauthnDeleted => AuthEventData::WebauthnDeleted {
            credential_id_hash: cred.clone(),
        },
        AuthEventType::WebauthnSignCountRegression => AuthEventData::WebauthnSignCountRegression {
            credential_id_hash: cred.clone(),
            previous_sign_count: 5,
            observed_sign_count: 3,
        },
        AuthEventType::WebauthnOriginAttackAttempt => AuthEventData::WebauthnOriginAttackAttempt {
            submitted_origin: "https://evil.example.com".to_string(),
        },
        AuthEventType::WebauthnNewDeviceUsed => AuthEventData::WebauthnNewDeviceUsed {
            credential_id_hash: cred.clone(),
        },
        AuthEventType::PatScopeEscalated => AuthEventData::PatScopeEscalated {
            pat_id_hash: pat.clone(),
            previous_scope_bitset: 0b1,
            new_scope_bitset: 0b11,
        },
        AuthEventType::AdminOpWebauthnAuthenticated => {
            AuthEventData::AdminOpWebauthnAuthenticated {
                op_class: "mass_revoke".to_string(),
            }
        }
        AuthEventType::AdminOpMassRevoke => AuthEventData::AdminOpMassRevoke {
            revoked_count: 1000,
        },
        AuthEventType::CrossRegionBurst => AuthEventData::CrossRegionBurst {
            distinct_regions: 4,
        },
    }
}
