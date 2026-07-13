//! Property tests pinning the load-bearing invariants of
//! `corelink-slo` at 10k iterations per check (PR-gate; nightly 100k
//! via `PROPTEST_CASES` env var override per S-07 P1-2 fix).
//!
//! Coverage map (mirrors WI-S09-006 §6.1.11 + §1 invariants):
//!
//! - `prop_burn_rate_calculation_correct` — for any (errors, total)
//!   with total > 0, `error_rate = errors/total`.
//! - `prop_alert_decision_canonical_table4` — at every multiplier ×
//!   error_budget_pct boundary the decision matches Google SRE
//!   Workbook Ch 5 Table 4 exactly.
//! - `prop_long_burn_does_not_page` — `Long3d × 1×` arm is always
//!   `TicketSev3` or `Quiet`, never any `Page*` arm.
//! - `prop_fast_burn_pages_immediately` — `Fast1h × 14.4×` arm above
//!   threshold is always `PageSev0`.
//! - `prop_tenant_isolation` — per-tenant ledger never leaks across
//!   tenants (INV-TENANT-ISOLATION canary).
//! - `prop_audit_emit_per_decision_arm` — every decision arm emits
//!   the canonical audit envelope (audit-emit-BEFORE-mutation).
//! - `prop_idempotent_no_burn` — `error_rate == 0.0` is always
//!   `Quiet` regardless of window or SLO.
//! - `prop_pagerduty_dispatch_idempotent_dedup_key` — repeated
//!   evaluation of the same `(sli, window, tenant)` page tuple
//!   collapses to a single open incident on the dispatcher side.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_slo::{
    canonical_alert_decisions, canonical_burn_rate_windows, canonical_pagerduty_actions,
    canonical_slis, canonical_slo_audit_event_strings, slo_schema_version, AlertDecision,
    BurnRateCalculator, BurnRateSample, BurnRateWindow, InMemoryPagerDutyDispatcher,
    InMemorySloAuditSink, MultiBurnRateAlert, PagerDutyServiceKey, Sli, SloAuditEventType,
    SloDefinition,
};
use proptest::prelude::*;
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha20Rng;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for PR gate; nightly job overrides to 100k.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

const ALL_SLIS: &[Sli] = &[
    Sli::AvailCasGet,
    Sli::AvailCasPut,
    Sli::AvailAcLookup,
    Sli::AvailAuth,
    Sli::LatencyCasGetP99,
    Sli::DedupRatio,
    Sli::RateLimitWithinQuota,
];

const ALL_WINDOWS: &[BurnRateWindow] = &[
    BurnRateWindow::Fast1h,
    BurnRateWindow::Medium6h,
    BurnRateWindow::Slow24h,
    BurnRateWindow::Long3d,
];

// ---------------------------------------------------------------------
// Canonical surface pinning (cheap; verifies the public API constants).
// ---------------------------------------------------------------------

#[test]
fn canonical_slis_count_pinned() {
    // Bumped 7 -> 12 by audit 2026-05-14 P0 closures
    // (`specs/_audits/sealed/2026-05-14-slo-instrumentation-gaps.md §5`):
    // +AvailControlPlane, +LatencyCasPutP99, +LatencyAcHitP99,
    // +CorrectnessCas, +CorrectnessTenantIsolation.
    //
    // Bumped 12 -> 17 by audit DR-16 wave-14 closures (pre-existing
    // DEBT-011-vintage gaps):
    // +BackupVerification, +ReplicationLagR2, +ReplicationLagD1,
    // +ReplicationLagKv, +ReplicationLagNeon.
    //
    // Bumped 17 -> 18 by audit
    // `specs/_audits/sealed/2026-05-15-dsr-worker-production.md §3`
    // (S-11 / WI-S11-002 SLI binding closure):
    // +FreshDsrErasure (`slo_catalog.md §4.12`).
    assert_eq!(canonical_slis().len(), 18);
}

#[test]
fn canonical_burn_rate_windows_count_pinned() {
    assert_eq!(canonical_burn_rate_windows().len(), 4);
}

#[test]
fn canonical_alert_decisions_count_pinned() {
    assert_eq!(canonical_alert_decisions().len(), 5);
}

#[test]
fn canonical_audit_event_strings_count_pinned() {
    assert_eq!(canonical_slo_audit_event_strings().len(), 5);
}

