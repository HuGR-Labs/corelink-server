//! [`RolloutController`] trait + [`InMemoryRolloutController`]
//! orchestrator (WI-S13-005).
//!
//! The in-memory orchestrator exercises every load-bearing invariant
//! the production Cloudflare Durable Object wiring satisfies, without
//! requiring a live CF account. Per `trait-abstraction-defer` charter.
//!
//! ## Invariants enforced
//!
//! - **INV-SUPPLY-SIGNED-DEPLOY**: `start()` rejects unsigned artifacts.
//! - **INV-ROLLOUT-SINGLE-ACTIVE**: at most 1 active rollout per env.
//! - **INV-ROLLOUT-BUDGET-CAP**: pre-start + continuous budget check.
//! - **INV-ROLLOUT-NO-STAGE-SKIP**: validated by state machine.
//! - **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER**: audit before mutation.
//!
//! Per F-001: per-instance `Arc<Mutex<>>` (no global state).

use std::sync::{Arc, Mutex};

use crate::audit::{
    FailingRolloutAuditSink, InMemoryRolloutAuditSink, RolloutAuditEventType, RolloutAuditRecord,
    RolloutAuditSink,
};
use crate::auto_rollback::AutoRollbackDriver;
use crate::budget::BudgetTracker;
use crate::error::RolloutError;
use crate::state_machine::RolloutStateMachine;
use crate::types::{
    AdminActor, AutoRollbackTrigger, DeployArtifact, GateMetrics, NextAction, RolloutDecision,
    RolloutHandle, RolloutStage, RolloutStatus,
};
use uuid::Uuid;

/// Rollout controller trait. Production implementation: CF Durable
/// Object `RolloutControllerSingletonDO-{env}` per environment.
pub trait RolloutController: Send + Sync {
    /// Initiate a new rollout (starts at `Stage1Pct`).
    ///
    /// Pre-conditions checked in order:
    /// 1. `artifact.is_cosign_signed()` → else `UnsignedDeploy`.
    /// 2. Budget not exceeded → else `BudgetExceeded`.
    /// 3. No active rollout in this env → else `RolloutInFlight`.
    ///
    /// # Errors
    ///
    /// [`RolloutError::UnsignedDeploy`] | [`RolloutError::BudgetExceeded`]
    /// | [`RolloutError::RolloutInFlight`] | [`RolloutError::Audit`]
    /// | [`RolloutError::Storage`].
    fn start(
        &self,
        artifact: &DeployArtifact,
        actor: &AdminActor,
    ) -> Result<RolloutHandle, RolloutError>;

    /// Probe current stage health; advance if gate criteria + dwell met.
    ///
    /// Should be called every 60s by the controller loop. Returns the
    /// decision taken and updates handle status.
    ///
    /// # Errors
    ///
    /// [`RolloutError::Internal`] if handle not found.
    fn probe_and_advance(
        &self,
        handle: &RolloutHandle,
        metrics: GateMetrics,
    ) -> Result<RolloutDecision, RolloutError>;

    /// Trigger auto-rollback explicitly (e.g., from chaos test driver).
    ///
    /// # Errors
    ///
    /// [`RolloutError::Audit`] | [`RolloutError::Storage`].
    fn auto_rollback(
        &self,
        handle: &RolloutHandle,
        trigger: AutoRollbackTrigger,
    ) -> Result<RolloutHandle, RolloutError>;

    /// Manual abort (admin override, dual-approval gated by WI-S13-002).
    ///
    /// # Errors
    ///
    /// [`RolloutError::Audit`] | [`RolloutError::Storage`].
    fn abort(
        &self,
        handle: &RolloutHandle,
        actor: &AdminActor,
    ) -> Result<RolloutHandle, RolloutError>;

    /// Current monthly error budget consumption ratio (0.0–1.0+).
    ///
    /// # Errors
    ///
    /// [`RolloutError::Storage`] on tracker failure.
    fn monthly_budget_consumed(&self) -> Result<f64, RolloutError>;
}

/// Internal per-session state held by the in-memory controller.
#[derive(Debug)]
struct SessionState {
    handle: RolloutHandle,
    rollback_driver: AutoRollbackDriver,
}

