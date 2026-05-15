//! `corelink-region` — CoreLink multi-region types and primitives.
//!
//! WI-S14-001 foundation crate. wasm32-clean (no tokio in src/).
//!
//! # Modules
//!
//! - [`region`] — [`Region`] enum + [`DoJurisdiction`] enum.
//! - [`event`] — [`ProvisioningEvent`] + [`MigrationEvent`] CloudEvent types.
//! - [`metrics`] — [`RegionMetrics`] struct (5 canonical metrics §6.1.6).
//! - [`migration`] — [`MigrationReport`] + [`MigrationDecision`] taxonomy.
//! - [`error`] — [`RegionError`] taxonomy.
//! - [`audit`] — [`RegionAuditSink`] trait + [`InMemoryRegionAuditSink`].
//! - [`replica_lag`] — D1 read-replica lag probe (DEBT-011 P0-002).
//! - [`kv_propagation`] — KV cross-region propagation-lag probe (DEBT-011 P1-001).
//! - [`do_sync_age`] — DO→D1 sync-age probe (DEBT-011 P1-003).
//! - [`r2_crr`] — R2 platform CRR indirect-lag probe (DEBT-011 P1-004).
//! - [`neon_replica_lag`] — Neon read-replica lag probe (DEBT-011 P2-001; soft SLO).
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::format_in_format_args)]

pub mod audit;
pub mod do_sync_age;
pub mod error;
pub mod event;
pub mod kv_propagation;
pub mod metrics;
pub mod migration;
pub mod neon_replica_lag;
pub mod r2_crr;
pub mod region;
pub mod replica_lag;

pub use do_sync_age::{
    DoClass, DoSyncAgeProbe, DoSyncAgeSample, FailingDoSyncAgeProbe, InMemoryDoSyncAgeProbe,
    DO_SYNC_AGE_CONFIG_SINGLETON_P99_CEILING_SECONDS, DO_SYNC_AGE_RATE_LIMITER_BUDGET_SECONDS,
    DO_SYNC_AGE_TENANT_QUOTA_P99_CEILING_SECONDS, METRIC_DO_SYNC_AGE_SECONDS,
};
pub use neon_replica_lag::{
    FailingNeonReplicaLagProbe, InMemoryNeonReplicaLagProbe, NeonReplicaLagProbe,
    NeonReplicaLagSample, METRIC_NEON_REPLICA_LAG_SECONDS, NEON_PROBE_CADENCE_SECONDS,
    NEON_REPLICA_LAG_P99_SOFT_CEILING_SECONDS, NEON_SLO_IS_INFORMATIONAL,
};
pub use kv_propagation::{
    fraction_within_typical, FailingKvPropagationProbe, InMemoryKvPropagationProbe,
    KvPropagationProbe, KvPropagationSample, KV_PROBE_CADENCE_SECONDS,
    KV_PROPAGATION_PESSIMISTIC_P99_CEILING_SECONDS, KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS,
    KV_PROPAGATION_TYPICAL_SAMPLE_FRACTION, METRIC_KV_PROPAGATION_LAG_SECONDS,
};
pub use r2_crr::{
    FailingR2CrrProbe, InMemoryR2CrrProbe, R2CrrProbe, R2CrrSample, METRIC_R2_CRR_LAG_SECONDS,
    R2_CRR_LAG_P99_CEILING_SECONDS, R2_CRR_OBJECT_MISSING_INCIDENT_SECONDS,
    R2_CRR_PROBE_CADENCE_SECONDS,
};
pub use region::Region;
pub use replica_lag::{
    D1LagSample, D1ReplicaLagProbe, FailingD1ReplicaLagProbe, InMemoryD1ReplicaLagProbe,
    D1_PROBE_CADENCE_SECONDS, D1_REPLICA_LAG_P99_CEILING_SECONDS, METRIC_D1_REPLICA_LAG_SECONDS,
};
