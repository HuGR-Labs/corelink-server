//! Property test: every issued JWT verifies with the matching public
//! key; tampering with any byte breaks verification (RS256 signature
//! integrity; INV-CONSENT-PROOF-VERIFIABLE).

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
    verify_receipt, ConsentProofPayload, DpaAcceptanceRequest, LocaleBcp47, RsaPublicKeyPem,
};
use proptest::prelude::*;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 16
/// for the PR gate; override via `PROPTEST_CASES=N` for stress runs.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(16)
}

fn build_req(ui_capture_ts: i64) -> DpaAcceptanceRequest {
    let registry = registry_three_locales();
    DpaAcceptanceRequest {
        proof: ConsentProofPayload {
            notice_text_hash: registry.hash_for(LocaleBcp47::PtBr).unwrap(),
            notice_version: "1.0.0".into(),
            dpa_version: "1.0.0".into(),
            locale: LocaleBcp47::PtBr,
            wording_id: "11111111-1111-7111-8111-111111111111".into(),
            ui_capture_ts,
            submission_ts: 0,
        },
    }
}

#[test]
fn signed_receipt_verifies_with_matching_key() {
    let (svc, pubkey) = build_service(1_700_000_000_000);
    let c = ctx("sig-jwt", LocaleBcp47::PtBr);
    let receipt = svc.accept(&c, build_req(1_699_999_999_000)).unwrap();
    let claims = verify_receipt(&pubkey, Some("kid-test-01"), &receipt.jwt_receipt).unwrap();
    assert_eq!(claims.sub, "tenant-sig-jwt");
    assert_eq!(claims.dpa_version, "1.0.0");
    assert_eq!(claims.jurisdiction, "BR");
    assert_eq!(claims.jti, receipt.jti);
}

#[test]
fn verify_rejects_wrong_kid() {
    let (svc, pubkey) = build_service(1_700_000_000_000);
    let c = ctx("sig-jwt-kid", LocaleBcp47::PtBr);
    let receipt = svc.accept(&c, build_req(0)).unwrap();
    let err = verify_receipt(&pubkey, Some("wrong-kid"), &receipt.jwt_receipt).unwrap_err();
    assert!(format!("{}", err).contains("kid mismatch"));
}

#[test]
fn verify_rejects_wrong_public_key() {
    let (svc, _own_pub) = build_service(1_700_000_000_000);
    let c = ctx("sig-jwt-otherkey", LocaleBcp47::PtBr);
    let receipt = svc.accept(&c, build_req(0)).unwrap();
    // Generate a foreign keypair and try to verify with its public key.
    let foreign_pub: RsaPublicKeyPem = common::gen_keys().public;
    let err = verify_receipt(&foreign_pub, Some("kid-test-01"), &receipt.jwt_receipt).unwrap_err();
    assert!(format!("{}", err).to_lowercase().contains("invalid"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]

    #[test]
    fn tampering_breaks_signature(flip_pos in 0usize..256) {
        let (svc, pubkey) = build_service(1_700_000_000_000);
        let c = ctx("sig-jwt-tamper", LocaleBcp47::PtBr);
        let receipt = svc.accept(&c, build_req(0)).unwrap();
        let mut bytes = receipt.jwt_receipt.clone().into_bytes();
        // Flip a byte in the payload segment (between the two dots).
        let dot1 = bytes.iter().position(|&b| b == b'.').unwrap();
        let dot2 = bytes.iter().rposition(|&b| b == b'.').unwrap();
        prop_assume!(dot2 > dot1 + 1);
        let region = dot2 - dot1 - 1;
        let idx = dot1 + 1 + (flip_pos % region);
        // Flip to a different base64url-safe char to keep the token
        // structurally parseable; signature still mismatches.
        let cur = bytes[idx];
        bytes[idx] = if cur == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(bytes).unwrap();
        let r = verify_receipt(&pubkey, Some("kid-test-01"), &tampered);
        prop_assert!(r.is_err(), "tampered token must fail verify");
    }
}
