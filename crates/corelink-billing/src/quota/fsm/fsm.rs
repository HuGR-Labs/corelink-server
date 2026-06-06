//! [`QuotaStateMachine`] trait + [`InMemoryQuotaStateMachine`]
//! orchestrator.
//!
//! ## Run pipeline
//!
//! Three operator entry points:
//!
//! 1. [`QuotaStateMachine::evaluate_utilization`] — called by the
//!    hourly evaluation cron (cooperation WI-S10-002 counter aggregator
//!    feed → utilization percentage → state-bucket transition).
//! 2. [`QuotaStateMachine::record_invoice_failure`] — called by the
//!    Stripe webhook adapter (WI-S10-003 inheritance) when an
//!    `invoice.payment_failed` event lands; the per-tenant counter is
//!    bumped + when the canonical threshold (default 3 per WI brief)
//!    is reached, the tenant transitions to
//!    [`QuotaState::SuspendedForNonPayment`].
//! 3. [`QuotaStateMachine::reinstate`] — called by the operator-driven
//!    admin path (production wiring's Tower middleware enforces the
//!    `billing_admin` role per CTRL-AUTHZ-001 + CTRL-AUTHZ-002 BEFORE
//!    invoking this method); resets the invoice-failure counter to 0
//!    and re-evaluates the utilization-derived state.
//!
//! Each entry point follows the canonical fail-CLOSED audit envelope
//! discipline:
//!
//! 1. **Per-instance Mutex acquired** (F-001 closure: per-instance,
//!    NOT process-global; mirrors S-07 `corelink-quota::check.rs`
//!    DO-actor model).
//! 2. **Lookup the per-tenant row** (genesis WithinPlan + 0 failures
//!    when missing).
//! 3. **Compute the target transition** (pure logic over the lookup +
//!    the input).
//! 4. **Audit emit BEFORE state mutation** — for every
//!    state-mutating arm, the canonical event lands BEFORE the store
//!    UPSERT. Audit failure aborts the call (NO state mutation,
//!    propagated as [`QuotaFsmError::Audit`]).
//! 5. **Telemetry-only `OverageTelemetryRecorded` audit** — fires AT
//!    the 80pct + 95pct entries IN ADDITION to the canonical
//!    `state_changed` audit. Per the orchestrator brief: this WI ships
//!    the telemetry contract; the actual email send is the S-13
//!    admin/notifications consumer that subscribes to the audit chain.
//! 6. **Store UPSERT** — the durable mutation. The canonical fail-CLOSED
//!    envelope: store failure surfaces AFTER the audit row already
//!    landed; the operator triages.
//!
//! ## Audit-fail-CLOSED at the trait surface
//!
//! Per WI-S10-005 §6.1 + sprint contract §5.5 R-S10-10, every
//! state-mutating arm fires its canonical audit BEFORE the
//! state-store UPSERT:
//!
//! - `state_changed` fires BEFORE the UPSERT on every utilization
//!   transition (TransitionedTo80pct / 95pct / 100pct).
//! - `overage_telemetry_recorded` fires BEFORE the UPSERT on the
//!   80pct + 95pct entries (in addition to `state_changed`).
//! - `suspended` fires BEFORE the UPSERT on the terminal-arm
//!   transition.
//! - `reinstated` fires BEFORE the UPSERT on the operator-driven
//!   reinstatement.
//! - `NoChange` fires NO audit by construction (the idempotent
//!   re-fire is silent at the audit-of-audit layer per the WI brief:
//!   "re-firing same state transition is no-op").
//!
//! Audit failure on any arm aborts the orchestrator + propagates
//! [`QuotaFsmError::Audit`] (caller sees no state mutation past the
//! point of the failure).
//!
//! ## F-001 closure
//!
//! Per-instance `Arc<Mutex<()>>` serializes the (lookup → compute →
//! audit emit → store upsert) sequence. Mirrors S-07 production DO
//! actor model byte-for-byte: concurrent transition attempts cannot
//! both observe the pre-call state + both flip to the post-call state
//! — the first one to acquire the lock observes the canonical
//! genesis row + flips it; the second observes the post-flip row +
//! short-circuits via the `NoChange` arm.
//!
//! ## S-10/S-13 boundary on email send + reinstate authorization gate
//!
//! Email send is REJECTED in S-10 per ADR-0020 FROZEN. The 80pct +
//! 95pct entries emit the canonical `overage_telemetry_recorded`
//! audit ONLY; the actual email delivery is the S-13
//! admin/notifications consumer that subscribes to the audit chain
//! and dispatches the customer-facing notification.
//!
//! Reinstate authorization (`billing_admin` role check per
//! CTRL-AUTHZ-001 + CTRL-AUTHZ-002) lives at the production wiring's
//! Tower middleware, NOT here — this crate ships the state-machine
//! contract; the role enforcement is the caller's responsibility per
//! the trait surface contract. Mirrors WI-S10-003 webhook adapter
//! pattern (the signature verification lives at the webhook endpoint,
//! not in the typed adapter).

