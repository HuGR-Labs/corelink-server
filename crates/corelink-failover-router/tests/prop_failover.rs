//! Property tests for the failover router (WI-S14-003).
//!
//! ## Test ladder (10k iter PR + 100k iter nightly via PROPTEST_CASES)
//!
//! 1. `prop_failover_overhead_within_slo` — simulate 10k failover decisions;
//!    assert overhead_ms ≤ 50ms p99 (SLO_FAILOVER_OVERHEAD_MS).
//! 2. `prop_failover_acyclic_graph` — verify ResidencyGraph.is_acyclic()
//!    for 10k traversals; assert no loops.
//! 3. `prop_healthy_region_primary_read` — healthy region always routes to
//!    primary (read_mode = Primary; no failover).
//! 4. `prop_degraded_region_sibling_read` — degraded region always routes to
//!    sibling (read_mode = Replica; write_mode = Blocked).
//! 5. `prop_multi_signal_threshold_canonical` — partial signal degradation
//!    (1 or 2 signals, not all 3) must NOT trigger failover.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use std::sync::Arc;

use proptest::prelude::*;

use corelink_failover_router::{
    FailoverAuditSink, FailoverRouter, InMemoryFailoverAuditSink, InMemoryFailoverRouter,
    InMemoryHealthProbe, HealthProbe, ReadMode, Region, ResidencyGraph, SLO_FAILOVER_OVERHEAD_MS,
    WriteMode,
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

fn new_probe() -> Arc<dyn HealthProbe> {
    Arc::new(InMemoryHealthProbe::new())
}

fn new_sink() -> Arc<dyn FailoverAuditSink> {
    Arc::new(InMemoryFailoverAuditSink::new())
}

/// prop_failover_overhead_within_slo
///
/// Simulates 10k failover decisions (both healthy and degraded paths);
/// asserts overhead_ms ≤ SLO_FAILOVER_OVERHEAD_MS (50ms).
#[test]
fn prop_failover_overhead_within_slo() {
    let cases = proptest_cases();
    let config = ProptestConfig::with_cases(cases);

    proptest!(config, |(
        region in arb_region(),
        degrade in any::<bool>(),
        tenant_idx in 0_u64..1000_u64,
    )| {
        let probe_inner = Arc::new(InMemoryHealthProbe::new());
        if degrade {
            probe_inner.inject_degraded(region);
        }
        let probe: Arc<dyn HealthProbe> = Arc::clone(&probe_inner) as Arc<dyn HealthProbe>;
        let router = InMemoryFailoverRouter::new(probe, new_sink());

        let decision = router
            .route_read(&format!("tenant-{tenant_idx}"), region, 0)
            .unwrap();

        prop_assert!(
            decision.overhead_ms <= SLO_FAILOVER_OVERHEAD_MS,
            "overhead {} ms exceeds SLO {} ms",
            decision.overhead_ms,
            SLO_FAILOVER_OVERHEAD_MS
        );
    });
}

/// prop_failover_acyclic_graph
///
/// For 10k traversals, asserts ResidencyGraph is acyclic and symmetric.
#[test]
fn prop_failover_acyclic_graph() {
    let cases = proptest_cases();
    let config = ProptestConfig::with_cases(cases);
    let graph = ResidencyGraph;

    proptest!(config, |(region in arb_region())| {
        prop_assert!(graph.is_acyclic(), "graph must be acyclic");

        // 2-hop symmetry: sibling(sibling(r)) == r
        if let Some(s1) = graph.sibling(region) {
            if let Some(s2) = graph.sibling(s1) {
                prop_assert_eq!(s2, region);
            }
        }

        // No self-loops.
        prop_assert!(
            graph.is_allowed(region, region).is_err(),
            "self-loop forbidden"
        );
    });
}

/// prop_healthy_region_primary_read
///
/// A healthy region always routes to primary (ReadMode::Primary,
/// WriteMode::Allowed, failover_active=false).
#[test]
fn prop_healthy_region_primary_read() {
    let cases = proptest_cases();
    let config = ProptestConfig::with_cases(cases);

    proptest!(config, |(
        region in arb_region(),
        tenant_idx in 0_u64..1000_u64,
    )| {
        let router = InMemoryFailoverRouter::new(new_probe(), new_sink());

        let decision = router
            .route_read(&format!("t-{tenant_idx}"), region, 0)
            .unwrap();

        prop_assert_eq!(decision.read_mode, ReadMode::Primary);
        prop_assert_eq!(decision.read_region, region);
        prop_assert!(!decision.failover_active, "no failover for healthy region");
        prop_assert_eq!(decision.write_mode, WriteMode::Allowed);
    });
}

/// prop_degraded_region_sibling_read
///
/// A degraded region always routes to sibling (ReadMode::Replica,
/// WriteMode::Blocked, failover_active=true).
#[test]
fn prop_degraded_region_sibling_read() {
    let cases = proptest_cases();
    let config = ProptestConfig::with_cases(cases);
    let graph = ResidencyGraph;

    proptest!(config, |(
        region in arb_region(),
        tenant_idx in 0_u64..1000_u64,
    )| {
        let probe_inner = Arc::new(InMemoryHealthProbe::new());
        probe_inner.inject_degraded(region);
        let probe: Arc<dyn HealthProbe> = probe_inner as Arc<dyn HealthProbe>;
        let router = InMemoryFailoverRouter::new(probe, new_sink());

        let decision = router
            .route_read(&format!("t-{tenant_idx}"), region, 0)
            .unwrap();

        let expected_sibling = graph.sibling(region).unwrap();

        prop_assert_eq!(decision.read_mode, ReadMode::Replica);
        prop_assert_eq!(decision.read_region, expected_sibling);
        prop_assert!(decision.failover_active, "failover must be active");
        prop_assert_eq!(decision.write_mode, WriteMode::Blocked);
        prop_assert!(
            decision.overhead_ms <= SLO_FAILOVER_OVERHEAD_MS,
            "overhead SLO preserved"
        );
    });
}

/// prop_multi_signal_threshold_canonical
///
/// Partial degradation (< 3 active signals) must NOT trigger failover.
#[test]
fn prop_multi_signal_threshold_canonical() {
    let cases = proptest_cases();
    let config = ProptestConfig::with_cases(cases);

    proptest!(config, |(
        region in arb_region(),
        // All below threshold.
        rate_5xx in 0.0_f64..=1.0_f64,
        latency_ms in 0_u64..=300_u64,
        consecutive in 0_u32..=2_u32,
    )| {
        let probe_inner = Arc::new(InMemoryHealthProbe::new());
        probe_inner.set_state(region, rate_5xx, latency_ms, consecutive);
        let probe: Arc<dyn HealthProbe> = probe_inner as Arc<dyn HealthProbe>;
        let router = InMemoryFailoverRouter::new(probe, new_sink());

        let decision = router.route_read("tenant-partial", region, 0).unwrap();

        // No signal is above threshold → must be primary, no failover.
        prop_assert_eq!(
            decision.read_mode,
            ReadMode::Primary,
            "sub-threshold signals must not trigger failover"
        );
        prop_assert!(!decision.failover_active, "no failover for sub-threshold signals");
    });
}
