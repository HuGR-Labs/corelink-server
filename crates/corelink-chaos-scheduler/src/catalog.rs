//! Canonical chaos catalog: 8 experiments × 8 distinct FM-IDs.
//!
//! Each entry maps to one row of `specs/_runbooks/RB-CHAOS-CATALOG.md` and
//! one FM-ID from `specs/04_sprints/S17/work_items/WI-S17-001-...md §6.1.5`:
//!
//! | id                | kind                  | target | FM-ID  |
//! |-------------------|-----------------------|--------|--------|
//! | `lat-r2-get`      | LatencyInjection      | r2     | FM-051 |
//! | `lat-d1-query`    | LatencyInjection      | d1     | FM-150 |
//! | `lat-neon-query`  | LatencyInjection      | neon   | FM-057 |
//! | `lat-kv`          | LatencyInjection      | kv     | FM-152 |
//! | `fail-r2-5xx`     | FailureInjection      | r2     | FM-054 |
//! | `fail-d1-timeout` | FailureInjection      | d1     | FM-202 |
//! | `res-do-storage`  | ResourceExhaustion    | do     | FM-059 |
//! | `net-cross-region`| NetworkPartition      | net    | FM-105 |
//!
//! The catalog covers **8 distinct FM-IDs** as required by AC gate
//! "Chaos catalog ≥ 8 FMs covered (Lote 10.17 codex P1 canonical fix)".
//!
//! All entries are `'static` so the catalog has zero allocation overhead.

use crate::types::{ChaosExperiment, ChaosExperimentId, ChaosKind};

/// Build the canonical catalog (8 entries).
#[must_use]
pub fn canonical_catalog() -> Vec<ChaosExperiment> {
    vec![
        ChaosExperiment {
            id: ChaosExperimentId::new("lat-r2-get"),
            fm_id: "FM-051",
            kind: ChaosKind::LatencyInjection,
            target: "r2",
            blast_radius_bps: 500, // 5% SLO impact tolerance
            rollback_seconds_max: 300,
            preconditions: &["r2-healthy", "slo-burn-rate-nominal"],
            steady_state_hypothesis:
                "P99 GET latency ≤ baseline + 500ms; error rate unchanged",
        },
        ChaosExperiment {
            id: ChaosExperimentId::new("lat-d1-query"),
            fm_id: "FM-150",
            kind: ChaosKind::LatencyInjection,
            target: "d1",
            blast_radius_bps: 400,
            rollback_seconds_max: 300,
            preconditions: &["d1-healthy", "slo-burn-rate-nominal"],
            steady_state_hypothesis:
                "P99 query latency ≤ baseline + 200ms; success rate unchanged",
        },
        ChaosExperiment {
            id: ChaosExperimentId::new("lat-neon-query"),
            fm_id: "FM-057",
            kind: ChaosKind::LatencyInjection,
            target: "neon",
            blast_radius_bps: 400,
            rollback_seconds_max: 300,
            preconditions: &["neon-primary-healthy"],
            steady_state_hypothesis:
                "P99 Neon latency ≤ baseline + 300ms; failover not triggered",
        },
        ChaosExperiment {
            id: ChaosExperimentId::new("lat-kv"),
            fm_id: "FM-152",
            kind: ChaosKind::LatencyInjection,
            target: "kv",
            blast_radius_bps: 200,
            rollback_seconds_max: 300,
            preconditions: &["kv-healthy"],
            steady_state_hypothesis:
                "P99 KV latency ≤ baseline + 100ms; stale-window unchanged",
        },
        ChaosExperiment {
            id: ChaosExperimentId::new("fail-r2-5xx"),
            fm_id: "FM-054",
            kind: ChaosKind::FailureInjection,
            target: "r2",
            blast_radius_bps: 100, // 1% R2 5xx
            rollback_seconds_max: 300,
            preconditions: &["r2-healthy", "retry-budget-available"],
            steady_state_hypothesis:
                "Client retries absorb 1% R2 5xx; user-visible errors ≤ baseline",
        },
        ChaosExperiment {
            id: ChaosExperimentId::new("fail-d1-timeout"),
            fm_id: "FM-202",
            kind: ChaosKind::FailureInjection,
            target: "d1",
            blast_radius_bps: 50, // 0.5% D1 timeouts
            rollback_seconds_max: 300,
            preconditions: &["d1-healthy"],
            steady_state_hypothesis:
                "D1 timeouts ≤ 0.5%; circuit-breaker remains closed",
        },
        ChaosExperiment {
            id: ChaosExperimentId::new("res-do-storage"),
            fm_id: "FM-059",
            kind: ChaosKind::ResourceExhaustion,
            target: "do",
            blast_radius_bps: 200,
            rollback_seconds_max: 300,
            preconditions: &["do-storage-below-90pct"],
            steady_state_hypothesis:
                "DO writes succeed up to 95% quota; eviction policy fires cleanly",
        },
        ChaosExperiment {
            id: ChaosExperimentId::new("net-cross-region"),
            fm_id: "FM-105",
            kind: ChaosKind::NetworkPartition,
            target: "cross-region",
            blast_radius_bps: 1000, // 10% — replication-bound
            rollback_seconds_max: 60,
            preconditions: &["multi-region-active", "replication-lag-nominal"],
            steady_state_hypothesis:
                "Cross-region partition 30s; reads served from local replica; \
                 no data loss; replication catches up ≤ 60s post-heal",
        },
    ]
}