use std::sync::{Arc, Mutex};

use uuid::Uuid;

use super::audit::{
    audit_event_for_transition, transition_emits_overage_telemetry, QuotaAuditEventType,
    QuotaAuditRecord, QuotaAuditSink,
};
use super::error::QuotaFsmError;
use super::event::{
    utilization_bucket, InvoiceFailureCount, QuotaFsmConfig, QuotaState, QuotaTransition,
    UtilizationPct,
};
use super::store::{QuotaFsmStateRow, QuotaFsmStore};

/// Quota state-machine trait. Production wiring composes the
/// `QuotaFsmDO` Cloudflare Durable Object per-tenant singleton (the
/// canonical actor model per S-07 `corelink-quota` inheritance; deferred
/// to WI-S10-007 PRR ship gate per the `trait-abstraction-defer` charter
/// pattern).
pub trait QuotaStateMachine: Send + Sync + core::fmt::Debug {
    /// Evaluate the utilization-derived state for `tenant_id`. Returns
    /// the canonical [`QuotaTransition`] arm.
    ///
    /// Idempotent: re-firing the same utilization → same destination
    /// state observes [`QuotaTransition::NoChange`].
    ///
    /// Idempotent at the suspension arm: a tenant in the terminal
    /// `SuspendedForNonPayment` state is a no-op for utilization
    /// evaluation — the suspension arm dominates the utilization bucket
    /// (the operator MUST `reinstate()` first). Returns
    /// [`QuotaTransition::NoChange`] with `state =
    /// SuspendedForNonPayment`.
    ///
    /// # Errors
    ///
    /// - [`QuotaFsmError::Audit`] when the audit envelope rejects any
    ///   transition arm (fail-CLOSED at the trait surface).
    /// - [`QuotaFsmError::Store`] when the state store rejects the
    ///   UPSERT (D1 backend failure).
    /// - [`QuotaFsmError::Internal`] when a per-instance mutex is
    ///   poisoned.
    fn evaluate_utilization(
        &self,
        tenant_id: Uuid,
        utilization: UtilizationPct,
        now_ms: u64,
    ) -> Result<QuotaTransition, QuotaFsmError>;

    /// Record an invoice-failure (called by the Stripe webhook
    /// adapter on `invoice.payment_failed`). Bumps the per-tenant
    /// counter; on threshold breach transitions to
    /// [`QuotaState::SuspendedForNonPayment`].
    ///
    /// Idempotent at the terminal arm: a tenant already in
    /// `SuspendedForNonPayment` continues to bump the counter (the
    /// suspension itself is a no-op per
    /// [`QuotaTransition::NoChange`]). The store row is still upserted
    /// so the counter monotonically advances + the post-mortem
    /// evidence trail is preserved.
    ///
    /// # Errors
    ///
    /// Same surface as [`QuotaStateMachine::evaluate_utilization`].
    fn record_invoice_failure(
        &self,
        tenant_id: Uuid,
        now_ms: u64,
    ) -> Result<QuotaTransition, QuotaFsmError>;

    /// Reinstate a tenant from the terminal `SuspendedForNonPayment`
    /// arm. Resets the per-tenant counter to 0; flips the canonical
    /// state to the utilization-derived bucket (caller passes the
    /// current utilization snapshot — production wiring reads from
    /// WI-S10-002 counter aggregator at the admin handler).
    ///
    /// Idempotent: a tenant NOT in `SuspendedForNonPayment` is a no-op
    /// — returns [`QuotaTransition::NoChange`] (the operator path is
    /// audit-trailed regardless).
    ///
    /// # Errors
    ///
    /// Same surface as [`QuotaStateMachine::evaluate_utilization`].
    fn reinstate(
        &self,
        tenant_id: Uuid,
        utilization: UtilizationPct,
        now_ms: u64,
    ) -> Result<QuotaTransition, QuotaFsmError>;
}

