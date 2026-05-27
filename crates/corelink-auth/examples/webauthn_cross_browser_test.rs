//! Example: simulate the Playwright cross-browser CI matrix
//! (4 browsers × 4 ceremonies = 16 scenarios).
//!
//! Real Playwright tests live in `e2e/webauthn/` (TypeScript) and
//! ride the same engine through a thin HTTP shim. This example
//! demonstrates the canonical flow + invariants the Playwright
//! harness verifies.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "example binary"
)]

use corelink_auth::webauthn::{
    Aaguid, AaguidPolicy, AuthenticationResponse, AuthenticatorAttachment, AuthenticatorFlags,
    Ceremony, CredentialId, EngineConfig, FixedClock, InMemoryEngine, Origin, OriginAllowlist,
    RegistrationResponse, RpId, SignCount, UserAccountId, WebAuthnEngine, COSE_ALG_ES256,
};

const BROWSERS: [&str; 4] = ["chrome", "firefox", "safari", "edge"];
const CEREMONIES: [&str; 4] = [
    "register_passkey",
    "register_yubikey",
    "auth_passkey",
    "auth_yubikey",
];

fn main() {
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
                .build(),
        )
        .build()
        .unwrap();
    let engine = InMemoryEngine::new(cfg, FixedClock::epoch());

    let mut total = 0;
    for browser in BROWSERS {
        for ceremony in CEREMONIES {
            let user = UserAccountId::new_v7();
            let aaguid = if ceremony.contains("yubikey") {
                Aaguid::yubikey_5()
            } else {
                Aaguid::touch_id()
            };
            let attachment = if ceremony.contains("yubikey") {
                AuthenticatorAttachment::CrossPlatform
            } else {
                AuthenticatorAttachment::Platform
            };
            // Always register first.
            let reg = engine.start_registration(user, attachment).unwrap();
            let cred = CredentialId::new(vec![0x33; 32]).unwrap();
            engine
                .finish_registration(
                    reg.id(),
                    RegistrationResponse::synthetic_for_test(
                        reg.id().clone(),
                        aaguid,
                        cred.clone(),
                        COSE_ALG_ES256,
                        AuthenticatorFlags::up_uv(),
                        0,
                        Origin::parse("https://app.corelink.humangr.com").unwrap(),
                    ),
                )
                .unwrap();
            // Then authenticate when the ceremony is an auth flow.
            if ceremony.starts_with("auth") {
                let auth = engine
                    .start_authentication(user, Ceremony::Authentication)
                    .unwrap();
                let response = AuthenticationResponse::synthetic_for_test(
                    auth.id().clone(),
                    cred,
                    AuthenticatorFlags::up_uv(),
                    SignCount::new(1),
                    Origin::parse("https://app.corelink.humangr.com").unwrap(),
                );
                engine.finish_authentication(auth.id(), response).unwrap();
            }
            total += 1;
            println!("scenario ok: {browser} / {ceremony}");
        }
    }
    println!("matrix-total {total}/16");
}
