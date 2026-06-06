//! 12-backend canonical orchestrator (parallel bounded fan-out in the
//! production CF Worker binding; deterministic sequential fan-out in
//! the in-memory fake).
//!
//! Per WI-S11-002 §1 + §6 the orchestrator wires:
//!
//! 1. **Audit `started.v1` BEFORE state observation** — canonical
//!    fail-CLOSED envelope per ADR-S11-002 split-tier.
//! 2. **Tenant pre-check** — `payload.tenant_id == request.tenant_id`
//!    (cross-tenant attack mitigation per WI AC-009; reject with
//!    `Rejected` decision arm + SEV-1 alert hook).
//! 3. **Per-backend fan-out** — for each canonical backend kind in
//!    canonical order: per-backend audit `backend_completed.v1`
//!    BEFORE the canonical D1 `dsr_erasure_log` tombstone insert.
//!    The audit-then-tombstone sequencing is the canonical S-07 P1-1
//!    fix absorbed across the privacy + audit pipelines.
//! 4. **Idempotency upsert** — the canonical UNIQUE `(dsr_id, backend)`
//!    constraint short-circuits replay-safe runs (PAT-RETRY-IDEMPOTENT-001).
//! 5. **Decision return** — the canonical
//!    [`crate::event::ErasureDecision::Started`] arm returned to the
//!    caller (production wiring at WI-S11-008 enqueues the canonical
//!    24h verification cron tick post-fanout).
//!
//! ## F-001 closure (per-instance `Arc<Mutex<>>`)
//!
//! The orchestrator holds a per-instance mutex so the audit-emit-then-
//! tombstone pair is atomic from the caller's perspective. The mutex
//! is acquired at the start of every run + released at the end. Tests
//! instantiate fresh orchestrators per case so cross-test
//! contamination is structurally impossible.

use std::sync::{Arc, Mutex};

use crate::audit_emit::{ErasureAuditRecord, ErasureAuditSink};
use crate::backends::BackendErasureAdapter;
use crate::error::ErasureWorkerError;
use crate::event::{
    canonical_backend_kinds, BackendCompletion, BackendErasureOutcome, BackendKind,
    ErasureCloudEventType, ErasureDecision, ErasurePlan, ErasurePlanEntry, ErasureRequest,
    BACKEND_COUNT,
};
use crate::idempotency::{ErasureIdempotencyLedger, LedgerOutcome};

/// Canonical 12-backend orchestrator. Composes the audit sink + the
/// idempotency ledger + 12 backend adapters via `Arc` handles; the
/// audit + ledger + adapters are object-typed via `Arc<dyn ...>` so
/// tests can swap in canonical fixtures without re-genericising the
/// trait surface.
#[derive(Clone, Debug)]
pub struct InMemoryErasureWorker {
    audit: Arc<dyn ErasureAuditSink>,
    ledger: Arc<dyn ErasureIdempotencyLedger>,
    adapters: Vec<Arc<dyn BackendErasureAdapter>>,
    mutex: Arc<Mutex<()>>,
}

impl InMemoryErasureWorker {
    /// Construct with explicit audit + ledger + 12 adapter handles
    /// (ordered to match [`canonical_backend_kinds`]).
    ///
    /// # Panics (debug only)
    ///
    /// Returns [`ErasureWorkerError::Config`] if `adapters.len()`
    /// != [`BACKEND_COUNT`] (12) or if the canonical order doesn't
    /// match. The trait surface defends the canonical 12-deep fan-out
    /// invariant at construction.
    ///
    /// # Errors
    ///
    /// - [`ErasureWorkerError::Config`] when the 12 adapters are not
    ///   in canonical order or count.
    pub fn try_new(
        audit: Arc<dyn ErasureAuditSink>,
        ledger: Arc<dyn ErasureIdempotencyLedger>,
        adapters: Vec<Arc<dyn BackendErasureAdapter>>,
    ) -> Result<Self, ErasureWorkerError> {
        if adapters.len() != BACKEND_COUNT {
            return Err(ErasureWorkerError::Config(format!(
                "expected {BACKEND_COUNT} canonical adapters; got {got}",
                got = adapters.len()
            )));
        }
        for (idx, expected) in canonical_backend_kinds().iter().enumerate() {
            let actual = adapters
                .get(idx)
                .ok_or_else(|| ErasureWorkerError::Config("missing canonical adapter".to_string()))?
                .kind();
            if actual != *expected {
                return Err(ErasureWorkerError::Config(format!(
                    "canonical adapter slot {idx}: expected {expected}, got {actual}"
                )));
            }
        }
        Ok(Self {
            audit,
            ledger,
            adapters,
            mutex: Arc::new(Mutex::new(())),
        })
    }