/// In-memory rollout controller for tests and CI.
///
/// Uses `Arc<Mutex<>>` per F-001; no global state.
#[derive(Debug)]
pub struct InMemoryRolloutController<A, B>
where
    A: RolloutAuditSink,
    B: BudgetTracker,
{
    audit: Arc<A>,
    budget: Arc<B>,
    state_machine: RolloutStateMachine,
    /// Active session (at most 1 at a time per INV-ROLLOUT-SINGLE-ACTIVE).
    active_session: Arc<Mutex<Option<SessionState>>>,
    /// "current time" in ms for dwell + budget window calculation.
    now_ms: Arc<Mutex<u64>>,
}

impl<A, B> InMemoryRolloutController<A, B>
where
    A: RolloutAuditSink,
    B: BudgetTracker,
{
    /// Create a new in-memory controller.
    #[must_use]
    pub fn new(audit: Arc<A>, budget: Arc<B>) -> Self {
        Self {
            audit,
            budget,
            state_machine: RolloutStateMachine::new(),
            active_session: Arc::new(Mutex::new(None)),
            now_ms: Arc::new(Mutex::new(0)),
        }
    }

    /// Advance the controller's notion of "now" (for dwell + window testing).
    ///
    /// # Errors
    ///
    /// Returns [`RolloutError::Internal`] if the mutex is poisoned.
    pub fn set_now_ms(&self, now_ms: u64) -> Result<(), RolloutError> {
        *self
            .now_ms
            .lock()
            .map_err(|e| RolloutError::Internal(format!("now_ms mutex poisoned: {e}")))? =
            now_ms;
        Ok(())
    }

    fn current_now_ms(&self) -> Result<u64, RolloutError> {
        self.now_ms
            .lock()
            .map_err(|e| RolloutError::Internal(format!("now_ms mutex poisoned: {e}")))
            .map(|g| *g)
    }

    fn emit_audit(
        &self,
        event_type: RolloutAuditEventType,
        handle: &RolloutHandle,
        rollback_trigger: Option<AutoRollbackTrigger>,
        detail: &str,
    ) -> Result<(), RolloutError> {
        let now = self.current_now_ms()?;
        let record = RolloutAuditRecord {
            event_id: Uuid::now_v7(),
            emitted_at_ms: now,
            event_type,
            handle_id: handle.handle_id,
            stage: handle.current_stage,
            status: handle.status,
            rollback_trigger,
            actor_user_id: Uuid::nil(),
            detail: detail.to_string(),
        };
        self.audit.emit(record)
    }
}