/// Look up an experiment by id (linear scan; catalog is small + const).
#[must_use]
pub fn lookup(catalog: &[ChaosExperiment], id: &str) -> Option<ChaosExperiment> {
    catalog.iter().find(|e| e.id.as_str() == id).cloned()
}

/// Count distinct FM-IDs covered by the catalog.
#[must_use]
pub fn distinct_fm_count(catalog: &[ChaosExperiment]) -> usize {
    let mut fms: Vec<&str> = catalog.iter().map(|e| e.fm_id).collect();
    fms.sort_unstable();
    fms.dedup();
    fms.len()
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

    #[test]
    fn catalog_has_8_entries() {
        assert_eq!(canonical_catalog().len(), 8);
    }

    #[test]
    fn catalog_covers_at_least_8_distinct_fms() {
        // AC gate — Lote 10.17 codex P1 canonical fix.
        let c = canonical_catalog();
        assert!(
            distinct_fm_count(&c) >= 8,
            "must cover ≥ 8 distinct FM-IDs"
        );
    }

    #[test]
    fn catalog_all_kinds_present() {
        let c = canonical_catalog();
        let kinds: std::collections::HashSet<_> = c.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&ChaosKind::LatencyInjection));
        assert!(kinds.contains(&ChaosKind::FailureInjection));
        assert!(kinds.contains(&ChaosKind::ResourceExhaustion));
        assert!(kinds.contains(&ChaosKind::NetworkPartition));
    }

    #[test]
    fn catalog_rollback_within_5_min() {
        // Charter: rollback ≤ 5 min (300s).
        for e in canonical_catalog() {
            assert!(
                e.rollback_seconds_max <= 300,
                "{} exceeds 5-min rollback cap",
                e.id
            );
        }
    }

    #[test]
    fn catalog_ids_unique() {
        let c = canonical_catalog();
        let mut ids: Vec<_> = c.iter().map(|e| e.id.as_str().to_owned()).collect();
        ids.sort();
        let pre = ids.len();
        ids.dedup();
        assert_eq!(pre, ids.len(), "catalog ids must be unique");
    }

    #[test]
    fn lookup_known_returns_some() {
        let c = canonical_catalog();
        assert!(lookup(&c, "lat-r2-get").is_some());
        assert!(lookup(&c, "fail-d1-timeout").is_some());
    }

    #[test]
    fn lookup_unknown_returns_none() {
        let c = canonical_catalog();
        assert!(lookup(&c, "nonexistent").is_none());
    }

    #[test]
    fn catalog_all_ga_mandatory() {
        for e in canonical_catalog() {
            assert!(
                e.is_ga_mandatory(),
                "{} not in GA-mandatory 8-set",
                e.id
            );
        }
    }
}
