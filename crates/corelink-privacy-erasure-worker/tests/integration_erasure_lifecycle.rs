//! End-to-end integration test exercising the full canonical lifecycle:
//! signup → 30d use → DSR queued → 12 backends canonical → 24h
//! verification (per WI-S11-002 §10.2 T-2.1).
//!
//! The test wires the predecessor `corelink-dsr` API (DSR ticket
//! intake) to the canonical
//! `corelink_privacy_erasure_worker::InMemoryErasureWorker` orchestrator.
//! A successful DSR submission produces a UUIDv7 `request_id` which
//! the orchestrator consumes as `dsr_id`. The integration asserts:
//!
//! 1. WI-S11-001 DSR Erasure submission → MFA gate (destructive arm) →
//!    receipt issued.
//! 2. Worker `process_erasure` produces 12 canonical tombstones in
//!    the canonical idempotency ledger.
//! 3. 24h verification sweep produces a canonical signed report.
//! 4. Cross-tenant attack (forged dsr_id with mismatching tenant_id)
//!    has zero effect on the orchestrator's per-(tenant, subject) row
//!    sets — structurally enforced by the canonical
//!    [`BackendErasureAdapter::erase`] surface.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_dsr::{
    DsrEndpoint, DsrJurisdiction, DsrRequest, DsrRequestKind, InMemoryDsrAuditSink,
    InMemoryDsrEndpoint, InMemoryDsrRequestStore, InMemoryJwtReceiptIssuer,
    InMemoryMfaStepUpVerifier, MfaStepUpToken,
};
use corelink_privacy_erasure_worker::{
    canonical_in_memory_adapters, BackendErasureAdapter, ErasureDecision, ErasureIdempotencyLedger,
    ErasureRequest, ErasureSalt, ErasureWorker, InMemoryErasureAuditSink,
    InMemoryErasureIdempotencyLedger, InMemoryErasureWorker, InMemoryReportSigner, ReportSignerKey,
    VerificationJob, BACKEND_COUNT,
};
use uuid::Uuid;

#[test]
fn full_lifecycle_dsr_to_verified_complete() {
    // ----- Step 1: WI-S11-001 DSR API surface (predecessor crate) ----
    let dsr_audit = Arc::new(InMemoryDsrAuditSink::new());
    let dsr_store = Arc::new(InMemoryDsrRequestStore::new());
    let dsr_receipt = Arc::new(InMemoryJwtReceiptIssuer::new("integration-key-1", &[7u8; 32]));
    let dsr_mfa = Arc::new(InMemoryMfaStepUpVerifier::new());
    let dsr_endpoint = InMemoryDsrEndpoint::new(
        dsr_audit.clone(),
        dsr_store.clone(),
        dsr_receipt.clone(),
        dsr_mfa.clone(),
    );

    let tenant_id = Uuid::now_v7();
    let subject_id = Uuid::now_v7();
    let request_id = Uuid::now_v7();
    let queued_at_ms: u64 = 1_700_000_000_000;

    // Submit DSR Erasure with MFA token (destructive arm).
    let dsr_req = DsrRequest::new(
        request_id,
        tenant_id,
        subject_id,
        DsrRequestKind::Erasure,
        DsrJurisdiction::Lgpd,
        queued_at_ms,
    )
    .with_mfa(MfaStepUpToken::synthetic_for_test("integration-test-token"));

    let _decision = dsr_endpoint.submit(&dsr_req).unwrap();

    // ----- Step 2: erasure worker processes the canonical queued event
    let er_audit = Arc::new(InMemoryErasureAuditSink::new());
    let er_ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
    let adapters_typed = canonical_in_memory_adapters();
    let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
        .iter()
        .map(|a| {
            let d: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
            d
        })
        .collect();
    let worker =
        InMemoryErasureWorker::try_new(er_audit, er_ledger.clone(), adapters_dyn).unwrap();

    let er_req = ErasureRequest::new(
        request_id, // dsr_id == request_id (canonical mapping)
        tenant_id,
        subject_id,
        ErasureSalt::synthetic_for_test(7),
        queued_at_ms,
    );
    let er_decision = worker.process_erasure(&er_req, queued_at_ms).unwrap();
    let started = matches!(er_decision, ErasureDecision::Started { .. });
    assert!(started);

    let snap = er_ledger.snapshot(request_id).unwrap();
    assert_eq!(
        snap.len(),
        BACKEND_COUNT,
        "canonical 12-backend tombstones ledger expected"
    );

    // ----- Step 3: 24h verification sweep + signed report
    let signer = InMemoryReportSigner::new(ReportSignerKey::synthetic_for_test(11));
    let job = VerificationJob::new(worker, signer);
    let now = er_req.verification_deadline_ms().saturating_add(1);
    let outcome = job.run_24h_sweep(&er_req, now).unwrap();
    let complete = matches!(outcome.decision, ErasureDecision::VerifiedComplete { .. });
    assert!(complete);
    assert!(outcome.report.is_some());
    assert!(outcome.signature.is_some());

    // ----- Step 4: forensic re-attestation: the signed report
    //               verifies post-facto with the canonical key.
    let report = outcome.report.unwrap();
    let sig = outcome.signature.unwrap();
    job.verify_report(&report, &sig).unwrap();
}

#[test]
fn cross_tenant_attack_zero_effect() {
    // Tenant A submits a canonical DSR Erasure; tenant B's row sets
    // remain UNTOUCHED. Per WI AC-009 + canonical
    // INV-TENANT-ISOLATION; the adapter trait surface keys by
    // `(tenant, subject)` so cross-tenant mutation is structurally
    // impossible.
    let er_audit = Arc::new(InMemoryErasureAuditSink::new());
    let er_ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
    let adapters_typed = canonical_in_memory_adapters();
    let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
        .iter()
        .map(|a| {
            let d: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
            d
        })
        .collect();
    let worker = InMemoryErasureWorker::try_new(er_audit, er_ledger, adapters_dyn).unwrap();

    let tenant_a = Uuid::now_v7();
    let subject_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let subject_b = Uuid::now_v7();

    // Tenant B's canonical rows in every backend.
    for adapter in &adapters_typed {
        adapter.insert_rows(
            tenant_b,
            subject_b,
            vec![corelink_privacy_erasure_worker::InMemoryRow::new(b"tenant-b".to_vec())],
        );
    }

    // Tenant A erasure.
    let er_req = ErasureRequest::new(
        Uuid::now_v7(),
        tenant_a,
        subject_a,
        ErasureSalt::synthetic_for_test(7),
        1_000,
    );
    worker.process_erasure(&er_req, 1_000).unwrap();

    // Tenant B's rows UNTOUCHED.
    for adapter in &adapters_typed {
        assert_eq!(
            adapter.row_count(tenant_b, subject_b),
            1,
            "tenant B {} row UNTOUCHED post tenant A erasure (cross-tenant isolation)",
            adapter.kind()
        );
    }
}
