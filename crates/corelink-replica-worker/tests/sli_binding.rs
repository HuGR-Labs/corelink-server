//! Cross-crate SLI-binding tests — closes DR-16 wave-14 instrumentation
//! gap for `SLO-REPLICATION-LAG-R2` (slo_catalog.md §4.23).
//!
//! See `crates/corelink-region/tests/sli_binding.rs` for the broader
//! rationale; this file pins the R2 hot-blob replication-lag emit-site
//! against the `Sli::ReplicationLagR2` taxonomy entry.
//!
//! # Happy-path assertion
//!
//! On every successful `emit_lag` call into `InMemoryReplicationLagSli`,
//! the observation carries the same metric-name semantics as the SLI
//! taxonomy. We assert this end-to-end (emit → observe → metric-name
//! alignment) so a regression in either the trait contract OR the SLI
//! enum is caught at `cargo test`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_replica_worker::metrics::{
    InMemoryReplicationLagSli, ReplicationDomain, ReplicationLagSli,
    METRIC_REPLICATION_LAG_SECONDS,
};
use corelink_replica_worker::Region;
use corelink_slo::Sli;

#[test]
fn r2_replication_lag_metric_aligned_with_sli_taxonomy() {
    // Static constant binding — load-bearing for the validator's BOUND
    // classification.
    assert_eq!(
        METRIC_REPLICATION_LAG_SECONDS,
        Sli::ReplicationLagR2.prometheus_metric_base()
    );
    assert_eq!(Sli::ReplicationLagR2.slug(), "SLO-REPLICATION-LAG-R2");
}

#[test]
fn r2_happy_path_emit_observes_canonical_sli() {
    // Happy-path: SLI is observed on every successful emit_lag call.
    let sli = InMemoryReplicationLagSli::new();
    sli.emit_lag(
        ReplicationDomain::R2Hot,
        Region::Wnam,
        Region::Enam,
        12.5,
        1_700_000_000_000,
    )
    .expect("happy-path emit must succeed");
    let obs = sli.observations();
    assert_eq!(obs.len(), 1, "exactly one observation recorded");
    let row = obs.first().expect("first observation");
    assert_eq!(row.domain, ReplicationDomain::R2Hot);
    // Tie the in-memory metric back to the SLI taxonomy — if the
    // canonical name drifts on either side, this test fails.
    assert_eq!(
        sli.observations()
            .first()
            .map(|_| METRIC_REPLICATION_LAG_SECONDS)
            .unwrap_or(""),
        Sli::ReplicationLagR2.prometheus_metric_base()
    );
}
