//! Example: admin operation gated by a YubiKey 5 step-up ceremony.
//!
//! Run via `cargo run --package corelink-webauthn --example yubikey_admin_op`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "example binary"
)]

use corelink_webauthn::{
    Aaguid, AaguidPolicy, AuthenticationResponse, AuthenticatorAttachment, AuthenticatorFlags,
    Ceremony, CredentialId, EngineConfig, FixedClock, InMemoryEngine, Origin, OriginAllowlist,
    RegistrationResponse, RpId, SignCount, UserAccountId, WebAuthnEngine, COSE_ALG_ES256,
};

fn main() {
    let cfg = EngineConfig::builder(RpId::new("corelink.dev").unwrap(), "CoreLink")
        .origins(
            OriginAllowlist::from_strings(["https://admin.corelink.dev"]).unwrap(),
        )
        .aaguids(
            AaguidPolicy::builder()
                .allow(Aaguid::yubikey_5())
                .build(),
        )
        .build()
        .unwrap();
    let engine = InMemoryEngine::new(cfg, FixedClock::epoch());
    let user = UserAccountId::new_v7();

    // Enrol the YubiKey first.
    let reg = engine
        .start_registration(user, AuthenticatorAttachment::CrossPlatform)
        .unwrap();
    let cred = CredentialId::new(vec![0xAA; 32]).unwrap();
    engine
        .finish_registration(
            reg.id(),
            RegistrationResponse::synthetic_for_test(
                reg.id().clone(),
                Aaguid::yubikey_5(),
                cred.clone(),
                COSE_ALG_ES256,
                AuthenticatorFlags::up_uv(),
                0,
                Origin::parse("https://admin.corelink.dev").unwrap(),
            ),
        )
        .unwrap();
    println!("YubiKey 5 enrolled.");

    // Step-up for "mass_revoke" admin op.
    let auth = engine
        .start_authentication(user, Ceremony::AdminStepUp)
        .unwrap();
    println!("Step-up challenge id={:?}", auth.id());
    let response = AuthenticationResponse::synthetic_for_test(
        auth.id().clone(),
        cred,
        AuthenticatorFlags::up_uv(),
        SignCount::new(1),
        Origin::parse("https://admin.corelink.dev").unwrap(),
    );
    let outcome = engine.finish_authentication(auth.id(), response).unwrap();
    println!(
        "Authenticated: admin_step_up={} sign_count={}",
        outcome.is_admin_step_up(),
        outcome.persisted_sign_count().value()
    );
}
