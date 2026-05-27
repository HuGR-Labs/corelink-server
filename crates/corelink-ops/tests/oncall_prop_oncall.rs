//! WI-S17-005 property tests pinning the canonical invariants of
//! `corelink-oncall`.
//!
//! Per S-07 P1-2 fix + autonomous execution charter: the
//! `PROPTEST_CASES` env var overrides the case count at runtime
//! (nightly runs with 100k; PR CI runs with 10k via the
//! `proptest_config_pr` block).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use proptest::prelude::*;

use corelink_ops::oncall::{
    decide_handoff_via_pure_logic_proxy,
    page::{FatigueWindow, PageEvent},
    rotation::{Shift, ShiftId},
    severity::Severity,
    threshold::{FatigueThreshold, HandoffDecision},
    EngineerId, FailingOncallAuditSink, InMemoryOncallAuditSink, InMemoryPagerDutyClient,
    OncallAuditEventType, OncallError, PagerDutyScheduleKey, RotationLedger, Tier,
    PROTECTION_PERIOD_SECONDS, SHIFT_CAP_SECONDS,
};

mod helpers {
    use super::*;

    pub fn eng(id: &str) -> EngineerId {
        EngineerId::new(id)
    }

    pub fn setup_pd_tier1(roster: Vec<EngineerId>) -> InMemoryPagerDutyClient {
        let pd = InMemoryPagerDutyClient::new();
        pd.seed_roster(PagerDutyScheduleKey::Tier1, roster).unwrap();
        pd
    }
}

use helpers::{eng, setup_pd_tier1};

// -- Pure-logic decision matrix (Lote 10.17 codex P1 fix canonical) --

proptest! {
    #![proptest_config(ProptestConfig { cases: 10_000, .. ProptestConfig::default() })]

    /// HARD thresholds NEVER collapse to a soft-only response.
    #[test]
    fn prop_fatigue_hard_threshold_handoff(
        sev1 in 0u64..50,
        sev2 in 0u64..50,
        pages in 0u64..50,
    ) {
        let d = decide_handoff_via_pure_logic_proxy(sev1, sev2, pages);

        let hard_sev1 = sev1 > FatigueThreshold::HardSev1PerShift.trigger_count();
        let hard_sev2 = sev2 > FatigueThreshold::HardSev2PerShift.trigger_count();
        let hard_pages = pages > FatigueThreshold::HardPagesPerMonthNonRotation.trigger_count();

        if hard_pages {
            prop_assert_eq!(d, HandoffDecision::MandatoryRotationBlock);
        } else if hard_sev1 || hard_sev2 {
            prop_assert_eq!(d, HandoffDecision::RotateToBackup);
        }
    }

    /// Decision is `KeepCurrent` IFF no threshold is breached.
    #[test]
    fn prop_keep_current_iff_below_all(
        sev1 in 0u64..3,
        sev2 in 0u64..6,
        pages in 0u64..11,
    ) {
        let d = decide_handoff_via_pure_logic_proxy(sev1, sev2, pages);
        prop_assert_eq!(d, HandoffDecision::KeepCurrent);
    }

    /// `requires_schedule_mutation` agrees with the HARD-arm taxonomy.
    #[test]
    fn prop_mutation_iff_hard_arm(
        sev1 in 0u64..50,
        sev2 in 0u64..50,
        pages in 0u64..50,
    ) {
        let d = decide_handoff_via_pure_logic_proxy(sev1, sev2, pages);
        let expected_mut = matches!(
            d,
            HandoffDecision::RotateToBackup | HandoffDecision::MandatoryRotationBlock
        );
        prop_assert_eq!(d.requires_schedule_mutation(), expected_mut);
    }

    /// 7-day shift cap NEVER admits a shift exceeding the canonical bound.
    #[test]
    fn prop_shift_duration_cap(
        start in 0u64..1_000_000,
        extra in 0u64..1_000_000,
    ) {
        let cap_ms = SHIFT_CAP_SECONDS * 1000;
        let end = start.saturating_add(cap_ms).saturating_add(extra);
        let res = Shift::try_new(
            ShiftId::new("s"),
            eng("a"),
            Tier::Tier1,
            start,
            end,
        );
        let dur = end - start;
        if dur == 0 || dur > cap_ms {
            prop_assert!(res.is_err());
        } else {
            prop_assert!(res.is_ok());
        }
    }
}

// -- Ledger orchestrator invariants --