impl<A, B> RolloutController for InMemoryRolloutController<A, B>
where
    A: RolloutAuditSink,
    B: BudgetTracker,
{
    fn start(
        &self,
        artifact: &DeployArtifact,
        actor: &AdminActor,
    ) -> Result<RolloutHandle, RolloutError> {
        // Gate 1: Cosign signature (INV-SUPPLY-SIGNED-DEPLOY)
        if !artifact.is_cosign_signed() {
            return Err(RolloutError::UnsignedDeploy);
        }

        // Gate 2: Budget cap (pre-start continuous check)
        let consumed = self.budget.consumed_ratio()?;
        if consumed > 1.0 {
            return Err(RolloutError::BudgetExceeded { consumed });
        }

        // Gate 3: Single active rollout (INV-ROLLOUT-SINGLE-ACTIVE)
        let mut session_guard = self
            .active_session
            .lock()
            .map_err(|e| RolloutError::Internal(format!("session mutex poisoned: {e}")))?;

        if let Some(existing) = session_guard.as_ref() {
            return Err(RolloutError::RolloutInFlight(
                existing.handle.handle_id,
            ));
        }

        let now_ms = self.current_now_ms()?;
        let handle = RolloutHandle {
            handle_id: Uuid::now_v7(),
            current_stage: RolloutStage::Stage1Pct,
            status: RolloutStatus::Active,
            started_at_ms: now_ms,
            stage_entered_at_ms: now_ms,
            rollback_trigger: None,
            rollback_at_ms: None,
            artifact_sha256_hex: artifact.sha256_hex.clone(),
        };

        // Audit BEFORE state mutation (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER)
        let audit_record = RolloutAuditRecord {
            event_id: Uuid::now_v7(),
            emitted_at_ms: now_ms,
            event_type: RolloutAuditEventType::Start,
            handle_id: handle.handle_id,
            stage: handle.current_stage,
            status: handle.status,
            rollback_trigger: None,
            actor_user_id: actor.user_id,
            detail: format!(
                "artifact={} cosign=true rekor={}",
                artifact.sha256_hex,
                artifact
                    .rekor_log_index
                    .map(|i| i.to_string())
                    .unwrap_or_default()
            ),
        };
        self.audit.emit(audit_record)?;

        // State mutation
        *session_guard = Some(SessionState {
            handle: handle.clone(),
            rollback_driver: AutoRollbackDriver::new(),
        });

        Ok(handle)
    }

    fn probe_and_advance(
        &self,
        handle: &RolloutHandle,
        metrics: GateMetrics,
    ) -> Result<RolloutDecision, RolloutError> {
        let mut session_guard = self
            .active_session
            .lock()
            .map_err(|e| RolloutError::Internal(format!("session mutex poisoned: {e}")))?;

        let session = session_guard.as_mut().ok_or_else(|| {
            RolloutError::Internal(format!("no active session for handle {}", handle.handle_id))
        })?;

        // Continuous budget check (Lote 10.13 codex P1 fix)
        let consumed = self.budget.consumed_ratio()?;
        if consumed > 1.0 {
            let new_handle = RolloutHandle {
                status: RolloutStatus::BudgetFrozen,
                ..session.handle.clone()
            };
            // Audit BEFORE mutation
            let now_ms = self.current_now_ms()?;
            let audit_record = RolloutAuditRecord {
                event_id: Uuid::now_v7(),
                emitted_at_ms: now_ms,
                event_type: RolloutAuditEventType::BudgetFreeze,
                handle_id: new_handle.handle_id,
                stage: new_handle.current_stage,
                status: new_handle.status,
                rollback_trigger: None,
                actor_user_id: Uuid::nil(),
                detail: format!("consumed_ratio={consumed:.4} > 1.0 (30% cap)"),
            };
            self.audit.emit(audit_record)?;
            session.handle = new_handle.clone();
            *session_guard = None;
            return Err(RolloutError::BudgetExceeded { consumed });
        }

        // Observe metrics in auto-rollback driver
        let sustained_trigger = session.rollback_driver.observe(&metrics, 60);

        let decision = self.state_machine.evaluate(
            session.handle.current_stage,
            metrics,
            sustained_trigger,
        );

        let now_ms = self.current_now_ms()?;

        match &decision.next_action {
            NextAction::AutoRollback(trigger) => {
                let trigger = *trigger;
                let new_handle = RolloutHandle {
                    status: RolloutStatus::AutoRolledBack,
                    rollback_trigger: Some(trigger),
                    rollback_at_ms: Some(now_ms),
                    ..session.handle.clone()
                };
                // Audit BEFORE mutation
                let audit_record = RolloutAuditRecord {
                    event_id: Uuid::now_v7(),
                    emitted_at_ms: now_ms,
                    event_type: RolloutAuditEventType::AutoRollback,
                    handle_id: new_handle.handle_id,
                    stage: new_handle.current_stage,
                    status: new_handle.status,
                    rollback_trigger: Some(trigger),
                    actor_user_id: Uuid::nil(),
                    detail: format!("trigger={}", trigger.as_metric_label()),
                };
                self.audit.emit(audit_record)?;
                session.handle = new_handle;
                *session_guard = None;
            }
            NextAction::Advance(next_stage) => {
                let next_stage = *next_stage;
                self.state_machine
                    .validate_advance(session.handle.current_stage, next_stage)?;
                let new_handle = RolloutHandle {
                    current_stage: next_stage,
                    stage_entered_at_ms: now_ms,
                    ..session.handle.clone()
                };
                // Audit BEFORE mutation
                let audit_record = RolloutAuditRecord {
                    event_id: Uuid::now_v7(),
                    emitted_at_ms: now_ms,
                    event_type: RolloutAuditEventType::StageAdvance,
                    handle_id: new_handle.handle_id,
                    stage: next_stage,
                    status: new_handle.status,
                    rollback_trigger: None,
                    actor_user_id: Uuid::nil(),
                    detail: format!("advanced to {}", next_stage.as_db_str()),
                };
                self.audit.emit(audit_record)?;
                session.handle = new_handle;
            }
            NextAction::Complete => {
                let new_handle = RolloutHandle {
                    status: RolloutStatus::Completed,
                    ..session.handle.clone()
                };
                // Audit BEFORE mutation
                let audit_record = RolloutAuditRecord {
                    event_id: Uuid::now_v7(),
                    emitted_at_ms: now_ms,
                    event_type: RolloutAuditEventType::Complete,
                    handle_id: new_handle.handle_id,
                    stage: new_handle.current_stage,
                    status: new_handle.status,
                    rollback_trigger: None,
                    actor_user_id: Uuid::nil(),
                    detail: "rollout completed at stage_100pct".to_string(),
                };
                self.audit.emit(audit_record)?;
                session.handle = new_handle;
                *session_guard = None;
            }
            NextAction::Hold(_) => {
                // Audit BEFORE mutation (state unchanged on Hold)
                let audit_record = RolloutAuditRecord {
                    event_id: Uuid::now_v7(),
                    emitted_at_ms: now_ms,
                    event_type: RolloutAuditEventType::Hold,
                    handle_id: session.handle.handle_id,
                    stage: session.handle.current_stage,
                    status: session.handle.status,
                    rollback_trigger: None,
                    actor_user_id: Uuid::nil(),
                    detail: "dwell not satisfied; holding".to_string(),
                };
                self.audit.emit(audit_record)?;
                // No state change
            }
        }

        Ok(decision)
    }

    fn auto_rollback(
        &self,
        handle: &RolloutHandle,
        trigger: AutoRollbackTrigger,
    ) -> Result<RolloutHandle, RolloutError> {
        let now_ms = self.current_now_ms()?;
        let new_handle = RolloutHandle {
            status: RolloutStatus::AutoRolledBack,
            rollback_trigger: Some(trigger),
            rollback_at_ms: Some(now_ms),
            ..handle.clone()
        };
        self.emit_audit(
            RolloutAuditEventType::AutoRollback,
            &new_handle,
            Some(trigger),
            &format!("explicit auto_rollback trigger={}", trigger.as_metric_label()),
        )?;
        let mut session_guard = self
            .active_session
            .lock()
            .map_err(|e| RolloutError::Internal(format!("session mutex poisoned: {e}")))?;
        if let Some(s) = session_guard.as_mut() {
            s.handle = new_handle.clone();
        }
        *session_guard = None;
        Ok(new_handle)
    }

    fn abort(
        &self,
        handle: &RolloutHandle,
        actor: &AdminActor,
    ) -> Result<RolloutHandle, RolloutError> {
        let now_ms = self.current_now_ms()?;
        let new_handle = RolloutHandle {
            status: RolloutStatus::ManuallyAborted,
            ..handle.clone()
        };
        // Audit BEFORE mutation
        let audit_record = RolloutAuditRecord {
            event_id: Uuid::now_v7(),
            emitted_at_ms: now_ms,
            event_type: RolloutAuditEventType::Abort,
            handle_id: new_handle.handle_id,
            stage: new_handle.current_stage,
            status: new_handle.status,
            rollback_trigger: None,
            actor_user_id: actor.user_id,
            detail: format!("manual abort by actor={}", actor.email),
        };
        self.audit.emit(audit_record)?;

        let mut session_guard = self
            .active_session
            .lock()
            .map_err(|e| RolloutError::Internal(format!("session mutex poisoned: {e}")))?;
        if let Some(s) = session_guard.as_mut() {
            s.handle = new_handle.clone();
        }
        *session_guard = None;
        Ok(new_handle)
    }

    fn monthly_budget_consumed(&self) -> Result<f64, RolloutError> {
        self.budget.consumed_ratio()
    }
}

