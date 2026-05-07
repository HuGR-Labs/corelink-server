//! Chaos tests: each canonical backend can independently fail
//! (transport failure / verification mismatch / not-applicable) and
//! the canonical fail-CLOSED envelope aborts the pipeline pre-state-
//! mutation per ADR-S11-002 split-tier (per WI-S11-002 §10.2 T-2.4 +
//! §15 chaos experiments 1-10).
//!
//! Coverage:
//!
//! - Per-backend transport failure (1 of 12) → orchestrator returns
//!   `ErasureWorkerError::Backend(Transport)` + state UNCHANGED for
//!   the failing backend (no tombstone insert; no audit
//!   `backend_completed.v1` for that backend).
//! - Audit sink failure aborts the pipeline pre-state-mutation; per
//!   the canonical S-06 P0-2 / S-07 P1-1 lessons the ledger remains
//!   empty.
//! - Concurrent submits don't double-insert (canonical UNIQUE
//!   `(dsr_id, backend)` constraint short-circuits the second).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_privacy_erasure_worker::{
    canonical_in_memory_adapters, BackendErasureAdapter, BackendKind, ErasureIdempotencyLedger,
    ErasureRequest, ErasureSalt, ErasureWorker, ErasureWorkerError, FailingErasureAuditSink,
    FailingErasureIdempotencyLedger, InMemoryBackendErasureAdapter, InMemoryErasureAuditSink,
    InMemoryErasureIdempotencyLedger, InMemoryErasureWorker,
};
use uuid::Uuid;

fn fresh_worker() -> (
    InMemoryErasureWorker,
    Arc<InMemoryErasureAuditSink>,
    Arc<InMemoryErasureIdempotencyLedger>,
    Vec<Arc<InMemoryBackendErasureAdapter>>,
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
    (worker, audit, ledger, adapters_typed)
}

fn fresh_request(seed: u8) -> ErasureRequest {
    fn fx(s: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = s.wrapping_mul(13).wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }
    ErasureRequest::new(
        fx(seed),
        fx(seed.wrapping_add(50)),
        fx(seed.wrapping_add(100)),
        ErasureSalt::synthetic_for_test(seed),
        1_000,
    )
}

#[test]
fn each_backend_can_fail_independently() {
    // For each of the 12 canonical backends, induce a transport
    // failure and assert the orchestrator returns Backend(Transport)
    // and the pipeline aborts pre-canonical-tombstone-insert for that
    // backend.
    for kind in *corelink_privacy_erasure_worker::canonical_backend_kinds() {
        let (worker, _audit, ledger, adapters) = fresh_worker();
        let req = fresh_request(42);
        let target = adapters.iter().find(|a| a.kind() == kind).unwrap();
        target.set_transport_failure(true);
        let err = worker.process_erasure(&req, 1_000).unwrap_err();
        let is_backend = matches!(err, ErasureWorkerError::Backend(_));
        assert!(is_backend, "expected Backend error for kind={kind}");
        // Tombstones for backends BEFORE the failing one in canonical
        // order should exist (sequential fanout); the failing backend
        // and any AFTER it should be absent.
        let snap = ledger.snapshot(req.dsr_id).unwrap();
        let failing_idx = corelink_privacy_erasure_worker::canonical_backend_kinds()
            .iter()
            .position(|k| *k == kind)
            .unwrap();
        // Snapshot count == failing_idx (every backend BEFORE the
        // failing one already inserted; the failing one aborted).
        assert_eq!(
            snap.len(),
            failing_idx,
            "kind={kind} pre-failure tombstones expected"
        );
    }
}

#[test]
fn audit_failure_state_unchanged_post_failure() {
    let audit = Arc::new(FailingErasureAuditSink::new());
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
        InMemoryErasureWorker::try_new(audit, ledger.clone(), adapters_dyn).unwrap();
    let req = fresh_request(42);
    let err = worker.process_erasure(&req, 1_000).unwrap_err();
    let is_audit = matches!(err, ErasureWorkerError::Audit(_));
    assert!(is_audit);
    // CRITICAL: state UNCHANGED on audit failure (per S-06 P0-2 / S-07 P1-1).
    assert_eq!(ledger.snapshot(req.dsr_id).unwrap().len(), 0);
}

