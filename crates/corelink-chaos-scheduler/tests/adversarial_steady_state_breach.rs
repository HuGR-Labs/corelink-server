//! Adversarial scenarios for the chaos scheduler (WI-S17-001 §15).
//!
//! Each test models one of the 5+ adversarial scenarios mandated by the
//! WI spec test plan §15 and exercises the runner against a malicious /
//! drifted environment.
//!
//! Scenarios covered:
//!
//! 1. **Chaos config flag set to production (HARD env check catches)** —
//!    `adversarial_prod_target_caught_first_line`.
//! 2. **Prod SEV-1 active during cron fire (chaos aborts)** —
//!    `adversarial_prod_sev1_active_aborts`.
//! 3. **Staging error rate spike > 50% (chaos aborts)** —
//!    `adversarial_staging_error_rate_high_aborts`.
//! 4. **Steady-state hypothesis breach (auto-rollback fires)** —
//!    `adversarial_steady_state_breach_detects_drift`.
//! 5. **Replay attack with stale seed (same seed must yield same outcome)** —
//!    `adversarial_replay_deterministic`.
//! 6. **Catalog drift — unknown experiment id** —
//!    `adversarial_catalog_lookup_unknown_returns_none`.

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
    canonical_catalog, lookup, run_experiment, ChaosAuditEvent, ChaosOutcome, ChaosRun,
    ChaosRunId, ChaosTarget, SafeModeSnapshot, Telemetry,
};

struct Recorder {
    events: RefCell<Vec<ChaosAuditEvent>>,
    impact_bps: u32,
}

impl Recorder {
    fn new(impact_bps: u32) -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            impact_bps,
        }
    }
}

impl Telemetry for Recorder {
    fn emit_audit(&self, _run: &ChaosRun, event: ChaosAuditEvent) {
        self.events.borrow_mut().push(event);
    }
    fn capture_state_pre(&self, _: &ChaosRun) -> u64 {
        0xa
    }
    fn capture_state_post(&self, _: &ChaosRun) -> u64 {
        0xb
    }
    fn measure_slo_impact(&self, _: &ChaosRun, _: u64, _: u64) -> u32 {
        self.impact_bps
    }
}

// ---------------------------------------------------------------------------
// Scenario 1 — prod target caught at first line
// ---------------------------------------------------------------------------

#[test]
fn adversarial_prod_target_caught_first_line() {
    let cat = canonical_catalog();
    let exp = cat[0].clone();
    let tele = Recorder::new(0);
    let snap = SafeModeSnapshot {
        // Misconfig — chaos targeted production.
        target: ChaosTarget::Production,
        prod_sev1_active: false,
        staging_error_rate_bps: 0,
    };
    let run = run_experiment(&exp, ChaosRunId::new("adv-1"), snap, &tele);
    match run.outcome {
        ChaosOutcome::Aborted { trigger } => assert_eq!(trigger, "prod_target_violation"),
        other => panic!("expected prod_target_violation abort, got {other:?}"),
    }
    // NO state-capture work happened — only the Aborted audit event.
    assert_eq!(tele.events.borrow().len(), 1);
    assert_eq!(tele.events.borrow()[0], ChaosAuditEvent::Aborted);
}

// ---------------------------------------------------------------------------
// Scenario 2 — prod SEV-1 active
// ---------------------------------------------------------------------------

