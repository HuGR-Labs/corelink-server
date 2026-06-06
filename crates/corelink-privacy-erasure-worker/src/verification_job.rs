//! 24h verification cron sweep entry point.
//!
//! Per WI-S11-002 §6.1 step 9 + AC-005/006: the canonical 24h
//! verification cron worker fires `verify_erasure` for every DSR
//! ticket whose `queued_at_ms + 24h <= now` AND `dsr_tickets.status =
//! 'in_progress'`. Mismatch → SEV-1 alert + RB-DSR-ERASURE-INCOMPLETE
//! runbook activation; success → terminal `completed.v1` event +
//! signed report uploaded to R2 evidence-dsr.
//!
//! The trait surface here ships the canonical
//! [`VerificationJob::run_24h_sweep`] helper that wraps
//! [`crate::orchestrator::InMemoryErasureWorker::verify_erasure`] +
//! the canonical [`crate::report::ReportSigner`] surface to produce a
//! BLAKE3-keyed MAC signed [`crate::report::ErasureReport`]. Production
//! wiring at WI-S11-008 binds the cron worker to this surface +
//! uploads the signed report to R2 evidence-dsr-`<region>` per
//! WI-S11-002 §13.

use uuid::Uuid;

use crate::error::ErasureWorkerError;
use crate::event::{
    canonical_cloudevent_types, BackendCompletion, ErasureDecision, ErasurePlan, ErasureRequest,
};
use crate::orchestrator::InMemoryErasureWorker;
use crate::report::{
    canonical_report_key, ErasureReport, InMemoryReportSigner, ReportSignature, ReportSigner,
};

/// Canonical Prometheus metric base name for the DSR erasure freshness
/// SLI per `slo_catalog.md §4.12` (SLO-FRESH-DSR-ERASURE). The cron
/// worker observes this histogram once per completed DSR ticket
/// (`request="erasure"` label). Bound from
/// `corelink-slo::Sli::FreshDsrErasure.prometheus_metric_base()` —
/// alignment regression-pinned in `tests/sli_binding.rs`.
///
/// Production wiring at WI-S11-008 emits a histogram `_bucket`
/// observation with `le ≤ 720` (720h = 30d SLA window per LGPD Art. 19
/// + GDPR Art. 12.3 + CCPA §1798.130 canonical bounds).
pub const METRIC_DSR_RESOLUTION_HOURS: &str = "corelink_dsr_resolution_hours";

/// Canonical SLI SLA window in hours (30 days canonical). Aligned with
/// the `slo_catalog.md §4.12` bucket bound `le=720`.
pub const SLA_WINDOW_HOURS: u64 = 720;

/// Compute the canonical SLI observation in hours for a DSR ticket
/// resolved at `verified_at_ms` relative to its `queued_at_ms`. The
/// production cron worker invokes this once per
/// [`ErasureDecision::VerifiedComplete`] / `VerifiedPartial` outcome
/// and emits the value as a histogram observation under
/// [`METRIC_DSR_RESOLUTION_HOURS`].
///
/// Returns `None` when `verified_at_ms < queued_at_ms` (clock skew
/// guard — never emit negative SLI observations).
#[must_use]
pub const fn dsr_resolution_hours(queued_at_ms: u64, verified_at_ms: u64) -> Option<u64> {
    if verified_at_ms < queued_at_ms {
        return None;
    }
    let elapsed_ms = verified_at_ms - queued_at_ms;
    Some(elapsed_ms / (60 * 60 * 1000))
}

/// Whether the canonical observation is within the SLA window
/// (le ≤ 720h numerator per `slo_catalog.md §4.12`).
#[must_use]
pub const fn within_sla_window(hours: u64) -> bool {
    hours <= SLA_WINDOW_HOURS
}

/// Canonical 24h verification sweep result. Bundles the canonical
/// [`ErasureDecision`] + the canonical
/// [`crate::report::ErasureReport`] + the BLAKE3-keyed MAC signature
/// for a complete forensic evidence trail.
///
/// # Wave-19 — `Serialize` / `Deserialize`
///
/// Serde derives land at wave-19 so the canonical
/// `dsr_erasure_log.outcome_json` snapshot column (migration `0049`)
/// can round-trip the bundle. Every field is already
/// `Serialize + Deserialize` (`ErasureDecision`, `ErasureReport`,
/// `ReportSignature`, `String`) so the derive is purely additive.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VerificationOutcome {
    /// Canonical 24h verification decision.
    pub decision: ErasureDecision,
    /// Canonical signed report (only emitted on the
    /// [`ErasureDecision::VerifiedComplete`] /
    /// [`ErasureDecision::VerifiedPartial`] arms; `None` on the
    /// [`ErasureDecision::SlaBreached`] arm because no per-backend
    /// tombstones exist to render).
    pub report: Option<ErasureReport>,
    /// Canonical BLAKE3-keyed MAC signature over the report (paired
    /// with `report`).
    pub signature: Option<ReportSignature>,
    /// Canonical R2 object-key the report would be uploaded to
    /// (production wiring at WI-S11-008 invokes the canonical
    /// signed URL 24h TTL surface).
    pub object_key: Option<String>,
}

