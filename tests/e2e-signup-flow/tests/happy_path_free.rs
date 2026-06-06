//! R3-1 Happy path — Free tier.
//!
//! Pipeline:
//!
//! 1. Signup orchestrator provisions tenant + first PAT atomically.
//! 2. DPA acceptance ledger records the 6-field consent + issues
//!    RS256 JWT receipt.
//! 3. Tier-selection ledger activates Free tier (no Stripe).
//! 4. First PAT is bound in the R2 client.
//! 5. PUT a blob → GET roundtrip → BLAKE3 verify client-side → stat
//!    reports hit.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};
use corelink_signup::SignupOutcome;
use corelink_tier_selection::{TenantId as TierTenantId, TierKind, TierSelectionReceipt};

use e2e_signup_flow::helpers::{
    dpa_ctx_for, dpa_request_for, make_test_tenant, setup_test_ledgers, tier_ctx_for,
    verify_audit_chain, ExpectedAuditEvent, ProvisionedTenant,
};
use e2e_signup_flow::r2::InMemoryR2Client;

fn run() {
    let env = setup_test_ledgers();
    let tenant = make_test_tenant("free-happy");

    // --- (1) Signup ---
    let resp = env.signup.provision(&tenant.signup_request).unwrap();
    let prov = ProvisionedTenant::from_response(resp.clone()).expect("provisioned");
    assert!(matches!(resp.outcome, SignupOutcome::Provisioned { .. }));

    // --- (2) DPA accept ---
    let dpa_req = dpa_request_for(&env, corelink_dpa_acceptance::LocaleBcp47::EnUs);
    let ctx = dpa_ctx_for(&prov, "sig-free-happy");
    let receipt = env.dpa.accept(&ctx, dpa_req).unwrap();
    // Receipt JWT verifies against the public key.
    let claims = corelink_dpa_acceptance::verify_receipt(
        &env.dpa_public_key,
        Some("kid-e2e-test-01"),
        &receipt.jwt_receipt,
    )
    .unwrap();
    assert_eq!(claims.dpa_version, "1.0.0");

    // --- (3) Mark DPA gate accepted + Free tier select ---
    env.dpa_gate
        .accept(TierTenantId::new(prov.tenant_id.as_str()), "1.0.0");
    let receipt = env
        .tier
        .select_tier(&tier_ctx_for(&prov, env.now_ms), TierKind::Free, "u@x.com")
        .unwrap();
    assert!(matches!(
        receipt,
        TierSelectionReceipt::FreeActivated { .. }
    ));
    // Free path does NOT touch Stripe.
    assert!(env.stripe.sessions().is_empty());

    // --- (4) Bind first PAT to R2 client. ---
    env.r2
        .bind_pat(prov.first_pat_hash.as_str(), prov.tenant_id.as_str());

    // --- (5) PUT → GET → BLAKE3 verify → stat hit ---
    let body = Bytes::from_static(b"r3-1: corelink content-addressable cache happy path");
    let digest = Digest::compute(&body);
    let vb = VerifiedBody::new(body.clone(), digest).unwrap();
    let put_digest = env.r2.put(prov.first_pat_hash.as_str(), &vb).unwrap();
    assert_eq!(put_digest, digest);
    let got = env.r2.get(prov.first_pat_hash.as_str(), &digest).unwrap();
    assert_eq!(got, body);
    // Client-side BLAKE3 verify of the returned bytes.
    let recomputed = Digest::compute(&got);
    assert!(recomputed.verify_constant_time(&digest));
    let stat = env.r2.stat(prov.tenant_id.as_str());
    assert_eq!(stat.puts, 1);
    assert_eq!(stat.get_hits, 1);
    assert_eq!(stat.get_misses, 0);

    // --- (6) Audit chain canonical ordering ---
    verify_audit_chain(
        &env,
        &[
            ExpectedAuditEvent::SignupStarted,
            ExpectedAuditEvent::SignupCompleted,
            ExpectedAuditEvent::DpaAccepted,
            ExpectedAuditEvent::TierAttempted,
            ExpectedAuditEvent::TierActivatedFree,
        ],
    )
    .unwrap();
}

#[test]
fn r3_1_happy_path_free_tier() {
    // Spin up a tokio runtime — the harness is sync-only today, but
    // the R2 trait surface is async-by-design (BlobStoreWrite future).
    // Per WI we always cross the async boundary explicitly in case
    // follow-on tests need it.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map(|rt| rt.block_on(async { run() }))
        .unwrap();
}

#[test]
fn r3_1_happy_path_free_tier_does_not_touch_r2_for_unbound_pat() {
    let env = setup_test_ledgers();
    let r2 = InMemoryR2Client::new();
    let body = Bytes::from_static(b"x");
    let digest = Digest::compute(&body);
    let vb = VerifiedBody::new(body, digest).unwrap();
    let err = r2.put("pat-not-bound", &vb).unwrap_err();
    assert!(matches!(err, e2e_signup_flow::r2::R2Error::Unauthorized));
    // Mute unused-env warning.
    let _ = env;
}
