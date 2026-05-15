//! Per-domain replication-lag bundle (4 domains: R2, D1, KV, Neon).
//!
//! All four SLOs are wired into `slo_catalog.md §4.23..§4.27` and the
//! Rust constants in `corelink-region` (D1) + `corelink-replica-worker`
//! (R2-hot) + `corelink-region::kv_propagation` (KV) +
//! `corelink-region::neon_replica_lag` (Neon — soft). The coordinator
//! consumes a frozen [`LagBundle`] at decision time so it can make a
//! single linearized promotion call.

use serde::{Deserialize, Serialize};

/// R2-hot replication lag SLO ceiling (per `slo_catalog.md §4.23`).
///
/// Bound to `corelink_replica_worker::region::REPLICATION_LAG_P99_SLO_SECS`
/// (= 60). Re-declared here so the coordinator does not depend on
/// constant-private accessor functions.
pub const SLO_REPLICATION_LAG_R2_SECONDS: u64 = 60;

/// D1 read-replica lag SLO ceiling (per `slo_catalog.md §4.25`).
///
/// Bound to `corelink_region::D1_REPLICA_LAG_P99_CEILING_SECONDS` (= 60).
pub const SLO_REPLICATION_LAG_D1_SECONDS: u64 = 60;

/// KV propagation lag SLO ceiling — typical case (per
/// `slo_catalog.md §4.25` — `KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS`
/// = 60). The pessimistic ceiling (300 s) is used for paging only; the
/// promotion decision uses the typical ceiling to be conservative.
pub const SLO_REPLICATION_LAG_KV_SECONDS: u64 = 60;

/// Neon replica lag soft SLO ceiling (per `slo_catalog.md §4.27` —
/// `NEON_REPLICA_LAG_P99_SOFT_CEILING_SECONDS` = 5). Informational only
/// — does NOT block promotion (`NEON_SLO_IS_INFORMATIONAL = true`).
pub const SLO_REPLICATION_LAG_NEON_SECONDS: u64 = 5;

/// Frozen 4-domain lag observation at a single instant.
///
/// All values are seconds. The coordinator consumes a single
/// `LagBundle` per promotion decision so the four probes are evaluated
/// against a consistent point-in-time snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LagBundle {
    /// R2-hot replication lag (seconds).
    pub r2_seconds: u64,
    /// D1 read-replica lag (seconds).
    pub d1_seconds: u64,
    /// KV cross-region propagation lag (seconds).
    pub kv_seconds: u64,
    /// Neon read-replica lag (seconds). Soft / informational.
    pub neon_seconds: u64,
}

impl LagBundle {
    /// A zero-lag bundle (every domain at 0 s) — used for tests and the
    /// nominal "everything healthy" initial state.
    #[must_use]
    pub const fn zero() -> Self {
        LagBundle {
            r2_seconds: 0,
            d1_seconds: 0,
            kv_seconds: 0,
            neon_seconds: 0,
        }
    }

    /// Whether every **hard** domain (R2, D1, KV) is within its SLO
    /// ceiling. Neon is soft / informational and is NOT a promotion
    /// blocker per `slo_catalog.md §4.27`.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_replication_coordinator::LagBundle;
    ///
    /// let healthy = LagBundle { r2_seconds: 10, d1_seconds: 10, kv_seconds: 10, neon_seconds: 100 };
    /// assert!(healthy.within_slo());
    ///
    /// let r2_breach = LagBundle { r2_seconds: 9999, ..LagBundle::zero() };
    /// assert!(!r2_breach.within_slo());
    /// ```
    #[must_use]
    pub fn within_slo(self) -> bool {
        self.r2_seconds <= SLO_REPLICATION_LAG_R2_SECONDS
            && self.d1_seconds <= SLO_REPLICATION_LAG_D1_SECONDS
            && self.kv_seconds <= SLO_REPLICATION_LAG_KV_SECONDS
    }

    /// Whether the Neon soft SLO is met. Reported separately for
    /// dashboard observability; never gates promotion.
    #[must_use]
    pub fn neon_within_soft_slo(self) -> bool {
        self.neon_seconds <= SLO_REPLICATION_LAG_NEON_SECONDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_bundle_is_within_slo() {
        assert!(LagBundle::zero().within_slo());
        assert!(LagBundle::zero().neon_within_soft_slo());
    }

    #[test]
    fn r2_breach_blocks_promotion() {
        let bundle = LagBundle {
            r2_seconds: SLO_REPLICATION_LAG_R2_SECONDS + 1,
            ..LagBundle::zero()
        };
        assert!(!bundle.within_slo());
    }

    #[test]
    fn d1_breach_blocks_promotion() {
        let bundle = LagBundle {
            d1_seconds: SLO_REPLICATION_LAG_D1_SECONDS + 1,
            ..LagBundle::zero()
        };
        assert!(!bundle.within_slo());
    }

    #[test]
    fn kv_breach_blocks_promotion() {
        let bundle = LagBundle {
            kv_seconds: SLO_REPLICATION_LAG_KV_SECONDS + 1,
            ..LagBundle::zero()
        };
        assert!(!bundle.within_slo());
    }

    #[test]
    fn neon_breach_does_not_block_promotion() {
        let bundle = LagBundle {
            neon_seconds: SLO_REPLICATION_LAG_NEON_SECONDS + 1_000,
            ..LagBundle::zero()
        };
        // Neon is soft / informational — promotion not blocked.
        assert!(bundle.within_slo());
        // But the soft SLO is reported breached for dashboards.
        assert!(!bundle.neon_within_soft_slo());
    }

    #[test]
    fn boundary_inclusive() {
        // Exactly at the ceiling = within SLO (<=).
        let bundle = LagBundle {
            r2_seconds: SLO_REPLICATION_LAG_R2_SECONDS,
            d1_seconds: SLO_REPLICATION_LAG_D1_SECONDS,
            kv_seconds: SLO_REPLICATION_LAG_KV_SECONDS,
            neon_seconds: SLO_REPLICATION_LAG_NEON_SECONDS,
        };
        assert!(bundle.within_slo());
        assert!(bundle.neon_within_soft_slo());
    }
}
