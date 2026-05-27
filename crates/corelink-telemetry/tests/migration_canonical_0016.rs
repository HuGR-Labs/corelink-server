//! Canonical SQL migration shape pin —
//! `migrations/d1/0016_log_schema.sql`.
//!
//! This test catches accidental schema drift between the embedded SQL
//! migration and the canonical surface assumed by the in-memory
//! redactor + sink. Mirrors `migration_canonical_0015` (S-09 WI-S09-001
//! analytics_cardinality) precedent.
//!
//! Specifically, this asserts:
//!
//! 1. The migration string compiles into the crate via `include_str!`.
//! 2. Both canonical tables are declared:
//!    - `log_schema_versions`
//!    - `log_redaction_patterns`
//! 3. Critical CHECK constraints are present (region 3-char +
//!    pattern_id canonical 5-element allowlist).
//! 4. Column names align with the per-region durable mirror surface.
//! 5. No `BEGIN`/`COMMIT` in the SQL (wrangler d1 migrations apply
//!    uses an implicit transaction per ADR-0036 Rule 3).
//! 6. No `_ms` column suffix per Lote 10.7bis P0-3 column-drift lesson.

use corelink_telemetry::logpush::MIGRATION_0016_LOG_SCHEMA;

#[test]
fn migration_includes_both_canonical_tables() {
    let sql = MIGRATION_0016_LOG_SCHEMA;
    assert!(sql.contains(
        "CREATE TABLE IF NOT EXISTS log_schema_versions"
    ));
    assert!(sql.contains(
        "CREATE TABLE IF NOT EXISTS log_redaction_patterns"
    ));
}

#[test]
fn migration_includes_canonical_column_names() {
    let sql = MIGRATION_0016_LOG_SCHEMA;
    assert!(sql.contains("region"));
    assert!(sql.contains("schema_version"));
    assert!(sql.contains("pattern_id"));
    assert!(sql.contains("placeholder"));
    assert!(sql.contains("activated_at"));
}

#[test]
fn migration_includes_pattern_id_canonical_check() {
    let sql = MIGRATION_0016_LOG_SCHEMA;
    // Pattern_id MUST be one of the 5 canonical slugs.
    assert!(sql.contains("'email'"));
    assert!(sql.contains("'ip'"));
    assert!(sql.contains("'token'"));
    assert!(sql.contains("'pan'"));
    assert!(sql.contains("'cpf_cnpj'"));
}

#[test]
fn migration_includes_region_3char_check() {
    let sql = MIGRATION_0016_LOG_SCHEMA;
    assert!(sql.contains("length(region) = 3"));
}

#[test]
fn migration_does_not_use_explicit_transactions() {
    // ADR-0036 Rule 3: wrangler d1 migrations apply uses implicit
    // transactions; explicit BEGIN/COMMIT in the migration script
    // produces "transaction within a transaction" errors.
    let sql = MIGRATION_0016_LOG_SCHEMA;
    assert!(!sql.contains("BEGIN;"));
    assert!(!sql.contains("BEGIN TRANSACTION"));
    assert!(!sql.contains("COMMIT;"));
}

#[test]
fn migration_does_not_use_underscore_ms_column_suffix() {
    // Lote 10.7bis P0-3 column-drift lesson: timestamps use canonical
    // `*_at` Unix epoch ms naming (NOT `*_ms` suffix).
    let sql = MIGRATION_0016_LOG_SCHEMA;
    let col_names = ["activated_at_ms", "snapshotted_at_ms"];
    for n in col_names {
        assert!(!sql.contains(n), "drift column found: {n}");
    }
}

#[test]
fn migration_includes_recent_index_and_region_index() {
    let sql = MIGRATION_0016_LOG_SCHEMA;
    assert!(sql.contains(
        "CREATE INDEX IF NOT EXISTS idx_log_redaction_patterns_recent"
    ));
    assert!(sql.contains(
        "CREATE INDEX IF NOT EXISTS idx_log_redaction_patterns_region"
    ));
}

#[test]
fn migration_references_canonical_invariants() {
    let sql = MIGRATION_0016_LOG_SCHEMA;
    assert!(sql.contains("CTRL-PRIV-001"));
    assert!(sql.contains("INV-AUDIT-APPEND-ONLY"));
    assert!(sql.contains("INV-TENANT-ISOLATION"));
}

#[test]
fn migration_references_wi_canonical_source() {
    let sql = MIGRATION_0016_LOG_SCHEMA;
    assert!(sql.contains("WI-S09-002"));
}

#[test]
fn schema_version_pinned() {
    assert_eq!(corelink_telemetry::logpush::logpush_schema_version(), 16);
}
