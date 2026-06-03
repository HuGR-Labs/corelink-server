//! Adversarial regression tests — pin every known-attack
//! `INV-AUTH-WEBAUTHN-*` invariant (`WI-S03-006 §6.1.9`).
//!
//! Each scenario corresponds to a well-known attack class:
//!
//! 1. `alg: none` injection.
//! 2. Origin spoof (`evil.corelink.humangr.com.attacker.com`).
//! 3. RP-ID confusion (browser sends spoofed RP-ID by way of an
//!    origin that does not share the canonical eTLD+1 suffix).
//! 4. UV downgrade in admin step-up.
//! 5. Attestation chain absent (`attestation_present = false`).
//! 6. AAGUID denylist (deprecated authenticator).
//! 7. AAGUID closed-default (unknown authenticator).
//! 8. Sign-count regression.
//! 9. Challenge replay (TTL expiry).
//! 10. Recovery OTP single-use violation.
//! 11. Recovery OTP rate limit (verify-attempts exhaustion).
//! 12. Magic-link recovery rejected at the type level.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test file"
)]

use std::time::Duration;

use corelink_auth::webauthn::{
    Aaguid, AaguidPolicy, AuthenticationResponse, AuthenticatorAttachment, AuthenticatorFlags,
    Ceremony, CredentialId, EngineConfig, FixedClock, InMemoryEngine, InMemoryRecoveryOtpStore,
    Origin, OriginAllowlist, RecoveryChannel, RecoveryOtpStore, RecoveryOtpVerifyOutcome,
    RecoveryRateLimit, RegistrationResponse, RpId, SignCount, UserAccountId, WebAuthnEngine,
    WebAuthnError, COSE_ALG_ES256,
};

fn engine() -> (InMemoryEngine<FixedClock>, FixedClock, UserAccountId) {
    let cfg = EngineConfig::builder(RpId::new("corelink.humangr.com").unwrap(), "CoreLink")
        .origins(
            OriginAllowlist::from_strings([
                "https://app.corelink.humangr.com",
                "https://admin.corelink.humangr.com",
            ])
            .unwrap(),
        )
        .aaguids(
            AaguidPolicy::builder()
                .allow(Aaguid::yubikey_5())
                .allow(Aaguid::touch_id())
                .deny(Aaguid::yubikey_4_deprecated())
                .build(),
        )
        .build()
        .unwrap();
    let clock = FixedClock::epoch();
    let engine = InMemoryEngine::new(cfg, clock.clone());
    (engine, clock, UserAccountId::new_v7())
}

