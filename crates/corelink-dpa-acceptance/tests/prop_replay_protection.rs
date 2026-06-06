//! Replay protection: an issued JWT continues to verify (it's a legal
//! receipt; intentional per WI §15 chaos #6) but the **service** does
//! not re-mint receipts on idempotent retry. Replay attempts that
//! change any of the persisted claims are detected by the verify path.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures"
)]

mod common;

use common::{build_service, ctx, registry_three_locales};

use corelink_dpa_acceptance::{
    verify_receipt, ConsentProofPayload, DpaAcceptanceRequest, LocaleBcp47,
};

fn build_req(wording_id: &str) -> DpaAcceptanceRequest {
    let registry = registry_three_locales();
    DpaAcceptanceRequest {
        proof: ConsentProofPayload {
            notice_text_hash: registry.hash_for(LocaleBcp47::Es419).unwrap(),
            notice_version: "1.0.0".into(),
            dpa_version: "1.0.0".into(),
            locale: LocaleBcp47::Es419,
            wording_id: wording_id.into(),
            ui_capture_ts: 1_699_999_999_000,
            submission_ts: 0,
        },
    }
}

#[test]
fn replay_returns_same_jti_no_re_sign() {
    let (svc, _pub) = build_service(1_700_000_000_000);
    let c = ctx("sig-replay", LocaleBcp47::Es419);
    let req = build_req("33333333-3333-7333-8333-333333333333");
    let first = svc.accept(&c, req.clone()).unwrap();
    let replay = svc.accept(&c, req).unwrap();
    assert_eq!(first.jti, replay.jti);
    // Notification fires only on the first acceptance — replay path
    // returns the recovered envelope without re-emitting.
    assert_eq!(svc.notification_sink().count(), 1);
}

#[test]
fn replay_intentionally_verifies_post_facto() {
    // WI §15 chaos #6: replay JWT 1y later is intentionally accepted by
    // the verify endpoint — the JWT is the legal receipt.
    let (svc, pubkey) = build_service(1_700_000_000_000);
    let c = ctx("sig-replay-verify", LocaleBcp47::Es419);
    let req = build_req("44444444-4444-7444-8444-444444444444");
    let receipt = svc.accept(&c, req).unwrap();
    let claims = verify_receipt(&pubkey, Some("kid-test-01"), &receipt.jwt_receipt).unwrap();
    assert_eq!(claims.jti, receipt.jti);
    // exp = iat + 10y → verifies far into the future.
    assert!(claims.exp - claims.iat >= 315_360_000);
}

#[test]
fn forged_claims_with_same_jti_rejected() {
    use jsonwebtoken::{decode_header, Algorithm, EncodingKey, Header};
    let (svc, pubkey) = build_service(1_700_000_000_000);
    let c = ctx("sig-forge", LocaleBcp47::Es419);
    let req = build_req("55555555-5555-7555-8555-555555555555");
    let receipt = svc.accept(&c, req).unwrap();
    // Sanity: header has expected kid.
    let header = decode_header(&receipt.jwt_receipt).unwrap();
    assert_eq!(header.kid.as_deref(), Some("kid-test-01"));
    // Forge: build a token with a foreign key + same kid + same jti.
    let foreign = common::gen_keys();
    let key = EncodingKey::from_rsa_pem(foreign.private.0.as_bytes()).unwrap();
    let mut hdr = Header::new(Algorithm::RS256);
    hdr.kid = Some("kid-test-01".into());
    let claims = corelink_dpa_acceptance::JwtReceiptClaims {
        sub: "tenant-sig-forge".into(),
        dpa_version: "9.9.9".into(), // tampered version
        accepted_at: 1_700_000_000_000,
        jurisdiction: "LATAM".into(),
        jti: receipt.jti.clone(),
        iat: 1_700_000_000,
        exp: 1_700_000_000 + 3600,
    };
    let forged = jsonwebtoken::encode(&hdr, &claims, &key).unwrap();
    // Verify against the **service's** public key → fails.
    let err = verify_receipt(&pubkey, Some("kid-test-01"), &forged).unwrap_err();
    assert!(format!("{}", err).to_lowercase().contains("invalid"));
}