#[test]
fn canonical_pagerduty_actions_count_pinned() {
    assert_eq!(canonical_pagerduty_actions().len(), 3);
}

#[test]
fn slo_schema_version_pinned() {
    assert_eq!(slo_schema_version(), 1);
}

#[test]
fn canonical_table4_multipliers_pinned() {
    // Documents the canonical Google SRE Workbook Ch 5 Table 4
    // multipliers; falsifies any silent calibration drift.
    assert_eq!(BurnRateWindow::Fast1h.threshold_multiplier(), 14.4);
    assert_eq!(BurnRateWindow::Medium6h.threshold_multiplier(), 6.0);
    assert_eq!(BurnRateWindow::Slow24h.threshold_multiplier(), 3.0);
    assert_eq!(BurnRateWindow::Long3d.threshold_multiplier(), 1.0);
}

// ---------------------------------------------------------------------
// Property tests (10k iter PR gate; nightly 100k via PROPTEST_CASES).
// ---------------------------------------------------------------------

prop_compose! {
    fn arb_sli()(idx in 0usize..ALL_SLIS.len()) -> Sli {
        // Closed list; bounded index avoids panic.
        ALL_SLIS.get(idx).copied().unwrap_or(Sli::AvailCasGet)
    }
}

prop_compose! {
    fn arb_window()(idx in 0usize..ALL_WINDOWS.len()) -> BurnRateWindow {
        ALL_WINDOWS.get(idx).copied().unwrap_or(BurnRateWindow::Fast1h)
    }
}

prop_compose! {
    // Targets in the practical 90 % .. 99.999 % range (skip the
    // pathological 100 % zero-budget arm; that's a separate test).
    fn arb_target_pct()(t in 0.90f64..0.99999f64) -> f64 {
        t
    }
}

prop_compose! {
    // (errors, total) with total ≥ 1 + errors ≤ total — bounded so
    // the float ratio is always representable.
    fn arb_sample()(
        total in 1u64..1_000_000u64,
        errors in 0u64..1_000_000u64,
    ) -> BurnRateSample {
        let e = errors.min(total);
        BurnRateSample::new(e, total)
    }
}

#[test]
fn prop_burn_rate_calculation_correct() {
    let cfg = ProptestConfig::with_cases(proptest_cases());
    proptest!(cfg, |(s in arb_sample())| {
        let c = BurnRateCalculator::new();
        let r = c.error_rate_in_window(s);
        let valid = if s.total_in_window == 0 {
            r == 0.0
        } else {
            let expected = (s.errors_in_window as f64) / (s.total_in_window as f64);
            (r - expected).abs() < 1e-12
        };
        prop_assert!(valid, "burn rate ratio mismatch: errors={}/total={} got={}",
            s.errors_in_window, s.total_in_window, r);
    });
}

#[test]
fn prop_alert_decision_canonical_table4() {
    let cfg = ProptestConfig::with_cases(proptest_cases());
    proptest!(cfg, |(
        sli in arb_sli(),
        window in arb_window(),
        target in arb_target_pct(),
        s in arb_sample(),
    )| {
        let slo = SloDefinition::new(sli, target).unwrap();
        let c = BurnRateCalculator::new();
        let decision = c.decide(slo, window, s);
        let r = c.error_rate_in_window(s);
        let threshold = window.threshold_multiplier() * slo.error_budget_pct;

        let expected_arm = if r <= 0.0 || !r.is_finite() || r < threshold {
            AlertDecision::Quiet
        } else if window == BurnRateWindow::Fast1h {
            AlertDecision::PageSev0
        } else if window == BurnRateWindow::Medium6h {
            AlertDecision::PageSev1
        } else if window == BurnRateWindow::Slow24h {
            AlertDecision::TicketSev2
        } else {
            AlertDecision::TicketSev3
        };

        let valid = decision == expected_arm;
        prop_assert!(valid,
            "Table 4 mismatch: sli={} window={} target={} sample=({},{}) r={} thr={} got={:?} expected={:?}",
            sli, window, target, s.errors_in_window, s.total_in_window,
            r, threshold, decision, expected_arm);
    });
}

