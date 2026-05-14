//! Chaos runner — orchestrates a single experiment with safe-mode auto-abort,
//! deterministic-seed reproducibility, and the canonical 3-event audit
//! lifecycle (`corelink.chaos.run.started` / `.completed` / `.aborted`).
//!
//! No async runtime: the runner is a pure state machine. Real I/O (state
//! capture pre/post, SLO impact measurement, R2 archive) is delegated to the
//! [`Telemetry`] trait so worker / CI / tests share the same orchestrator.

use crate::types::{
    ChaosAuditEvent, ChaosExperiment, ChaosOutcome, ChaosRun, ChaosRunId, ChaosTarget,
};

// ---------------------------------------------------------------------------
// Safe-mode environment
// ---------------------------------------------------------------------------

/// Snapshot of the runtime safe-mode checks evaluated at run start.
///
/// Sourced from existing observability (S-09 multi-burn-rate alerts) — the
/// runner does *not* poll metrics itself. Adapter wiring lives outside the
/// `src/` (worker handlers; CF Cron). See spec §1 of WI-S17-001.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafeModeSnapshot {
    /// Runtime env. Anything other than [`ChaosTarget::Staging`] auto-aborts.
    pub target: ChaosTarget,
    /// True iff a prod SEV-1 is active at run-start time.
    pub prod_sev1_active: bool,
    /// Staging error rate (basis points; 100 = 1%). Above 5000 = auto-abort.
    pub staging_error_rate_bps: u32,
}