/// In-memory quota state-machine orchestrator. Composes the audit sink
/// plus state store via `Arc` handles; both are generic over their
/// trait so test fakes (e.g. [`super::audit::FailingQuotaAuditSink`]
/// plus [`super::store::FailingQuotaFsmStore`]) compose directly at
/// construction.
pub struct InMemoryQuotaStateMachine<A, S>
where
    A: QuotaAuditSink + 'static,
    S: QuotaFsmStore + 'static,
{
    audit: Arc<A>,
    store: Arc<S>,
    config: QuotaFsmConfig,
    transition_lock: Mutex<()>,
}

impl<A, S> core::fmt::Debug for InMemoryQuotaStateMachine<A, S>
where
    A: QuotaAuditSink + 'static,
    S: QuotaFsmStore + 'static,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryQuotaStateMachine")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<A, S> InMemoryQuotaStateMachine<A, S>
where
    A: QuotaAuditSink + 'static,
    S: QuotaFsmStore + 'static,
{
    /// Construct an orchestrator with the canonical config defaults.
    pub fn new(audit: Arc<A>, store: Arc<S>) -> Self {
        Self {
            audit,
            store,
            config: QuotaFsmConfig::default(),
            transition_lock: Mutex::new(()),
        }
    }

    /// Construct an orchestrator with an explicit [`QuotaFsmConfig`].
    pub fn with_config(audit: Arc<A>, store: Arc<S>, config: QuotaFsmConfig) -> Self {
        Self {
            audit,
            store,
            config,
            transition_lock: Mutex::new(()),
        }
    }

    /// Borrow the audit sink (for tests + production observability).
    #[must_use]
    pub fn audit(&self) -> &Arc<A> {
        &self.audit
    }

    /// Borrow the state store.
    #[must_use]
    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    /// Borrow the config snapshot.
    #[must_use]
    pub const fn config(&self) -> QuotaFsmConfig {
        self.config
    }

    /// Read the per-tenant row, returning the canonical genesis
    /// (WithinPlan + 0 failures) when missing.
    fn lookup_or_genesis(
        &self,
        tenant_id: Uuid,
        now_ms: u64,
    ) -> Result<QuotaFsmStateRow, QuotaFsmError> {
        match self.store.lookup(tenant_id)? {
            Some(r) => Ok(r),
            None => Ok(QuotaFsmStateRow::genesis(tenant_id, now_ms)),
        }
    }

    /// Compute the canonical transition from a current row + a target
    /// utilization-derived state.
    fn compute_utilization_transition(
        current_state: QuotaState,
        target_state: QuotaState,
    ) -> QuotaTransition {
        if current_state == target_state {
            return QuotaTransition::NoChange {
                state: current_state,
            };
        }
        match target_state {
            QuotaState::WithinPlan => QuotaTransition::NoChange {
                state: current_state,
            },
            // Downward bounce into 80pct (e.g. from 95pct → 80pct) is
            // routed as TransitionedTo80pct so the audit row + state
            // store reflect the canonical state edge. The S-13
            // consumer can dedup at its layer (the audit chain stores
            // the typed transition; the consumer sees the directional
            // edge clearly).
            QuotaState::SoftWarning80pct => QuotaTransition::TransitionedTo80pct {
                from: current_state,
            },
            QuotaState::SoftWarning95pct => QuotaTransition::TransitionedTo95pct {
                from: current_state,
            },
            QuotaState::OverQuota100pct => QuotaTransition::TransitionedTo100pct {
                from: current_state,
            },
            QuotaState::SuspendedForNonPayment => QuotaTransition::NoChange {
                state: current_state,
            },
        }
    }

    /// Build the canonical audit record for a transition.
    fn audit_record(
        event_type: QuotaAuditEventType,
        tenant_id: Uuid,
        from_state: Option<QuotaState>,
        to_state: QuotaState,
        now_ms: u64,
        context: String,
    ) -> QuotaAuditRecord {
        QuotaAuditRecord {
            event_type,
            tenant_id,
            from_state,
            to_state,
            now_ms,
            context,
        }
    }

    /// Emit the canonical audit row(s) for a transition (state_changed
    /// + the optional overage_telemetry_recorded sibling for the 80pct
    /// + 95pct entries).
    ///
    /// Returns the destination state for the convenience of the
    /// caller (the orchestrator uses the state to route the store
    /// UPSERT).
    fn emit_transition_audits(
        &self,
        transition: QuotaTransition,
        tenant_id: Uuid,
        from_state: QuotaState,
        to_state: QuotaState,
        now_ms: u64,
    ) -> Result<(), QuotaFsmError> {
        if let Some(event_type) = audit_event_for_transition(transition) {
            self.audit.emit(Self::audit_record(
                event_type,
                tenant_id,
                Some(from_state),
                to_state,
                now_ms,
                format!(
                    "transition={} from={} to={}",
                    transition.as_str(),
                    from_state,
                    to_state
                ),
            ))?;
        }
        // Telemetry-only sibling fires on 80pct + 95pct entries (the
        // S-13 admin/notifications consumer subscribes to this).
        if transition_emits_overage_telemetry(transition) {
            self.audit.emit(Self::audit_record(
                QuotaAuditEventType::OverageTelemetryRecorded,
                tenant_id,
                Some(from_state),
                to_state,
                now_ms,
                format!(
                    "telemetry={} from={} to={}",
                    transition.as_str(),
                    from_state,
                    to_state
                ),
            ))?;
        }
        Ok(())
    }
}

