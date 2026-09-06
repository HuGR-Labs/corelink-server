use super::*;

/// Test-only helper exposing the canonical synthetic data builder so
/// integration tests + property tests can construct events without
/// duplicating the variant pairing matrix.
///
/// # Errors
///
/// Returns [`crate::AuditError`] if the underlying hash derivation
/// rejects empty input. The caller passes hard-coded non-empty
/// constants below so this branch is unreachable in practice; it is
/// surfaced as a `Result` to satisfy the crate-strict lint that
/// forbids `unwrap`/`expect` outside `#[cfg(test)]` modules.
#[doc(hidden)]
#[allow(
    clippy::items_after_test_module,
    reason = "doc-hidden test helper intentionally lives after the unit-test module — production callers use the crate's typed constructors directly"
)]
pub fn synthetic_data_for(t: AuthEventType) -> Result<AuthEventData, crate::AuditError> {
    let cred = WebAuthnCredentialIdHash::derive(b"cid_xyz")?;
    let pat = PatIdHash::derive("pat_xyz")?;
    let email = EmailHash::derive("u@x.io")?;
    Ok(match t {
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
    })
}
