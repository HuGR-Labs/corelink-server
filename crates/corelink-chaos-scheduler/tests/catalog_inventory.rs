//! Catalog inventory tests — gate the "≥ 8 distinct FMs covered" AC.
//!
//! Mirrors `validate_chaos_catalog.py` CI check (WI spec §6.1.5):
//!
//! - 8 entries minimum.
//! - 8 distinct FM-IDs minimum (per Lote 10.17 codex P1 canonical fix).
//! - All 4 GA-mandatory chaos kinds present (latency / failure /
//!   resource exhaustion / network partition).
//! - All entries declare a non-empty steady-state hypothesis + ≥ 1
//!   precondition.
//! - All entries have rollback time ≤ 5 min (charter constraint).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr
)]

use corelink_chaos_scheduler::{
    canonical_catalog, distinct_fm_count, weekly_rotation, ChaosKind,
};

#[test]
fn catalog_has_at_least_8_entries() {
    assert!(canonical_catalog().len() >= 8);
}

#[test]
fn catalog_covers_8_distinct_fms() {
    let c = canonical_catalog();
    assert!(
        distinct_fm_count(&c) >= 8,
        "AC gate: ≥ 8 distinct FM-IDs required (got {})",
        distinct_fm_count(&c)
    );
}

#[test]
fn catalog_has_all_4_ga_mandatory_kinds() {
    let c = canonical_catalog();
    let kinds: std::collections::HashSet<_> = c.iter().map(|e| e.kind).collect();
    for kind in [
        ChaosKind::LatencyInjection,
        ChaosKind::FailureInjection,
        ChaosKind::ResourceExhaustion,
        ChaosKind::NetworkPartition,
    ] {
        assert!(kinds.contains(&kind), "missing GA-mandatory kind: {kind:?}");
    }
}

#[test]
fn catalog_steady_state_hypothesis_non_empty() {
    for e in canonical_catalog() {
        assert!(
            !e.steady_state_hypothesis.is_empty(),
            "{} missing steady-state hypothesis",
            e.id
        );
    }
}

#[test]
fn catalog_preconditions_non_empty() {
    for e in canonical_catalog() {
        assert!(
            !e.preconditions.is_empty(),
            "{} missing preconditions (charter: must declare)",
            e.id
        );
    }
}

#[test]
fn catalog_rollback_within_charter() {
    // Charter: rollback ≤ 5 min (300s) for staging chaos.
    for e in canonical_catalog() {
        assert!(
            e.rollback_seconds_max <= 300,
            "{} exceeds 5-min rollback cap (got {}s)",
            e.id,
            e.rollback_seconds_max
        );
    }
}

#[test]
fn catalog_ids_are_kebab_case_and_unique() {
    let c = canonical_catalog();
    let mut ids: Vec<&str> = c.iter().map(|e| e.id.as_str()).collect();
    for id in &ids {
        for ch in id.chars() {
            assert!(
                ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-',
                "id '{id}' not kebab-case",
            );
        }
    }
    ids.sort();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len(), "duplicate catalog ids");
}

#[test]
fn weekly_rotation_visits_every_experiment_once_per_cycle() {
    let c = canonical_catalog();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for w in 0..c.len() {
        seen.insert(weekly_rotation(w as u32).as_str().to_owned());
    }
    assert_eq!(seen.len(), c.len(), "rotation must cover every entry");
}

#[test]
fn catalog_blast_radius_positive() {
    for e in canonical_catalog() {
        assert!(e.blast_radius_bps > 0, "{} blast_radius_bps must be > 0", e.id);
    }
}