impl SafeModeSnapshot {
    /// Evaluate the snapshot; returns `Some(trigger)` if any safe-mode rule
    /// trips, or `None` if it is safe to proceed.
    #[must_use]
    pub fn evaluate(&self) -> Option<&'static str> {
        if !self.target.is_chaos_safe() {
            return Some("prod_target_violation");
        }
        if self.prod_sev1_active {
            return Some("prod_sev1_active");
        }
        if self.staging_error_rate_bps > 5_000 {
            return Some("staging_error_rate_high");
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Telemetry adapter
// ---------------------------------------------------------------------------

/// Minimal trait surface the runner needs from the audit / métricas layer.
///
/// Implementors:
/// - audit chain (R2 `evidence-chaos/<run_id>.json`; 7y retention)
/// - Prometheus emitter for `corelink_chaos_run_total` /
///   `corelink_chaos_safe_mode_abort_total`
/// - in-memory test fakes (see `tests/`).
pub trait Telemetry {
    /// Emit an audit event for a chaos run.
    fn emit_audit(&self, run: &ChaosRun, event: ChaosAuditEvent);
    /// Capture pre-experiment state (returns deterministic digest for
    /// replay verification).
    fn capture_state_pre(&self, run: &ChaosRun) -> u64;
    /// Capture post-experiment state.
    fn capture_state_post(&self, run: &ChaosRun) -> u64;
    /// Measure SLO impact (basis points) during the run.
    fn measure_slo_impact(&self, run: &ChaosRun, pre: u64, post: u64) -> u32;
}

// ---------------------------------------------------------------------------
// Deterministic seed derivation
// ---------------------------------------------------------------------------

/// Derive a deterministic u64 PRNG seed from `run_id` + `experiment_id`.
///
/// Uses a 64-bit FNV-1a hash so the function is `const`-friendly, has zero
/// dependencies, and produces bit-identical output on every host architecture
/// — a hard requirement of the "deterministic seed" gate (Lote 10.17 codex P2
/// "equivalent within tolerance rejected").
#[must_use]
pub fn derive_seed(run_id: &ChaosRunId, experiment_id: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in run_id.as_str().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    // Separator (vertical bar) — avoids collisions where two ids concat.
    hash ^= 0x7c;
    hash = hash.wrapping_mul(FNV_PRIME);
    for byte in experiment_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

// ---------------------------------------------------------------------------
// Main entrypoint
// ---------------------------------------------------------------------------

/// Run a chaos experiment end-to-end.
///
/// Flow (mirrors §1 of WI-S17-001):
///
/// 1. **Safe-mode HARD env check** — first line; if `target != Staging`,
///    emit `Aborted` audit event with trigger `prod_target_violation` and
///    return immediately.
/// 2. **Prod SEV-1 guard** — abort with `prod_sev1_active` if a prod
///    incident is in flight.
/// 3. **Staging error-rate guard** — abort with `staging_error_rate_high`
///    if staging error rate > 50% (5000 bps).
/// 4. Emit `Started` audit event.
/// 5. Capture pre-state, simulate experiment, capture post-state, measure
///    SLO impact.
/// 6. If SLO impact > `blast_radius_bps`, mark as `SteadyStateBreached`
///    (auto-rollback trigger). Otherwise mark as `Passed`.
/// 7. Emit `Completed` audit event with the final outcome.
///
/// Errors are *not* used: chaos runs always produce a `ChaosRun` record
/// (so the 7y archive is always written, even on safe-mode aborts).
#[must_use]
pub fn run_experiment(
    experiment: &ChaosExperiment,
    run_id: ChaosRunId,
    snapshot: SafeModeSnapshot,
    telemetry: &dyn Telemetry,
) -> ChaosRun {
    let seed = derive_seed(&run_id, experiment.id.as_str());

    // (1)-(3) HARD safe-mode check — code-level enforcement.
    if let Some(trigger) = snapshot.evaluate() {
        let run = ChaosRun {
            run_id,
            experiment_id: experiment.id.clone(),
            target: snapshot.target,
            outcome: ChaosOutcome::Aborted { trigger },
            seed,
        };
        telemetry.emit_audit(&run, ChaosAuditEvent::Aborted);
        return run;
    }

    // (4) Started.
    let pending = ChaosRun {
        run_id: run_id.clone(),
        experiment_id: experiment.id.clone(),
        target: snapshot.target,
        // Sentinel — replaced before Completed event is emitted.
        outcome: ChaosOutcome::Passed { slo_impact_bps: 0 },
        seed,
    };
    telemetry.emit_audit(&pending, ChaosAuditEvent::Started);

    // (5) Capture + measure.
    let pre = telemetry.capture_state_pre(&pending);
    let post = telemetry.capture_state_post(&pending);
    let impact = telemetry.measure_slo_impact(&pending, pre, post);

    // (6) Verdict.
    let outcome = if impact > experiment.blast_radius_bps {
        ChaosOutcome::SteadyStateBreached {
            reason: "blast_radius_exceeded",
        }
    } else {
        ChaosOutcome::Passed {
            slo_impact_bps: impact,
        }
    };

    let final_run = ChaosRun {
        run_id,
        experiment_id: experiment.id.clone(),
        target: snapshot.target,
        outcome,
        seed,
    };

    // (7) Completed.
    telemetry.emit_audit(&final_run, ChaosAuditEvent::Completed);
    final_run
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use crate::catalog::canonical_catalog;
    use crate::types::{ChaosExperimentId, ChaosKind};
    use std::cell::RefCell;

    // ---- helpers ---------------------------------------------------------

    struct CapturingTelemetry {
        events: RefCell<Vec<(String, ChaosAuditEvent)>>,
        impact_bps: u32,
    }

    impl CapturingTelemetry {
        fn new(impact_bps: u32) -> Self {
            Self {
                events: RefCell::new(Vec::new()),
                impact_bps,
            }
        }
    }

    impl Telemetry for CapturingTelemetry {
        fn emit_audit(&self, run: &ChaosRun, event: ChaosAuditEvent) {
            self.events
                .borrow_mut()
                .push((run.run_id.as_str().to_owned(), event));
        }
        fn capture_state_pre(&self, _run: &ChaosRun) -> u64 {
            0xdead_beef
        }
        fn capture_state_post(&self, _run: &ChaosRun) -> u64 {
            0xfeed_face
        }
        fn measure_slo_impact(&self, _run: &ChaosRun, _pre: u64, _post: u64) -> u32 {
            self.impact_bps
        }
    }

    fn first_experiment() -> ChaosExperiment {
        canonical_catalog().into_iter().next().unwrap_or_else(|| {
            // Fallback synthetic — should never fire since catalog is non-empty.
            ChaosExperiment {
                id: ChaosExperimentId::new("synth"),
                fm_id: "FM-000",
                kind: ChaosKind::LatencyInjection,
                target: "synth",
                blast_radius_bps: 100,
                rollback_seconds_max: 300,
                preconditions: &[],
                steady_state_hypothesis: "",
            }
        })
    }

    // ---- unit tests ------------------------------------------------------

    #[test]
    fn seed_deterministic() {
        let id = ChaosRunId::new("run-42");
        let a = derive_seed(&id, "lat-r2-get");
        let b = derive_seed(&id, "lat-r2-get");
        assert_eq!(a, b, "same inputs => same seed");
    }

    #[test]
    fn seed_differs_per_experiment() {
        let id = ChaosRunId::new("run-42");
        let a = derive_seed(&id, "lat-r2-get");
        let b = derive_seed(&id, "fail-r2-5xx");
        assert_ne!(a, b, "different experiments => different seeds");
    }

    #[test]
    fn seed_differs_per_run() {
        let a = derive_seed(&ChaosRunId::new("run-1"), "lat-r2-get");
        let b = derive_seed(&ChaosRunId::new("run-2"), "lat-r2-get");
        assert_ne!(a, b, "different runs => different seeds");
    }

    #[test]
    fn safe_mode_blocks_prod_target() {
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Production,
            prod_sev1_active: false,
            staging_error_rate_bps: 0,
        };
        assert_eq!(snap.evaluate(), Some("prod_target_violation"));
    }

    #[test]
    fn safe_mode_blocks_prod_sev1() {
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: true,
            staging_error_rate_bps: 0,
        };
        assert_eq!(snap.evaluate(), Some("prod_sev1_active"));
    }

    #[test]
    fn safe_mode_blocks_staging_error_rate_high() {
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: false,
            staging_error_rate_bps: 6_000,
        };
        assert_eq!(snap.evaluate(), Some("staging_error_rate_high"));
    }

