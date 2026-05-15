//! R3-4 property test — deterministic seed ⇒ replay-identical `ChaosRun`
//! over 200 random `(run_id, experiment_id, impact_bps)` tuples.
//!
//! Pin: FNV-1a 64-bit `derive_seed(run_id, experiment_id)` is byte-stable
//! across replays, and the runner's outcome is a pure function of
//! `(experiment.blast_radius_bps, configured_impact_bps)` once safe-mode
//! is clean. Therefore two independent invocations with identical inputs
//! produce identical `ChaosRun` records (run_id + experiment_id + target
//! + seed + outcome).
//!
//! Charter ref: Lote 10.17 codex P2 ("equivalent within tolerance
//! rejected") — only bit-identical replay satisfies the gate.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_chaos_scheduler::derive_seed;
use corelink_chaos_scheduler::{ChaosRunId, ChaosTarget};
use e2e_chaos::{make_experiment, run_clean_staging, ChaosE2eDrill};
use proptest::prelude::*;

fn arb_drill() -> impl Strategy<Value = ChaosE2eDrill> {
    prop_oneof![
        Just(ChaosE2eDrill::NetworkPartition),
        Just(ChaosE2eDrill::CpuPressureSustained),
        Just(ChaosE2eDrill::MemoryPressureOomkAvoided),
        Just(ChaosE2eDrill::R2DiskFillQuarantine),
        Just(ChaosE2eDrill::LatencyInjectionP99Bounded),
        Just(ChaosE2eDrill::DnsFailureDualResolverFailover),
        Just(ChaosE2eDrill::ClerkOutageGracePeriod),
        Just(ChaosE2eDrill::KvD1ColdStartBelowSla),
    ]
}

fn arb_run_id() -> impl Strategy<Value = String> {
    "[a-z0-9]{4,32}".prop_map(|s| format!("r3-4-prop-{s}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// For any random `(drill, run_id, impact)` tuple, two independent
    /// runs through the chaos runner produce a bit-identical `ChaosRun`
    /// record AND a matching FNV-1a seed.
    #[test]
    fn prop_seed_replay_identical(
        drill in arb_drill(),
        run_id in arb_run_id(),
        impact_bps in 0u32..=2_000,
    ) {
        // Replay 1.
        let (run1, _t1) = run_clean_staging(drill, &run_id, impact_bps);
        // Replay 2 — identical inputs.
        let (run2, _t2) = run_clean_staging(drill, &run_id, impact_bps);

        // Bit-identical seed (FNV-1a hash match).
        let expected_seed = derive_seed(&ChaosRunId::new(run_id.as_str()), drill.experiment_id());
        prop_assert_eq!(run1.seed, run2.seed);
        prop_assert_eq!(run1.seed, expected_seed);

        // Bit-identical run record (run_id, experiment_id, target, outcome, seed).
        prop_assert_eq!(run1.run_id.as_str(), run2.run_id.as_str());
        prop_assert_eq!(run1.experiment_id.as_str(), run2.experiment_id.as_str());
        prop_assert_eq!(run1.target, ChaosTarget::Staging);
        prop_assert_eq!(run2.target, ChaosTarget::Staging);
        prop_assert_eq!(run1.outcome, run2.outcome);

        // Sanity: the experiment id matches the drill mapping.
        let exp = make_experiment(drill);
        prop_assert_eq!(run1.experiment_id.as_str(), exp.id.as_str());
    }

    /// Different `run_id` values yield different seeds (no collision over
    /// 200 randomly drawn pairs in this case range — the FNV-1a output
    /// is essentially random for distinct inputs).
    #[test]
    fn prop_distinct_run_ids_distinct_seeds(
        drill in arb_drill(),
        run_id_a in arb_run_id(),
        run_id_b in arb_run_id(),
    ) {
        prop_assume!(run_id_a != run_id_b);
        let a = derive_seed(&ChaosRunId::new(run_id_a.as_str()), drill.experiment_id());
        let b = derive_seed(&ChaosRunId::new(run_id_b.as_str()), drill.experiment_id());
        prop_assert_ne!(a, b);
    }
}
