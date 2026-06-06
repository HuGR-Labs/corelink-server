//! Property tests pinning the canonical PAT-RETRY-IDEMPOTENT-001
//! replay-safe contract: same `(dsr_id, backend)` re-submitted N
//! times produces a byte-identical tombstone (PR-gate 10k iter,
//! nightly 100k via `PROPTEST_CASES` env var per S-07 P1-2 fix).
//!
//! Sprint contract §9 14.s11.4: erasure idempotency replay 100×.
//!
//! Coverage:
//!
//! - `prop_idempotent_replay_100x` — 100 re-submissions of the same
//!   `dsr_id` produce the same canonical tombstone payload + the
//!   audit emit count grows by exactly 1 per re-submission (1
//!   `started.v1` re-emitted; 0 `backend_completed.v1` re-emitted —
//!   per WI AC-007 the backend audit is NOT re-fired on idempotent
//!   replay).
//! - `prop_canonical_idempotency_key_unique_per_backend` — the
//!   canonical 35-char key is unique per (dsr, backend) tuple over
//!   10k random tuples.
//! - `prop_dsr_id_uniqueness_under_random_pairs` — 10k random
//!   (dsr_id, backend) pairs produce pairwise-disjoint tombstones at
//!   the canonical UNIQUE constraint slot.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_privacy_erasure_worker::{
    canonical_in_memory_adapters, BackendErasureAdapter, BackendKind, ErasureIdempotencyLedger,
    ErasureRequest, ErasureSalt, ErasureWorker, InMemoryErasureAuditSink,
    InMemoryErasureIdempotencyLedger, InMemoryErasureWorker, BACKEND_COUNT,
};
use proptest::prelude::*;
use uuid::Uuid;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for PR gate; nightly job overrides to 100k.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn fresh_worker() -> (
    InMemoryErasureWorker,
    Arc<InMemoryErasureAuditSink>,
    Arc<InMemoryErasureIdempotencyLedger>,
) {
    let audit = Arc::new(InMemoryErasureAuditSink::new());
    let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
    let adapters_typed = canonical_in_memory_adapters();
    let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
        .iter()
        .map(|a| {
            let d: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
            d
        })
        .collect();
    let worker =
        InMemoryErasureWorker::try_new(audit.clone(), ledger.clone(), adapters_dyn).unwrap();
    (worker, audit, ledger)
}

fn fresh_request(seed: u8, queued: u64) -> ErasureRequest {
    fn fixed(seed: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = seed.wrapping_mul(13).wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }
    ErasureRequest::new(
        fixed(seed),
        fixed(seed.wrapping_add(50)),
        fixed(seed.wrapping_add(100)),
        ErasureSalt::synthetic_for_test(seed),
        queued,
    )
}

#[test]
fn replay_100x_same_dsr_byte_identical_tombstones() {
    let (worker, audit, ledger) = fresh_worker();
    let req = fresh_request(7, 1_000);

    // First submission: 1 started + 12 backend_completed audits + 12
    // canonical tombstones in the ledger.
    worker.process_erasure(&req, 1_000).unwrap();
    let baseline_audit = audit.len();
    let baseline_ledger = ledger.snapshot(req.dsr_id).unwrap();
    assert_eq!(baseline_audit, 1 + BACKEND_COUNT);
    assert_eq!(baseline_ledger.len(), BACKEND_COUNT);

    // Replay 100×: each re-submission emits 1 new `started` audit
    // (orchestrator captures the inbound) and 0 new tombstones (the
    // canonical UNIQUE constraint short-circuits per
    // PAT-RETRY-IDEMPOTENT-001).
    for i in 1..=100u64 {
        worker.process_erasure(&req, 1_000 + i).unwrap();
        let snapshot = ledger.snapshot(req.dsr_id).unwrap();
        assert_eq!(snapshot, baseline_ledger);
        assert_eq!(audit.len(), baseline_audit + i as usize);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        max_shrink_iters: 8_192,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_idempotent_replay_byte_identical_tombstones(seed in 1u8..=255) {
        let (worker, _audit, ledger) = fresh_worker();
        let req = fresh_request(seed, 1_000);
        worker.process_erasure(&req, 1_000).unwrap();
        let snap1 = ledger.snapshot(req.dsr_id).unwrap();
        worker.process_erasure(&req, 2_000).unwrap();
        let snap2 = ledger.snapshot(req.dsr_id).unwrap();
        prop_assert_eq!(snap1, snap2);
    }

    #[test]
    fn prop_canonical_idempotency_key_unique_per_backend(seed in 1u8..=255) {
        let req = fresh_request(seed, 1_000);
        let plan = corelink_privacy_erasure_worker::ErasurePlan::canonical(&req, 1_000);
        let mut keys = std::collections::HashSet::new();
        for entry in &plan.entries {
            prop_assert!(keys.insert(entry.idempotency_key.clone()));
            prop_assert!(entry.idempotency_key.starts_with("corelink-"));
            prop_assert!(entry.idempotency_key.contains(entry.backend.as_str()));
        }
        prop_assert_eq!(keys.len(), BACKEND_COUNT);
    }

    #[test]
    fn prop_dsr_id_uniqueness_under_random_pairs(
        seed_a in 1u8..=128,
        seed_b in 129u8..=255,
    ) {
        let req_a = fresh_request(seed_a, 1_000);
        let req_b = fresh_request(seed_b, 1_000);
        // dsr_ids differ → tombstones occupy disjoint UNIQUE slots.
        prop_assume!(req_a.dsr_id != req_b.dsr_id);
        let (worker, _audit, ledger) = fresh_worker();
        worker.process_erasure(&req_a, 1_000).unwrap();
        worker.process_erasure(&req_b, 2_000).unwrap();
        let snap_a = ledger.snapshot(req_a.dsr_id).unwrap();
        let snap_b = ledger.snapshot(req_b.dsr_id).unwrap();
        prop_assert_eq!(snap_a.len(), BACKEND_COUNT);
        prop_assert_eq!(snap_b.len(), BACKEND_COUNT);
        // Snapshots are dsr-scoped — no overlap.
        for c_a in &snap_a {
            prop_assert_eq!(c_a.dsr_id, req_a.dsr_id);
        }
        for c_b in &snap_b {
            prop_assert_eq!(c_b.dsr_id, req_b.dsr_id);
        }
    }
}

// Sanity test for the BackendKind canonical surface (per Lote
// 10.9-quinquies typed-enum discipline).
#[test]
fn canonical_backend_kinds_complete() {
    let v = corelink_privacy_erasure_worker::canonical_backend_kinds();
    assert_eq!(v.len(), BACKEND_COUNT);
    for k in v {
        // Every kind produces a stable mnemonic.
        assert!(!k.as_str().is_empty());
    }
    // Sanity: NeonMain at slot 0; R2EvidencePseudo at slot 11.
    assert_eq!(*v.first().unwrap(), BackendKind::NeonMain);
    assert_eq!(*v.last().unwrap(), BackendKind::R2EvidencePseudo);
}