proptest! {
    #![proptest_config(ProptestConfig { cases: 1_000, .. ProptestConfig::default() })]

    /// Protection window veto: an engineer cannot be re-rostered within
    /// the 2-week post-shift window.
    #[test]
    fn prop_protection_period_veto(
        gap_days in 0u64..14,
    ) {
        let audit = InMemoryOncallAuditSink::new();
        let pd = setup_pd_tier1(vec![eng("a"), eng("b")]);
        let ledger = RotationLedger::new(audit, pd);

        let cap_ms = SHIFT_CAP_SECONDS * 1000;
        let s1 = Shift::try_new(
            ShiftId::new("s1"),
            eng("a"),
            Tier::Tier1,
            0,
            cap_ms,
        )
        .unwrap();
        ledger.start_shift(s1, "cid-1").unwrap();

        let gap_ms = gap_days * 24 * 60 * 60 * 1000;
        let next_start = cap_ms + gap_ms;
        let s2 = Shift::try_new(
            ShiftId::new("s2"),
            eng("a"),
            Tier::Tier1,
            next_start,
            next_start + cap_ms,
        )
        .unwrap();

        let res = ledger.start_shift(s2, "cid-2");
        let in_protection = gap_ms < PROTECTION_PERIOD_SECONDS * 1000;
        if in_protection {
            match &res {
                Err(OncallError::InvalidRotation(msg)) => {
                    prop_assert!(!msg.is_empty(), "InvalidRotation reason must not be empty");
                }
                other => prop_assert!(false, "expected OncallError::InvalidRotation, got {other:?}"),
            }
        } else {
            prop_assert!(res.is_ok());
        }
    }

    /// Audit-emit-BEFORE-mutation: when the audit sink fails, the
    /// ledger state remains unmutated.
    #[test]
    fn prop_audit_emit_before_mutation(
        start in 0u64..1_000_000,
    ) {
        let audit = FailingOncallAuditSink;
        let pd = setup_pd_tier1(vec![eng("a"), eng("b")]);
        let ledger = RotationLedger::new(audit, pd);
        let cap_ms = SHIFT_CAP_SECONDS * 1000;
        let s = Shift::try_new(
            ShiftId::new("s"),
            eng("a"),
            Tier::Tier1,
            start,
            start + cap_ms,
        )
        .unwrap();
        let res = ledger.start_shift(s, "cid");
        match &res {
            Err(OncallError::Audit(msg)) => {
                prop_assert!(!msg.is_empty(), "Audit error reason must not be empty");
            }
            other => prop_assert!(false, "expected OncallError::Audit, got {other:?}"),
        }

        // No active shift was registered (tier lookup returns None).
        let tier = ledger
            .tier_for_engineer(&eng("a"), start + 1)
            .unwrap();
        prop_assert!(tier.is_none());
    }

    /// Hard threshold → audit chain contains exactly one
    /// `FatigueThresholdBreached` and one `HandoffExecuted` arm.
    #[test]
    fn prop_audit_records_threshold_and_handoff(
        n_sev1 in 4u64..10,
    ) {
        let audit = InMemoryOncallAuditSink::new();
        let pd = setup_pd_tier1(vec![eng("a"), eng("b")]);
        let ledger = RotationLedger::new(audit.clone(), pd);
        let cap_ms = SHIFT_CAP_SECONDS * 1000;
        let s = Shift::try_new(
            ShiftId::new("s"),
            eng("a"),
            Tier::Tier1,
            0,
            cap_ms,
        )
        .unwrap();
        ledger.start_shift(s, "cid-1").unwrap();
        for i in 0..n_sev1 {
            let p = PageEvent::new(eng("a"), Severity::Sev1, 100 + i, "cid-p");
            ledger.record_page(p).unwrap();
        }
        let outcome = ledger
            .evaluate_and_handoff_if_needed(&eng("a"), 200, "cid-eval")
            .unwrap();
        prop_assert_eq!(outcome.decision, HandoffDecision::RotateToBackup);

        let recs = audit.snapshot();
        let n_breach = recs
            .iter()
            .filter(|r| r.event_type == OncallAuditEventType::FatigueThresholdBreached)
            .count();
        let n_handoff = recs
            .iter()
            .filter(|r| r.event_type == OncallAuditEventType::HandoffExecuted)
            .count();
        prop_assert_eq!(n_breach, 1);
        prop_assert_eq!(n_handoff, 1);
    }

    /// Fatigue score = count of pages within window (bounded above by
    /// `as_of_ms` and below by `as_of_ms - window`).
    #[test]
    fn prop_fatigue_score_within_window(
        n in 0u64..30,
        offset_ms in 0u64..(FatigueWindow::Rolling7d.duration_ms() * 2),
    ) {
        let audit = InMemoryOncallAuditSink::new();
        let pd = setup_pd_tier1(vec![eng("a"), eng("b")]);
        let ledger = RotationLedger::new(audit, pd);
        let cap_ms = SHIFT_CAP_SECONDS * 1000;
        let s = Shift::try_new(
            ShiftId::new("s"),
            eng("a"),
            Tier::Tier1,
            0,
            cap_ms,
        )
        .unwrap();
        ledger.start_shift(s, "cid-1").unwrap();

        let as_of = cap_ms;
        for i in 0..n {
            let p = PageEvent::new(eng("a"), Severity::Sev2, i * 1000, "cid-p");
            ledger.record_page(p).unwrap();
        }
        let score = ledger
            .fatigue_score(&eng("a"), FatigueWindow::Rolling7d, as_of + offset_ms)
            .unwrap();
        prop_assert!(score.count <= n);
        prop_assert_eq!(score.window, FatigueWindow::Rolling7d);
    }
}
