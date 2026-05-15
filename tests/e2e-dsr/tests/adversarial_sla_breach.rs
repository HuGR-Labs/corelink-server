//! Adversarial 4 — Erasure worker stalled past the 24h verification
//! deadline. The verification sweep at `now_ms > queued_at + 24h`
//! WITHOUT any backend completions emits the canonical SEV-1
//! `SlaBreached` decision per FM-450 + RB-DSR-ERASURE-INCOMPLETE
//! runbook.

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_privacy_erasure_worker::{ErasureDecision, BACKEND_COUNT, VERIFICATION_SLA_MS};
use e2e_dsr::{canonical_erasure_for, make_test_tenant, setup_test_env};

#[test]
fn sla_breach_fires_sev1_after_24h() {
    let env = setup_test_env();
    let tenant = make_test_tenant("sla-breach-tenant");
    let dsr_id = uuid::Uuid::now_v7();
    let erasure = canonical_erasure_for(&tenant, dsr_id, env.now_ms);

    // Verification sweep WITHOUT calling process_erasure first (the
    // worker stalled before even starting the fanout). At
    // now = queued + 24h + 1, the canonical SLA gate trips.
    let breach_time = env.now_ms.saturating_add(VERIFICATION_SLA_MS).saturating_add(1);
    let outcome = match env.verification_job.run_24h_sweep(&erasure, breach_time) {
        Ok(o) => o,
        Err(e) => panic!("verification sweep failed: {e:?}"),
    };
    match outcome.decision {
        ErasureDecision::SlaBreached {
            unverified_count,
            elapsed_ms,
        } => {
            assert_eq!(unverified_count, BACKEND_COUNT, "all 12 canonical backends unverified");
            assert!(
                elapsed_ms > VERIFICATION_SLA_MS,
                "elapsed_ms ({elapsed_ms}) must exceed SLA"
            );
        }
        other => panic!("expected SlaBreached, got {other:?}"),
    }
    // No report / signature / object_key on the SLA-breach arm.
    assert!(outcome.report.is_none());
    assert!(outcome.signature.is_none());
    assert!(outcome.object_key.is_none());
}