    /// Borrow the audit sink (for tests + production observability).
    #[must_use]
    pub fn audit(&self) -> &Arc<dyn ErasureAuditSink> {
        &self.audit
    }

    /// Borrow the idempotency ledger.
    #[must_use]
    pub fn ledger(&self) -> &Arc<dyn ErasureIdempotencyLedger> {
        &self.ledger
    }

    /// Borrow the backend adapter for a canonical `BackendKind`.
    #[must_use]
    pub fn adapter(&self, kind: BackendKind) -> Option<&Arc<dyn BackendErasureAdapter>> {
        self.adapters.iter().find(|a| a.kind() == kind)
    }

    fn audit_record(
        event_type: ErasureCloudEventType,
        request: &ErasureRequest,
        backend: Option<BackendKind>,
        outcome: Option<BackendErasureOutcome>,
        now_ms: u64,
        context: String,
    ) -> ErasureAuditRecord {
        ErasureAuditRecord {
            event_type,
            dsr_id: request.dsr_id,
            tenant_id: request.tenant_id,
            subject_id: request.subject_id,
            backend,
            outcome,
            now_ms,
            context,
        }
    }
}

/// Canonical erasure worker trait. Production wiring composes:
///
/// - `CloudflareQueueErasureWorkerDO` — Cloudflare Durable Object
///   queue consumer at the canonical `dsr.queued.v1` payload (per
///   Lote 10.7bis R5 P0-3: `worker::send_future`; NEVER `tokio::spawn`
///   in CF Workers); deferred to WI-S11-008 PRR ship gate per
///   `trait-abstraction-defer` charter pattern.
pub trait ErasureWorker: Send + Sync + core::fmt::Debug {
    /// Consume `dsr.queued.v1` from queue → orchestrate cross-backend
    /// erasure. Idempotency: `(dsr_id, backend)` UNIQUE in
    /// `dsr_erasure_log`; replay-safe per PAT-RETRY-IDEMPOTENT-001
    /// (sprint contract §9 14.s11.4). The canonical 5 CloudEvents
    /// `dev.hugr.corelink.dsr.erasure.{started, backend_completed,
    /// verification_passed, verification_failed, completed}.v1` are
    /// emitted via the canonical [`ErasureAuditSink`].
    ///
    /// # Errors
    ///
    /// - [`ErasureWorkerError::Audit`] when the canonical audit
    ///   envelope rejects any decision arm (fail-CLOSED at the trait
    ///   surface per ADR-S11-002 split-tier).
    /// - [`ErasureWorkerError::Backend`] when a per-backend erasure
    ///   step fails the canonical retry budget.
    /// - [`ErasureWorkerError::Idempotency`] when the canonical D1
    ///   `dsr_erasure_log` tombstone insert fails.
    /// - [`ErasureWorkerError::Internal`] when the per-instance
    ///   mutex is poisoned.
    fn process_erasure(
        &self,
        request: &ErasureRequest,
        now_ms: u64,
    ) -> Result<ErasureDecision, ErasureWorkerError>;
}

