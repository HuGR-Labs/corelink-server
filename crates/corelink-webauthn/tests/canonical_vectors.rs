//! Canonical vectors — pin algorithmic invariants across refactors.
//!
//! These tests are NOT property tests; they capture the canonical
//! shapes documented in `WI-S03-006 §6.1.1 + §9.x` so that any future
//! refactor that drifts the on-the-wire surface fails immediately.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test file — explicit failure modes are clearer than Result threading"
)]

use std::time::Duration;

use corelink_webauthn::{
    parse_cose_algorithm, Aaguid, AaguidPolicy, AuthenticationResponse, AuthenticatorAttachment,
    AuthenticatorFlags, Ceremony, ChallengeTtl, CredentialId, EngineConfig, FixedClock,
    InMemoryEngine, Origin, OriginAllowlist, RegistrationResponse, RpId, SignCount, UserAccountId,
    WebAuthnEngine, WebAuthnError, COSE_ALG_EDDSA, COSE_ALG_ES256, COSE_ALG_RS256,
    RECOVERY_OTP_DIGITS, RECOVERY_OTP_TTL_DEFAULT,
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
                .allow(Aaguid::windows_hello())
                .allow(Aaguid::android_biometrics())
                .allow(Aaguid::icloud_passkey())
                .deny(Aaguid::yubikey_4_deprecated())
                .build(),
        )
        .challenge_ttl(ChallengeTtl::default())
        .build()
        .unwrap();
    let clock = FixedClock::epoch();
    let engine = InMemoryEngine::new(cfg, clock.clone());
    let user = UserAccountId::new_v7();
    (engine, clock, user)
}

#[test]
fn rp_id_canonical_eqs_corelink_dev() {
    let rp = RpId::new("corelink.humangr.com").unwrap();
    assert_eq!(rp.as_str(), "corelink.humangr.com");
}

#[test]
fn rp_id_rejects_localhost() {
    assert!(matches!(
        RpId::new("localhost"),
        Err(WebAuthnError::Malformed(_))
    ));
}

#[test]
fn rp_id_rejects_loopback() {
    for raw in ["127.0.0.1", "0.0.0.0", "::1"] {
        assert!(matches!(RpId::new(raw), Err(WebAuthnError::Malformed(_))));
    }
}

#[test]
fn rp_id_rejects_single_label() {
    assert!(matches!(
        RpId::new("corelink"),
        Err(WebAuthnError::Malformed(_))
    ));
}

#[test]
fn rp_id_hash_is_sha256() {
    use sha2::{Digest, Sha256};
    let rp = RpId::new("corelink.humangr.com").unwrap();
    let mut h = Sha256::new();
    h.update(b"corelink.humangr.com");
    let canonical: [u8; 32] = h.finalize().into();
    assert_eq!(rp.hash(), canonical);
}

#[test]
fn origin_allowlist_is_exact_match() {
    let allow = OriginAllowlist::from_strings([
        "https://app.corelink.humangr.com",
        "https://admin.corelink.humangr.com",
    ])
    .unwrap();
    let good = Origin::parse("https://app.corelink.humangr.com").unwrap();
    let bad = Origin::parse("https://evil.corelink.humangr.com.attacker.com").unwrap();
    assert!(allow.contains(&good));
    assert!(!allow.contains(&bad));
}

#[test]
fn origin_rejects_http() {
    assert!(matches!(
        Origin::parse("http://app.corelink.humangr.com"),
        Err(WebAuthnError::Malformed(_))
    ));
}

#[test]
fn origin_rejects_userinfo() {
    assert!(matches!(
        Origin::parse("https://user:pass@app.corelink.humangr.com"),
        Err(WebAuthnError::Malformed(_))
    ));
}

#[test]
fn origin_subdomain_check() {
    let rp = RpId::new("corelink.humangr.com").unwrap();
    let app = Origin::parse("https://app.corelink.humangr.com").unwrap();
    assert!(app.is_subdomain_of(&rp));
    let evil = Origin::parse("https://evil.corelink.humangr.com.attacker.com").unwrap();
    assert!(!evil.is_subdomain_of(&rp));
    let bare = Origin::parse("https://corelink.humangr.com").unwrap();
    assert!(bare.is_subdomain_of(&rp));
}

#[test]
fn challenge_ttl_default_300s() {
    let ttl = ChallengeTtl::default();
    assert_eq!(ttl.as_duration(), Duration::from_secs(300));
}

#[test]
fn challenge_ttl_rejects_above_max() {
    assert!(matches!(
        ChallengeTtl::new(Duration::from_secs(900)),
        Err(WebAuthnError::Malformed(_))
    ));
}

#[test]
fn cose_algorithm_alg_none_rejected() {
    assert!(matches!(
        parse_cose_algorithm(0),
        Err(WebAuthnError::Malformed(_))
    ));
}

