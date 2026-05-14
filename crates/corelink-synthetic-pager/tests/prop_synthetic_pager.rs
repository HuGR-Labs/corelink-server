//! WI-S20-006 property tests pinning the canonical invariants of
//! `corelink-synthetic-pager`.
//!
//! Per S-07 P1-2 fix + autonomous execution charter: the
//! `PROPTEST_CASES` env var overrides the case count at runtime
//! (nightly runs with 100k; PR CI runs with 10k via the per-block
//! `ProptestConfig`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use proptest::prelude::*;

use corelink_synthetic_pager::{
    decide_drill_outcome, AckOutcome, AckVector, DrillRecord, DrillRecorder,
    FailingDrillRecorder, InMemoryDrillRecorder, MttaMs, Region, SyntheticDrillError,
    SyntheticDrillId, MTTA_BUDGET_MS, UNACK_HARD_WINDOW_MS,
};

/// Resolve PROPTEST_CASES at runtime per charter constraint #15 — the
/// canonical reference is `corelink-lighthouse-tracker/src/lib.rs:984`.
/// Nightly CI sets `PROPTEST_CASES=100000`; PR CI defaults to the per-block
/// fallback (10_000 or 1_000).
fn proptest_cases(default: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases(10_000)))]

    /// MTTA budget cap is enforced: any ack with `mtta <=
    /// MTTA_BUDGET_MS` is `Acked`; any ack `mtta > MTTA_BUDGET_MS` is
    /// `Escalated`. No silent collapse to Acked above budget.
    #[test]
    fn prop_mtta_budget_cap_enforced(
        emit in 0_i64..1_000_000,
        mtta in 0_i64..(UNACK_HARD_WINDOW_MS * 4),
    ) {
        let ack = emit.saturating_add(mtta);
        let now = ack.saturating_add(1);
        let (outcome, m) = decide_drill_outcome(emit, Some(ack), now).unwrap();
        let measured = m.unwrap().as_ms();
        prop_assert_eq!(measured, mtta);
        if mtta <= MTTA_BUDGET_MS {
            prop_assert_eq!(outcome, AckOutcome::Acked);
        } else {
            prop_assert_eq!(outcome, AckOutcome::Escalated);
        }
    }

    /// 15-minute hard unack window: when no ack and `now - emit >=
    /// UNACK_HARD_WINDOW_MS`, the outcome is `Unacked`. When the
    /// window has NOT elapsed and there is no ack, the decider returns
    /// `Internal` (premature classification).
    #[test]
    fn prop_unack_hard_window(
        emit in 0_i64..1_000_000,
        elapsed in 0_i64..(UNACK_HARD_WINDOW_MS * 4),
    ) {
        let now = emit.saturating_add(elapsed);
        let r = decide_drill_outcome(emit, None, now);
        if elapsed < UNACK_HARD_WINDOW_MS {
            prop_assert!(matches!(r, Err(SyntheticDrillError::Internal(_))));
        } else {
            let (outcome, m) = r.unwrap();
            prop_assert_eq!(outcome, AckOutcome::Unacked);
            prop_assert!(m.is_none());
        }
    }

    /// Ack timestamp MUST be `>= emit_ts_ms`. Ack-before-emit is
    /// rejected (would yield negative MTTA).
    #[test]
    fn prop_ack_after_emit(
        emit in 1_i64..1_000_000,
        offset in 1_i64..1_000_000,
    ) {
        // ack BEFORE emit
        let ack = emit.saturating_sub(offset);
        let now = emit.saturating_add(offset);
        let r = decide_drill_outcome(emit, Some(ack), now);
        let is_ack_before = matches!(r, Err(SyntheticDrillError::AckBeforeEmit { .. }));
        prop_assert!(is_ack_before);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases(1_000)))]

    /// In-memory recorder: per-region `latest_for_region` always
    /// returns the drill with the maximum `emit_ts_ms` for that
    /// region.
    #[test]
    fn prop_inmemory_recorder_latest_per_region(
        emits in proptest::collection::vec(0_i64..1_000_000, 1..32),
    ) {
        let rec = InMemoryDrillRecorder::new();
        let region = Region::Americas;
        for (i, e) in emits.iter().enumerate() {
            let id = SyntheticDrillId::new(format!("SP-W{i}")).unwrap();
            let ack = e.saturating_add(60_000);
            let r = DrillRecord::new(
                id,
                region,
                "eng-001",
                *e,
                Some(ack),
                Some(AckVector::MobilePush),
                Some(MttaMs::from_diff(*e, ack)),
                AckOutcome::Acked,
                "corr",
            )
            .unwrap();
            rec.record(&r).unwrap();
        }
        let latest = rec.latest_for_region(region).unwrap().unwrap();
        let max = *emits.iter().max().unwrap();
        prop_assert_eq!(latest.emit_ts_ms, max);
    }

    /// Failing recorder: every `record` call surfaces a `Recorder`
    /// error (fail-CLOSED — never collapses to silent success).
    #[test]
    fn prop_failing_recorder_fail_closed(seed in 0u32..1_000) {
        let f = FailingDrillRecorder;
        let id = SyntheticDrillId::new(format!("SP-X{seed}")).unwrap();
        let r = DrillRecord::new(
            id,
            Region::Emea,
            "eng-002",
            1_000,
            Some(2_000),
            Some(AckVector::Sms),
            Some(MttaMs::from_diff(1_000, 2_000)),
            AckOutcome::Acked,
            "corr",
        )
        .unwrap();
        let res = f.record(&r);
        prop_assert!(matches!(res, Err(SyntheticDrillError::Recorder(_))));
    }

    /// Region UTC-hour mapping is total: every hour 0..=23 maps to
    /// exactly one canonical region and the 3 regions cover the full
    /// 24h cycle disjointly.
    #[test]
    fn prop_region_hour_coverage_total(h in 0u8..24) {
        let r = Region::for_utc_hour(h);
        match h {
            0..=7 => prop_assert_eq!(r, Region::Emea),
            8..=15 => prop_assert_eq!(r, Region::Apac),
            _ => prop_assert_eq!(r, Region::Americas),
        }
    }
}