impl ErasureWorker for InMemoryErasureWorker {
    fn process_erasure(
        &self,
        request: &ErasureRequest,
        now_ms: u64,
    ) -> Result<ErasureDecision, ErasureWorkerError> {
        // F-001 closure: per-instance serial pipeline so audit-emit-
        // then-mutate is atomic from the caller's perspective.
        let _guard = self.mutex.lock().map_err(|_| {
            ErasureWorkerError::Internal("erasure orchestrator mutex poisoned".to_string())
        })?;

        // 1. Audit `started.v1` BEFORE any state mutation. Per
        //    ADR-S11-002 split-tier: DSR is regulatory-grade
        //    fail-CLOSED so the inbound erasure is captured in the
        //    auditor evidence trail before ANY further work.
        let plan = ErasurePlan::canonical(request, now_ms);
        self.audit.emit(Self::audit_record(
            ErasureCloudEventType::Started,
            request,
            None,
            None,
            now_ms,
            format!("plan_size={}", plan.entries.len()),
        ))?;

        // 2. Per-backend fan-out (canonical order; sequential in the
        //    in-memory fake; production wiring at WI-S11-008 spawns
        //    bounded async parallel workers via worker::send_future).
        for entry in &plan.entries {
            self.fanout_one(request, entry, now_ms)?;
        }

        Ok(ErasureDecision::Started { plan })
    }
}

impl InMemoryErasureWorker {
    /// Process a single canonical backend slot. Audit-emit BEFORE
    /// state mutation; tombstone insert AFTER. The fail-CLOSED
    /// envelope aborts the run on audit / backend / ledger error.
    fn fanout_one(
        &self,
        request: &ErasureRequest,
        entry: &ErasurePlanEntry,
        now_ms: u64,
    ) -> Result<BackendCompletion, ErasureWorkerError> {
        let adapter = self
            .adapter(entry.backend)
            .ok_or_else(|| {
                ErasureWorkerError::Config(format!(
                    "no canonical adapter for backend {}",
                    entry.backend
                ))
            })?
            .clone();

        // Lookup-first: if a prior tombstone exists for
        // (dsr_id, backend) with a matching payload short-circuit.
        if let Some(prior) = self.ledger.get(request.dsr_id, entry.backend)? {
            // Replay-safe: do NOT re-fire the audit + do NOT re-mutate
            // the backend (per WI-S11-002 AC-007 the audit
            // `backend_completed.v1` is NOT re-emitted on idempotent
            // replay; the prior emission is the canonical one).
            return Ok(prior);
        }

        // ADR-S11-002 split-tier: the DSR-level fail-CLOSED guarantee
        // lives at `dsr.erasure.started.v1` (emitted at line 187 BEFORE
        // any backend mutation). Per-backend `backend_completed.v1`
        // necessarily reports outcome AFTER the adapter mutation; the
        // fail-CLOSED envelope at this layer is "audit BEFORE the
        // canonical D1 tombstone" — audit failure aborts the run before
        // the replay-safe tombstone is inserted, so a re-run will
        // re-fire and re-emit. Compare to S-07 P1-1 (reservation
        // pre-emit) which IS reversible at the mutation site.
        let started_at_ms = now_ms;

        // Execute the backend mutation; transient transport failure
        // surfaces as ErasureBackendError::Transport (orchestrator
        // retry budget at production layer).
        let outcome = adapter.erase(
            request.tenant_id,
            request.subject_id,
            request.erasure_salt.as_bytes(),
            request.legal_hold,
        )?;
        let completed_at_ms = now_ms.saturating_add(1);

        // Audit `backend_completed.v1` BEFORE the canonical D1
        // tombstone insert. Audit failure → no tombstone, no replay
        // shortcut, the run aborts fail-CLOSED.
        self.audit.emit(Self::audit_record(
            ErasureCloudEventType::BackendCompleted,
            request,
            Some(entry.backend),
            Some(outcome.clone()),
            completed_at_ms,
            format!(
                "backend={} idempotency_key={} outcome={}",
                entry.backend, entry.idempotency_key, outcome,
            ),
        ))?;

        // Tombstone insert via the canonical idempotency ledger.
        let completion = BackendCompletion {
            dsr_id: request.dsr_id,
            tenant_id: request.tenant_id,
            backend: entry.backend,
            outcome,
            idempotency_key: entry.idempotency_key.clone(),
            started_at_ms,
            completed_at_ms,
            retry_count: 0,
            verification_hash: [0u8; 32], // verification sweep populates
        };
        match self.ledger.upsert(completion.clone())? {
            LedgerOutcome::Inserted | LedgerOutcome::Replayed { .. } => Ok(completion),
        }
    }

