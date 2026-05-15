//! Tenant-offboarding orchestrator: state machine + audit fail-CLOSED
//! envelope + durable store mutation.
//!
//! ## Pipeline order (canonical)
//!
//! For every state transition request:
//!
//! 1. Read the current record from the store (read-only; no mutation).
//! 2. Resolve `(from, trigger) → to` via the canonical
//!    [`crate::state::TenantOffboardingTransition::resolve`] table.
//!    Illegal → return
//!    [`crate::error::TenantOffboardingError::IllegalTransition`];
//!    no audit row.
//! 3. Validate the canonical grace window (when `trigger == TimerExpired`
//!    and the request carries a wall-clock `now_ms`, refuse advance
//!    if `now_ms < cancel_requested_at_ms + canonical_threshold`).
//!    Refusal returns
//!    [`crate::error::TenantOffboardingError::GraceNotElapsed`].
//! 4. For `AdminCommitErasure` ONLY: require the canonical
//!    [`AdminCommitErasureRequest::dry_run_preview_confirmed`] flag.
//!    Missing → return
//!    [`crate::error::TenantOffboardingError::DryRunPreviewMissing`].
//! 5. Emit the canonical audit row BEFORE mutating the store.
//!    Failure → fail-CLOSED; the store remains at its pre-call
//!    state.
//! 6. Advance the durable store. Failure → propagate.
//!
//! This ordering is the canonical Lote 10.6bis pattern + S-07 P1-1
//! fix + ADR-S11-002 split-tier discipline inherited from
//! `corelink-dsr`.

use std::sync::Arc;

use crate::audit::{
    TenantOffboardingAuditEventType, TenantOffboardingAuditRecord, TenantOffboardingAuditSink,
};
use crate::error::TenantOffboardingError;
use crate::state::{
    TenantOffboardingState, TenantOffboardingTransition, TransitionTrigger,
};
use crate::store::{TenantOffboardingRecord, TenantOffboardingStore};

/// Canonical grace-period duration in days (T+1..T+30 = 30 days of
/// soft-write window after the canonical T+0 anchor).
pub const CANONICAL_GRACE_PERIOD_DAYS: u32 = 30;

/// Canonical read-only window in days (T+30..T+45 = 15 days of
/// read-only window after the grace window closes; this is also
/// the canonical end of the self-service revert window).
pub const CANONICAL_READ_ONLY_DAYS: u32 = 15;

/// Canonical suspended window in days (T+45..T+90 = 45 days of
/// suspended window before final erasure).
pub const CANONICAL_SUSPENDED_DAYS: u32 = 45;

/// Canonical total T+0 → T+90 = 90 days. Sum of the three above.
pub const CANONICAL_TOTAL_T_PLUS_90_DAYS: u32 = CANONICAL_GRACE_PERIOD_DAYS
    + CANONICAL_READ_ONLY_DAYS
    + CANONICAL_SUSPENDED_DAYS;

const MS_PER_DAY: i64 = 86_400_000;

/// Canonical request envelope for the
/// `AdminCommitErasure` path. The orchestrator REQUIRES
/// `dry_run_preview_confirmed = true` (mirror of the runbook
/// belt-and-braces step).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminCommitErasureRequest {
    /// Tenant identifier (opaque).
    pub tenant_id: String,
    /// Operator id (RBAC role `admin` + dual-approval pair; the
    /// production wiring at PRR ship gate validates this via
    /// `corelink-dual-approval`).
    pub operator_id: String,
    /// Whether the canonical dry-run preview was generated AND
    /// confirmed by the operator BEFORE this request was issued.
    pub dry_run_preview_confirmed: bool,
    /// Wall-clock instant at which the request was issued
    /// (ms since Unix epoch).
    pub now_ms: i64,
}

