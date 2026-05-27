//! Property tests for the DR drill scheduler + CF region outage simulator
//! (WI-S17-002).
//!
//! ## Test ladder (10k iter PR + 100k iter nightly via PROPTEST_CASES)
//!
//! 1. `prop_prod_env_always_rejected` — `Production` env MUST always fail
//!    with `ProdEnvForbidden` for both scheduler and simulator (Lote 10.17 P0).
//! 2. `prop_staging_env_always_admitted` — `Staging` / `Test` envs schedule
//!    successfully when the region has a sibling.
//! 3. `prop_failover_target_is_canonical_sibling` — failover target matches
//!    `ResidencyGraph::sibling(region)` for every region.
//! 4. `prop_rto_within_slo_admits` — RTO ≤ 1800 and RPO ≤ 60 always yields
//!    `within_slo == true` and no `SloViolation`.
//! 5. `prop_rto_over_slo_rejects` — RTO > 1800 always yields
//!    `SloViolation { slo: "rto" }`.
//! 6. `prop_rpo_over_slo_rejects` — RTO within budget + RPO > 60 yields
//!    `SloViolation { slo: "rpo" }`.
//! 7. `prop_cycle_canonical_cadence_stable` — every cycle round-trips
//!    cycle_number ↔ canonical_cadence ↔ as_str.
//! 8. `prop_drill_status_state_machine` — terminal/non-terminal classification.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use proptest::prelude::*;

use corelink_ops::dr::drill::{
    CfRegionOutageSimulator, DrillCadence, DrillCycle, DrillEnv, DrillError, DrillScheduler,
    DrillStatus, InMemoryCfRegionOutageSimulator, InMemoryDrillScheduler, Region, ResidencyGraph,
    RPO_CEIL_SECONDS, RTO_CEIL_SECONDS, SEMESTRAL_CRON,
};

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}

fn arb_region() -> impl Strategy<Value = Region> {
    prop_oneof![
        Just(Region::Wnam),
        Just(Region::Enam),
        Just(Region::Weur),
        Just(Region::Sam),
    ]
}

fn arb_chaos_env() -> impl Strategy<Value = DrillEnv> {
    prop_oneof![Just(DrillEnv::Staging), Just(DrillEnv::Test)]
}

