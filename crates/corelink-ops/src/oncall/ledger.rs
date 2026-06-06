//! Oncall rotation ledger orchestrator.
//!
//! The ledger tracks rotations + page events per engineer, exposes
//! the canonical fatigue scores, and orchestrates the Lote 10.17
//! codex P1 fix decision matrix (soft alert vs HARD automatic
//! handoff). Every state mutation emits a `corelink.oncall.*` audit
//! record BEFORE the mutation, satisfying
//! `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`; audit failure aborts the
//! mutation + returns a typed error (fail-CLOSED).
//!
//! ## Concurrency model
//!
//! State is wrapped in `Arc<Mutex<>>` per Lote 10.6bis pattern. The
//! ledger is `Send + Sync` and may be shared across worker tasks;
//! the production wiring serialises mutations behind a Durable
//! Object so the mutex contention floor is bounded.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::audit::{OncallAuditEventType, OncallAuditRecord, OncallAuditSink};
use super::engineer::EngineerId;
use super::error::OncallError;
use super::page::{FatigueScore, FatigueWindow, PageEvent};
use super::pagerduty::{PagerDutyAssignment, PagerDutyClient, PagerDutyScheduleKey};
use super::rotation::{Rotation, Shift};
use super::severity::Severity;
use super::threshold::{decide_handoff, FatigueThreshold, HandoffDecision};
use super::tier::Tier;

/// Ledger outcome surface — what the caller learns after invoking
/// [`RotationLedger::evaluate_and_handoff_if_needed`]. Decision arm
/// drives downstream Grafana alert / Prometheus counter emit.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LedgerOutcome {
    /// Decision the ledger reached.
    pub decision: HandoffDecision,
    /// Threshold that fired (None when `decision == KeepCurrent`).
    pub threshold_fired: Option<FatigueThreshold>,
    /// PagerDuty assignment record (Some when the decision required
    /// schedule mutation and the PagerDuty client confirmed the
    /// handoff).
    pub assignment: Option<PagerDutyAssignment>,
}

/// Rotation ledger orchestrator.
#[derive(Clone, Debug)]
pub struct RotationLedger<A: OncallAuditSink, P: PagerDutyClient> {
    state: Arc<Mutex<LedgerState>>,
    audit: A,
    pagerduty: P,
}

#[derive(Debug, Default)]
struct LedgerState {
    rotations: HashMap<Tier, Rotation>,
    pages: HashMap<EngineerId, Vec<PageEvent>>,
}