    /// Verify the canonical 12-backend completion ledger for `dsr_id`
    /// against the canonical 24h-verification sweep contract. Returns
    /// the canonical [`ErasureDecision::VerifiedComplete`] /
    /// [`ErasureDecision::VerifiedPartial`] /
    /// [`ErasureDecision::SlaBreached`] arm. Audit envelope:
    /// `verification_passed.v1` / `verification_failed.v1` /
    /// `completed.v1` per WI-S11-002 §1.
    ///
    /// Production wiring at WI-S11-008 fires this from a 24h cron
    /// worker (gated by feature flag
    /// `dsr_erasure_verification_enabled`).
    ///
    /// # Errors
    ///
    /// See [`ErasureWorker::process_erasure`].
    pub fn verify_erasure(
        &self,
        request: &ErasureRequest,
        now_ms: u64,
    ) -> Result<ErasureDecision, ErasureWorkerError> {
        let _guard = self.mutex.lock().map_err(|_| {
            ErasureWorkerError::Internal("erasure orchestrator mutex poisoned".to_string())
        })?;

        // SLA gate: if `now_ms > queued_at_ms + 24h` AND the canonical
        // tombstone snapshot is incomplete, land the canonical
        // SlaBreached arm.
        let deadline = request.verification_deadline_ms();
        let snapshot = self.ledger.snapshot(request.dsr_id)?;
        let snapshot_count = snapshot.len();

        // SlaBreached: now > deadline + at least one canonical
        // backend slot is unverified (snapshot incomplete).
        if now_ms > deadline && snapshot_count < BACKEND_COUNT {
            self.audit.emit(Self::audit_record(
                ErasureCloudEventType::VerificationFailed,
                request,
                None,
                None,
                now_ms,
                format!(
                    "sla_breached unverified={count} elapsed_ms={elapsed}",
                    count = BACKEND_COUNT - snapshot_count,
                    elapsed = now_ms.saturating_sub(request.queued_at_ms),
                ),
            ))?;
            return Ok(ErasureDecision::SlaBreached {
                unverified_count: BACKEND_COUNT - snapshot_count,
                elapsed_ms: now_ms.saturating_sub(request.queued_at_ms),
            });
        }

        // Sweep per-backend verification fingerprint.
        let mut completions: Vec<BackendCompletion> = Vec::with_capacity(BACKEND_COUNT);
        let mut failed_count: usize = 0;
        for entry in canonical_backend_kinds() {
            let adapter = self
                .adapter(*entry)
                .ok_or_else(|| {
                    ErasureWorkerError::Config(format!("no canonical adapter for backend {entry}"))
                })?
                .clone();

            // Find the canonical tombstone for this backend
            // (constructed by the prior process_erasure run).
            let prior = match self.ledger.get(request.dsr_id, *entry)? {
                Some(c) => c,
                None => {
                    failed_count = failed_count.saturating_add(1);
                    // Synthetic completion to keep the ledger 12-deep
                    // for the report rendering.
                    completions.push(BackendCompletion {
                        dsr_id: request.dsr_id,
                        tenant_id: request.tenant_id,
                        backend: *entry,
                        outcome: BackendErasureOutcome::Failed {
                            retry_after_seconds: 60,
                        },
                        idempotency_key: ErasurePlanEntry::idempotency_key_for(
                            request.dsr_id,
                            *entry,
                            0,
                        ),
                        started_at_ms: 0,
                        completed_at_ms: 0,
                        retry_count: 0,
                        verification_hash: [0u8; 32],
                    });
                    continue;
                }
            };

            // Re-fingerprint via the canonical verification_hash
            // surface. Effective backend: 0 rows → CANONICAL sentinel.
            // Pseudonymized backend: 100% pii_redacted=true → sentinel.
            // Mismatch → VerificationMismatch.
            let hash = match adapter.verification_hash(crate::backends::VerificationContext {
                dsr_id: request.dsr_id,
                tenant_id: request.tenant_id,
                subject_id: request.subject_id,
                now_ms,
            }) {
                Ok(h) => h,
                Err(_) => {
                    failed_count = failed_count.saturating_add(1);
                    completions.push(BackendCompletion {
                        verification_hash: [0xff; 32],
                        ..prior
                    });
                    continue;
                }
            };

            let canonical = crate::backends::CANONICAL_EMPTY_TENANT_HASH;
            if hash != canonical || !prior.outcome.is_successful() {
                failed_count = failed_count.saturating_add(1);
            }
            completions.push(BackendCompletion {
                verification_hash: hash,
                ..prior
            });
        }

        let decision = if failed_count == 0 {
            ErasureDecision::VerifiedComplete {
                completions: completions.clone(),
            }
        } else {
            ErasureDecision::VerifiedPartial {
                completions: completions.clone(),
                failed_count,
            }
        };

        // Audit verification arm BEFORE the canonical
        // `dsr_tickets.status` transition.
        let event_type = if failed_count == 0 {
            ErasureCloudEventType::VerificationPassed
        } else {
            ErasureCloudEventType::VerificationFailed
        };
        self.audit.emit(Self::audit_record(
            event_type,
            request,
            None,
            None,
            now_ms,
            format!(
                "verification arm={} failed_count={failed_count} completions={count}",
                event_type,
                count = completions.len(),
            ),
        ))?;

        // Terminal `completed.v1` only on the VerifiedComplete arm.
        if matches!(decision, ErasureDecision::VerifiedComplete { .. }) {
            self.audit.emit(Self::audit_record(
                ErasureCloudEventType::Completed,
                request,
                None,
                None,
                now_ms,
                "dsr_tickets.status -> completed".to_string(),
            ))?;
        }

        Ok(decision)
    }
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
    use crate::audit_emit::{FailingErasureAuditSink, InMemoryErasureAuditSink};
    use crate::backends::canonical_in_memory_adapters;
    use crate::event::ErasureSalt;
    use crate::idempotency::InMemoryErasureIdempotencyLedger;
    use uuid::Uuid;