/// Convenience alias: in-memory controller with in-memory audit + budget.
pub type DefaultInMemoryController = InMemoryRolloutController<
    InMemoryRolloutAuditSink,
    crate::budget::InMemoryBudgetTracker,
>;

/// Convenience alias: controller with failing audit (for fail-CLOSED tests).
pub type FailingAuditController = InMemoryRolloutController<
    FailingRolloutAuditSink,
    crate::budget::InMemoryBudgetTracker,
>;

/// Construct a default in-memory controller (convenient for tests).
#[must_use]
pub fn default_controller() -> (
    DefaultInMemoryController,
    Arc<InMemoryRolloutAuditSink>,
    Arc<crate::budget::InMemoryBudgetTracker>,
) {
    let audit = Arc::new(InMemoryRolloutAuditSink::new());
    let budget = Arc::new(crate::budget::InMemoryBudgetTracker::new());
    let controller = InMemoryRolloutController::new(Arc::clone(&audit), Arc::clone(&budget));
    (controller, audit, budget)
}

/// Build a signed deploy artifact (for tests).
#[must_use]
pub fn signed_artifact() -> DeployArtifact {
    DeployArtifact {
        sha256_hex: "a".repeat(64),
        cosign_signature_url: Some(
            "https://ghcr.io/humangr-labs/corelink@sha256:aaaa::sig".to_string(),
        ),
        rekor_log_index: Some(12345),
        description: "test artifact".to_string(),
    }
}

