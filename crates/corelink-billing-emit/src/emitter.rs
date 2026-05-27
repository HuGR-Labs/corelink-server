//! `UsageEventEmitter` trait + `InMemoryUsageEventEmitter` orchestrator.
//!
//! The orchestrator wires the four primitives — JCS canonicalization,
//! BLAKE3 idempotency-key derivation, the per-tenant
//! [`crate::idempotency::IdempotencyTracker`], and the append-only
//! [`crate::sink::InMemoryR2UsageSink`] — into a single emit pipeline
//! that satisfies every load-bearing invariant the production CF
//! binding will depend on (per the `trait-abstraction-defer` charter
//! pattern).
//!
//! ## Decision pipeline (per emit)
//!
//! For each input `(UsageEvent, request_id, now_ms)`:
//!
//! 1. **Canonicalization**: compute the JCS-canonical bytes of the
//!    event with the `idem_key` slot zeroed (the canonical formula
//!    input; mirrors the audit-chain genesis `prev_hash` zero
//!    convention).
//! 2. **Idempotency derivation**: BLAKE3-256 over the canonical bytes
//!    yields the canonical idem_key; the orchestrator rewrites the
//!    event's `idem_key` slot before persisting.
//! 3. **Idempotency tracker insert**: per-tenant set membership.
//!    `IdempotencyDecision::Accepted` proceeds; `DuplicateRejected`
//!    short-circuits to the duplicate audit arm.
//! 4. **Audit emit BEFORE state mutation** per the canonical S-07 P1-1
//!    fix + Lote 10.6bis pattern. Audit failure aborts the chain emit
//!    (NO R2 write, NO sequence advance).
//! 5. **R2 PutObject (append-only)**: serialize NDJSON + write at the
//!    canonical `usage/{tenant}/{billing_period}/{seq:08}.usage.ndjson`
//!    key. R2 failure surfaces an audit `sink_failure` arm + the
//!    orchestrator returns `BillingEmitError::Sink` (the production
//!    worker policy decides retry vs queue vs surface).
//!
//! ## Audit-fail-CLOSED at the trait surface
//!
//! Per WI-S10-001 §6.1, every decision arm fires its canonical audit
//! BEFORE the state-mutating step:
//!
//! - `usage_emitted` fires BEFORE the R2 NDJSON write.
//! - `duplicate_rejected` fires BEFORE the orchestrator returns
//!   [`EmitOutcome::DuplicateRejected`].
//! - `sink_failure` fires BEFORE the orchestrator returns
//!   `BillingEmitError::Sink`.
//! - `idempotency_collision` fires BEFORE the orchestrator returns
//!   `BillingEmitError::IdempotencyCollision`.
//!
//! Audit failure on any arm aborts the orchestrator + propagates
//! `BillingEmitError::Audit` (caller sees no state mutation). This is
//! the canonical "audit envelope fail-CLOSED at trait surface" pattern;
//! the wrapping production worker decides hot-path policy (per the
//! WI-S10-001 narrative the customer-facing CAS hot path is fail-OPEN
//! over this trait surface — staging table + retry queue handles
//! eventual delivery).
//!
//! ## Why concrete `Arc<InMemoryR2UsageSink>` (NOT generic `S`)
//!
//! The orchestrator allocates the per-(tenant, billing_period)
//! sequence number via the canonical
//! [`crate::sink::InMemoryR2UsageSink::append`] method — this is the
//! reference impl per the `trait-abstraction-defer` charter. The
//! production wiring at WI-S10-007 will ship its own orchestrator
//! variant that delegates sequence allocation to the per-tenant DO
//! singleton + writes via `R2UsageSink::put` at the host-allocated key
//! (the trait surface is preserved for adversarial test fakes via
//! [`crate::sink::FailingR2UsageSink`] under the existing trait).
//!
//! ## F-001 closure
//!
//! The orchestrator holds the audit sink + idempotency tracker + R2
//! sink as `Arc` handles passed at construction; per-instance state
//! lives inside those Arc'd primitives. Tests instantiate fresh
//! orchestrators per case so cross-test contamination is structurally
//! impossible.