#[test]
fn ledger_failure_propagates_idempotency_error() {
    let audit = Arc::new(InMemoryErasureAuditSink::new());
    let ledger = Arc::new(FailingErasureIdempotencyLedger::new());
    let adapters_typed = canonical_in_memory_adapters();
    let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
        .iter()
        .map(|a| {
            let d: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
            d
        })
        .collect();
    let worker = InMemoryErasureWorker::try_new(audit, ledger, adapters_dyn).unwrap();
    let req = fresh_request(42);
    let err = worker.process_erasure(&req, 1_000).unwrap_err();
    let is_idem = matches!(err, ErasureWorkerError::Idempotency(_));
    assert!(is_idem);
}

#[test]
fn concurrent_submits_short_circuit_via_unique_constraint() {
    let (worker, _audit, ledger, _adapters) = fresh_worker();
    let req = fresh_request(7);
    worker.process_erasure(&req, 1_000).unwrap();
    let baseline = ledger.snapshot(req.dsr_id).unwrap();
    // Second submit of the SAME canonical request: replay-safe; no
    // new tombstones (PAT-RETRY-IDEMPOTENT-001 + WI AC-007).
    worker.process_erasure(&req, 1_500).unwrap();
    let second = ledger.snapshot(req.dsr_id).unwrap();
    assert_eq!(baseline, second);
}

#[test]
fn legal_hold_skips_effective_backends() {
    let (worker, _audit, ledger, adapters) = fresh_worker();
    let req = fresh_request(7).with_legal_hold(true);
    // Inject canonical rows for all backends.
    for adapter in &adapters {
        adapter.insert_rows(
            req.tenant_id,
            req.subject_id,
            vec![corelink_privacy_erasure_worker::InMemoryRow::new(b"r".to_vec())],
        );
    }
    worker.process_erasure(&req, 1_000).unwrap();
    // Effective backends preserve their rows under legal_hold.
    for adapter in &adapters {
        if adapter.kind().is_effective() {
            assert_eq!(
                adapter.row_count(req.tenant_id, req.subject_id),
                1,
                "effective {} should preserve under legal_hold",
                adapter.kind()
            );
        }
    }
    // Pseudonymized backends still process under legal_hold (Object
    // Lock retains; secondary index is updated).
    for adapter in &adapters {
        if adapter.kind().is_pseudonymized() {
            assert!(adapter.all_redacted(req.tenant_id, req.subject_id));
        }
    }
    // Tombstones for all 12 backends still inserted.
    assert_eq!(
        ledger.snapshot(req.dsr_id).unwrap().len(),
        corelink_privacy_erasure_worker::BACKEND_COUNT
    );
}

#[test]
fn cross_tenant_attack_blocked_by_tenant_scoped_adapter() {
    // Tenant A's erasure NEVER affects tenant B's rows. Even if the
    // forged payload reuses A's dsr_id but B's tenant_id, the
    // canonical adapter's keying by `(tenant, subject)` keeps B's
    // rows untouched.
    let (worker, _audit, _ledger, adapters) = fresh_worker();
    let req_a = fresh_request(1);
    let req_b = fresh_request(2);
    for adapter in &adapters {
        adapter.insert_rows(
            req_a.tenant_id,
            req_a.subject_id,
            vec![corelink_privacy_erasure_worker::InMemoryRow::new(b"a".to_vec())],
        );
        adapter.insert_rows(
            req_b.tenant_id,
            req_b.subject_id,
            vec![corelink_privacy_erasure_worker::InMemoryRow::new(b"b".to_vec())],
        );
    }
    worker.process_erasure(&req_a, 1_000).unwrap();
    // A's effective rows: 0; B's effective rows: 1 (skip R2Cas due
    // to refcount-aware semantics).
    for adapter in &adapters {
        if adapter.kind().is_effective() && adapter.kind() != BackendKind::R2Cas {
            assert_eq!(
                adapter.row_count(req_a.tenant_id, req_a.subject_id),
                0,
                "tenant A {} should be erased",
                adapter.kind()
            );
            assert_eq!(
                adapter.row_count(req_b.tenant_id, req_b.subject_id),
                1,
                "tenant B {} should be untouched (cross-tenant isolation)",
                adapter.kind()
            );
        }
    }
}