impl<A: OncallAuditSink, P: PagerDutyClient> RotationLedger<A, P> {
    /// Construct a new ledger backed by `audit` + `pagerduty`.
    #[must_use]
    pub fn new(audit: A, pagerduty: P) -> Self {
        Self {
            state: Arc::new(Mutex::new(LedgerState::default())),
            audit,
            pagerduty,
        }
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, LedgerState>, OncallError> {
        self.state
            .lock()
            .map_err(|e| OncallError::Internal(format!("ledger mutex poisoned: {e}")))
    }

    /// Append a [`Shift`] to the appropriate rotation. Emits
    /// `corelink.oncall.shift_started` BEFORE the mutation.
    pub fn start_shift(
        &self,
        shift: Shift,
        correlation_id: impl Into<String>,
    ) -> Result<(), OncallError> {
        let correlation_id = correlation_id.into();
        // Veto: an engineer in their post-shift protection window
        // cannot be assigned a new shift on the same rotation.
        {
            let g = self.lock_state()?;
            if let Some(rot) = g.rotations.get(&shift.tier) {
                if rot.engineer_in_protection(&shift.engineer, shift.start_ms) {
                    return Err(OncallError::InvalidRotation(format!(
                        "engineer {} is within their 2-week protection window at ts {}",
                        shift.engineer, shift.start_ms
                    )));
                }
            }
        }

        // Audit BEFORE mutation.
        let rec = OncallAuditRecord::new(
            OncallAuditEventType::ShiftStarted,
            shift.engineer.clone(),
            shift.tier,
            shift.start_ms,
            correlation_id,
        );
        self.audit
            .emit(&rec)
            .map_err(|e| OncallError::Audit(e.to_string()))?;

        // Mutation.
        let mut g = self.lock_state()?;
        let rot = g
            .rotations
            .entry(shift.tier)
            .or_insert_with(|| Rotation::new(shift.tier));
        rot.try_append(shift)?;
        Ok(())
    }

    /// Record a [`PageEvent`]. Emits `corelink.oncall.page_recorded`
    /// BEFORE the mutation.
    pub fn record_page(&self, page: PageEvent) -> Result<(), OncallError> {
        let (eng, sev, ts, cid) = (
            page.engineer.clone(),
            page.severity,
            page.ts_ms,
            page.correlation_id.clone(),
        );
        // The page record audits regardless of severity (Sev3 still
        // bookkept for the dashboard); the threshold matrix
        // discriminates inside `evaluate_and_handoff_if_needed`.
        let _ = sev;
        let tier = self.tier_for_engineer(&eng, ts)?.unwrap_or(Tier::Tier1);
        let rec = OncallAuditRecord::new(
            OncallAuditEventType::PageRecorded,
            eng.clone(),
            tier,
            ts,
            cid,
        );
        self.audit
            .emit(&rec)
            .map_err(|e| OncallError::Audit(e.to_string()))?;

        let mut g = self.lock_state()?;
        g.pages.entry(eng).or_default().push(page);
        Ok(())
    }

    /// Tier the engineer was rostered into at `ts_ms`, if any.
    pub fn tier_for_engineer(
        &self,
        engineer: &EngineerId,
        ts_ms: u64,
    ) -> Result<Option<Tier>, OncallError> {
        let g = self.lock_state()?;
        for (tier, rot) in &g.rotations {
            for shift in &rot.shifts {
                if &shift.engineer == engineer && shift.covers(ts_ms) {
                    return Ok(Some(*tier));
                }
            }
        }
        Ok(None)
    }

    /// True if `engineer` was in an active shift at `ts_ms` on any
    /// tier.
    pub fn in_active_shift(&self, engineer: &EngineerId, ts_ms: u64) -> Result<bool, OncallError> {
        Ok(self.tier_for_engineer(engineer, ts_ms)?.is_some())
    }

    /// Count pages received by `engineer` in the trailing window
    /// ending at `as_of_ms`.
    pub fn fatigue_score(
        &self,
        engineer: &EngineerId,
        window: FatigueWindow,
        as_of_ms: u64,
    ) -> Result<FatigueScore, OncallError> {
        let g = self.lock_state()?;
        let pages = g.pages.get(engineer);
        let lower = as_of_ms.saturating_sub(window.duration_ms());
        let count: u64 = pages
            .map(|v| {
                v.iter()
                    .filter(|p| p.ts_ms >= lower && p.ts_ms <= as_of_ms)
                    .count() as u64
            })
            .unwrap_or(0);
        Ok(FatigueScore::new(count, window, as_of_ms))
    }

    /// Count Sev1 pages received during the active shift containing
    /// `as_of_ms` (returns 0 if the engineer is not on shift).
    pub fn sev1_per_shift(&self, engineer: &EngineerId, as_of_ms: u64) -> Result<u64, OncallError> {
        self.sev_per_shift(engineer, as_of_ms, Severity::Sev1)
    }

    /// Count Sev2 pages received during the active shift containing
    /// `as_of_ms` (returns 0 if the engineer is not on shift).
    pub fn sev2_per_shift(&self, engineer: &EngineerId, as_of_ms: u64) -> Result<u64, OncallError> {
        self.sev_per_shift(engineer, as_of_ms, Severity::Sev2)
    }

    fn sev_per_shift(
        &self,
        engineer: &EngineerId,
        as_of_ms: u64,
        sev: Severity,
    ) -> Result<u64, OncallError> {
        let g = self.lock_state()?;
        // Locate the active shift for the engineer at as_of_ms.
        let mut shift_bounds: Option<(u64, u64)> = None;
        for rot in g.rotations.values() {
            for s in &rot.shifts {
                if &s.engineer == engineer && s.covers(as_of_ms) {
                    shift_bounds = Some((s.start_ms, s.end_ms));
                }
            }
        }
        let (start, end) = match shift_bounds {
            Some(b) => b,
            None => return Ok(0),
        };
        let count: u64 = g
            .pages
            .get(engineer)
            .map(|v| {
                v.iter()
                    .filter(|p| p.severity == sev && p.ts_ms >= start && p.ts_ms < end)
                    .count() as u64
            })
            .unwrap_or(0);
        Ok(count)
    }

    /// Count page events received outside the engineer's active
    /// rotation window in the trailing 30d.
    pub fn pages_non_rotation_30d(
        &self,
        engineer: &EngineerId,
        as_of_ms: u64,
    ) -> Result<u64, OncallError> {
        let g = self.lock_state()?;
        let lower = as_of_ms.saturating_sub(FatigueWindow::Rolling30d.duration_ms());
        let pages = g.pages.get(engineer);
        let in_rotation = |ts: u64| -> bool {
            for rot in g.rotations.values() {
                for s in &rot.shifts {
                    if &s.engineer == engineer && s.covers(ts) {
                        return true;
                    }
                }
            }
            false
        };
        let count: u64 = pages
            .map(|v| {
                v.iter()
                    .filter(|p| p.ts_ms >= lower && p.ts_ms <= as_of_ms && !in_rotation(p.ts_ms))
                    .count() as u64
            })
            .unwrap_or(0);
        Ok(count)
    }

    /// Evaluate the canonical fatigue threshold matrix and, if a
    /// HARD threshold is breached, dispatch an automatic handoff via
    /// the PagerDuty client.
    ///
    /// Returns a [`LedgerOutcome`] describing the decision arm taken.
    ///
    /// Audit emit order:
    ///   1. `FatigueThresholdBreached` (when threshold != KeepCurrent)
    ///   2. `HandoffExecuted` (when `decision.requires_schedule_mutation()`)
    ///
    /// Each emit is fail-CLOSED.
    pub fn evaluate_and_handoff_if_needed(
        &self,
        engineer: &EngineerId,
        as_of_ms: u64,
        correlation_id: impl Into<String>,
    ) -> Result<LedgerOutcome, OncallError> {
        let correlation_id = correlation_id.into();
        let sev1 = self.sev1_per_shift(engineer, as_of_ms)?;
        let sev2 = self.sev2_per_shift(engineer, as_of_ms)?;
        let non_rot = self.pages_non_rotation_30d(engineer, as_of_ms)?;
        let decision = decide_handoff(sev1, sev2, non_rot);

        let threshold = match decision {
            HandoffDecision::KeepCurrent => None,
            HandoffDecision::SoftAlert => {
                if sev1 > FatigueThreshold::SoftSev1PerShift.trigger_count() {
                    Some(FatigueThreshold::SoftSev1PerShift)
                } else if sev2 > FatigueThreshold::SoftSev2PerShift.trigger_count() {
                    Some(FatigueThreshold::SoftSev2PerShift)
                } else {
                    Some(FatigueThreshold::SoftPagesPerMonthNonRotation)
                }
            }
            HandoffDecision::RotateToBackup => {
                if sev1 > FatigueThreshold::HardSev1PerShift.trigger_count() {
                    Some(FatigueThreshold::HardSev1PerShift)
                } else {
                    Some(FatigueThreshold::HardSev2PerShift)
                }
            }
            HandoffDecision::MandatoryRotationBlock => {
                Some(FatigueThreshold::HardPagesPerMonthNonRotation)
            }
        };

        let tier = self
            .tier_for_engineer(engineer, as_of_ms)?
            .unwrap_or(Tier::Tier1);

        if let Some(th) = threshold {
            let rec = OncallAuditRecord::new(
                OncallAuditEventType::FatigueThresholdBreached,
                engineer.clone(),
                tier,
                as_of_ms,
                correlation_id.clone(),
            )
            .with_threshold(th);
            self.audit
                .emit(&rec)
                .map_err(|e| OncallError::Audit(e.to_string()))?;
        }

        let assignment = if decision.requires_schedule_mutation() {
            let schedule = PagerDutyScheduleKey::from_tier(tier);
            let asn = self.pagerduty.handoff_to_backup(schedule, as_of_ms)?;
            let rec = OncallAuditRecord::new(
                OncallAuditEventType::HandoffExecuted,
                engineer.clone(),
                tier,
                as_of_ms,
                correlation_id,
            )
            .with_decision(decision);
            self.audit
                .emit(&rec)
                .map_err(|e| OncallError::Audit(e.to_string()))?;
            Some(asn)
        } else {
            None
        };

        Ok(LedgerOutcome {
            decision,
            threshold_fired: threshold,
            assignment,
        })
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
    use super::super::audit::{FailingOncallAuditSink, InMemoryOncallAuditSink};
    use super::super::pagerduty::{FailingPagerDutyClient, InMemoryPagerDutyClient};
    use super::super::rotation::ShiftId;
    use super::super::SHIFT_CAP_SECONDS;
    use super::*;

    fn eng(id: &str) -> EngineerId {
        EngineerId::new(id)
    }

    fn setup_pd(roster: Vec<EngineerId>) -> InMemoryPagerDutyClient {
        let pd = InMemoryPagerDutyClient::new();
        pd.seed_roster(PagerDutyScheduleKey::Tier1, roster).unwrap();
        pd
    }

    #[test]
    fn start_shift_emits_audit_then_mutates() {
        let audit = InMemoryOncallAuditSink::new();
        let pd = setup_pd(vec![eng("a"), eng("b")]);
        let ledger = RotationLedger::new(audit.clone(), pd);
        let shift = Shift::try_new(
            ShiftId::new("s1"),
            eng("a"),
            Tier::Tier1,
            0,
            SHIFT_CAP_SECONDS * 1000,
        )
        .unwrap();
        ledger.start_shift(shift, "cid-1").unwrap();
        let recs = audit.snapshot();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].event_type, OncallAuditEventType::ShiftStarted);
    }

