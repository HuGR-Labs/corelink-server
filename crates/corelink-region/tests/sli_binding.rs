//! Cross-crate SLI-binding tests — closes DR-16 wave-14 instrumentation
//! gap for the three replica-lag SLOs whose emit-site canonical metric
//! names live in this crate (`SLO-REPLICATION-LAG-D1`,
//! `SLO-REPLICATION-LAG-KV`, `SLO-REPLICATION-LAG-NEON`).
//!
//! # Why these tests
//!
//! `corelink-slo::Sli` is the canonical SLI taxonomy (closed enum, slug +
//! prometheus base name). Each emit-site crate carries its own
//! `METRIC_*` constant for compile-time stability of the Prometheus
//! metric name. The contract is: **the constant in the emit-site crate
//! MUST equal `Sli::*::prometheus_metric_base()`**. A regression in
//! either side is caught here at `cargo test` time, before the validator
//! script even runs.
//!
//! The Sli enum slug binding is asserted in the `corelink-slo` crate's
//! own unit tests; here we assert the **emit-site ↔ taxonomy alignment**
//! that the `scripts/validate_slo_instrumentation.py` BOUND gate relies
//! on.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_region::kv_propagation::METRIC_KV_PROPAGATION_LAG_SECONDS;
use corelink_region::neon_replica_lag::METRIC_NEON_REPLICA_LAG_SECONDS;
use corelink_region::replica_lag::METRIC_D1_REPLICA_LAG_SECONDS;
use corelink_slo::Sli;

#[test]
fn d1_replica_lag_metric_aligned_with_sli_taxonomy() {
    // `SLO-REPLICATION-LAG-D1` (slo_catalog.md §4.24) — emit-site
    // constant must match the canonical `Sli::ReplicationLagD1`
    // prometheus base.
    assert_eq!(
        METRIC_D1_REPLICA_LAG_SECONDS,
        Sli::ReplicationLagD1.prometheus_metric_base()
    );
    assert_eq!(Sli::ReplicationLagD1.slug(), "SLO-REPLICATION-LAG-D1");
}

#[test]
fn kv_propagation_lag_metric_aligned_with_sli_taxonomy() {
    // `SLO-REPLICATION-LAG-KV` (slo_catalog.md §4.25).
    assert_eq!(
        METRIC_KV_PROPAGATION_LAG_SECONDS,
        Sli::ReplicationLagKv.prometheus_metric_base()
    );
    assert_eq!(Sli::ReplicationLagKv.slug(), "SLO-REPLICATION-LAG-KV");
}

#[test]
fn neon_replica_lag_metric_aligned_with_sli_taxonomy() {
    // `SLO-REPLICATION-LAG-NEON` (slo_catalog.md §4.27) — soft /
    // informational; no paging at GA. SLI taxonomy binding is still
    // load-bearing for the dashboard panel + verifier script.
    assert_eq!(
        METRIC_NEON_REPLICA_LAG_SECONDS,
        Sli::ReplicationLagNeon.prometheus_metric_base()
    );
    assert_eq!(Sli::ReplicationLagNeon.slug(), "SLO-REPLICATION-LAG-NEON");
}

#[test]
fn replica_lag_slugs_disjoint_from_p0_closures() {
    // Defensive: confirm the new DR-16 wave-14 closures do not collide
    // with the audit 2026-05-14 P0 closures (which were AvailControlPlane
    // / LatencyCasPutP99 / LatencyAcHitP99 / CorrectnessCas /
    // CorrectnessTenantIsolation).
    let dr16 = [
        Sli::ReplicationLagD1.slug(),
        Sli::ReplicationLagKv.slug(),
        Sli::ReplicationLagNeon.slug(),
    ];
    let p0 = [
        Sli::AvailControlPlane.slug(),
        Sli::LatencyCasPutP99.slug(),
        Sli::LatencyAcHitP99.slug(),
        Sli::CorrectnessCas.slug(),
        Sli::CorrectnessTenantIsolation.slug(),
    ];
    for d in dr16 {
        for p in p0 {
            assert_ne!(d, p, "DR-16 closure {d} collides with P0 closure {p}");
        }
    }
}