fn arb_cycle() -> impl Strategy<Value = DrillCycle> {
    prop_oneof![
        Just(DrillCycle::CfRegionOutage),
        Just(DrillCycle::D1PrimaryLoss),
        Just(DrillCycle::ByokKeyCompromise),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_prod_env_always_rejected(region in arb_region(), cycle in arb_cycle()) {
        let sched = InMemoryDrillScheduler::new();
        let err = sched
            .schedule(
                "prod-rejected".into(),
                cycle,
                DrillEnv::Production,
                0,
                region,
            )
            .unwrap_err();
        // SOTA-OK: variant-only assertion sufficient — DrillError::ProdEnvForbidden is a unit variant carrying no semantic state.
        prop_assert!(matches!(err, DrillError::ProdEnvForbidden));

        let sim = InMemoryCfRegionOutageSimulator::new();
        let err = sim
            .simulate(DrillEnv::Production, region, 100, 10)
            .unwrap_err();
        // SOTA-OK: variant-only assertion sufficient — DrillError::ProdEnvForbidden is a unit variant carrying no semantic state.
        prop_assert!(matches!(err, DrillError::ProdEnvForbidden));
    }

    #[test]
    fn prop_staging_env_always_admitted(
        env in arb_chaos_env(),
        region in arb_region(),
        cycle in arb_cycle(),
        ts in 0u64..1_000_000_000_000u64,
    ) {
        let sched = InMemoryDrillScheduler::new();
        let run = sched
            .schedule(
                format!("drill-{}-{}", region.as_str(), ts),
                cycle,
                env,
                ts,
                region,
            )
            .unwrap();
        prop_assert_eq!(run.status, DrillStatus::Scheduled);
        prop_assert_eq!(run.env, env);
        prop_assert_eq!(run.simulated_region, region);
        prop_assert_eq!(run.cycle, cycle);
    }

    #[test]
    fn prop_failover_target_is_canonical_sibling(
        env in arb_chaos_env(),
        region in arb_region(),
        rto in 0u64..=RTO_CEIL_SECONDS,
        rpo in 0u64..=RPO_CEIL_SECONDS,
    ) {
        let sim = InMemoryCfRegionOutageSimulator::new();
        let outcome = sim.simulate(env, region, rto, rpo).unwrap();
        let graph = ResidencyGraph;
        let expected = graph.sibling(region).unwrap();
        prop_assert_eq!(outcome.failover_target, expected);
        prop_assert_eq!(outcome.simulated_region, region);
    }

    #[test]
    fn prop_rto_within_slo_admits(
        env in arb_chaos_env(),
        region in arb_region(),
        rto in 0u64..=RTO_CEIL_SECONDS,
        rpo in 0u64..=RPO_CEIL_SECONDS,
    ) {
        let sim = InMemoryCfRegionOutageSimulator::new();
        let outcome = sim.simulate(env, region, rto, rpo).unwrap();
        prop_assert!(outcome.within_slo);
        prop_assert!(outcome.measurement.rto_seconds <= RTO_CEIL_SECONDS);
        prop_assert!(outcome.measurement.rpo_seconds <= RPO_CEIL_SECONDS);
    }

    #[test]
    fn prop_rto_over_slo_rejects(
        env in arb_chaos_env(),
        region in arb_region(),
        rto in (RTO_CEIL_SECONDS + 1)..=10_000u64,
        rpo in 0u64..=RPO_CEIL_SECONDS,
    ) {
        let sim = InMemoryCfRegionOutageSimulator::new();
        let err = sim.simulate(env, region, rto, rpo).unwrap_err();
        let is_rto = matches!(err, DrillError::SloViolation { slo, .. } if slo == "rto");
        prop_assert!(is_rto);
    }

    #[test]
    fn prop_rpo_over_slo_rejects(
        env in arb_chaos_env(),
        region in arb_region(),
        rto in 0u64..=RTO_CEIL_SECONDS,
        rpo in (RPO_CEIL_SECONDS + 1)..=10_000u64,
    ) {
        let sim = InMemoryCfRegionOutageSimulator::new();
        let err = sim.simulate(env, region, rto, rpo).unwrap_err();
        let is_rpo = matches!(err, DrillError::SloViolation { slo, .. } if slo == "rpo");
        prop_assert!(is_rpo);
    }

    #[test]
    fn prop_cycle_canonical_cadence_stable(cycle in arb_cycle()) {
        let n = cycle.cycle_number();
        prop_assert!((1..=3).contains(&n));
        let cadence = cycle.canonical_cadence();
        match cycle {
            DrillCycle::CfRegionOutage => {
                prop_assert_eq!(cadence, DrillCadence::Semestral);
            }
            DrillCycle::D1PrimaryLoss | DrillCycle::ByokKeyCompromise => {
                prop_assert_eq!(cadence, DrillCadence::Annual);
            }
            _ => {}
        }
        prop_assert!(!cycle.as_str().is_empty());
    }
}

#[test]
fn drill_status_state_machine_terminal_classification() {
    assert!(!DrillStatus::Scheduled.is_terminal());
    assert!(!DrillStatus::InProgress.is_terminal());
    assert!(DrillStatus::Completed.is_terminal());
    assert!(DrillStatus::Failed.is_terminal());
    assert!(DrillStatus::Aborted.is_terminal());

    assert!(DrillStatus::Completed.is_success());
    assert!(!DrillStatus::Failed.is_success());
    assert!(!DrillStatus::Aborted.is_success());
}

#[test]
fn semestral_cron_canonical() {
    assert_eq!(SEMESTRAL_CRON, "0 6 1 1,7 *");
}

#[test]
fn scheduler_list_get_update_round_trip() {
    let sched = InMemoryDrillScheduler::new();
    let run = sched
        .schedule(
            "rt-1".into(),
            DrillCycle::CfRegionOutage,
            DrillEnv::Staging,
            42,
            Region::Weur,
        )
        .unwrap();
    assert_eq!(run.status, DrillStatus::Scheduled);

    let fetched = sched.get("rt-1").unwrap().unwrap();
    assert_eq!(fetched.drill_id, "rt-1");

    sched
        .update_status("rt-1", DrillStatus::InProgress)
        .unwrap();
    let fetched = sched.get("rt-1").unwrap().unwrap();
    assert_eq!(fetched.status, DrillStatus::InProgress);

    sched
        .update_status("rt-1", DrillStatus::Completed)
        .unwrap();
    let listed = sched.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].status.is_terminal());
    assert!(listed[0].status.is_success());
}

#[test]
fn simulator_outage_marking_round_trip() {
    let sim = InMemoryCfRegionOutageSimulator::new();
    assert!(sim.synthetic_outages().unwrap().is_empty());

    sim.simulate(DrillEnv::Staging, Region::Weur, 120, 30)
        .unwrap();
    let outages = sim.synthetic_outages().unwrap();
    assert_eq!(outages.len(), 1);
    assert_eq!(outages[0], Region::Weur);

    sim.clear().unwrap();
    assert!(sim.synthetic_outages().unwrap().is_empty());
}
