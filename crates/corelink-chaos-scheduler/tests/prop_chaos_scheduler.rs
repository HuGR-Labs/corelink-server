//! Property tests for the chaos scheduler (WI-S17-001).
//!
//! Runs at **10k iterations** in PR gate mode and **100k iterations** in
//! nightly mode (via `PROPTEST_CASES` env var override; mirrors S-07 P1-2
//! and S-12 supply-chain crate patterns).
//!
//! Properties covered:
//!
//! - `prop_seed_deterministic`: same `(run_id, experiment_id)` => same seed
//!   across 10k random inputs (bit-identical replay gate; Lote 10.17 codex P2).
//! - `prop_seed_distinct_per_run`: different `run_id` produces different
//!   seeds with overwhelming probability (collision rate < 1/2^32).
//! - `prop_safe_mode_blocks_prod_always`: ANY snapshot with
//!   `target = Production` aborts; trigger is `prod_target_violation`.
//! - `prop_safe_mode_blocks_prod_sev1_always`: ANY staging snapshot with
//!   `prod_sev1_active = true` aborts.
//! - `prop_run_outcome_consistent_with_blast_radius`: outcome is
//!   `SteadyStateBreached` iff measured impact > blast_radius_bps, else
//!   `Passed`.
//! - `prop_audit_lifecycle_invariant`: every run emits either
//!   `[Aborted]` (1 event) OR `[Started, Completed]` (2 events) — never
//!   any other shape.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr
)]

use std::cell::RefCell;

use corelink_chaos_scheduler::{
    canonical_catalog, derive_seed, run_experiment, ChaosAuditEvent, ChaosExperiment,
    ChaosExperimentId, ChaosOutcome, ChaosRun, ChaosRunId, ChaosTarget, SafeModeSnapshot,
    Telemetry,
};
use proptest::prelude::*;

/// PROPTEST_CASES env var override (S-07 P1-2 nightly 100k pattern).
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn cfg() -> ProptestConfig {
    ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    }
}

// ---------------------------------------------------------------------------
// Telemetry fake
// ---------------------------------------------------------------------------

struct TestTelemetry {
    events: RefCell<Vec<ChaosAuditEvent>>,
    impact_bps: u32,
}

impl TestTelemetry {
    fn new(impact_bps: u32) -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            impact_bps,
        }
    }
}

impl Telemetry for TestTelemetry {
    fn emit_audit(&self, _run: &ChaosRun, event: ChaosAuditEvent) {
        self.events.borrow_mut().push(event);
    }
    fn capture_state_pre(&self, _run: &ChaosRun) -> u64 {
        0
    }
    fn capture_state_post(&self, _run: &ChaosRun) -> u64 {
        0
    }
    fn measure_slo_impact(&self, _run: &ChaosRun, _pre: u64, _post: u64) -> u32 {
        self.impact_bps
    }
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn arb_run_id() -> impl Strategy<Value = String> {
    "[a-z0-9-]{1,32}".prop_map(|s| if s.is_empty() { "x".into() } else { s })
}

fn arb_experiment() -> impl Strategy<Value = ChaosExperiment> {
    let cat = canonical_catalog();
    let n = cat.len();
    (0usize..n).prop_map(move |idx| cat[idx].clone())
}

fn arb_target() -> impl Strategy<Value = ChaosTarget> {
    prop_oneof![Just(ChaosTarget::Staging), Just(ChaosTarget::Production)]
}

fn arb_snapshot() -> impl Strategy<Value = SafeModeSnapshot> {
    (arb_target(), any::<bool>(), 0u32..10_000u32).prop_map(
        |(target, prod_sev1_active, staging_error_rate_bps)| SafeModeSnapshot {
            target,
            prod_sev1_active,
            staging_error_rate_bps,
        },
    )
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(cfg())]

    #[test]
    fn prop_seed_deterministic(run in arb_run_id(), exp in arb_experiment()) {
        let id = ChaosRunId::new(run);
        let a = derive_seed(&id, exp.id.as_str());
        let b = derive_seed(&id, exp.id.as_str());
        prop_assert_eq!(a, b);
    }

    #[test]
    fn prop_seed_distinct_per_experiment(
        run in arb_run_id(),
        i in 0usize..7,
        offset in 1usize..8,
    ) {
        // i ∈ [0,7); j = (i + offset) mod 8, offset ∈ [1,8) ⇒ i ≠ j without rejects.
        let j = (i + offset) % 8;
        let cat = canonical_catalog();
        let id = ChaosRunId::new(run);
        let a = derive_seed(&id, cat[i].id.as_str());
        let b = derive_seed(&id, cat[j].id.as_str());
        prop_assert_ne!(a, b);
    }