/// Orchestrator trait. The production wiring at PRR ship gate
/// binds this to a CF Worker handler.
pub trait TenantOffboardingOrchestrator: Send + Sync + core::fmt::Debug {
    /// Customer click + anti-fraud verified: open a fresh
    /// offboarding ticket. The orchestrator validates that no
    /// existing record is present (re-cancel is a runbook concern).
    ///
    /// # Errors
    ///
    /// - [`TenantOffboardingError::IllegalTransition`] if the
    ///   tenant already has an open offboarding record.
    /// - [`TenantOffboardingError::Audit`] / [`TenantOffboardingError::Store`]
    ///   on backend failure (fail-CLOSED).
    fn initiate_cancel(
        &self,
        tenant_id: &str,
        initiator_user_id: Option<String>,
        reason: Option<String>,
        now_ms: i64,
    ) -> Result<TenantOffboardingRecord, TenantOffboardingError>;

    /// Customer self-service revert (T+0..T+45 window).
    ///
    /// # Errors
    ///
    /// - [`TenantOffboardingError::IllegalTransition`] if the
    ///   current state is outside the canonical revert window
    ///   (`Suspended` / `Erased`).
    /// - Fail-CLOSED on audit / store backend failure.
    fn customer_revert(
        &self,
        tenant_id: &str,
        now_ms: i64,
    ) -> Result<TenantOffboardingRecord, TenantOffboardingError>;

    /// Daily-cron tick: advance any tenant whose canonical timer
    /// threshold is reached. The orchestrator validates the canonical
    /// `cancel_requested_at_ms + canonical_threshold` envelope.
    ///
    /// # Errors
    ///
    /// - [`TenantOffboardingError::GraceNotElapsed`] if the canonical
    ///   threshold has not been reached.
    /// - [`TenantOffboardingError::IllegalTransition`] if the
    ///   current state has no canonical timer (Active / Suspended /
    ///   Erased).
    /// - Fail-CLOSED on audit / store backend failure.
    fn cron_advance(
        &self,
        tenant_id: &str,
        now_ms: i64,
    ) -> Result<TenantOffboardingRecord, TenantOffboardingError>;

    /// Operator force-advance (auditable, runbook-gated).
    ///
    /// # Errors
    ///
    /// - [`TenantOffboardingError::IllegalTransition`] if force-
    ///   advance is illegal from the current state (Suspended /
    ///   Erased).
    /// - Fail-CLOSED on audit / store backend failure.
    fn ops_force_advance(
        &self,
        tenant_id: &str,
        operator_id: String,
        now_ms: i64,
    ) -> Result<TenantOffboardingRecord, TenantOffboardingError>;

    /// Final erasure commit (irreversible). Requires the canonical
    /// dry-run preview confirmation.
    ///
    /// # Errors
    ///
    /// - [`TenantOffboardingError::DryRunPreviewMissing`] if the
    ///   request carries `dry_run_preview_confirmed = false`.
    /// - [`TenantOffboardingError::IllegalTransition`] if the
    ///   current state is not `Suspended`.
    /// - Fail-CLOSED on audit / store backend failure.
    fn admin_commit_erasure(
        &self,
        req: AdminCommitErasureRequest,
    ) -> Result<TenantOffboardingRecord, TenantOffboardingError>;

    /// Read the current record (status poll).
    ///
    /// # Errors
    ///
    /// - [`TenantOffboardingError::Store`] on backend failure.
    fn status(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantOffboardingRecord>, TenantOffboardingError>;
}

/// In-memory orchestrator that exercises the canonical pipeline +
/// fail-CLOSED audit envelope. Production wiring at PRR ship gate
/// binds this to a CF Worker handler.
#[derive(Debug)]
pub struct InMemoryTenantOffboardingOrchestrator {
    audit: Arc<dyn TenantOffboardingAuditSink>,
    store: Arc<dyn TenantOffboardingStore>,
}

impl InMemoryTenantOffboardingOrchestrator {
    /// Fresh orchestrator with the canonical fail-CLOSED audit sink
    /// + durable store.
    #[must_use]
    pub fn new(
        audit: Arc<dyn TenantOffboardingAuditSink>,
        store: Arc<dyn TenantOffboardingStore>,
    ) -> Self {
        Self { audit, store }
    }