impl VerificationOutcome {
    /// Canonical SLO-FRESH-DSR-ERASURE observation per
    /// `slo_catalog.md §4.12`. Returns the elapsed hours from
    /// `plan.generated_at_ms` (set to `request.queued_at_ms` at plan
    /// emission per [`ErasurePlan::canonical`]) to the verification
    /// timestamp baked into the signed report — the value the
    /// production cron worker emits to the
    /// [`METRIC_DSR_RESOLUTION_HOURS`] histogram. `None` on the
    /// `SlaBreached` / `Started` / `Rejected` / `VerificationFailed`
    /// arms (no completed report to anchor the observation).
    #[must_use]
    pub fn sli_resolution_hours(&self) -> Option<u64> {
        let report = self.report.as_ref()?;
        dsr_resolution_hours(report.plan.generated_at_ms, report.verified_at_ms)
    }

    /// Whether the canonical observation is within the SLA window
    /// (LGPD Art. 19 + GDPR Art. 12.3 + CCPA §1798.130 → 30 days).
    /// `None` when no observation is available.
    #[must_use]
    pub fn sli_within_sla(&self) -> Option<bool> {
        self.sli_resolution_hours().map(within_sla_window)
    }
}

/// Canonical 24h verification cron worker entry point. Wraps the
/// orchestrator's `verify_erasure` + the canonical report signer.
#[derive(Clone, Debug)]
pub struct VerificationJob {
    worker: InMemoryErasureWorker,
    signer: InMemoryReportSigner,
}

impl VerificationJob {
    /// Construct with explicit orchestrator + signer handles.
    #[must_use]
    pub const fn new(worker: InMemoryErasureWorker, signer: InMemoryReportSigner) -> Self {
        Self { worker, signer }
    }

    /// Borrow the underlying orchestrator.
    #[must_use]
    pub const fn worker(&self) -> &InMemoryErasureWorker {
        &self.worker
    }

    /// Run the canonical 24h sweep for `(request, now_ms)` and return
    /// the canonical [`VerificationOutcome`].
    ///
    /// # Errors
    ///
    /// - [`ErasureWorkerError::Audit`] when the canonical audit
    ///   envelope rejects any decision arm.
    /// - [`ErasureWorkerError::Backend`] when a per-backend
    ///   verification fingerprint fetch fails.
    /// - [`ErasureWorkerError::Idempotency`] when the canonical D1
    ///   `dsr_erasure_log` snapshot fails.
    /// - [`ErasureWorkerError::Report`] when JCS canonicalization
    ///   rejects the payload OR the BLAKE3-keyed MAC signing fails.
    /// - [`ErasureWorkerError::Internal`] when the per-instance
    ///   mutex is poisoned.
    pub fn run_24h_sweep(
        &self,
        request: &ErasureRequest,
        now_ms: u64,
    ) -> Result<VerificationOutcome, ErasureWorkerError> {
        let decision = self.worker.verify_erasure(request, now_ms)?;

        let (report, signature, object_key) = match &decision {
            ErasureDecision::VerifiedComplete { completions }
            | ErasureDecision::VerifiedPartial { completions, .. } => {
                let plan = ErasurePlan::canonical(request, request.queued_at_ms);
                let report = build_report(
                    request,
                    plan,
                    completions.clone(),
                    now_ms,
                    matches!(decision, ErasureDecision::VerifiedComplete { .. }),
                );
                let sig = self.signer.sign(&report)?;
                let key = canonical_report_key(request.tenant_id, request.dsr_id);
                (Some(report), Some(sig), Some(key))
            }
            ErasureDecision::SlaBreached { .. }
            | ErasureDecision::VerificationFailed { .. }
            | ErasureDecision::Started { .. }
            | ErasureDecision::Rejected { .. } => (None, None, None),
        };

        let outcome = VerificationOutcome {
            decision,
            report,
            signature,
            object_key,
        };

        // Wave-19: persist the canonical `outcome_json` snapshot
        // against the canonical D1 `dsr_erasure_log.outcome_json`
        // column (migration `0049`) — same JSON the cron
        // `D1Wasm32RowSource::fetch_window_async` rehydrates on the
        // 06:00 UTC sweep.
        //
        // Serialization is best-effort: a serde_json failure here
        // does NOT abort the verification (the typed decision +
        // report are still emitted to the audit envelope) — the
        // snapshot is a denormalized read-side optimisation, NOT a
        // forensic source-of-truth. Fail-CLOSED applies on the
        // reader side (parse-error → publish abort).
        if let Ok(json) = serde_json::to_string(&outcome) {
            // Silent on ledger failure: the verification decision is
            // already final; a follow-up cron tick will re-attempt
            // the snapshot persistence on the next 24h sweep.
            let _ = self
                .worker
                .ledger()
                .set_outcome_snapshot(request.dsr_id, &json);
        }

        Ok(outcome)
    }