#[test]
fn prop_long_burn_does_not_page() {
    let cfg = ProptestConfig::with_cases(proptest_cases());
    proptest!(cfg, |(
        sli in arb_sli(),
        target in arb_target_pct(),
        s in arb_sample(),
    )| {
        let slo = SloDefinition::new(sli, target).unwrap();
        let c = BurnRateCalculator::new();
        let decision = c.decide(slo, BurnRateWindow::Long3d, s);
        let valid = matches!(decision, AlertDecision::Quiet | AlertDecision::TicketSev3);
        prop_assert!(valid,
            "Long3d MUST NOT page: sli={} target={} sample=({},{}) got={:?}",
            sli, target, s.errors_in_window, s.total_in_window, decision);
    });
}

#[test]
fn prop_fast_burn_pages_immediately() {
    let cfg = ProptestConfig::with_cases(proptest_cases());
    proptest!(cfg, |(
        sli in arb_sli(),
        target in arb_target_pct(),
        s in arb_sample(),
    )| {
        let slo = SloDefinition::new(sli, target).unwrap();
        let c = BurnRateCalculator::new();
        let decision = c.decide(slo, BurnRateWindow::Fast1h, s);
        // Either Quiet (below threshold) OR PageSev0 (above). Never
        // any other arm.
        let valid = matches!(decision, AlertDecision::Quiet | AlertDecision::PageSev0);
        prop_assert!(valid,
            "Fast1h MUST be Quiet or PageSev0: sli={} target={} sample=({},{}) got={:?}",
            sli, target, s.errors_in_window, s.total_in_window, decision);

        // When the burn is unambiguously above threshold, the arm
        // MUST be PageSev0.
        let r = c.error_rate_in_window(s);
        let threshold = BurnRateWindow::Fast1h.threshold_multiplier() * slo.error_budget_pct;
        if r > threshold * 1.01 {
            let pages = decision == AlertDecision::PageSev0;
            prop_assert!(pages, "Fast1h above threshold MUST PageSev0: r={} thr={} got={:?}",
                r, threshold, decision);
        }
    });
}

#[test]
fn prop_idempotent_no_burn() {
    let cfg = ProptestConfig::with_cases(proptest_cases());
    proptest!(cfg, |(
        sli in arb_sli(),
        window in arb_window(),
        target in arb_target_pct(),
        total in 0u64..10_000_000u64,
    )| {
        let slo = SloDefinition::new(sli, target).unwrap();
        let c = BurnRateCalculator::new();
        let s = BurnRateSample::new(0, total);
        let decision = c.decide(slo, window, s);
        let valid = decision == AlertDecision::Quiet;
        prop_assert!(valid,
            "no burn MUST be Quiet: sli={} window={} target={} total={} got={:?}",
            sli, window, target, total, decision);
    });
}

#[test]
fn prop_tenant_isolation() {
    // Tenant A page evaluations MUST NOT inflate tenant B's
    // evaluation count + MUST NOT collapse the dispatcher dedup key
    // across tenants.
    let cfg = ProptestConfig::with_cases(proptest_cases());
    proptest!(cfg, |(seed in any::<u64>())| {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let audit = Arc::new(InMemorySloAuditSink::new());
        let dispatcher = Arc::new(InMemoryPagerDutyDispatcher::new());
        let alert = MultiBurnRateAlert::new(
            Arc::clone(&audit),
            Arc::clone(&dispatcher),
            PagerDutyServiceKey::ProdUs,
        );
        let slo = SloDefinition::new(Sli::AvailCasGet, 0.999).unwrap();
        let count_a: u32 = rng.random_range(1u32..10);
        let count_b: u32 = rng.random_range(1u32..10);
        // Sample at 5 % > 1.44 % threshold → PageSev0.
        let s = BurnRateSample::new(50, 1_000);
        for _ in 0..count_a {
            alert.evaluate(slo, BurnRateWindow::Fast1h, "tA", s, 1).unwrap();
        }
        for _ in 0..count_b {
            alert.evaluate(slo, BurnRateWindow::Fast1h, "tB", s, 1).unwrap();
        }
        let isolated = alert.tenant_evaluation_count("tA") == count_a as u64
            && alert.tenant_evaluation_count("tB") == count_b as u64;
        prop_assert!(isolated, "tenant ledger leak: A={} B={} expected=({},{})",
            alert.tenant_evaluation_count("tA"),
            alert.tenant_evaluation_count("tB"),
            count_a, count_b);
        // 2 incidents (1 per tenant) regardless of attempt count.
        let dedup_correct = dispatcher.open_incident_count() == 2;
        prop_assert!(dedup_correct,
            "expected exactly 2 open incidents (one per tenant); got {}",
            dispatcher.open_incident_count());
    });
}