    /// Compute the wall-clock instant (ms since Unix epoch) at which
    /// the next canonical timer transition becomes legal for the
    /// given `(current_state, cancel_requested_at_ms)` pair, or
    /// `None` if no timer applies.
    #[must_use]
    pub const fn next_timer_threshold_ms(
        current_state: TenantOffboardingState,
        cancel_requested_at_ms: i64,
    ) -> Option<i64> {
        match current_state {
            TenantOffboardingState::CancelRequested => {
                // Smallest tick after the T+0 anchor advances to
                // GRACE_PERIOD. Canonical 1-day tick (the daily
                // cron) means the threshold is the next-day
                // anchor; we model this as `+ 1 day` so a
                // same-day request is rejected with
                // `GraceNotElapsed`.
                Some(cancel_requested_at_ms.saturating_add(MS_PER_DAY))
            }
            TenantOffboardingState::GracePeriod => Some(
                cancel_requested_at_ms.saturating_add(
                    (CANONICAL_GRACE_PERIOD_DAYS as i64).saturating_mul(MS_PER_DAY),
                ),
            ),
            TenantOffboardingState::ReadOnly => Some(
                cancel_requested_at_ms.saturating_add(
                    ((CANONICAL_GRACE_PERIOD_DAYS + CANONICAL_READ_ONLY_DAYS) as i64)
                        .saturating_mul(MS_PER_DAY),
                ),
            ),
            // Suspended needs AdminCommitErasure (not a bare timer).
            TenantOffboardingState::Suspended
            | TenantOffboardingState::Active
            | TenantOffboardingState::Erased => None,
        }
    }

    fn emit_then_mutate(
        &self,
        record: TenantOffboardingAuditRecord,
        tenant_id: &str,
        new_state: TenantOffboardingState,
    ) -> Result<(), TenantOffboardingError> {
        // Fail-CLOSED: audit BEFORE store mutation.
        self.audit.emit(record)?;
        self.store.advance_state(tenant_id, new_state)?;
        Ok(())
    }

    fn build_audit(
        tenant_id: &str,
        from: TenantOffboardingState,
        to: TenantOffboardingState,
        trigger: TransitionTrigger,
        now_ms: i64,
        operator_id: Option<String>,
    ) -> Result<TenantOffboardingAuditRecord, TenantOffboardingError> {
        let event_type = TenantOffboardingAuditEventType::for_destination(to).ok_or_else(
            || {
                TenantOffboardingError::Internal(format!(
                    "no canonical audit event for destination state {to}"
                ))
            },
        )?;
        Ok(TenantOffboardingAuditRecord {
            event_type,
            tenant_id: tenant_id.to_string(),
            from,
            to,
            trigger,
            occurred_at_ms: now_ms,
            operator_id,
        })
    }
}

impl TenantOffboardingOrchestrator for InMemoryTenantOffboardingOrchestrator {
    fn initiate_cancel(
        &self,
        tenant_id: &str,
        initiator_user_id: Option<String>,
        reason: Option<String>,
        now_ms: i64,
    ) -> Result<TenantOffboardingRecord, TenantOffboardingError> {
        if tenant_id.is_empty() {
            return Err(TenantOffboardingError::Config(
                "tenant_id must be non-empty".to_string(),
            ));
        }
        if self.store.get(tenant_id)?.is_some() {
            return Err(TenantOffboardingError::IllegalTransition {
                from: TenantOffboardingState::CancelRequested, // placeholder; record exists
                trigger: TransitionTrigger::CustomerInitiated,
            });
        }

        let audit = Self::build_audit(
            tenant_id,
            TenantOffboardingState::Active,
            TenantOffboardingState::CancelRequested,
            TransitionTrigger::CustomerInitiated,
            now_ms,
            None,
        )?;
        // Fail-CLOSED ordering: audit BEFORE store insert.
        self.audit.emit(audit)?;

        let rec = TenantOffboardingRecord::at_cancel(
            tenant_id.to_string(),
            now_ms,
            initiator_user_id,
            reason,
        );
        self.store.insert(rec.clone())?;
        Ok(rec)
    }