    /// Verify a previously-signed report against the canonical signer
    /// key. Production wiring uses this for forensic re-attestation
    /// (e.g. an auditor produces the canonical signed report + this
    /// surface re-verifies the BLAKE3-keyed MAC).
    ///
    /// # Errors
    ///
    /// See [`crate::report::ReportSigner::verify`].
    pub fn verify_report(
        &self,
        report: &ErasureReport,
        signature: &ReportSignature,
    ) -> Result<(), ErasureWorkerError> {
        self.signer.verify(report, signature)?;
        Ok(())
    }
}

/// Canonical report builder. Pinned for cross-component regression
/// tests + dashboard widget configuration.
#[must_use]
fn build_report(
    request: &ErasureRequest,
    plan: ErasurePlan,
    completions: Vec<BackendCompletion>,
    verified_at_ms: u64,
    verified_complete: bool,
) -> ErasureReport {
    ErasureReport {
        dsr_id: request.dsr_id,
        tenant_id: request.tenant_id,
        plan,
        completions,
        verified_at_ms,
        verified_complete,
        cloudevent_types: canonical_cloudevent_types()
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    }
}

/// Canonical 24h cron sweep: returns the list of canonical
/// `(dsr_id, request)` pairs whose verification deadline has elapsed
/// and the canonical sweep should run. Pure-logic helper exposed for
/// the production wiring at WI-S11-008 to enumerate the cron tick.
#[must_use]
pub fn elapsed_dsr_ids<'a>(pending: &'a [(Uuid, &'a ErasureRequest)], now_ms: u64) -> Vec<Uuid> {
    pending
        .iter()
        .filter(|(_, r)| now_ms >= r.verification_deadline_ms())
        .map(|(d, _)| *d)
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::audit_emit::InMemoryErasureAuditSink;
    use crate::backends::canonical_in_memory_adapters;
    use crate::event::ErasureSalt;
    use crate::idempotency::InMemoryErasureIdempotencyLedger;
    use crate::orchestrator::ErasureWorker;
    use crate::report::ReportSignerKey;
    use std::sync::Arc;

    fn fixed_uuid(seed: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = seed.wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }

    fn fresh_job() -> VerificationJob {
        let audit = Arc::new(InMemoryErasureAuditSink::new());
        let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
        let adapters_typed = canonical_in_memory_adapters();
        let adapters_dyn: Vec<Arc<dyn crate::backends::BackendErasureAdapter>> = adapters_typed
            .iter()
            .map(|a| {
                let dyn_arc: Arc<dyn crate::backends::BackendErasureAdapter> = Arc::clone(a) as _;
                dyn_arc
            })
            .collect();
        let worker = InMemoryErasureWorker::try_new(audit, ledger, adapters_dyn).unwrap();
        let signer = InMemoryReportSigner::new(ReportSignerKey::synthetic_for_test(7));
        VerificationJob::new(worker, signer)
    }

    fn fresh_request() -> ErasureRequest {
        ErasureRequest::new(
            fixed_uuid(11),
            fixed_uuid(12),
            fixed_uuid(13),
            ErasureSalt::synthetic_for_test(7),
            1_000,
        )
    }

    #[test]
    fn full_lifecycle_complete() {
        let job = fresh_job();
        let req = fresh_request();
        // Step 1: orchestrator process.
        job.worker().process_erasure(&req, 1_000).unwrap();
        // Step 2: verification sweep at deadline + 1ms.
        let outcome = job
            .run_24h_sweep(&req, req.verification_deadline_ms().saturating_add(1))
            .unwrap();
        let complete = matches!(outcome.decision, ErasureDecision::VerifiedComplete { .. });
        assert!(complete);
        assert!(outcome.report.is_some());
        assert!(outcome.signature.is_some());
        let key = outcome.object_key.unwrap();
        assert!(key.starts_with("dsr-reports/"));
    }

    #[test]
    fn signed_report_verifies_post_facto() {
        let job = fresh_job();
        let req = fresh_request();
        job.worker().process_erasure(&req, 1_000).unwrap();
        let outcome = job
            .run_24h_sweep(&req, req.verification_deadline_ms().saturating_add(1))
            .unwrap();
        let report = outcome.report.unwrap();
        let sig = outcome.signature.unwrap();
        job.verify_report(&report, &sig).unwrap();
    }

    #[test]
    fn sla_breach_no_report() {
        let job = fresh_job();
        let req = fresh_request();
        // Skip process; sweep post-deadline.
        let outcome = job
            .run_24h_sweep(&req, req.verification_deadline_ms().saturating_add(1))
            .unwrap();
        let breached = matches!(outcome.decision, ErasureDecision::SlaBreached { .. });
        assert!(breached);
        assert!(outcome.report.is_none());
        assert!(outcome.signature.is_none());
        assert!(outcome.object_key.is_none());
    }

    #[test]
    fn sli_resolution_hours_within_sla_on_happy_path() {
        // SLO-FRESH-DSR-ERASURE per `slo_catalog.md §4.12`: a happy-path
        // 24h-deadline verification falls well within the 720h (30d)
        // SLA window. The cron worker emits this observation to the
        // canonical `corelink_dsr_resolution_hours` histogram.
        let job = fresh_job();
        let req = fresh_request();
        job.worker().process_erasure(&req, 1_000).unwrap();
        // Verification 25 hours after queue → resolution_hours ≈ 25.
        let verified_at = req.queued_at_ms.saturating_add(25 * 60 * 60 * 1000);
        let outcome = job.run_24h_sweep(&req, verified_at).unwrap();
        assert!(matches!(
            outcome.decision,
            ErasureDecision::VerifiedComplete { .. }
        ));
        let hours = outcome.sli_resolution_hours().unwrap();
        assert_eq!(hours, 25);
        assert_eq!(outcome.sli_within_sla(), Some(true));
        assert!(within_sla_window(hours));
    }

    #[test]
    fn sli_resolution_hours_none_on_sla_breach() {
        // SlaBreached → no signed report → no SLI observation.
        let job = fresh_job();
        let req = fresh_request();
        // Skip process_erasure; sweep post-deadline.
        let outcome = job
            .run_24h_sweep(&req, req.verification_deadline_ms().saturating_add(1))
            .unwrap();
        assert!(matches!(
            outcome.decision,
            ErasureDecision::SlaBreached { .. }
        ));
        assert!(outcome.sli_resolution_hours().is_none());
        assert!(outcome.sli_within_sla().is_none());
    }

    #[test]
    fn sli_helper_handles_clock_skew() {
        // Guards against negative observations from upstream clock skew.
        assert_eq!(dsr_resolution_hours(2_000, 1_000), None);
        assert_eq!(dsr_resolution_hours(0, 3_600_000), Some(1));
        assert_eq!(dsr_resolution_hours(0, 720 * 3_600_000), Some(720));
        // Boundary: exactly 720h is within SLA (le ≤ 720 canonical).
        assert!(within_sla_window(720));
        assert!(!within_sla_window(721));
    }

    #[test]
    fn metric_base_pinned() {
        // Pinned to catch accidental rename. Cross-crate SLI binding
        // test lives in `tests/sli_binding.rs`.
        assert_eq!(METRIC_DSR_RESOLUTION_HOURS, "corelink_dsr_resolution_hours");
        assert_eq!(SLA_WINDOW_HOURS, 720);
    }

    #[test]
    fn elapsed_helper_filters_pre_deadline() {
        let req_a = fresh_request();
        let req_b = ErasureRequest::new(
            fixed_uuid(21),
            fixed_uuid(22),
            fixed_uuid(23),
            ErasureSalt::synthetic_for_test(8),
            1_000_000,
        );
        let pending = vec![(req_a.dsr_id, &req_a), (req_b.dsr_id, &req_b)];
        let now = req_a.verification_deadline_ms().saturating_add(1);
        let elapsed = elapsed_dsr_ids(&pending, now);
        assert!(elapsed.contains(&req_a.dsr_id));
        // req_b deadline is queued_at_ms 1_000_000 + 24h ≫ now.
        assert!(!elapsed.contains(&req_b.dsr_id));
    }
}