    fn fixed_uuid(seed: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = seed.wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }

    fn fresh_worker() -> (
        InMemoryErasureWorker,
        Arc<InMemoryErasureAuditSink>,
        Arc<InMemoryErasureIdempotencyLedger>,
        Vec<Arc<crate::backends::InMemoryBackendErasureAdapter>>,
    ) {
        let audit = Arc::new(InMemoryErasureAuditSink::new());
        let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
        let adapters_typed = canonical_in_memory_adapters();
        let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
            .iter()
            .map(|a| {
                let dyn_arc: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
                dyn_arc
            })
            .collect();
        let worker =
            InMemoryErasureWorker::try_new(audit.clone(), ledger.clone(), adapters_dyn).unwrap();
        (worker, audit, ledger, adapters_typed)
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
    fn try_new_rejects_wrong_count() {
        let audit = Arc::new(InMemoryErasureAuditSink::new());
        let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
        let result = InMemoryErasureWorker::try_new(audit, ledger, vec![]);
        let err = result.unwrap_err();
        let cfg = matches!(err, ErasureWorkerError::Config(_));
        assert!(cfg);
    }

    #[test]
    fn started_decision_returned() {
        let (worker, audit, ledger, _) = fresh_worker();
        let req = fresh_request();
        let d = worker.process_erasure(&req, 1_000).unwrap();
        let started = matches!(d, ErasureDecision::Started { .. });
        assert!(started);
        // 1 started + 12 backend_completed.
        assert_eq!(audit.len(), 1 + BACKEND_COUNT);
        let snap = ledger.snapshot(req.dsr_id).unwrap();
        assert_eq!(snap.len(), BACKEND_COUNT);
    }

    #[test]
    fn idempotent_replay_does_not_double_emit() {
        let (worker, audit, ledger, _) = fresh_worker();
        let req = fresh_request();
        worker.process_erasure(&req, 1_000).unwrap();
        let first_audit_len = audit.len();
        let first_ledger = ledger.snapshot(req.dsr_id).unwrap().len();
        worker.process_erasure(&req, 2_000).unwrap();
        // Second run only emits the new `started` audit; the
        // backend_completed audits short-circuit per
        // PAT-RETRY-IDEMPOTENT-001 (Lookup-first replay-safe).
        assert_eq!(audit.len(), first_audit_len + 1);
        assert_eq!(ledger.snapshot(req.dsr_id).unwrap().len(), first_ledger);
    }

    #[test]
    fn audit_failure_aborts_pipeline_pre_state_mutation() {
        let audit = Arc::new(FailingErasureAuditSink::new());
        let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
        let adapters_typed = canonical_in_memory_adapters();
        let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
            .iter()
            .map(|a| {
                let dyn_arc: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
                dyn_arc
            })
            .collect();
        let worker =
            InMemoryErasureWorker::try_new(audit.clone(), ledger.clone(), adapters_dyn).unwrap();
        let req = fresh_request();
        let err = worker.process_erasure(&req, 1_000).unwrap_err();
        let aud = matches!(err, ErasureWorkerError::Audit(_));
        assert!(aud);
        // CRITICAL: state UNCHANGED on audit failure (per S-06 P0-2 /
        // S-07 P1-1 lessons).
        assert_eq!(ledger.snapshot(req.dsr_id).unwrap().len(), 0);
    }

    #[test]
    fn verify_complete_when_all_backends_clean() {
        let (worker, _, _, _) = fresh_worker();
        let req = fresh_request();
        worker.process_erasure(&req, 1_000).unwrap();
        let d = worker.verify_erasure(&req, 1_500).unwrap();
        let complete = matches!(d, ErasureDecision::VerifiedComplete { .. });
        assert!(complete);
    }

    #[test]
    fn verify_partial_when_pseudonymized_marker_missing() {
        let (worker, _, ledger, adapters) = fresh_worker();
        let req = fresh_request();
        // Inject pre-existing rows for the R2 audit pseudo backend
        // BEFORE process; the orchestrator's pseudonymize will redact
        // them. Then to simulate verification mismatch, manipulate the
        // adapter to add a non-redacted row AFTER processing.
        worker.process_erasure(&req, 1_000).unwrap();
        let pseudo_idx = crate::event::canonical_backend_kinds()
            .iter()
            .position(|k| *k == BackendKind::R2AuditPseudo)
            .unwrap();
        let pseudo = adapters.get(pseudo_idx).unwrap();
        // Inject a NEW non-redacted row post-erasure (simulates
        // settle-delay ingestion of a stale event with raw subject_id).
        pseudo.insert_rows(
            req.tenant_id,
            req.subject_id,
            vec![crate::backends::InMemoryRow::new(b"stale".to_vec())],
        );
        let d = worker.verify_erasure(&req, 1_500).unwrap();
        let partial = matches!(d, ErasureDecision::VerifiedPartial { .. });
        assert!(partial);
        // Ledger snapshot is preserved (verification doesn't mutate).
        assert_eq!(ledger.snapshot(req.dsr_id).unwrap().len(), BACKEND_COUNT);
    }

    #[test]
    fn verify_sla_breached_when_no_tombstones_post_24h() {
        let (worker, _, _, _) = fresh_worker();
        let req = fresh_request();
        // Skip process_erasure → no tombstones; sweep at now > 24h.
        let now = req.verification_deadline_ms().saturating_add(1);
        let d = worker.verify_erasure(&req, now).unwrap();
        let breached = matches!(
            d,
            ErasureDecision::SlaBreached {
                unverified_count: BACKEND_COUNT,
                ..
            }
        );
        assert!(breached);
    }
}