    fn customer_revert(
        &self,
        tenant_id: &str,
        now_ms: i64,
    ) -> Result<TenantOffboardingRecord, TenantOffboardingError> {
        let current = self.store.get(tenant_id)?.ok_or(
            TenantOffboardingError::IllegalTransition {
                from: TenantOffboardingState::Active,
                trigger: TransitionTrigger::CustomerReverted,
            },
        )?;

        let to = TenantOffboardingTransition::resolve(
            current.state,
            TransitionTrigger::CustomerReverted,
        )
        .ok_or(TenantOffboardingError::IllegalTransition {
            from: current.state,
            trigger: TransitionTrigger::CustomerReverted,
        })?;

        let audit = Self::build_audit(
            tenant_id,
            current.state,
            to,
            TransitionTrigger::CustomerReverted,
            now_ms,
            None,
        )?;
        self.emit_then_mutate(audit, tenant_id, to)?;

        let mut updated = current;
        updated.state = to;
        Ok(updated)
    }

    fn cron_advance(
        &self,
        tenant_id: &str,
        now_ms: i64,
    ) -> Result<TenantOffboardingRecord, TenantOffboardingError> {
        let current = self.store.get(tenant_id)?.ok_or(
            TenantOffboardingError::IllegalTransition {
                from: TenantOffboardingState::Active,
                trigger: TransitionTrigger::TimerExpired,
            },
        )?;

        let to = TenantOffboardingTransition::resolve(
            current.state,
            TransitionTrigger::TimerExpired,
        )
        .ok_or(TenantOffboardingError::IllegalTransition {
            from: current.state,
            trigger: TransitionTrigger::TimerExpired,
        })?;

        // INV-OFFBOARDING-GRACE-RESPECTED: validate canonical timer.
        match Self::next_timer_threshold_ms(current.state, current.cancel_requested_at_ms) {
            Some(threshold) if now_ms < threshold => {
                return Err(TenantOffboardingError::GraceNotElapsed {
                    state: current.state,
                    not_before_ms: threshold,
                });
            }
            _ => {}
        }

        let audit = Self::build_audit(
            tenant_id,
            current.state,
            to,
            TransitionTrigger::TimerExpired,
            now_ms,
            None,
        )?;
        self.emit_then_mutate(audit, tenant_id, to)?;

        let mut updated = current;
        updated.state = to;
        Ok(updated)
    }

    fn ops_force_advance(
        &self,
        tenant_id: &str,
        operator_id: String,
        now_ms: i64,
    ) -> Result<TenantOffboardingRecord, TenantOffboardingError> {
        if operator_id.is_empty() {
            return Err(TenantOffboardingError::Config(
                "operator_id must be non-empty for OpsForced".to_string(),
            ));
        }
        let current = self.store.get(tenant_id)?.ok_or(
            TenantOffboardingError::IllegalTransition {
                from: TenantOffboardingState::Active,
                trigger: TransitionTrigger::OpsForced,
            },
        )?;

        let to = TenantOffboardingTransition::resolve(
            current.state,
            TransitionTrigger::OpsForced,
        )
        .ok_or(TenantOffboardingError::IllegalTransition {
            from: current.state,
            trigger: TransitionTrigger::OpsForced,
        })?;

        let audit = Self::build_audit(
            tenant_id,
            current.state,
            to,
            TransitionTrigger::OpsForced,
            now_ms,
            Some(operator_id),
        )?;
        self.emit_then_mutate(audit, tenant_id, to)?;

        let mut updated = current;
        updated.state = to;
        Ok(updated)
    }

    fn admin_commit_erasure(
        &self,
        req: AdminCommitErasureRequest,
    ) -> Result<TenantOffboardingRecord, TenantOffboardingError> {
        if req.operator_id.is_empty() {
            return Err(TenantOffboardingError::Config(
                "operator_id must be non-empty for AdminCommitErasure".to_string(),
            ));
        }
        if !req.dry_run_preview_confirmed {
            return Err(TenantOffboardingError::DryRunPreviewMissing);
        }

        let current = self.store.get(&req.tenant_id)?.ok_or(
            TenantOffboardingError::IllegalTransition {
                from: TenantOffboardingState::Active,
                trigger: TransitionTrigger::AdminCommitErasure,
            },
        )?;

        let to = TenantOffboardingTransition::resolve(
            current.state,
            TransitionTrigger::AdminCommitErasure,
        )
        .ok_or(TenantOffboardingError::IllegalTransition {
            from: current.state,
            trigger: TransitionTrigger::AdminCommitErasure,
        })?;

        let audit = Self::build_audit(
            &req.tenant_id,
            current.state,
            to,
            TransitionTrigger::AdminCommitErasure,
            req.now_ms,
            Some(req.operator_id),
        )?;
        self.emit_then_mutate(audit, &req.tenant_id, to)?;

        let mut updated = current;
        updated.state = to;
        Ok(updated)
    }