#[test]
fn prop_audit_emit_per_decision_arm() {
    // Every evaluation emits the canonical envelope:
    // - quiet: 2 audits (BurnRateEvaluated + AlertQuiet).
    // - ticket: 3 audits (BurnRateEvaluated + AlertFired + TicketFiled).
    // - page: 3 audits (BurnRateEvaluated + AlertFired + PageDispatched).
    let cfg = ProptestConfig::with_cases(proptest_cases());
    proptest!(cfg, |(
        window in arb_window(),
        s in arb_sample(),
        seed in any::<u64>(),
    )| {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let audit = Arc::new(InMemorySloAuditSink::new());
        let dispatcher = Arc::new(InMemoryPagerDutyDispatcher::new());
        let alert = MultiBurnRateAlert::new(
            Arc::clone(&audit),
            Arc::clone(&dispatcher),
            PagerDutyServiceKey::ProdUs,
        );
        let target: f64 = 0.99 + rng.random_range(0.0f64..0.0099f64);
        let slo = SloDefinition::new(Sli::AvailCasGet, target).unwrap();
        let outcome = alert
            .evaluate(slo, window, "tenant", s, 1)
            .unwrap();

        let snap = audit.snapshot();
        let expected_count = if outcome.decision.is_quiet() {
            2
        } else {
            3
        };
        let count_correct = snap.len() == expected_count;
        prop_assert!(count_correct, "expected {} audit rows; got {}",
            expected_count, snap.len());

        let first_is_evaluated = snap.first().map(|r| r.event_type)
            == Some(SloAuditEventType::BurnRateEvaluated);
        prop_assert!(first_is_evaluated, "first audit MUST be BurnRateEvaluated");

        if outcome.decision.is_quiet() {
            let second_is_quiet = snap.get(1).map(|r| r.event_type)
                == Some(SloAuditEventType::AlertQuiet);
            prop_assert!(second_is_quiet, "quiet path MUST emit AlertQuiet");
        } else {
            let second_is_fired = snap.get(1).map(|r| r.event_type)
                == Some(SloAuditEventType::AlertFired);
            prop_assert!(second_is_fired, "non-quiet path MUST emit AlertFired");
            let third = snap.get(2).map(|r| r.event_type);
            if outcome.decision.is_page() {
                let valid = third == Some(SloAuditEventType::PageDispatched);
                prop_assert!(valid, "page path MUST emit PageDispatched; got {:?}", third);
            } else {
                let valid = third == Some(SloAuditEventType::TicketFiled);
                prop_assert!(valid, "ticket path MUST emit TicketFiled; got {:?}", third);
            }
        }
    });
}

#[test]
fn prop_pagerduty_dispatch_idempotent_dedup_key() {
    // Repeated page evaluation under the same (sli, window, tenant)
    // tuple MUST collapse to a single open incident on the
    // dispatcher side per PagerDuty Events API v2 §dedup_key.
    let cfg = ProptestConfig::with_cases(proptest_cases());
    proptest!(cfg, |(
        repeats in 1u32..20,
        seed in any::<u64>(),
    )| {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let audit = Arc::new(InMemorySloAuditSink::new());
        let dispatcher = Arc::new(InMemoryPagerDutyDispatcher::new());
        let alert = MultiBurnRateAlert::new(
            Arc::clone(&audit),
            Arc::clone(&dispatcher),
            PagerDutyServiceKey::ProdUs,
        );
        let slo = SloDefinition::new(Sli::AvailCasGet, 0.999).unwrap();
        // Sample chosen well above the Fast1h × 14.4 × 0.001 = 1.44%
        // threshold to keep every iteration in the PageSev0 arm.
        let denom: u64 = 1_000 + (rng.random_range(0u64..100));
        let numerator: u64 = denom * 5 / 100;
        let s = BurnRateSample::new(numerator, denom);
        for _ in 0..repeats {
            alert.evaluate(slo, BurnRateWindow::Fast1h, "tenant-x", s, 1).unwrap();
        }
        let attempts_correct =
            dispatcher.attempt_count() as u32 == repeats;
        prop_assert!(attempts_correct,
            "expected {} dispatch attempts; got {}",
            repeats, dispatcher.attempt_count());
        let single_incident = dispatcher.open_incident_count() == 1;
        prop_assert!(single_incident,
            "expected exactly 1 open incident; got {}",
            dispatcher.open_incident_count());
    });
}