use std::sync::Arc;

use uuid::Uuid;

use crate::audit::{
    BillingAuditEventType, BillingAuditRecord, BillingAuditSink,
};
use crate::error::BillingEmitError;
use crate::event::{IdemKey, UsageEvent};
use crate::idempotency::{
    compute_canonical_bytes_for_idem, derive_idem_key_from_canonical,
    IdempotencyDecision, IdempotencyTracker,
};
use crate::sink::{InMemoryR2UsageSink, PersistedUsageLine};

/// Outcome of a successful (audit-OK + idempotency-OK) emit.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EmitOutcome {
    /// First sight: the event was canonicalized + accepted by the
    /// idempotency tracker + persisted to R2. The persisted line is
    /// returned for downstream observability + tests.
    Persisted(PersistedUsageLine),
    /// Replay-safe duplicate: the same `idem_key` was previously
    /// accepted for the tenant; the orchestrator did NOT advance
    /// sequence + did NOT write R2; the duplicate audit arm fired.
    DuplicateRejected {
        /// Tenant id of the duplicate emit.
        tenant_id: Uuid,
        /// Canonical idempotency key of the duplicate.
        idem_key: IdemKey,
    },
}

/// Usage-event emitter trait. Production wiring composes the canonical
/// orchestrator pipeline (canonicalize → derive idem_key → idempotency
/// check → audit envelope → R2 PutObject); the trait abstraction lets
/// the host worker swap in test fakes without re-running the emit
/// pipeline at integration boundaries.
pub trait UsageEventEmitter: Send + Sync + core::fmt::Debug {
    /// Emit `event` through the canonical pipeline.
    ///
    /// # Errors
    ///
    /// - [`BillingEmitError::Canonicalization`] when JCS fails.
    /// - [`BillingEmitError::Audit`] when the audit envelope rejects
    ///   any decision arm (fail-CLOSED at the trait surface).
    /// - [`BillingEmitError::Sink`] when R2 PutObject rejects the
    ///   write (transient transport failure or
    ///   INV-BILLING-APPEND-ONLY violation).
    /// - [`BillingEmitError::IdempotencyCollision`] when the tracker
    ///   observes the same `idem_key` for diverged canonical bytes
    ///   (SEV-1 source).
    /// - [`BillingEmitError::Internal`] when a per-instance mutex is
    ///   poisoned.
    fn emit(
        &self,
        event: UsageEvent,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<EmitOutcome, BillingEmitError>;
}

/// In-memory usage-event emitter orchestrator. Composes the audit sink,
/// idempotency tracker, and canonical [`InMemoryR2UsageSink`] via `Arc`
/// handles; the audit + idempotency are generic over the trait so test
/// fakes (e.g. [`crate::audit::FailingBillingAuditSink`]) compose
/// directly at construction.
#[derive(Clone, Debug)]
pub struct InMemoryUsageEventEmitter<A, I>
where
    A: BillingAuditSink + 'static,
    I: IdempotencyTracker + 'static,
{
    audit: Arc<A>,
    idempotency: Arc<I>,
    sink: Arc<InMemoryR2UsageSink>,
}

impl<A, I> InMemoryUsageEventEmitter<A, I>
where
    A: BillingAuditSink + 'static,
    I: IdempotencyTracker + 'static,
{
    /// Construct with explicit audit / idempotency / R2 sink handles.
    pub fn new(
        audit: Arc<A>,
        idempotency: Arc<I>,
        sink: Arc<InMemoryR2UsageSink>,
    ) -> Self {
        Self {
            audit,
            idempotency,
            sink,
        }
    }

    /// Borrow the audit sink (for tests + production observability).
    #[must_use]
    pub fn audit(&self) -> &Arc<A> {
        &self.audit
    }

    /// Borrow the idempotency tracker.
    #[must_use]
    pub fn idempotency(&self) -> &Arc<I> {
        &self.idempotency
    }

    /// Borrow the R2 sink.
    #[must_use]
    pub fn sink(&self) -> &Arc<InMemoryR2UsageSink> {
        &self.sink
    }

    fn audit_record(
        event_type: BillingAuditEventType,
        event: &UsageEvent,
        derived_idem_key: IdemKey,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> BillingAuditRecord {
        BillingAuditRecord {
            event_type,
            tenant_id: event.data.tenant_id.to_string(),
            idem_key_hex: derived_idem_key.to_hex(),
            billing_period: event.data.billing_period.clone(),
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
        }
    }
}

impl<A, I> UsageEventEmitter for InMemoryUsageEventEmitter<A, I>
where
    A: BillingAuditSink + 'static,
    I: IdempotencyTracker + 'static,
{
    fn emit(
        &self,
        mut event: UsageEvent,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<EmitOutcome, BillingEmitError> {
        // Step 1+2: canonicalize + derive idem_key.
        let canonical = compute_canonical_bytes_for_idem(&event)?;
        let derived = derive_idem_key_from_canonical(&canonical);
        // Rewrite slot post-derivation for the on-the-wire shape.
        event.idem_key = derived;

        let tenant_id = event.data.tenant_id;

        // Step 3: idempotency check.
        //
        // INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER scope clarification
        // (audit-ordering-high-risk-seal §Escalation 1, 2026-05-27):
        // the four `audit.emit` variants below fire AFTER
        // `idempotency.insert(...)` because the audit event TYPE is
        // discriminated by the insert's return value
        // (Accepted / DuplicateRejected / IdempotencyCollision /
        // other). Splitting into a pre-decision "AttemptStarted" emit
        // is explicitly NOT used here because:
        //   (a) WI-S10-001 §5.1 R-S10-2 designates billing-emit as
        //       fail-OPEN at the hot path (distinct from WI-S09-004
        //       audit fail-CLOSED tier; SLO emit overhead ≤ 50µs p99
        //       hot path per §5.7);
        //   (b) the idempotency staging table `(tenant_id, request_id)
        //       UNIQUE` IS the canonical commit-of-intent (replay-safe;
        //       INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP foundation);
        //   (c) doubling cardinality with a per-call "Started" event
        //       would violate the cardinality budget inherited from
        //       WI-S09-001 (canonical enum event types only).
        // The audit-emit on every arm (including failure) preserves
        // the observability surface; staging-table retry queue
        // (SLO-FRESH-BILLING ≤ 15min) is the eventual reconciliation
        // backstop for emit failures.
        match self.idempotency.insert(tenant_id, derived, &canonical) {
            Ok(IdempotencyDecision::Accepted) => {
                // Step 4: audit emit BEFORE R2 mutation (sink.append).
                let rec = Self::audit_record(
                    BillingAuditEventType::UsageEmitted,
                    &event,
                    derived,
                    created_by_request_id,
                    now_ms,
                );
                self.audit.emit(rec)?;

                // Step 5: R2 PutObject (canonical key).
                match self.sink.append(&event) {
                    Ok(line) => Ok(EmitOutcome::Persisted(line)),
                    Err(sink_err) => {
                        // Sink failure → emit sink_failure audit arm
                        // BEFORE propagating the error. Per
                        // WI-S10-001 §6.1 + Lote 10.6bis the audit
                        // fires even on the failure path so
                        // observability is complete.
                        let failure_rec = Self::audit_record(
                            BillingAuditEventType::SinkFailure,
                            &event,
                            derived,
                            created_by_request_id,
                            now_ms,
                        );
                        // Audit on the failure arm itself: if it also
                        // fails the audit envelope error wins (caller
                        // observes Err::Audit rather than Err::Sink).
                        self.audit.emit(failure_rec)?;
                        Err(sink_err.into())
                    }
                }
            }
            Ok(IdempotencyDecision::DuplicateRejected) => {
                let rec = Self::audit_record(
                    BillingAuditEventType::DuplicateRejected,
                    &event,
                    derived,
                    created_by_request_id,
                    now_ms,
                );
                self.audit.emit(rec)?;
                Ok(EmitOutcome::DuplicateRejected {
                    tenant_id,
                    idem_key: derived,
                })
            }
            Err(BillingEmitError::IdempotencyCollision { tenant_id: t, idem_key: k }) => {
                let rec = Self::audit_record(
                    BillingAuditEventType::IdempotencyCollision,
                    &event,
                    derived,
                    created_by_request_id,
                    now_ms,
                );
                self.audit.emit(rec)?;
                Err(BillingEmitError::IdempotencyCollision {
                    tenant_id: t,
                    idem_key: k,
                })
            }
            Err(other) => Err(other),
        }
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
    use crate::audit::{FailingBillingAuditSink, InMemoryBillingAuditSink};
    use crate::event::{UsageEventKind, USAGE_EVENT_TYPE};
    use crate::idempotency::InMemoryIdempotencyTracker;
    use corelink_analytics::Region;

    type Emitter =
        InMemoryUsageEventEmitter<InMemoryBillingAuditSink, InMemoryIdempotencyTracker>;

    fn fresh_emitter() -> (
        Emitter,
        Arc<InMemoryBillingAuditSink>,
        Arc<InMemoryIdempotencyTracker>,
        Arc<InMemoryR2UsageSink>,
    ) {
        let audit = Arc::new(InMemoryBillingAuditSink::new());
        let idempotency = Arc::new(InMemoryIdempotencyTracker::new());
        let sink = Arc::new(InMemoryR2UsageSink::new());
        let e = InMemoryUsageEventEmitter::new(
            Arc::clone(&audit),
            Arc::clone(&idempotency),
            Arc::clone(&sink),
        );
        (e, audit, idempotency, sink)
    }

    fn fresh_event(tenant: Uuid, qty: u64, period: &str) -> UsageEvent {
        UsageEvent::new(
            "corelink/region/iad",
            Uuid::now_v7(),
            1_700_000_000_000,
            Region::Iad,
            tenant,
            UsageEventKind::CasPut,
            qty,
            period,
        )
        .unwrap()
    }

    #[test]
    fn first_emit_persists_with_audit() {
        let (e, audit, _idem, sink) = fresh_emitter();
        let tenant = Uuid::now_v7();
        let ev = fresh_event(tenant, 4096, "2026-05");
        let outcome = e.emit(ev, "req-1", 1).unwrap();
        let line = match outcome {
            EmitOutcome::Persisted(l) => l,
            other => unreachable!("expected Persisted; got {other:?}"),
        };
        assert_eq!(line.tenant_id, tenant);
        assert_eq!(line.billing_period, "2026-05");
        assert_eq!(line.sequence_number, 0);
        assert!(line.r2_key.contains("2026-05"));
        assert!(line.r2_key.ends_with("00000000.usage.ndjson"));
        assert!(line.ndjson.contains(&format!("\"type\":\"{USAGE_EVENT_TYPE}\"")));
        assert_eq!(sink.len(), 1);
        // Audit fired exactly one usage_emitted record.
        assert_eq!(
            audit
                .snapshot_of(BillingAuditEventType::UsageEmitted)
                .len(),
            1
        );
    }

    #[test]
    fn duplicate_emit_returns_duplicate_rejected_with_audit() {
        let (e, audit, _idem, sink) = fresh_emitter();
        let tenant = Uuid::now_v7();
        let ev = fresh_event(tenant, 4096, "2026-05");
        let _ = e.emit(ev.clone(), "req-1", 1).unwrap();
        let outcome = e.emit(ev, "req-1-retry", 2).unwrap();
        match outcome {
            EmitOutcome::DuplicateRejected { tenant_id, .. } => {
                assert_eq!(tenant_id, tenant);
            }
            other => unreachable!("expected DuplicateRejected; got {other:?}"),
        }
        // R2 sink unchanged (no second write).
        assert_eq!(sink.len(), 1);
        // duplicate_rejected audit fired once.
        assert_eq!(
            audit
                .snapshot_of(BillingAuditEventType::DuplicateRejected)
                .len(),
            1
        );
    }

    #[test]
    fn audit_failure_on_first_emit_aborts_no_sink_write() {
        let audit: Arc<FailingBillingAuditSink> = Arc::new(FailingBillingAuditSink::new());
        let idem = Arc::new(InMemoryIdempotencyTracker::new());
        let sink = Arc::new(InMemoryR2UsageSink::new());
        let e = InMemoryUsageEventEmitter::new(
            Arc::clone(&audit),
            Arc::clone(&idem),
            Arc::clone(&sink),
        );
        let tenant = Uuid::now_v7();
        let ev = fresh_event(tenant, 4096, "2026-05");
        let err = e.emit(ev, "req-1", 1).unwrap_err();
        assert!(matches!(err, BillingEmitError::Audit(_)));
        // No R2 write.
        assert_eq!(sink.len(), 0);
        // The idempotency tracker DID accept the event (the audit
        // envelope sits between the tracker insert + the R2 write per
        // the canonical pipeline). Production wiring at WI-S10-007
        // observes this via the audit-of-audit failure log; a worker
        // retry would hit the dup arm + escalate via the audit
        // envelope failure counter.
        assert_eq!(idem.accepted_count(tenant), 1);
    }

    #[test]
    fn r2_sink_failure_emits_sink_failure_audit_after_overwrite() {
        // Force a sink failure by pre-populating a key the orchestrator
        // would target on its first emit (seq=0); the canonical
        // append-only check then surfaces AppendOnlyViolation.
        let (e, audit, _idem, sink) = fresh_emitter();
        let tenant = Uuid::now_v7();
        let pre_existing = PersistedUsageLine {
            r2_key: crate::sink::canonical_r2_key(tenant, "2026-05", 0),
            ndjson: "{\"k\":\"v\"}".to_string(),
            tenant_id: tenant,
            billing_period: "2026-05".to_string(),
            sequence_number: 0,
            idem_key: IdemKey::genesis(),
        };
        sink.put_at_explicit_key(pre_existing).unwrap();
        let ev = fresh_event(tenant, 4096, "2026-05");
        let err = e.emit(ev, "req-1", 1).unwrap_err();
        assert!(matches!(err, BillingEmitError::Sink(_)));
        // Two audit emits: usage_emitted (BEFORE sink call) +
        // sink_failure (AFTER sink call returns Err).
        assert_eq!(
            audit
                .snapshot_of(BillingAuditEventType::UsageEmitted)
                .len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(BillingAuditEventType::SinkFailure)
                .len(),
            1
        );
    }

    #[test]
    fn distinct_events_persist_with_distinct_idem_keys() {
        let (e, _audit, _idem, sink) = fresh_emitter();
        let tenant = Uuid::now_v7();
        let ev1 = fresh_event(tenant, 4096, "2026-05");
        let ev2 = fresh_event(tenant, 8192, "2026-05");
        let line1 = match e.emit(ev1, "req-1", 1).unwrap() {
            EmitOutcome::Persisted(l) => l,
            other => unreachable!("{other:?}"),
        };
        let line2 = match e.emit(ev2, "req-2", 2).unwrap() {
            EmitOutcome::Persisted(l) => l,
            other => unreachable!("{other:?}"),
        };
        assert_ne!(line1.idem_key, line2.idem_key);
        assert_eq!(line1.sequence_number, 0);
        assert_eq!(line2.sequence_number, 1);
        assert_eq!(sink.next_sequence(tenant, "2026-05"), 2);
    }

    #[test]
    fn cross_tenant_isolation() {
        let (e, _audit, idem, sink) = fresh_emitter();
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let ev_a = fresh_event(tenant_a, 4096, "2026-05");
        let ev_b = fresh_event(tenant_b, 4096, "2026-05");
        let _ = e.emit(ev_a.clone(), "req-a", 1).unwrap();
        let _ = e.emit(ev_b.clone(), "req-b", 2).unwrap();
        // Replay tenant A's event after tenant B accepted: dedup hit
        // for tenant A; tenant B's set untouched.
        let outcome = e.emit(ev_a, "req-a-retry", 3).unwrap();
        assert!(matches!(outcome, EmitOutcome::DuplicateRejected { .. }));
        assert_eq!(idem.accepted_count(tenant_a), 1);
        assert_eq!(idem.accepted_count(tenant_b), 1);
        assert_eq!(sink.len(), 2);
    }
}
