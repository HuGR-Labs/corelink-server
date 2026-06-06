//! 24h verification job sweep tests (WI-S11-002 §10.2 T-2.7 +
//! AC-005/006/011).
//!
//! Coverage:
//!
//! - Happy path: full lifecycle (process_erasure + 24h verify) lands
//!   the canonical `VerifiedComplete` arm + a BLAKE3-signed report
//!   uploaded to canonical R2 evidence-dsr key.
//! - Partial failure: a pseudonymized backend has a residual
//!   non-redacted row → `VerifiedPartial` arm + SEV-1 alert hook.
//! - SLA breach: > 24h elapsed with incomplete tombstones →
//!   `SlaBreached` arm + SEV-1 alert hook.
//! - Signed report verifies post-facto (forensic re-attestation).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_privacy_erasure_worker::{
    canonical_cloudevent_types, canonical_in_memory_adapters, BackendErasureAdapter, BackendKind,
    ErasureCloudEventType, ErasureDecision, ErasureRequest, ErasureSalt, ErasureWorker,
    InMemoryErasureAuditSink, InMemoryErasureIdempotencyLedger, InMemoryErasureWorker,
    InMemoryReportSigner, InMemoryRow, ReportSignerKey, VerificationJob,
};
use uuid::Uuid;

fn fixed(seed: u8) -> Uuid {
    let mut b = [0u8; 16];
    for (i, x) in b.iter_mut().enumerate() {
        *x = seed.wrapping_mul(13).wrapping_add(i as u8);
    }
    Uuid::from_bytes(b)
}

fn fresh_job() -> (
    VerificationJob,
    Arc<InMemoryErasureAuditSink>,
    Vec<Arc<corelink_privacy_erasure_worker::InMemoryBackendErasureAdapter>>,
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
    let worker = InMemoryErasureWorker::try_new(audit.clone(), ledger, adapters_dyn).unwrap();
    let signer = InMemoryReportSigner::new(ReportSignerKey::synthetic_for_test(7));
    let job = VerificationJob::new(worker, signer);
    (job, audit, adapters_typed)
}

fn fresh_request(seed: u8) -> ErasureRequest {
    ErasureRequest::new(
        fixed(seed),
        fixed(seed.wrapping_add(50)),
        fixed(seed.wrapping_add(100)),
        ErasureSalt::synthetic_for_test(seed),
        1_000,
    )
}

#[test]
fn happy_path_24h_verification_pass_signed_report() {
    let (job, audit, _) = fresh_job();
    let req = fresh_request(7);
    job.worker().process_erasure(&req, 1_000).unwrap();
    let now = req.verification_deadline_ms().saturating_add(1);
    let outcome = job.run_24h_sweep(&req, now).unwrap();
    let complete = matches!(outcome.decision, ErasureDecision::VerifiedComplete { .. });
    assert!(complete);

    // Canonical signed report present + verifies post-facto.
    let report = outcome.report.unwrap();
    let sig = outcome.signature.unwrap();
    job.verify_report(&report, &sig).unwrap();

    // Canonical R2 object key present.
    let key = outcome.object_key.unwrap();
    assert!(key.starts_with("dsr-reports/"));
    assert!(key.ends_with("/erasure-report.json"));

    // Canonical 5 CloudEvents types present in the report.
    assert_eq!(report.cloudevent_types.len(), 5);
    for canonical in canonical_cloudevent_types() {
        assert!(report.cloudevent_types.iter().any(|s| s == canonical));
    }

    // Canonical audit sink captured: 1 started + 12 backend_completed
    // + 1 verification_passed + 1 completed.
    let started = audit
        .snapshot()
        .iter()
        .filter(|r| r.event_type == ErasureCloudEventType::Started)
        .count();
    let backend = audit
        .snapshot()
        .iter()
        .filter(|r| r.event_type == ErasureCloudEventType::BackendCompleted)
        .count();
    let passed = audit
        .snapshot()
        .iter()
        .filter(|r| r.event_type == ErasureCloudEventType::VerificationPassed)
        .count();
    let completed = audit
        .snapshot()
        .iter()
        .filter(|r| r.event_type == ErasureCloudEventType::Completed)
        .count();
    assert_eq!(started, 1);
    assert_eq!(backend, 12);
    assert_eq!(passed, 1);
    assert_eq!(completed, 1);
}

