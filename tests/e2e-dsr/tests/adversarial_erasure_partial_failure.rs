//! Adversarial 3 — Erasure worker partial backend failure.
//!
//! 1 of the 12 canonical backends has transport failures induced via
//! the in-memory chaos toggle. The worker fans out, captures the
//! transport-failure-induced backend error, and surfaces it via the
//! orchestrator's canonical fail-CLOSED envelope (the
//! `process_erasure` call returns an `ErasureWorkerError::Backend`
//! error). No tombstone is inserted for the failed backend so a
//! re-run can complete the canonical sweep — replay-safe per
//! PAT-RETRY-IDEMPOTENT-001.
//!
//! Once the chaos flag is cleared, the second call completes the
//! cascade. The 24h verification sweep then reports `VerifiedComplete`
//! after every backend has a successful completion.

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_privacy_erasure_worker::{
    BackendErasureAdapter, BackendKind, ErasureDecision, ErasureIdempotencyLedger, ErasureWorker,
    ErasureWorkerError,
};
use e2e_dsr::{canonical_erasure_for, make_test_tenant, seed_backends_for, setup_test_env};

#[test]
fn partial_backend_failure_aborts_then_replays() {
    let env = setup_test_env();
    let tenant = make_test_tenant("partial-failure-tenant");
    seed_backends_for(&env, &tenant, 2);

    // Induce a transport failure on the canonical Stripe backend (one
    // of the 8 effective backends; matches the production semantics
    // where Stripe API has the most network surface area).
    let stripe = env
        .backend_adapters
        .iter()
        .find(|a| a.kind() == BackendKind::Stripe);
    let stripe = match stripe {
        Some(a) => a.clone(),
        None => panic!("missing canonical Stripe adapter"),
    };
    stripe.set_transport_failure(true);

    let dsr_id = uuid::Uuid::now_v7();
    let erasure = canonical_erasure_for(&tenant, dsr_id, env.now_ms);

    // First run: aborts at the Stripe slot with a transport backend
    // error per the canonical fail-CLOSED envelope.
    let err = match env.erasure_worker.process_erasure(&erasure, env.now_ms) {
        Ok(d) => panic!("expected backend error, got {d:?}"),
        Err(e) => e,
    };
    assert!(
        matches!(err, ErasureWorkerError::Backend(_)),
        "expected ErasureWorkerError::Backend, got {err:?}"
    );

    // Stripe tombstone was NOT inserted (audit-then-mutate rolls
    // back the half-completed slot via the canonical replay path).
    let snapshot = match env.erasure_ledger.snapshot(dsr_id) {
        Ok(s) => s,
        Err(e) => panic!("ledger snapshot: {e:?}"),
    };
    // Backends that completed before Stripe in canonical order should
    // have been inserted; Stripe + the 4 backends after it should be
    // empty. Canonical order: NeonMain, NeonBilling, R2Cas, R2Ac, D1,
    // Kv, Stripe, Loki, R2AuditPseudo, NeonPitrPseudo,
    // R2CasLegalHoldPseudo, R2EvidencePseudo — Stripe is index 6.
    assert!(
        snapshot.len() < 12,
        "expected partial snapshot, got {}",
        snapshot.len()
    );
    assert!(
        !snapshot.iter().any(|c| c.backend == BackendKind::Stripe),
        "Stripe tombstone must NOT exist on partial-failure path"
    );

    // ---- Replay: clear chaos toggle + re-run ----
    stripe.set_transport_failure(false);
    let dispatch = match env
        .erasure_worker
        .process_erasure(&erasure, env.now_ms.saturating_add(1))
    {
        Ok(d) => d,
        Err(e) => panic!("replay process_erasure failed: {e:?}"),
    };
    assert!(matches!(dispatch, ErasureDecision::Started { .. }));

    // Now every canonical backend has a tombstone.
    let snapshot2 = match env.erasure_ledger.snapshot(dsr_id) {
        Ok(s) => s,
        Err(e) => panic!("ledger snapshot: {e:?}"),
    };
    assert_eq!(snapshot2.len(), 12, "12 canonical backends post-replay");

    // Verification sweep is VerifiedComplete.
    let outcome = match env
        .verification_job
        .run_24h_sweep(&erasure, env.now_ms.saturating_add(60_000))
    {
        Ok(o) => o,
        Err(e) => panic!("verification sweep: {e:?}"),
    };
    assert!(
        matches!(outcome.decision, ErasureDecision::VerifiedComplete { .. }),
        "expected VerifiedComplete after replay, got {:?}",
        outcome.decision
    );
}