#[test]
fn cose_algorithm_canonical_ints() {
    assert_eq!(COSE_ALG_ES256, -7);
    assert_eq!(COSE_ALG_EDDSA, -8);
    assert_eq!(COSE_ALG_RS256, -257);
}

#[test]
fn aaguid_policy_closed_default_rejects_all() {
    let policy = AaguidPolicy::empty();
    assert!(matches!(
        policy.evaluate(Aaguid::yubikey_5()),
        Err(WebAuthnError::AaguidNotAllowed)
    ));
}

#[test]
fn aaguid_policy_denylist_overrides_allowlist() {
    let yk = Aaguid::yubikey_4_deprecated();
    let policy = AaguidPolicy::builder().allow(yk).deny(yk).build();
    assert!(matches!(
        policy.evaluate(yk),
        Err(WebAuthnError::AaguidDenied)
    ));
}

#[test]
fn registration_happy_path_persists_credential() {
    let (engine, _clock, user) = engine();
    let challenge = engine
        .start_registration(user, AuthenticatorAttachment::Platform)
        .unwrap();
    let response = RegistrationResponse::synthetic_for_test(
        challenge.id().clone(),
        Aaguid::touch_id(),
        CredentialId::new(vec![0x01; 32]).unwrap(),
        COSE_ALG_ES256,
        AuthenticatorFlags::up_uv_be(),
        0,
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    let cred_id = engine.finish_registration(challenge.id(), response).unwrap();
    assert_eq!(engine.credential_store().list_for_user(user).len(), 1);
    let stored = engine
        .credential_store()
        .get_by_credential_id(&cred_id)
        .unwrap();
    assert_eq!(stored.aaguid, Aaguid::touch_id());
    assert_eq!(stored.cose_algorithm.as_i32(), COSE_ALG_ES256);
}

#[test]
fn admin_step_up_requires_uv() {
    let (engine, _clock, user) = engine();
    let reg = engine
        .start_registration(user, AuthenticatorAttachment::CrossPlatform)
        .unwrap();
    let _ = engine
        .finish_registration(
            reg.id(),
            RegistrationResponse::synthetic_for_test(
                reg.id().clone(),
                Aaguid::yubikey_5(),
                CredentialId::new(vec![0xCC; 32]).unwrap(),
                COSE_ALG_ES256,
                AuthenticatorFlags::up_uv(),
                0,
                Origin::parse("https://admin.corelink.humangr.com").unwrap(),
            ),
        )
        .unwrap();
    let auth = engine
        .start_authentication(user, Ceremony::AdminStepUp)
        .unwrap();
    // Send UP-only — no UV.
    let response = AuthenticationResponse::synthetic_for_test(
        auth.id().clone(),
        auth.allowed_credentials()[0].clone(),
        AuthenticatorFlags::up_only(),
        SignCount::new(1),
        Origin::parse("https://admin.corelink.humangr.com").unwrap(),
    );
    let err = engine
        .finish_authentication(auth.id(), response)
        .unwrap_err();
    assert!(matches!(err, WebAuthnError::UserVerificationMissing));
}

#[test]
fn challenge_is_single_use() {
    let (engine, _clock, user) = engine();
    let reg = engine
        .start_registration(user, AuthenticatorAttachment::Platform)
        .unwrap();
    let cred = CredentialId::new(vec![0xBB; 32]).unwrap();
    let response = RegistrationResponse::synthetic_for_test(
        reg.id().clone(),
        Aaguid::touch_id(),
        cred.clone(),
        COSE_ALG_ES256,
        AuthenticatorFlags::up_uv(),
        0,
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    let _ = engine.finish_registration(reg.id(), response.clone()).unwrap();
    // Replaying the same challenge id MUST fail (single-use store).
    assert!(matches!(
        engine.finish_registration(reg.id(), response),
        Err(WebAuthnError::InvalidChallenge)
    ));
}

#[test]
fn challenge_expires_at_ttl() {
    let (engine, clock, user) = engine();
    let reg = engine
        .start_registration(user, AuthenticatorAttachment::Platform)
        .unwrap();
    clock.advance_ms(301_000);
    let cred = CredentialId::new(vec![0xDD; 32]).unwrap();
    let response = RegistrationResponse::synthetic_for_test(
        reg.id().clone(),
        Aaguid::touch_id(),
        cred,
        COSE_ALG_ES256,
        AuthenticatorFlags::up_uv(),
        0,
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    assert!(matches!(
        engine.finish_registration(reg.id(), response),
        Err(WebAuthnError::ChallengeExpired)
    ));
}

#[test]
fn recovery_otp_digits_canonical_six() {
    assert_eq!(RECOVERY_OTP_DIGITS, 6);
}

#[test]
fn recovery_otp_ttl_default_under_ceiling() {
    // Lote 10.3-tris caps the OTP TTL at 10 min; default is 600 s.
    assert_eq!(RECOVERY_OTP_TTL_DEFAULT, Duration::from_secs(600));
}