    fn status(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantOffboardingRecord>, TenantOffboardingError> {
        Ok(self.store.get(tenant_id)?)
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
    use crate::audit::{
        FailingTenantOffboardingAuditSink, InMemoryTenantOffboardingAuditSink,
    };
    use crate::store::{FailingTenantOffboardingStore, InMemoryTenantOffboardingStore};

    fn fresh() -> (
        Arc<InMemoryTenantOffboardingAuditSink>,
        Arc<InMemoryTenantOffboardingStore>,
        InMemoryTenantOffboardingOrchestrator,
    ) {
        let audit = Arc::new(InMemoryTenantOffboardingAuditSink::new());
        let store = Arc::new(InMemoryTenantOffboardingStore::new());
        let orch = InMemoryTenantOffboardingOrchestrator::new(
            audit.clone(),
            store.clone(),
        );
        (audit, store, orch)
    }

    #[test]
    fn canonical_constants_sum_to_90_days() {
        assert_eq!(CANONICAL_TOTAL_T_PLUS_90_DAYS, 90);
        assert_eq!(
            CANONICAL_GRACE_PERIOD_DAYS + CANONICAL_READ_ONLY_DAYS + CANONICAL_SUSPENDED_DAYS,
            90
        );
    }

    #[test]
    fn initiate_cancel_writes_audit_then_store() {
        let (audit, store, orch) = fresh();
        let rec = orch.initiate_cancel("t", None, None, 1_000).unwrap();
        assert_eq!(rec.state, TenantOffboardingState::CancelRequested);
        assert_eq!(audit.len().unwrap(), 1);
        assert_eq!(store.len().unwrap(), 1);
        let row = audit.snapshot().unwrap();
        assert_eq!(
            row[0].event_type,
            TenantOffboardingAuditEventType::CancelRequested
        );
    }

    #[test]
    fn double_initiate_rejected() {
        let (_a, _s, orch) = fresh();
        orch.initiate_cancel("t", None, None, 1).unwrap();
        let r = orch.initiate_cancel("t", None, None, 2);
        assert!(matches!(
            r,
            Err(TenantOffboardingError::IllegalTransition { .. })
        ));
    }

    #[test]
    fn customer_revert_returns_to_active() {
        let (_a, _s, orch) = fresh();
        orch.initiate_cancel("t", None, None, 0).unwrap();
        let rec = orch.customer_revert("t", 100).unwrap();
        assert_eq!(rec.state, TenantOffboardingState::Active);
    }

    #[test]
    fn cron_grace_not_elapsed_blocks_advance() {
        let (_a, _s, orch) = fresh();
        orch.initiate_cancel("t", None, None, 0).unwrap();
        // Same instant — no progress allowed (cancel→grace
        // requires +1 day per canonical daily-cron tick).
        let r = orch.cron_advance("t", 0);
        assert!(matches!(r, Err(TenantOffboardingError::GraceNotElapsed { .. })));
    }

    #[test]
    fn cron_grace_elapsed_advances_to_grace_period() {
        let (audit, _s, orch) = fresh();
        orch.initiate_cancel("t", None, None, 0).unwrap();
        let rec = orch.cron_advance("t", MS_PER_DAY).unwrap();
        assert_eq!(rec.state, TenantOffboardingState::GracePeriod);
        // Two audit rows: cancel_requested + grace_started.
        assert_eq!(audit.len().unwrap(), 2);
    }

    #[test]
    fn full_happy_path_to_erased() {
        let (audit, _s, orch) = fresh();
        orch.initiate_cancel("t", None, None, 0).unwrap();
        orch.cron_advance("t", MS_PER_DAY).unwrap();
        let t30 = (CANONICAL_GRACE_PERIOD_DAYS as i64) * MS_PER_DAY;
        orch.cron_advance("t", t30).unwrap();
        let t45 = ((CANONICAL_GRACE_PERIOD_DAYS + CANONICAL_READ_ONLY_DAYS) as i64) * MS_PER_DAY;
        orch.cron_advance("t", t45).unwrap();
        // Suspended now; final erasure requires AdminCommitErasure.
        let r = orch.cron_advance("t", t45 + MS_PER_DAY);
        assert!(matches!(
            r,
            Err(TenantOffboardingError::IllegalTransition { .. })
        ));
        let rec = orch
            .admin_commit_erasure(AdminCommitErasureRequest {
                tenant_id: "t".to_string(),
                operator_id: "op-1".to_string(),
                dry_run_preview_confirmed: true,
                now_ms: t45 + MS_PER_DAY * 50,
            })
            .unwrap();
        assert_eq!(rec.state, TenantOffboardingState::Erased);
        // 5 audit rows: cancel + grace + read_only + suspended + erased.
        assert_eq!(audit.len().unwrap(), 5);
    }

    #[test]
    fn admin_commit_without_dry_run_rejected() {
        let (_a, _s, orch) = fresh();
        orch.initiate_cancel("t", None, None, 0).unwrap();
        let r = orch.admin_commit_erasure(AdminCommitErasureRequest {
            tenant_id: "t".to_string(),
            operator_id: "op-1".to_string(),
            dry_run_preview_confirmed: false,
            now_ms: 1,
        });
        assert!(matches!(r, Err(TenantOffboardingError::DryRunPreviewMissing)));
    }

    #[test]
    fn admin_commit_from_non_suspended_rejected() {
        let (_a, _s, orch) = fresh();
        orch.initiate_cancel("t", None, None, 0).unwrap();
        // We are at CancelRequested, not Suspended.
        let r = orch.admin_commit_erasure(AdminCommitErasureRequest {
            tenant_id: "t".to_string(),
            operator_id: "op-1".to_string(),
            dry_run_preview_confirmed: true,
            now_ms: 1,
        });
        assert!(matches!(
            r,
            Err(TenantOffboardingError::IllegalTransition { .. })
        ));
    }

    #[test]
    fn audit_failure_aborts_state_advance_fail_closed() {
        let store = Arc::new(InMemoryTenantOffboardingStore::new());
        let audit = Arc::new(FailingTenantOffboardingAuditSink::new());
        let orch =
            InMemoryTenantOffboardingOrchestrator::new(audit.clone(), store.clone());

        let r = orch.initiate_cancel("t", None, None, 0);
        assert!(matches!(r, Err(TenantOffboardingError::Audit(_))));
        // Fail-CLOSED: no store row inserted.
        assert_eq!(store.len().unwrap(), 0);
    }

    #[test]
    fn store_failure_propagates() {
        let store = Arc::new(FailingTenantOffboardingStore::new());
        let audit = Arc::new(InMemoryTenantOffboardingAuditSink::new());
        let orch = InMemoryTenantOffboardingOrchestrator::new(audit, store);
        let r = orch.initiate_cancel("t", None, None, 0);
        assert!(matches!(r, Err(TenantOffboardingError::Store(_))));
    }

    #[test]
    fn empty_tenant_id_rejected() {
        let (_a, _s, orch) = fresh();
        let r = orch.initiate_cancel("", None, None, 0);
        assert!(matches!(r, Err(TenantOffboardingError::Config(_))));
    }

    #[test]
    fn status_returns_none_for_unknown_tenant() {
        let (_a, _s, orch) = fresh();
        let r = orch.status("ghost").unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn ops_force_advance_skips_grace_check() {
        let (_a, _s, orch) = fresh();
        orch.initiate_cancel("t", None, None, 0).unwrap();
        // Force advance immediately (no 1-day wait); legal because
        // OpsForced does not consult the canonical timer.
        let rec = orch.ops_force_advance("t", "op-1".to_string(), 0).unwrap();
        assert_eq!(rec.state, TenantOffboardingState::GracePeriod);
    }

    #[test]
    fn ops_force_requires_operator_id() {
        let (_a, _s, orch) = fresh();
        orch.initiate_cancel("t", None, None, 0).unwrap();
        let r = orch.ops_force_advance("t", String::new(), 0);
        assert!(matches!(r, Err(TenantOffboardingError::Config(_))));
    }
}
