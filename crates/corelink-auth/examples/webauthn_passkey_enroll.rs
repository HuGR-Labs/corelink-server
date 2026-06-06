//! Example: enrol a platform-authenticator passkey (Touch ID).
//!
//! Run via `cargo run --package corelink-webauthn --example passkey_enroll`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "example binary"
)]

use corelink_auth::webauthn::{
    Aaguid, AaguidPolicy, AuthenticatorAttachment, AuthenticatorFlags, CredentialId, EngineConfig,
    FixedClock, InMemoryEngine, Origin, OriginAllowlist, RegistrationResponse, RpId, UserAccountId,
    WebAuthnEngine, COSE_ALG_ES256,
};

fn main() {
    let cfg = EngineConfig::builder(RpId::new("corelink.humangr.com").unwrap(), "CoreLink")
        .origins(OriginAllowlist::from_strings(["https://app.corelink.humangr.com"]).unwrap())
        .aaguids(AaguidPolicy::builder().allow(Aaguid::touch_id()).build())
        .build()
        .unwrap();
    let engine = InMemoryEngine::new(cfg, FixedClock::epoch());
    let user = UserAccountId::new_v7();

    let challenge = engine
        .start_registration(user, AuthenticatorAttachment::Platform)
        .unwrap();
    println!("Challenge issued: id={:?}", challenge.id());

    let response = RegistrationResponse::synthetic_for_test(
        challenge.id().clone(),
        Aaguid::touch_id(),
        CredentialId::new(vec![0xCD; 32]).unwrap(),
        COSE_ALG_ES256,
        AuthenticatorFlags::up_uv_be(),
        0,
        Origin::parse("https://app.corelink.humangr.com").unwrap(),
    );
    let cred_id = engine
        .finish_registration(challenge.id(), response)
        .unwrap();
    println!("Credential persisted: {cred_id:?}");

    let creds = engine.credential_store().list_for_user(user);
    println!("User has {} credential(s)", creds.len());
}