    #[test]
    fn start_shift_audit_fail_closed() {
        let audit = FailingOncallAuditSink;
        let pd = setup_pd(vec![eng("a"), eng("b")]);
        let ledger = RotationLedger::new(audit, pd);
        let shift = Shift::try_new(
            ShiftId::new("s1"),
            eng("a"),
            Tier::Tier1,
            0,
            SHIFT_CAP_SECONDS * 1000,
        )
        .unwrap();
        let res = ledger.start_shift(shift, "cid-1");
        assert!(matches!(res, Err(OncallError::Audit(_))));
    }

    #[test]
    fn start_shift_vetoes_protection_window() {
        let audit = InMemoryOncallAuditSink::new();
        let pd = setup_pd(vec![eng("a"), eng("b")]);
        let ledger = RotationLedger::new(audit, pd);
        let s1 = Shift::try_new(
            ShiftId::new("s1"),
            eng("a"),
            Tier::Tier1,
            0,
            SHIFT_CAP_SECONDS * 1000,
        )
        .unwrap();
        ledger.start_shift(s1, "cid-1").unwrap();

        // Engineer "a" is now in protection window for 2 weeks; a
        // new shift at end+1d MUST be vetoed.
        let next_start = SHIFT_CAP_SECONDS * 1000 + 24 * 60 * 60 * 1000;
        let s2 = Shift::try_new(
            ShiftId::new("s2"),
            eng("a"),
            Tier::Tier1,
            next_start,
            next_start + SHIFT_CAP_SECONDS * 1000,
        )
        .unwrap();
        let err = ledger.start_shift(s2, "cid-2").unwrap_err();
        assert!(matches!(err, OncallError::InvalidRotation(_)));
    }