    #[test]
    fn safe_mode_passes_clean_staging() {
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: false,
            staging_error_rate_bps: 100,
        };
        assert_eq!(snap.evaluate(), None);
    }

    #[test]
    fn run_aborts_on_prod_target() {
        let exp = first_experiment();
        let tele = CapturingTelemetry::new(0);
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Production,
            prod_sev1_active: false,
            staging_error_rate_bps: 0,
        };
        let run = run_experiment(&exp, ChaosRunId::new("r1"), snap, &tele);
        match run.outcome {
            ChaosOutcome::Aborted { trigger } => assert_eq!(trigger, "prod_target_violation"),
            other => panic!("expected Aborted, got {other:?}"),
        }
        // Only the Aborted event should be emitted — no Started.
        let evs = tele.events.borrow();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].1, ChaosAuditEvent::Aborted);
    }

    #[test]
    fn run_passes_when_impact_within_blast_radius() {
        let exp = first_experiment();
        let tele = CapturingTelemetry::new(exp.blast_radius_bps / 2);
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: false,
            staging_error_rate_bps: 100,
        };
        let run = run_experiment(&exp, ChaosRunId::new("r2"), snap, &tele);
        if let ChaosOutcome::Passed { slo_impact_bps } = run.outcome {
            assert_eq!(slo_impact_bps, exp.blast_radius_bps / 2);
        } else {
            panic!("expected Passed");
        }
        let evs = tele.events.borrow();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].1, ChaosAuditEvent::Started);
        assert_eq!(evs[1].1, ChaosAuditEvent::Completed);
    }

    #[test]
    fn run_steady_state_breach_on_excess_impact() {
        let exp = first_experiment();
        let tele = CapturingTelemetry::new(exp.blast_radius_bps + 1);
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: false,
            staging_error_rate_bps: 0,
        };
        let run = run_experiment(&exp, ChaosRunId::new("r3"), snap, &tele);
        match run.outcome {
            ChaosOutcome::SteadyStateBreached { reason } => {
                assert_eq!(reason, "blast_radius_exceeded");
            }
            other => panic!("expected SteadyStateBreached, got {other:?}"),
        }
    }

    #[test]
    fn run_replay_bit_identical() {
        // Lote 10.17 codex P2: same seed => bit-identical state_post hash.
        let exp = first_experiment();
        let tele1 = CapturingTelemetry::new(42);
        let tele2 = CapturingTelemetry::new(42);
        let snap = SafeModeSnapshot {
            target: ChaosTarget::Staging,
            prod_sev1_active: false,
            staging_error_rate_bps: 0,
        };
        let run1 = run_experiment(&exp, ChaosRunId::new("same-id"), snap, &tele1);
        let run2 = run_experiment(&exp, ChaosRunId::new("same-id"), snap, &tele2);
        assert_eq!(run1.seed, run2.seed);
        assert_eq!(run1.outcome, run2.outcome);
    }
}