#[test]
fn adversarial_prod_sev1_active_aborts() {
    let cat = canonical_catalog();
    let exp = cat[1].clone();
    let tele = Recorder::new(0);
    let snap = SafeModeSnapshot {
        target: ChaosTarget::Staging,
        prod_sev1_active: true,
        staging_error_rate_bps: 0,
    };
    let run = run_experiment(&exp, ChaosRunId::new("adv-2"), snap, &tele);
    match run.outcome {
        ChaosOutcome::Aborted { trigger } => assert_eq!(trigger, "prod_sev1_active"),
        other => panic!("expected prod_sev1_active abort, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Scenario 3 — staging error-rate spike
// ---------------------------------------------------------------------------

#[test]
fn adversarial_staging_error_rate_high_aborts() {
    let cat = canonical_catalog();
    let exp = cat[2].clone();
    let tele = Recorder::new(0);
    let snap = SafeModeSnapshot {
        target: ChaosTarget::Staging,
        prod_sev1_active: false,
        staging_error_rate_bps: 7_500, // 75% — clear breach
    };
    let run = run_experiment(&exp, ChaosRunId::new("adv-3"), snap, &tele);
    match run.outcome {
        ChaosOutcome::Aborted { trigger } => {
            assert_eq!(trigger, "staging_error_rate_high");
        }
        other => panic!("expected staging_error_rate_high, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Scenario 4 — adversarial steady-state hypothesis breach
// ---------------------------------------------------------------------------

#[test]
fn adversarial_steady_state_breach_detects_drift() {
    // Inject SLO impact exceeding blast-radius — runner MUST flag breach +
    // emit Completed (so 7y archive still records the failure).
    let cat = canonical_catalog();
    let exp = cat[0].clone();
    let impact = exp.blast_radius_bps + 1;
    let tele = Recorder::new(impact);
    let snap = SafeModeSnapshot {
        target: ChaosTarget::Staging,
        prod_sev1_active: false,
        staging_error_rate_bps: 100,
    };
    let run = run_experiment(&exp, ChaosRunId::new("adv-4"), snap, &tele);
    match run.outcome {
        ChaosOutcome::SteadyStateBreached { reason } => {
            assert_eq!(reason, "blast_radius_exceeded");
        }
        other => panic!("expected SteadyStateBreached, got {other:?}"),
    }
    let evs = tele.events.borrow();
    assert_eq!(evs.len(), 2, "Started + Completed expected");
    assert_eq!(evs[0], ChaosAuditEvent::Started);
    assert_eq!(evs[1], ChaosAuditEvent::Completed);
}

// ---------------------------------------------------------------------------
// Scenario 5 — replay attack (deterministic seed)
// ---------------------------------------------------------------------------

#[test]
fn adversarial_replay_deterministic() {
    // Lote 10.17 codex P2: same seed inputs => bit-identical outcome.
    let cat = canonical_catalog();
    let exp = cat[3].clone();
    let snap = SafeModeSnapshot {
        target: ChaosTarget::Staging,
        prod_sev1_active: false,
        staging_error_rate_bps: 0,
    };
    let r1 = run_experiment(
        &exp,
        ChaosRunId::new("replay-fixed"),
        snap,
        &Recorder::new(42),
    );
    let r2 = run_experiment(
        &exp,
        ChaosRunId::new("replay-fixed"),
        snap,
        &Recorder::new(42),
    );
    assert_eq!(r1.seed, r2.seed);
    assert_eq!(r1.outcome, r2.outcome);
    assert_eq!(r1.experiment_id, r2.experiment_id);
}

// ---------------------------------------------------------------------------
// Scenario 6 — catalog drift (unknown experiment id)
// ---------------------------------------------------------------------------

#[test]
fn adversarial_catalog_lookup_unknown_returns_none() {
    let cat = canonical_catalog();
    // Adversarial input — chaos config flag tried to inject unknown exp.
    assert!(lookup(&cat, "rogue-experiment").is_none());
    assert!(lookup(&cat, "").is_none());
    assert!(lookup(&cat, "lat-r2-get").is_some());
}

// ---------------------------------------------------------------------------
// Scenario 7 — multiple safe-mode triggers; HARDest one wins
// ---------------------------------------------------------------------------

#[test]
fn adversarial_safe_mode_priority_prod_target_wins() {
    // All three triggers tripped — prod target violation has top priority.
    let cat = canonical_catalog();
    let exp = cat[0].clone();
    let tele = Recorder::new(0);
    let snap = SafeModeSnapshot {
        target: ChaosTarget::Production,
        prod_sev1_active: true,
        staging_error_rate_bps: 9_000,
    };
    let run = run_experiment(&exp, ChaosRunId::new("adv-7"), snap, &tele);
    match run.outcome {
        ChaosOutcome::Aborted { trigger } => {
            assert_eq!(trigger, "prod_target_violation", "priority wrong");
        }
        other => panic!("expected prod_target_violation, got {other:?}"),
    }
}