    #[test]
    fn keep_current_when_no_pages() {
        let audit = InMemoryOncallAuditSink::new();
        let pd = setup_pd(vec![eng("a"), eng("b")]);
        let ledger = RotationLedger::new(audit.clone(), pd);
        let shift = Shift::try_new(
            ShiftId::new("s1"),
            eng("a"),
            Tier::Tier1,
            0,
            SHIFT_CAP_SECONDS * 1000,
        )
        .unwrap();
        ledger.start_shift(shift, "cid-1").unwrap();

        let outcome = ledger
            .evaluate_and_handoff_if_needed(&eng("a"), 1000, "cid-eval")
            .unwrap();
        assert_eq!(outcome.decision, HandoffDecision::KeepCurrent);
        assert!(outcome.assignment.is_none());
    }

    #[test]
    fn hard_sev1_triggers_rotate_to_backup() {
        let audit = InMemoryOncallAuditSink::new();
        let pd = setup_pd(vec![eng("a"), eng("b")]);
        let ledger = RotationLedger::new(audit.clone(), pd);
        let shift = Shift::try_new(
            ShiftId::new("s1"),
            eng("a"),
            Tier::Tier1,
            0,
            SHIFT_CAP_SECONDS * 1000,
        )
        .unwrap();
        ledger.start_shift(shift, "cid-1").unwrap();

        // Record 4 Sev1 pages during the active shift (> hard threshold 3).
        for i in 0..4 {
            let p = PageEvent::new(
                eng("a"),
                Severity::Sev1,
                100 + (i as u64),
                format!("cid-page-{i}"),
            );
            ledger.record_page(p).unwrap();
        }

        let outcome = ledger
            .evaluate_and_handoff_if_needed(&eng("a"), 200, "cid-eval")
            .unwrap();
        assert_eq!(outcome.decision, HandoffDecision::RotateToBackup);
        assert_eq!(
            outcome.threshold_fired,
            Some(FatigueThreshold::HardSev1PerShift)
        );
        let asn = outcome.assignment.expect("rotate-to-backup must dispatch");
        assert_eq!(asn.engineer, eng("b"));

        // Audit chain: shift_started + 4 × page_recorded + threshold_breached + handoff_executed = 7.
        let recs = audit.snapshot();
        assert_eq!(recs.len(), 7);
        assert_eq!(
            recs[5].event_type,
            OncallAuditEventType::FatigueThresholdBreached
        );
        assert_eq!(recs[6].event_type, OncallAuditEventType::HandoffExecuted);
    }

