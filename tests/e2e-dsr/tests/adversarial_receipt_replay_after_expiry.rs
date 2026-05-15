//! Adversarial 5 — Replay a JWT receipt AFTER the canonical 90d
//! anti-replay window has elapsed.
//!
//! Per WI-S11-001 §1 `RECEIPT_EXPIRY_DAYS = 90`: a receipt verified at
//! `now_ms >= submitted + 90d` MUST surface as
//! `DsrReceiptError::Expired` so the customer SDK / UI knows to
//! re-prompt for a fresh submission.

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_dsr::{
    DsrDecision, DsrEndpoint, DsrJurisdiction, DsrReceiptError, DsrRequestKind, JwtReceiptIssuer,
};
use e2e_dsr::{canonical_dsr_for, make_test_tenant, setup_test_env};

#[test]
fn receipt_replay_after_90d_rejected() {
    let env = setup_test_env();
    let tenant = make_test_tenant("replay-tenant");

    let request = canonical_dsr_for(
        &tenant,
        DsrRequestKind::Access,
        DsrJurisdiction::Gdpr,
        env.now_ms,
    );
    let decision = match env.dsr.submit(&request) {
        Ok(d) => d,
        Err(e) => panic!("submit: {e:?}"),
    };
    let receipt = match decision {
        DsrDecision::RequestAccepted { receipt, .. } => receipt,
        other => panic!("expected RequestAccepted, got {other:?}"),
    };

    // Within the 90d window: verify succeeds.
    if let Err(e) = env
        .dsr_receipt_issuer
        .verify(&receipt, env.now_ms.saturating_add(60_000))
    {
        panic!("in-window verify failed: {e:?}");
    }

    // At submitted + 90d + 1ms: verify rejects with Expired.
    let past_window = env
        .now_ms
        .saturating_add(90 * 86_400_000)
        .saturating_add(1);
    let err = match env.dsr_receipt_issuer.verify(&receipt, past_window) {
        Ok(_) => panic!("expired receipt must NOT verify"),
        Err(e) => e,
    };
    assert!(
        matches!(err, DsrReceiptError::Expired),
        "expected Expired, got {err:?}"
    );

    // Exactly AT the expiry boundary (>= per is_expired): also Expired.
    let at_window = env.now_ms.saturating_add(90 * 86_400_000);
    let err = match env.dsr_receipt_issuer.verify(&receipt, at_window) {
        Ok(_) => panic!("at-boundary receipt must NOT verify"),
        Err(e) => e,
    };
    assert!(matches!(err, DsrReceiptError::Expired));
}