impl<A, S> QuotaStateMachine for InMemoryQuotaStateMachine<A, S>
where
    A: QuotaAuditSink + 'static,
    S: QuotaFsmStore + 'static,
{
    fn evaluate_utilization(
        &self,
        tenant_id: Uuid,
        utilization: UtilizationPct,
        now_ms: u64,
    ) -> Result<QuotaTransition, QuotaFsmError> {
        let _lock = self
            .transition_lock
            .lock()
            .map_err(|_| QuotaFsmError::Internal("transition mutex poisoned".to_string()))?;

        let row = self.lookup_or_genesis(tenant_id, now_ms)?;

        // Suspension dominates utilization: the operator MUST reinstate
        // first. The audit-of-audit row is silent (NoChange arm) so the
        // hourly evaluation cron does not flood the chain with redundant
        // suspension audits.
        if row.current_state.is_suspended() {
            return Ok(QuotaTransition::NoChange {
                state: row.current_state,
            });
        }

        let target_state = utilization_bucket(utilization, &self.config);
        let transition = Self::compute_utilization_transition(row.current_state, target_state);

        if !transition.mutated() {
            return Ok(transition);
        }

        self.emit_transition_audits(
            transition,
            tenant_id,
            row.current_state,
            target_state,
            now_ms,
        )?;

        let new_row = QuotaFsmStateRow {
            tenant_id,
            current_state: target_state,
            invoice_failure_count: row.invoice_failure_count,
            updated_at_ms: now_ms,
        };
        self.store.upsert(&new_row)?;

        Ok(transition)
    }

    fn record_invoice_failure(
        &self,
        tenant_id: Uuid,
        now_ms: u64,
    ) -> Result<QuotaTransition, QuotaFsmError> {
        let _lock = self
            .transition_lock
            .lock()
            .map_err(|_| QuotaFsmError::Internal("transition mutex poisoned".to_string()))?;

        let row = self.lookup_or_genesis(tenant_id, now_ms)?;
        let new_count = row.invoice_failure_count.incremented();
        let threshold = self.config.suspension_invoice_failure_threshold();
        let crosses_threshold =
            new_count.value() >= threshold && row.invoice_failure_count.value() < threshold;

        // Compute the canonical transition arm. Two cases:
        //   1. crosses_threshold → Suspended (state edge fires).
        //   2. else → NoChange (counter monotonically advances; the
        //      tenant either was already suspended OR has not yet
        //      reached the threshold).
        let (transition, target_state) = if crosses_threshold {
            (
                QuotaTransition::Suspended {
                    invoice_failures: new_count.value(),
                },
                QuotaState::SuspendedForNonPayment,
            )
        } else {
            (
                QuotaTransition::NoChange {
                    state: row.current_state,
                },
                row.current_state,
            )
        };

        if transition.mutated() {
            self.audit.emit(Self::audit_record(
                QuotaAuditEventType::Suspended,
                tenant_id,
                Some(row.current_state),
                target_state,
                now_ms,
                format!(
                    "transition={} invoice_failures={}",
                    transition.as_str(),
                    new_count.value()
                ),
            ))?;
        }

        let new_row = QuotaFsmStateRow {
            tenant_id,
            current_state: target_state,
            invoice_failure_count: new_count,
            updated_at_ms: now_ms,
        };
        self.store.upsert(&new_row)?;

        Ok(transition)
    }

    fn reinstate(
        &self,
        tenant_id: Uuid,
        utilization: UtilizationPct,
        now_ms: u64,
    ) -> Result<QuotaTransition, QuotaFsmError> {
        let _lock = self
            .transition_lock
            .lock()
            .map_err(|_| QuotaFsmError::Internal("transition mutex poisoned".to_string()))?;

        let row = self.lookup_or_genesis(tenant_id, now_ms)?;

        if !row.current_state.is_suspended() {
            return Ok(QuotaTransition::NoChange {
                state: row.current_state,
            });
        }

        let target_state = utilization_bucket(utilization, &self.config);
        let transition = QuotaTransition::Reinstated {
            new_state: target_state,
        };

        self.audit.emit(Self::audit_record(
            QuotaAuditEventType::Reinstated,
            tenant_id,
            Some(row.current_state),
            target_state,
            now_ms,
            format!(
                "transition={} new_state={}",
                transition.as_str(),
                target_state
            ),
        ))?;

        let new_row = QuotaFsmStateRow {
            tenant_id,
            current_state: target_state,
            invoice_failure_count: InvoiceFailureCount::zero(),
            updated_at_ms: now_ms,
        };
        self.store.upsert(&new_row)?;

        Ok(transition)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives; float_cmp is acceptable for canonical-percentage-pin assertions where the values are constructed deterministically."
)]
mod tests {
    use super::super::audit::{FailingQuotaAuditSink, InMemoryQuotaAuditSink};
    use super::super::store::{FailingQuotaFsmStore, InMemoryQuotaFsmStore};
    use super::*;