/// Build an unsigned deploy artifact (for negative tests).
#[must_use]
pub fn unsigned_artifact() -> DeployArtifact {
    DeployArtifact {
        sha256_hex: "b".repeat(64),
        cosign_signature_url: None,
        rekor_log_index: None,
        description: "unsigned test artifact".to_string(),
    }
}

/// Build an actor with fresh MFA (mfa_verified_at_ms = now_ms).
#[must_use]
pub fn fresh_actor(now_ms: u64) -> AdminActor {
    AdminActor {
        user_id: Uuid::now_v7(),
        email: "admin@humangr-labs.io".to_string(),
        mfa_verified_at_ms: now_ms,
    }
}

/// Build gate metrics that pass all criteria for the given stage.
#[must_use]
pub fn passing_metrics(stage: RolloutStage) -> GateMetrics {
    GateMetrics {
        error_rate: 0.001,
        error_rate_baseline: 0.01,
        error_rate_sigma: 0.001,
        slo_burn_rate_1h: 0.5,
        p99_latency_ms: 50.0,
        p99_baseline_ms: 60.0,
        stage_elapsed_secs: u64::from(stage.dwell_minutes_min()) * 60 + 60,
    }
}

/// Build gate metrics that trigger error-rate rollback.
#[must_use]
pub fn error_rate_trigger_metrics() -> GateMetrics {
    GateMetrics {
        error_rate: 0.10,
        error_rate_baseline: 0.005,
        error_rate_sigma: 0.001,
        slo_burn_rate_1h: 0.5,
        p99_latency_ms: 50.0,
        p99_baseline_ms: 60.0,
        stage_elapsed_secs: 1800,
    }
}

/// Build gate metrics that trigger SLO burn rollback.
#[must_use]
pub fn slo_burn_trigger_metrics() -> GateMetrics {
    GateMetrics {
        error_rate: 0.001,
        error_rate_baseline: 0.01,
        error_rate_sigma: 0.001,
        slo_burn_rate_1h: 15.0,
        p99_latency_ms: 50.0,
        p99_baseline_ms: 60.0,
        stage_elapsed_secs: 1800,
    }
}

/// Build gate metrics that trigger p99 latency rollback.
#[must_use]
pub fn p99_trigger_metrics() -> GateMetrics {
    GateMetrics {
        error_rate: 0.001,
        error_rate_baseline: 0.01,
        error_rate_sigma: 0.001,
        slo_burn_rate_1h: 0.5,
        p99_latency_ms: 200.0,
        p99_baseline_ms: 60.0,
        stage_elapsed_secs: 1800,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests use panics as assertions"
)]
mod tests {
    use super::*;

    #[test]
    fn start_signed_artifact_succeeds() {
        let (ctrl, audit, _) = default_controller();
        let actor = fresh_actor(0);
        let handle = ctrl.start(&signed_artifact(), &actor).unwrap();
        assert_eq!(handle.current_stage, RolloutStage::Stage1Pct);
        assert_eq!(handle.status, RolloutStatus::Active);
        let records = audit.records().unwrap();
        assert_eq!(records.len(), 1);
        let first = records.first().expect("expected at least one audit record");
        assert_eq!(first.event_type, crate::audit::RolloutAuditEventType::Start);
    }

    #[test]
    fn start_unsigned_artifact_rejected() {
        let (ctrl, _, _) = default_controller();
        let actor = fresh_actor(0);
        let err = ctrl.start(&unsigned_artifact(), &actor).unwrap_err();
        assert!(matches!(err, RolloutError::UnsignedDeploy));
    }