fn enroll_passkey(
    engine: &InMemoryEngine<FixedClock>,
    user: UserAccountId,
    aaguid: Aaguid,
) -> CredentialId {
    let reg = engine
        .start_registration(user, AuthenticatorAttachment::Platform)
        .unwrap();
    let cred = CredentialId::new(vec![0xAB; 32]).unwrap();
    let response = RegistrationResponse::synthetic_for_test(
        reg.id().clone(),
        aaguid,
        cred.clone(),
        COSE_ALG_ES256,
        AuthenticatorFlags::up_uv_be(),
        0,
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    engine.finish_registration(reg.id(), response).unwrap();
    cred
}

#[test]
fn test_alg_none_rejected() {
    let (engine, _clock, user) = engine();
    let reg = engine
        .start_registration(user, AuthenticatorAttachment::Platform)
        .unwrap();
    let response = RegistrationResponse::synthetic_for_test(
        reg.id().clone(),
        Aaguid::touch_id(),
        CredentialId::new(vec![0x01; 32]).unwrap(),
        0, // canonical "alg: none" attack
        AuthenticatorFlags::up_uv(),
        0,
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    let err = engine.finish_registration(reg.id(), response).unwrap_err();
    assert!(matches!(err, WebAuthnError::Malformed(_)));
}

#[test]
fn test_origin_spoof_rejected() {
    let (engine, _clock, user) = engine();
    let reg = engine
        .start_registration(user, AuthenticatorAttachment::Platform)
        .unwrap();
    let evil = Origin::parse("https://evil.corelink.humangr.com.attacker.com").unwrap();
    let response = RegistrationResponse::synthetic_for_test(
        reg.id().clone(),
        Aaguid::touch_id(),
        CredentialId::new(vec![0x02; 32]).unwrap(),
        COSE_ALG_ES256,
        AuthenticatorFlags::up_uv(),
        0,
        evil,
    );
    let err = engine.finish_registration(reg.id(), response).unwrap_err();
    assert!(matches!(err, WebAuthnError::OriginMismatch));
}

#[test]
fn test_rp_id_confusion_via_unrelated_host() {
    // Even if the attacker convinces the operator to ALLOW
    // `https://corelink.example` (different eTLD), the engine's
    // builder enforces consistency with the canonical RP-ID and
    // returns Malformed before construction.
    let res = OriginAllowlist::from_strings(["https://corelink.example", "https://attacker.com"]);
    let allow = res.unwrap();
    let consistency = allow.require_consistency_with(&RpId::new("corelink.humangr.com").unwrap());
    assert!(matches!(consistency, Err(WebAuthnError::Malformed(_))));
}

#[test]
fn test_uv_required_in_admin_step_up() {
    let (engine, _clock, user) = engine();
    let cred = enroll_passkey(&engine, user, Aaguid::touch_id());
    let auth = engine
        .start_authentication(user, Ceremony::AdminStepUp)
        .unwrap();
    let response = AuthenticationResponse::synthetic_for_test(
        auth.id().clone(),
        cred,
        AuthenticatorFlags::up_only(),
        SignCount::new(1),
        Origin::parse("https://admin.corelink.humangr.com").unwrap(),
    );
    assert!(matches!(
        engine.finish_authentication(auth.id(), response),
        Err(WebAuthnError::UserVerificationMissing)
    ));
}

#[test]
fn test_attestation_required_at_registration() {
    let (engine, _clock, user) = engine();
    let reg = engine
        .start_registration(user, AuthenticatorAttachment::Platform)
        .unwrap();
    let mut response = RegistrationResponse::synthetic_for_test(
        reg.id().clone(),
        Aaguid::touch_id(),
        CredentialId::new(vec![0x03; 32]).unwrap(),
        COSE_ALG_ES256,
        AuthenticatorFlags::up_uv(),
        0,
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    response.attestation_present = false;
    assert!(matches!(
        engine.finish_registration(reg.id(), response),
        Err(WebAuthnError::AttestationInvalid)
    ));
}

#[test]
fn test_aaguid_denylist_blocks_deprecated_authenticator() {
    let (engine, _clock, user) = engine();
    let reg = engine
        .start_registration(user, AuthenticatorAttachment::CrossPlatform)
        .unwrap();
    let response = RegistrationResponse::synthetic_for_test(
        reg.id().clone(),
        Aaguid::yubikey_4_deprecated(),
        CredentialId::new(vec![0x04; 32]).unwrap(),
        COSE_ALG_ES256,
        AuthenticatorFlags::up_uv(),
        0,
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    assert!(matches!(
        engine.finish_registration(reg.id(), response),
        Err(WebAuthnError::AaguidDenied)
    ));
}

#[test]
fn test_aaguid_closed_default_blocks_unknown() {
    let (engine, _clock, user) = engine();
    let reg = engine
        .start_registration(user, AuthenticatorAttachment::CrossPlatform)
        .unwrap();
    let response = RegistrationResponse::synthetic_for_test(
        reg.id().clone(),
        Aaguid::nil(),
        CredentialId::new(vec![0x05; 32]).unwrap(),
        COSE_ALG_ES256,
        AuthenticatorFlags::up_uv(),
        0,
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    assert!(matches!(
        engine.finish_registration(reg.id(), response),
        Err(WebAuthnError::AaguidNotAllowed)
    ));
}

#[test]
fn test_sign_count_regression_detected() {
    let (engine, _clock, user) = engine();
    let cred = enroll_passkey(&engine, user, Aaguid::yubikey_5());

    // First successful auth — sign_count goes 0 → 5.
    let auth = engine
        .start_authentication(user, Ceremony::Authentication)
        .unwrap();
    let response = AuthenticationResponse::synthetic_for_test(
        auth.id().clone(),
        cred.clone(),
        AuthenticatorFlags::up_uv(),
        SignCount::new(5),
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    let outcome = engine.finish_authentication(auth.id(), response).unwrap();
    assert_eq!(outcome.persisted_sign_count(), SignCount::new(5));

    // Replay with sign_count = 2 — regression.
    let auth2 = engine
        .start_authentication(user, Ceremony::Authentication)
        .unwrap();
    let response2 = AuthenticationResponse::synthetic_for_test(
        auth2.id().clone(),
        cred,
        AuthenticatorFlags::up_uv(),
        SignCount::new(2),
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    assert!(matches!(
        engine.finish_authentication(auth2.id(), response2),
        Err(WebAuthnError::SignCountRegression { .. })
    ));
}

#[test]
fn test_passkey_sign_count_zero_exempt_from_regression() {
    let (engine, _clock, user) = engine();
    let cred = enroll_passkey(&engine, user, Aaguid::touch_id());

    for _ in 0..5 {
        let auth = engine
            .start_authentication(user, Ceremony::Authentication)
            .unwrap();
        let response = AuthenticationResponse::synthetic_for_test(
            auth.id().clone(),
            cred.clone(),
            AuthenticatorFlags::up_uv(),
            SignCount::zero(),
            Origin::parse("https://app.corelink.humangr.com").unwrap(),
        );
        let outcome = engine.finish_authentication(auth.id(), response).unwrap();
        assert!(outcome.is_authenticated());
        assert_eq!(outcome.persisted_sign_count(), SignCount::zero());
    }
}

#[test]
fn test_challenge_replay_after_ttl_expiry() {
    let (engine, clock, user) = engine();
    let cred = enroll_passkey(&engine, user, Aaguid::touch_id());
    let auth = engine
        .start_authentication(user, Ceremony::Authentication)
        .unwrap();
    clock.advance_ms(301_000);
    let response = AuthenticationResponse::synthetic_for_test(
        auth.id().clone(),
        cred,
        AuthenticatorFlags::up_uv(),
        SignCount::new(1),
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    assert!(matches!(
        engine.finish_authentication(auth.id(), response),
        Err(WebAuthnError::ChallengeExpired)
    ));
}

#[test]
fn test_recovery_otp_single_use() {
    let store = InMemoryRecoveryOtpStore::new(RecoveryRateLimit::canonical());
    let user = UserAccountId::new_v7();
    let now_ms: u64 = 1_700_000_000_000;
    let minted = corelink_auth::webauthn::recovery::mint_otp(
        user,
        now_ms,
        Duration::from_secs(600),
        RecoveryChannel::ClerkSsoEmail,
        5,
    )
    .unwrap();
    let plaintext = minted.plaintext.into_string();
    store.put(minted.record).unwrap();

    let outcome = store
        .verify_and_consume(user, &plaintext, now_ms + 1_000)
        .unwrap();
    assert!(matches!(outcome, RecoveryOtpVerifyOutcome::Consumed { .. }));

    // Re-use rejected.
    assert!(matches!(
        store.verify_and_consume(user, &plaintext, now_ms + 2_000),
        Err(WebAuthnError::RecoveryOtpAlreadyConsumed)
    ));
}

#[test]
fn test_recovery_otp_verify_attempts_exhaustion() {
    let store = InMemoryRecoveryOtpStore::new(RecoveryRateLimit::canonical());
    let user = UserAccountId::new_v7();
    let now_ms: u64 = 1_700_000_000_000;
    let minted = corelink_auth::webauthn::recovery::mint_otp(
        user,
        now_ms,
        Duration::from_secs(600),
        RecoveryChannel::ClerkSsoEmail,
        2, // tighter than canonical for the test
    )
    .unwrap();
    store.put(minted.record).unwrap();

    // Two wrong attempts.
    let r1 = store
        .verify_and_consume(user, "000000", now_ms + 1_000)
        .unwrap();
    assert!(matches!(r1, RecoveryOtpVerifyOutcome::Mismatch { .. }));
    let r2 = store
        .verify_and_consume(user, "000000", now_ms + 2_000)
        .unwrap();
    assert!(matches!(r2, RecoveryOtpVerifyOutcome::Mismatch { .. }));
    // Third attempt must hit RateLimited.
    assert!(matches!(
        store.verify_and_consume(user, "000000", now_ms + 3_000),
        Err(WebAuthnError::RecoveryOtpRateLimited)
    ));
}

#[test]
fn test_magic_link_channel_unrepresentable() {
    // The RecoveryChannel enum has exactly one variant. A future
    // attempt to add `MagicLink` would force every match site to add
    // a new arm — caught at compile time.
    let channel = RecoveryChannel::ClerkSsoEmail;
    match channel {
        RecoveryChannel::ClerkSsoEmail => {}
    }
}

#[test]
fn test_step_up_token_op_class_binding() {
    let user = UserAccountId::new_v7();
    let cred = CredentialId::new(vec![0xEE; 32]).unwrap();
    let now_ms: u64 = 1_700_000_000_000;
    let token = corelink_auth::webauthn::step_up::StepUpToken::new(
        user,
        cred,
        "mass_revoke",
        Duration::from_secs(300),
        now_ms,
    )
    .unwrap();
    let secret = token.secret_bytes().to_vec();
    // Wrong op_class — rejected.
    assert!(matches!(
        token.validate(&secret, user, "billing_change", now_ms + 1_000),
        Err(WebAuthnError::StepUpRequired)
    ));
    // Wrong user — rejected.
    assert!(matches!(
        token.validate(
            &secret,
            UserAccountId::new_v7(),
            "mass_revoke",
            now_ms + 1_000
        ),
        Err(WebAuthnError::StepUpRequired)
    ));
    // Wrong secret — rejected.
    let mut tampered = secret.clone();
    if let Some(b) = tampered.first_mut() {
        *b = b.wrapping_add(1);
    }
    assert!(matches!(
        token.validate(&tampered, user, "mass_revoke", now_ms + 1_000),
        Err(WebAuthnError::StepUpRequired)
    ));
    // Correct everything — accepted.
    token
        .validate(&secret, user, "mass_revoke", now_ms + 1_000)
        .unwrap();
    // Past TTL — rejected.
    assert!(matches!(
        token.validate(&secret, user, "mass_revoke", now_ms + 301_000),
        Err(WebAuthnError::StepUpRequired)
    ));
}
