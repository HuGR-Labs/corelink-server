//! Canonical SQL migration shape pin — `migrations/d1/0015_analytics_cardinality_budgets.sql`.
//!
//! This test catches accidental schema drift between the embedded SQL
//! migration and the canonical surface assumed by the in-memory
//! validator + observer. Mirrors `migration_canonical_0010` (S-08
//! WI-S08-001 ratelimit_buckets) precedent.
//!
//! Specifically, this asserts:
//!
//! 1. The migration string compiles into the crate via `include_str!`.
//! 2. Both canonical tables are declared:
//!    - `analytics_cardinality_budgets`
//!    - `analytics_cardinality_observed`
//! 3. Critical CHECK constraints are present (per-metric ≤ 100k +
//!    non-negative observed_unique_tuples).
//! 4. Column names align with the per-instance cardinality validator
//!    state (the in-memory ledger key is the canonical metric_name
//!    `RedMetricKind::as_str()`).
//! 5. No `BEGIN`/`COMMIT` in the SQL (wrangler d1 migrations apply
//!    uses an implicit transaction per ADR-0036 Rule 3).
//! 6. No `_ms` column suffix per Lote 10.7bis P0-3 column-drift lesson.

use corelink_analytics::MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS;

#[test]
fn migration_includes_both_canonical_tables() {
    let sql = MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS;
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS analytics_cardinality_budgets"));
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS analytics_cardinality_observed"));
}

#[test]
fn migration_includes_canonical_column_names() {
    let sql = MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS;
    assert!(sql.contains("metric_name"));
    assert!(sql.contains("budget_unique_tuples"));
    assert!(sql.contains("observed_unique_tuples"));
    assert!(sql.contains("updated_at"));
    assert!(sql.contains("snapshotted_at"));
}

#[test]
fn migration_includes_per_metric_global_check() {
    let sql = MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS;
    // Per-metric budget MUST NOT exceed the global ceiling.
    assert!(sql.contains("budget_unique_tuples <= 100000"));
    // Per-metric budget MUST be at least 1.
    assert!(sql.contains("budget_unique_tuples >= 1"));
    // Observed count MUST be non-negative.
    assert!(sql.contains("observed_unique_tuples >= 0"));
}

#[test]
fn migration_does_not_use_explicit_transactions() {
    // ADR-0036 Rule 3: wrangler d1 migrations apply uses implicit
    // transactions; explicit BEGIN/COMMIT in the migration script
    // produces "transaction within a transaction" errors.
    let sql = MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS;
    assert!(!sql.contains("BEGIN;"));
    assert!(!sql.contains("BEGIN TRANSACTION"));
    assert!(!sql.contains("COMMIT;"));
}

#[test]
fn migration_does_not_use_underscore_ms_column_suffix() {
    // Lote 10.7bis P0-3 column-drift lesson: timestamps use canonical
    // `*_at` Unix epoch ms naming (NOT `*_ms` suffix); analytics
    // tables follow the global migration vocabulary.
    let sql = MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS;
    let col_names = ["updated_at_ms", "snapshotted_at_ms"];
    for n in col_names {
        assert!(!sql.contains(n), "drift column found: {n}");
    }
}

#[test]
fn migration_includes_observed_recent_index() {
    let sql = MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS;
    assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_analytics_observed_recent"));
    assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_analytics_observed_metric"));
}

#[test]
fn migration_references_canonical_invariant() {
    let sql = MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS;
    assert!(sql.contains("INV-OBS-CARDINALITY-BUDGET"));
    assert!(sql.contains("INV-AUDIT-APPEND-ONLY"));
    assert!(sql.contains("INV-TENANT-ISOLATION"));
}

#[test]
fn migration_references_wi_canonical_source() {
    let sql = MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS;
    assert!(sql.contains("WI-S09-001"));
}