    #[test]
    fn concurrent_start_blocked() {
        let (ctrl, _, _) = default_controller();
        let actor = fresh_actor(0);
        ctrl.start(&signed_artifact(), &actor).unwrap();
        let err = ctrl.start(&signed_artifact(), &actor).unwrap_err();
        assert!(matches!(err, RolloutError::RolloutInFlight(_)));
    }

    #[test]
    fn probe_hold_when_dwell_not_satisfied() {
        let (ctrl, _, _) = default_controller();
        let actor = fresh_actor(0);
        let handle = ctrl.start(&signed_artifact(), &actor).unwrap();
        let mut m = passing_metrics(RolloutStage::Stage1Pct);
        m.stage_elapsed_secs = 0; // not satisfied
        let decision = ctrl.probe_and_advance(&handle, m).unwrap();
        assert!(matches!(decision.next_action, NextAction::Hold(_)));
    }

    #[test]
    fn probe_advance_when_gate_passes() {
        let (ctrl, _, _) = default_controller();
        let actor = fresh_actor(0);
        let handle = ctrl.start(&signed_artifact(), &actor).unwrap();
        let m = passing_metrics(RolloutStage::Stage1Pct);
        let decision = ctrl.probe_and_advance(&handle, m).unwrap();
        assert!(matches!(
            decision.next_action,
            NextAction::Advance(RolloutStage::Stage10Pct)
        ));
    }

    #[test]
    fn auto_rollback_trigger_fires_after_sustained_5min() {
        let (ctrl, _, _) = default_controller();
        let actor = fresh_actor(0);
        let handle = ctrl.start(&signed_artifact(), &actor).unwrap();
        // 5 probes at 60s interval (error rate trigger)
        let mut decision = None;
        for _ in 0..5 {
            decision = Some(
                ctrl.probe_and_advance(&handle, error_rate_trigger_metrics())
                    .unwrap(),
            );
        }
        let d = decision.unwrap();
        assert!(matches!(
            d.next_action,
            NextAction::AutoRollback(AutoRollbackTrigger::ErrorRateExceedsBaseline3Sigma)
        ));
    }

    #[test]
    fn manual_abort_requires_no_active_session_after() {
        let (ctrl, _, _) = default_controller();
        let actor = fresh_actor(0);
        let handle = ctrl.start(&signed_artifact(), &actor).unwrap();
        let aborted = ctrl.abort(&handle, &actor).unwrap();
        assert_eq!(aborted.status, RolloutStatus::ManuallyAborted);
        // Now a new rollout can start
        let handle2 = ctrl.start(&signed_artifact(), &actor).unwrap();
        assert_eq!(handle2.current_stage, RolloutStage::Stage1Pct);
    }

    #[test]
    fn failing_audit_blocks_start() {
        let budget = Arc::new(crate::budget::InMemoryBudgetTracker::new());
        let audit = Arc::new(FailingRolloutAuditSink);
        let ctrl: FailingAuditController = InMemoryRolloutController::new(audit, budget);
        let actor = fresh_actor(0);
        let err = ctrl.start(&signed_artifact(), &actor).unwrap_err();
        assert!(matches!(err, RolloutError::Audit(_)));
    }

    #[test]
    fn budget_exceeded_blocks_start() {
        let (ctrl, _, budget) = default_controller();
        // Record 3 rollbacks of 1100 bps each = 3300 > 3000 bps = >30%
        let now_ms = 1_000_000u64;
        ctrl.set_now_ms(now_ms).unwrap();
        budget.set_now_ms(now_ms).unwrap();
        for _ in 0..3 {
            budget
                .record_rollback(crate::budget::BudgetRecord {
                    handle_id: Uuid::now_v7(),
                    rollback_started_ms: now_ms - 1_000,
                    rollback_completed_ms: now_ms,
                    error_count_consumed: 100,
                    monthly_error_budget_target: 10_000,
                    budget_consumed_bps: 1100,
                })
                .unwrap();
        }
        let actor = fresh_actor(0);
        let err = ctrl.start(&signed_artifact(), &actor).unwrap_err();
        assert!(matches!(err, RolloutError::BudgetExceeded { .. }));
    }
}