    type Fsm = InMemoryQuotaStateMachine<InMemoryQuotaAuditSink, InMemoryQuotaFsmStore>;

    fn fresh() -> (Fsm, Arc<InMemoryQuotaAuditSink>, Arc<InMemoryQuotaFsmStore>) {
        let audit = Arc::new(InMemoryQuotaAuditSink::new());
        let store = Arc::new(InMemoryQuotaFsmStore::new());
        let fsm = InMemoryQuotaStateMachine::new(Arc::clone(&audit), Arc::clone(&store));
        (fsm, audit, store)
    }

    fn util(v: f64) -> UtilizationPct {
        UtilizationPct::new(v).unwrap()
    }

    #[test]
    fn within_plan_at_low_utilization_no_audit() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        let trans = fsm.evaluate_utilization(t, util(50.0), 100).unwrap();
        let no_change = matches!(
            trans,
            QuotaTransition::NoChange {
                state: QuotaState::WithinPlan
            }
        );
        assert!(no_change);
        assert_eq!(audit.len(), 0);
        // Genesis row not upserted (no mutation needed).
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn transitions_to_80pct_at_canonical_boundary() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        let trans = fsm.evaluate_utilization(t, util(80.0), 100).unwrap();
        let to_80 = matches!(
            trans,
            QuotaTransition::TransitionedTo80pct {
                from: QuotaState::WithinPlan
            }
        );
        assert!(to_80);
        // Two audit rows: state_changed + overage_telemetry_recorded.
        assert_eq!(audit.len(), 2);
        assert_eq!(
            audit.snapshot_of(QuotaAuditEventType::StateChanged).len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(QuotaAuditEventType::OverageTelemetryRecorded)
                .len(),
            1
        );
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.current_state, QuotaState::SoftWarning80pct);
    }

    #[test]
    fn transitions_to_95pct_at_canonical_boundary() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        let trans = fsm.evaluate_utilization(t, util(95.0), 100).unwrap();
        let to_95 = matches!(
            trans,
            QuotaTransition::TransitionedTo95pct {
                from: QuotaState::WithinPlan
            }
        );
        assert!(to_95);
        assert_eq!(audit.len(), 2);
        assert_eq!(
            audit
                .snapshot_of(QuotaAuditEventType::OverageTelemetryRecorded)
                .len(),
            1
        );
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.current_state, QuotaState::SoftWarning95pct);
    }

    #[test]
    fn transitions_to_100pct_writes_429_state() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        let trans = fsm.evaluate_utilization(t, util(100.0), 100).unwrap();
        let to_100 = matches!(
            trans,
            QuotaTransition::TransitionedTo100pct {
                from: QuotaState::WithinPlan
            }
        );
        assert!(to_100);
        // 100pct does NOT fire overage_telemetry (telemetry is for 80
        // + 95 only); only the state_changed audit fires.
        assert_eq!(audit.len(), 1);
        assert_eq!(
            audit.snapshot_of(QuotaAuditEventType::StateChanged).len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(QuotaAuditEventType::OverageTelemetryRecorded)
                .len(),
            0
        );
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.current_state, QuotaState::OverQuota100pct);
        assert!(row.current_state.writes_429());
    }

    #[test]
    fn idempotent_rerun_same_state_no_change_no_audit() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        let _ = fsm.evaluate_utilization(t, util(85.0), 100).unwrap();
        let audit_count_after_first = audit.len();
        // Re-fire same utilization → NoChange.
        let trans = fsm.evaluate_utilization(t, util(85.0), 200).unwrap();
        let no_change = matches!(
            trans,
            QuotaTransition::NoChange {
                state: QuotaState::SoftWarning80pct
            }
        );
        assert!(no_change);
        // No fresh audit rows.
        assert_eq!(audit.len(), audit_count_after_first);
        // Store row preserved (no UPSERT).
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.updated_at_ms, 100);
    }

    #[test]
    fn invoice_failure_below_threshold_no_suspension() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        let trans1 = fsm.record_invoice_failure(t, 100).unwrap();
        let trans2 = fsm.record_invoice_failure(t, 200).unwrap();
        let no1 = matches!(trans1, QuotaTransition::NoChange { .. });
        let no2 = matches!(trans2, QuotaTransition::NoChange { .. });
        assert!(no1);
        assert!(no2);
        // No state-changed audit; no suspended audit.
        assert_eq!(audit.len(), 0);
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.invoice_failure_count.value(), 2);
        assert_eq!(row.current_state, QuotaState::WithinPlan);
    }

    #[test]
    fn three_invoice_failures_suspend() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        fsm.record_invoice_failure(t, 100).unwrap();
        fsm.record_invoice_failure(t, 200).unwrap();
        let trans3 = fsm.record_invoice_failure(t, 300).unwrap();
        let suspended = matches!(
            trans3,
            QuotaTransition::Suspended {
                invoice_failures: 3
            }
        );
        assert!(suspended);
        assert_eq!(audit.snapshot_of(QuotaAuditEventType::Suspended).len(), 1);
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.current_state, QuotaState::SuspendedForNonPayment);
        assert_eq!(row.invoice_failure_count.value(), 3);
    }

    #[test]
    fn additional_invoice_failures_after_suspension_are_no_change() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        for i in 0..5 {
            fsm.record_invoice_failure(t, 100 * (i + 1)).unwrap();
        }
        // Only one Suspended audit (the canonical edge from 2→3
        // failures); the 4th + 5th are NoChange.
        assert_eq!(audit.snapshot_of(QuotaAuditEventType::Suspended).len(), 1);
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.current_state, QuotaState::SuspendedForNonPayment);
        assert_eq!(row.invoice_failure_count.value(), 5);
    }

    #[test]
    fn evaluate_utilization_dominated_by_suspension() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        for _ in 0..3 {
            fsm.record_invoice_failure(t, 100).unwrap();
        }
        // Tenant now suspended. Hourly evaluation cron runs; even at
        // high utilization the suspension dominates.
        let trans = fsm.evaluate_utilization(t, util(100.0), 1000).unwrap();
        let no_change = matches!(
            trans,
            QuotaTransition::NoChange {
                state: QuotaState::SuspendedForNonPayment
            }
        );
        assert!(no_change);
        // No fresh state_changed audit fired.
        assert_eq!(
            audit.snapshot_of(QuotaAuditEventType::StateChanged).len(),
            0
        );
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.current_state, QuotaState::SuspendedForNonPayment);
    }

    #[test]
    fn reinstate_clears_suspension_to_utilization_bucket() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        for _ in 0..3 {
            fsm.record_invoice_failure(t, 100).unwrap();
        }
        // Reinstate at low utilization → WithinPlan.
        let trans = fsm.reinstate(t, util(50.0), 1000).unwrap();
        let reinstated = matches!(
            trans,
            QuotaTransition::Reinstated {
                new_state: QuotaState::WithinPlan
            }
        );
        assert!(reinstated);
        assert_eq!(audit.snapshot_of(QuotaAuditEventType::Reinstated).len(), 1);
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.current_state, QuotaState::WithinPlan);
        assert_eq!(row.invoice_failure_count.value(), 0);
    }

    #[test]
    fn reinstate_at_high_utilization_lands_in_correct_bucket() {
        let (fsm, _audit, store) = fresh();
        let t = Uuid::now_v7();
        for _ in 0..3 {
            fsm.record_invoice_failure(t, 100).unwrap();
        }
        let trans = fsm.reinstate(t, util(85.0), 1000).unwrap();
        let reinstated = matches!(
            trans,
            QuotaTransition::Reinstated {
                new_state: QuotaState::SoftWarning80pct
            }
        );
        assert!(reinstated);
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.current_state, QuotaState::SoftWarning80pct);
        assert_eq!(row.invoice_failure_count.value(), 0);
    }

    #[test]
    fn reinstate_when_not_suspended_is_no_change() {
        let (fsm, audit, _store) = fresh();
        let t = Uuid::now_v7();
        let trans = fsm.reinstate(t, util(50.0), 100).unwrap();
        let no_change = matches!(
            trans,
            QuotaTransition::NoChange {
                state: QuotaState::WithinPlan
            }
        );
        assert!(no_change);
        assert_eq!(audit.len(), 0);
    }

    #[test]
    fn audit_failure_aborts_no_state_mutation() {
        let audit = Arc::new(FailingQuotaAuditSink::new());
        let store = Arc::new(InMemoryQuotaFsmStore::new());
        let fsm = InMemoryQuotaStateMachine::new(Arc::clone(&audit), Arc::clone(&store));
        let t = Uuid::now_v7();
        let err = fsm.evaluate_utilization(t, util(85.0), 100).unwrap_err();
        let audit_err = matches!(err, QuotaFsmError::Audit(_));
        assert!(audit_err);
        assert!(store.is_empty());
    }

    #[test]
    fn store_failure_on_lookup_aborts_pre_audit() {
        // The orchestrator's first store touch is a `lookup()` (the
        // genesis-row fallback path). A failing lookup aborts the call
        // BEFORE any audit row lands; the canonical fail-CLOSED
        // envelope still holds (no state mutation, no audit, no S-13
        // dispatch). The audit-emit-BEFORE-mutation discipline is
        // exercised by the upsert path: see
        // `store_failure_on_upsert_lands_audit_then_propagates`.
        let audit: Arc<InMemoryQuotaAuditSink> = Arc::new(InMemoryQuotaAuditSink::new());
        let store: Arc<FailingQuotaFsmStore> = Arc::new(FailingQuotaFsmStore::new());
        let fsm = InMemoryQuotaStateMachine::new(Arc::clone(&audit), Arc::clone(&store));
        let t = Uuid::now_v7();
        let err = fsm.evaluate_utilization(t, util(85.0), 100).unwrap_err();
        let store_err = matches!(err, QuotaFsmError::Store(_));
        assert!(store_err);
        // Lookup failed BEFORE any audit landed.
        assert_eq!(audit.len(), 0);
    }

    #[test]
    fn store_failure_on_upsert_lands_audit_then_propagates() {
        // A store that succeeds on lookup but fails on upsert
        // exercises the canonical fail-CLOSED envelope: the audit row
        // lands BEFORE the upsert; the upsert failure surfaces AFTER
        // the audit row is durable so the operator triages.
        #[derive(Debug)]
        struct LookupOkUpsertFailStore;
        impl QuotaFsmStore for LookupOkUpsertFailStore {
            fn lookup(
                &self,
                tenant_id: Uuid,
            ) -> Result<Option<QuotaFsmStateRow>, super::super::error::QuotaFsmStoreError>
            {
                Ok(Some(QuotaFsmStateRow::genesis(tenant_id, 0)))
            }
            fn upsert(
                &self,
                _row: &QuotaFsmStateRow,
            ) -> Result<(), super::super::error::QuotaFsmStoreError> {
                Err(super::super::error::QuotaFsmStoreError::Backend(
                    "induced upsert failure".to_string(),
                ))
            }
        }
        let audit = Arc::new(InMemoryQuotaAuditSink::new());
        let store = Arc::new(LookupOkUpsertFailStore);
        let fsm = InMemoryQuotaStateMachine::new(Arc::clone(&audit), Arc::clone(&store));
        let t = Uuid::now_v7();
        let err = fsm.evaluate_utilization(t, util(85.0), 100).unwrap_err();
        let store_err = matches!(err, QuotaFsmError::Store(_));
        assert!(store_err);
        // Audit rows already landed (state_changed + overage_telemetry)
        // BEFORE the upsert attempt failed.
        assert_eq!(audit.len(), 2);
    }

    #[test]
    fn tenant_isolation_per_tenant_state() {
        let (fsm, _audit, store) = fresh();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        // Suspend t1.
        for _ in 0..3 {
            fsm.record_invoice_failure(t1, 100).unwrap();
        }
        // Evaluate t2 at high utilization.
        fsm.evaluate_utilization(t2, util(100.0), 200).unwrap();
        let r1 = store.lookup(t1).unwrap().unwrap();
        let r2 = store.lookup(t2).unwrap().unwrap();
        assert_eq!(r1.current_state, QuotaState::SuspendedForNonPayment);
        assert_eq!(r2.current_state, QuotaState::OverQuota100pct);
    }

    #[test]
    fn config_override_threshold_routes_decision() {
        let audit = Arc::new(InMemoryQuotaAuditSink::new());
        let store = Arc::new(InMemoryQuotaFsmStore::new());
        // Custom: 50/75/90 + 1 invoice failure = suspend.
        let cfg = QuotaFsmConfig::new(50.0, 75.0, 90.0, 1).unwrap();
        let fsm =
            InMemoryQuotaStateMachine::with_config(Arc::clone(&audit), Arc::clone(&store), cfg);
        let t = Uuid::now_v7();
        // 1 failure → suspended under tighter threshold.
        let trans = fsm.record_invoice_failure(t, 100).unwrap();
        let suspended = matches!(
            trans,
            QuotaTransition::Suspended {
                invoice_failures: 1
            }
        );
        assert!(suspended);
    }

    #[test]
    fn ladder_walk_within_plan_to_80pct_to_95pct_to_100pct() {
        let (fsm, audit, store) = fresh();
        let t = Uuid::now_v7();
        // Walk the ladder.
        let _ = fsm.evaluate_utilization(t, util(50.0), 100).unwrap();
        let _ = fsm.evaluate_utilization(t, util(85.0), 200).unwrap();
        let _ = fsm.evaluate_utilization(t, util(96.0), 300).unwrap();
        let _ = fsm.evaluate_utilization(t, util(101.0), 400).unwrap();
        // 3 state_changed audits (genesis to 80, 80 to 95, 95 to 100).
        assert_eq!(
            audit.snapshot_of(QuotaAuditEventType::StateChanged).len(),
            3
        );
        // 2 overage_telemetry audits (80 + 95 entries).
        assert_eq!(
            audit
                .snapshot_of(QuotaAuditEventType::OverageTelemetryRecorded)
                .len(),
            2
        );
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.current_state, QuotaState::OverQuota100pct);
    }

    #[test]
    fn downward_bounce_routes_through_80_arm() {
        let (fsm, _audit, store) = fresh();
        let t = Uuid::now_v7();
        // Up to 95.
        let _ = fsm.evaluate_utilization(t, util(96.0), 100).unwrap();
        // Down to 85 → routes via TransitionedTo80pct (canonical
        // state edge per the orchestrator design).
        let trans = fsm.evaluate_utilization(t, util(85.0), 200).unwrap();
        let to_80 = matches!(
            trans,
            QuotaTransition::TransitionedTo80pct {
                from: QuotaState::SoftWarning95pct
            }
        );
        assert!(to_80);
        let row = store.lookup(t).unwrap().unwrap();
        assert_eq!(row.current_state, QuotaState::SoftWarning80pct);
    }

    #[test]
    fn downward_to_within_plan_is_no_change_arm() {
        // Per the orchestrator design: downward bounces into
        // WithinPlan are routed as NoChange (no state edge audit on
        // the silent operation arm — the customer leaving the warning
        // band is informational only; no S-13 consumer dispatch).
        let (fsm, audit, _store) = fresh();
        let t = Uuid::now_v7();
        // Up to 85.
        let _ = fsm.evaluate_utilization(t, util(85.0), 100).unwrap();
        let audit_after_first = audit.len();
        // Down to 50 → NoChange.
        let trans = fsm.evaluate_utilization(t, util(50.0), 200).unwrap();
        let no_change = matches!(
            trans,
            QuotaTransition::NoChange {
                state: QuotaState::SoftWarning80pct
            }
        );
        assert!(no_change);
        // No new audits.
        assert_eq!(audit.len(), audit_after_first);
    }
}