#[test]
fn partial_failure_24h_verification_landed_partial_arm() {
    let (job, audit, adapters) = fresh_job();
    let req = fresh_request(7);
    job.worker().process_erasure(&req, 1_000).unwrap();

    // Inject a canonical post-erasure stale row in a pseudonymized
    // backend (simulates Loki cold archive ingestion of a pre-erasure
    // event with raw subject_id; per WI AC-006 settle-delay path).
    let pseudo = adapters
        .iter()
        .find(|a| a.kind() == BackendKind::R2EvidencePseudo)
        .unwrap();
    pseudo.insert_rows(
        req.tenant_id,
        req.subject_id,
        vec![InMemoryRow::new(b"stale-evidence".to_vec())],
    );

    let now = req.verification_deadline_ms().saturating_add(1);
    let outcome = job.run_24h_sweep(&req, now).unwrap();
    let partial = matches!(outcome.decision, ErasureDecision::VerifiedPartial { .. });
    assert!(partial);

    // Audit fired `verification_failed.v1` (SEV-1 alert path).
    let failed = audit
        .snapshot()
        .iter()
        .filter(|r| r.event_type == ErasureCloudEventType::VerificationFailed)
        .count();
    assert_eq!(failed, 1);

    // Report still emitted (forensic trail) but `verified_complete=false`.
    let report = outcome.report.unwrap();
    assert!(!report.verified_complete);
}

#[test]
fn sla_breach_24h_no_tombstones_alerts() {
    let (job, audit, _) = fresh_job();
    let req = fresh_request(7);
    // Skip process_erasure entirely → no tombstones; sweep at
    // verification_deadline + 1ms.
    let now = req.verification_deadline_ms().saturating_add(1);
    let outcome = job.run_24h_sweep(&req, now).unwrap();
    let breached = matches!(
        outcome.decision,
        ErasureDecision::SlaBreached {
            unverified_count: 12,
            ..
        }
    );
    assert!(breached);

    // No report (no tombstones to render).
    assert!(outcome.report.is_none());

    // Audit fired `verification_failed.v1` (SEV-1 alert path).
    let failed = audit
        .snapshot()
        .iter()
        .filter(|r| r.event_type == ErasureCloudEventType::VerificationFailed)
        .count();
    assert_eq!(failed, 1);
}

#[test]
fn signed_report_rejects_tampering() {
    let (job, _audit, _) = fresh_job();
    let req = fresh_request(7);
    job.worker().process_erasure(&req, 1_000).unwrap();
    let now = req.verification_deadline_ms().saturating_add(1);
    let outcome = job.run_24h_sweep(&req, now).unwrap();
    let mut report = outcome.report.unwrap();
    let sig = outcome.signature.unwrap();
    // Tamper with the report: flip verified_complete.
    report.verified_complete = !report.verified_complete;
    let err = job.verify_report(&report, &sig).unwrap_err();
    let invalid = matches!(
        err,
        corelink_privacy_erasure_worker::ErasureWorkerError::Report(_)
    );
    assert!(invalid);
}

#[test]
fn verification_pre_24h_does_not_breach_sla() {
    let (job, _audit, _) = fresh_job();
    let req = fresh_request(7);
    job.worker().process_erasure(&req, 1_000).unwrap();
    // Now is BEFORE the canonical 24h deadline.
    let now = req.queued_at_ms.saturating_add(1_000);
    let outcome = job.run_24h_sweep(&req, now).unwrap();
    let complete = matches!(outcome.decision, ErasureDecision::VerifiedComplete { .. });
    assert!(complete);
}