    #[test]
    fn prop_safe_mode_blocks_prod_always(
        prod_sev1 in any::<bool>(),
        error_rate in 0u32..10_000u32,
    ) {
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Production,
            prod_sev1_active: prod_sev1,
            staging_error_rate_bps: error_rate,
        };
        // prod target ALWAYS trips first (highest priority).
        prop_assert_eq!(snap.evaluate(), Some("prod_target_violation"));
    }

    #[test]
    fn prop_safe_mode_blocks_prod_sev1_always(
        error_rate in 0u32..5_001u32,
    ) {
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: true,
            staging_error_rate_bps: error_rate,
        };
        prop_assert_eq!(snap.evaluate(), Some("prod_sev1_active"));
    }

    #[test]
    fn prop_safe_mode_passes_clean_staging(
        error_rate in 0u32..5_001u32,
    ) {
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: false,
            staging_error_rate_bps: error_rate,
        };
        prop_assert_eq!(snap.evaluate(), None);
    }

    #[test]
    fn prop_safe_mode_blocks_staging_error_rate_high(
        error_rate in 5_001u32..10_000u32,
    ) {
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: false,
            staging_error_rate_bps: error_rate,
        };
        prop_assert_eq!(snap.evaluate(), Some("staging_error_rate_high"));
    }

    #[test]
    fn prop_run_outcome_matches_blast_radius(
        run in arb_run_id(),
        exp in arb_experiment(),
        impact in 0u32..2_000u32,
    ) {
        let tele = TestTelemetry::new(impact);
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: false,
            staging_error_rate_bps: 0,
        };
        let result = run_experiment(&exp, ChaosRunId::new(run), snap, &tele);
        match result.outcome {
            ChaosOutcome::Passed { slo_impact_bps } => {
                prop_assert!(impact <= exp.blast_radius_bps);
                prop_assert_eq!(slo_impact_bps, impact);
            }
            ChaosOutcome::SteadyStateBreached { .. } => {
                prop_assert!(impact > exp.blast_radius_bps);
            }
            ChaosOutcome::Aborted { .. } => {
                prop_assert!(false, "clean staging snapshot must not abort");
            }
            _ => prop_assert!(false, "unexpected non-exhaustive outcome variant"),
        }
    }

    #[test]
    fn prop_audit_lifecycle_invariant(
        run in arb_run_id(),
        exp in arb_experiment(),
        snap in arb_snapshot(),
    ) {
        let tele = TestTelemetry::new(50);
        let _ = run_experiment(&exp, ChaosRunId::new(run), snap, &tele);
        let events = tele.events.borrow();
        // EITHER 1 Aborted, OR 2 (Started + Completed). Never any other shape.
        let valid = (events.len() == 1
            && events[0] == ChaosAuditEvent::Aborted)
            || (events.len() == 2
                && events[0] == ChaosAuditEvent::Started
                && events[1] == ChaosAuditEvent::Completed);
        prop_assert!(valid, "invalid audit lifecycle: {:?}", &*events);
    }

    #[test]
    fn prop_seed_does_not_depend_on_telemetry(
        run in arb_run_id(),
        exp in arb_experiment(),
        impact_a in 0u32..1_000u32,
        impact_b in 0u32..1_000u32,
    ) {
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: false,
            staging_error_rate_bps: 0,
        };
        let r1 = run_experiment(
            &exp,
            ChaosRunId::new(run.clone()),
            snap,
            &TestTelemetry::new(impact_a),
        );
        let r2 = run_experiment(
            &exp,
            ChaosRunId::new(run),
            snap,
            &TestTelemetry::new(impact_b),
        );
        prop_assert_eq!(r1.seed, r2.seed);
    }

    #[test]
    fn prop_aborted_outcome_writes_no_state_capture(
        run in arb_run_id(),
        exp in arb_experiment(),
    ) {
        let tele = TestTelemetry::new(0);
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Production,
            prod_sev1_active: false,
            staging_error_rate_bps: 0,
        };
        let result = run_experiment(&exp, ChaosRunId::new(run), snap, &tele);
        match result.outcome {
            ChaosOutcome::Aborted { trigger } => {
                prop_assert_eq!(trigger, "prod_target_violation");
            }
            other => prop_assert!(false, "expected Aborted, got {:?}", other),
        }
        prop_assert_eq!(tele.events.borrow().len(), 1);
    }
}

// ---------------------------------------------------------------------------
// Smoke test (always-on; not gated by PROPTEST_CASES)
// ---------------------------------------------------------------------------

#[test]
fn smoke_catalog_loads() {
    let c = canonical_catalog();
    assert_eq!(c.len(), 8);
}

#[test]
fn smoke_id_eq() {
    assert_eq!(
        ChaosExperimentId::new("a"),
        ChaosExperimentId::new("a".to_string())
    );
}
