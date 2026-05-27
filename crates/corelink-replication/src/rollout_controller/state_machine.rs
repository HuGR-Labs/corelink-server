//! Rollout state machine primitive (WI-S13-005).
//!
//! Validates state transitions, enforces stage progression order
//! (INV-ROLLOUT-NO-STAGE-SKIP), and produces [`TransitionResult`]
//! for each decision arm.

use super::error::RolloutError;
use super::types::{AutoRollbackTrigger, GateMetrics, NextAction, RolloutDecision, RolloutStage, RolloutStatus};

/// Result of a validated state-machine transition.
#[derive(Debug, Clone)]
pub struct TransitionResult {
    /// New handle status after this transition.
    pub new_status: RolloutStatus,
    /// New stage after this transition (may equal old stage on Hold).
    pub new_stage: RolloutStage,
    /// Recommended next action.
    pub next_action: NextAction,
}

/// State machine for progressive rollout.
///
/// All transition methods validate that the proposed transition is
/// legal before returning. Illegal transitions (e.g., stage bypass)
/// return [`RolloutError::StageBypassed`].
#[derive(Debug, Clone)]
pub struct RolloutStateMachine;

impl RolloutStateMachine {
    /// Create a new state machine validator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Evaluate gate metrics and produce a [`RolloutDecision`].
    ///
    /// - If any auto-rollback trigger is sustained → `NextAction::AutoRollback`.
    /// - If dwell not satisfied → `NextAction::Hold`.
    /// - If at terminal stage → `NextAction::Complete`.
    /// - Otherwise → `NextAction::Advance`.
    #[must_use]
    pub fn evaluate(
        &self,
        current_stage: RolloutStage,
        metrics: GateMetrics,
        rollback_trigger: Option<AutoRollbackTrigger>,
    ) -> RolloutDecision {
        let next_action = if let Some(trigger) = rollback_trigger {
            NextAction::AutoRollback(trigger)
        } else if !metrics.dwell_satisfied(current_stage) {
            NextAction::Hold(format!(
                "dwell not satisfied: {}s elapsed < {}s required",
                metrics.stage_elapsed_secs,
                u64::from(current_stage.dwell_minutes_min()) * 60,
            ))
        } else {
            match current_stage.next() {
                Some(next_stage) => NextAction::Advance(next_stage),
                None => NextAction::Complete,
            }
        };

        RolloutDecision {
            current_stage,
            next_action,
            gate_metrics: metrics,
        }
    }

    /// Validate that advancing from `current` to `proposed_next` is
    /// legal (i.e., `current.next() == Some(proposed_next)`).
    ///
    /// # Errors
    ///
    /// Returns [`RolloutError::StageBypassed`] if `proposed_next` is
    /// not the immediate next stage.
    pub fn validate_advance(
        &self,
        current: RolloutStage,
        proposed_next: RolloutStage,
    ) -> Result<(), RolloutError> {
        match current.next() {
            Some(expected) if expected == proposed_next => Ok(()),
            _ => Err(RolloutError::StageBypassed),
        }
    }
}

impl Default for RolloutStateMachine {
    fn default() -> Self {
        Self::new()
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

    fn metrics_gate_pass(stage: RolloutStage) -> GateMetrics {
        GateMetrics {
            error_rate: 0.001,
            error_rate_baseline: 0.01,
            error_rate_sigma: 0.001,
            slo_burn_rate_1h: 0.5,
            p99_latency_ms: 50.0,
            p99_baseline_ms: 60.0,
            stage_elapsed_secs: u64::from(stage.dwell_minutes_min()) * 60 + 1,
        }
    }

    fn metrics_gate_fail(stage: RolloutStage) -> GateMetrics {
        let mut m = metrics_gate_pass(stage);
        m.stage_elapsed_secs = 0; // dwell not satisfied
        m
    }

    #[test]
    fn advance_stage1_to_stage10() {
        let sm = RolloutStateMachine::new();
        let m = metrics_gate_pass(RolloutStage::Stage1Pct);
        let decision = sm.evaluate(RolloutStage::Stage1Pct, m, None);
        assert!(matches!(
            decision.next_action,
            NextAction::Advance(RolloutStage::Stage10Pct)
        ));
    }

    #[test]
    fn hold_when_dwell_not_satisfied() {
        let sm = RolloutStateMachine::new();
        let m = metrics_gate_fail(RolloutStage::Stage10Pct);
        let decision = sm.evaluate(RolloutStage::Stage10Pct, m, None);
        assert!(matches!(decision.next_action, NextAction::Hold(_)));
    }

    #[test]
    fn complete_at_terminal() {
        let sm = RolloutStateMachine::new();
        let m = metrics_gate_pass(RolloutStage::Stage100Pct);
        let decision = sm.evaluate(RolloutStage::Stage100Pct, m, None);
        assert!(matches!(decision.next_action, NextAction::Complete));
    }

    #[test]
    fn rollback_trigger_overrides_advance() {
        let sm = RolloutStateMachine::new();
        let m = metrics_gate_pass(RolloutStage::Stage50Pct);
        let trigger = Some(AutoRollbackTrigger::SloBurnRateExceeds14_4);
        let decision = sm.evaluate(RolloutStage::Stage50Pct, m, trigger);
        assert!(matches!(
            decision.next_action,
            NextAction::AutoRollback(AutoRollbackTrigger::SloBurnRateExceeds14_4)
        ));
    }

    #[test]
    fn validate_advance_legal() {
        let sm = RolloutStateMachine::new();
        sm.validate_advance(RolloutStage::Stage1Pct, RolloutStage::Stage10Pct)
            .unwrap();
        sm.validate_advance(RolloutStage::Stage10Pct, RolloutStage::Stage50Pct)
            .unwrap();
        sm.validate_advance(RolloutStage::Stage50Pct, RolloutStage::Stage100Pct)
            .unwrap();
    }

    #[test]
    fn validate_advance_bypass_rejected() {
        let sm = RolloutStateMachine::new();
        // Skip Stage10Pct → bypass
        let err = sm
            .validate_advance(RolloutStage::Stage1Pct, RolloutStage::Stage50Pct)
            .unwrap_err();
        assert!(matches!(err, RolloutError::StageBypassed));
    }

    #[test]
    fn validate_advance_terminal_rejected() {
        let sm = RolloutStateMachine::new();
        // Cannot advance from terminal
        let err = sm
            .validate_advance(RolloutStage::Stage100Pct, RolloutStage::Stage1Pct)
            .unwrap_err();
        assert!(matches!(err, RolloutError::StageBypassed));
    }
}