    #[test]
    fn pagerduty_failure_propagates() {
        let audit = InMemoryOncallAuditSink::new();
        let pd = FailingPagerDutyClient;
        let ledger = RotationLedger::new(audit, pd);
        let shift = Shift::try_new(
            ShiftId::new("s1"),
            eng("a"),
            Tier::Tier1,
            0,
            SHIFT_CAP_SECONDS * 1000,
        )
        .unwrap();
        ledger.start_shift(shift, "cid-1").unwrap();
        for i in 0..4 {
            let p = PageEvent::new(eng("a"), Severity::Sev1, 100 + i, "cid-p");
            ledger.record_page(p).unwrap();
        }
        let err = ledger
            .evaluate_and_handoff_if_needed(&eng("a"), 200, "cid-eval")
            .unwrap_err();
        assert!(matches!(err, OncallError::PagerDuty(_)));
    }

    #[test]
    fn fatigue_score_window_boundary() {
        let audit = InMemoryOncallAuditSink::new();
        let pd = setup_pd(vec![eng("a"), eng("b")]);
        let ledger = RotationLedger::new(audit, pd);
        let shift = Shift::try_new(
            ShiftId::new("s1"),
            eng("a"),
            Tier::Tier1,
            0,
            SHIFT_CAP_SECONDS * 1000,
        )
        .unwrap();
        ledger.start_shift(shift, "cid-1").unwrap();
        for i in 0..3 {
            let p = PageEvent::new(eng("a"), Severity::Sev2, 100 + i, "cid-p");
            ledger.record_page(p).unwrap();
        }
        let score = ledger
            .fatigue_score(&eng("a"), FatigueWindow::Rolling7d, 200)
            .unwrap();
        assert_eq!(score.count, 3);
        assert_eq!(score.window, FatigueWindow::Rolling7d);
    }
}
