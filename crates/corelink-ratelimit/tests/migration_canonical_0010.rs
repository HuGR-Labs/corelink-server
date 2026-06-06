//! Canonical-text regression tests for
//! `migrations/d1/0010_ratelimit_buckets.sql` (WI-S08-001; per-tenant
//! token-bucket durable mirror).
//!
//! The orchestrator already covers the algorithmic invariants. This
//! file pins down a small set of textual properties of the SQL artifact
//! so that an accidental delete of a `CREATE TABLE` / index / CHECK
//! clause turns the test red instead of silently shipping. A live-D1
//! roundtrip lands alongside WI-S08-006 (PRR ship gate).
//!
//! Coverage (mirror of `migration_canonical_0009.rs`):
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` uses
//!    `IF NOT EXISTS`.
//! 2. **PRIMARY KEY direction** — composite PK `(tenant_id,
//!    key_dimension, scope_key)` tenant-leftmost.
//! 3. **CHECK constraint coverage** — every constraint listed in
//!    WI §6 must be present.
//! 4. **key_dimension CHECK lists the canonical 3 literals**.
//! 5. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden.
//! 6. **No BEGIN/COMMIT** — `wrangler d1 migrations apply` uses an
//!    implicit transaction.
//! 7. **Tenant-leftmost** — the first PK column is `tenant_id`.
//! 8. **Schema version bump to 10**.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_ratelimit::{
    ratelimit_schema_version, KeyDimension, MIGRATION_0010_RATELIMIT_BUCKETS,
};

/// Strip `-- …` line comments before scanning.
fn migration_sql_no_comments() -> String {
    MIGRATION_0010_RATELIMIT_BUCKETS
        .lines()
        .map(|line| match line.find("--") {
            Some(idx) => match line.get(..idx) {
                Some(s) => s,
                None => line,
            },
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn migration_is_non_empty_and_versioned() {
    assert!(!MIGRATION_0010_RATELIMIT_BUCKETS.is_empty());
    assert!(MIGRATION_0010_RATELIMIT_BUCKETS.contains("migration 0010"));
    assert_eq!(ratelimit_schema_version(), 10);
}

#[test]
fn create_table_and_indexes_are_idempotent() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS ratelimit_buckets"));
    assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_ratelimit_buckets_tenant_dimension"));
    assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_ratelimit_buckets_updated_at_ms"));
}

#[test]
fn primary_key_is_tenant_leftmost() {
    let sql = migration_sql_no_comments();
    // The PK declaration tightly: PRIMARY KEY (tenant_id, key_dimension, scope_key).
    assert!(
        sql.contains("PRIMARY KEY (tenant_id, key_dimension, scope_key)"),
        "PK must be (tenant_id, key_dimension, scope_key) tenant-leftmost"
    );
}

#[test]
fn key_dimension_check_lists_canonical_3_literals() {
    let sql = migration_sql_no_comments();
    assert!(sql
        .contains("CHECK (key_dimension IN ('per_tenant', 'per_ip', 'per_tenant_per_endpoint'))"));
    // Cross-check against the Rust enum.
    for d in [
        KeyDimension::PerTenant,
        KeyDimension::PerIp,
        KeyDimension::PerTenantPerEndpoint,
    ] {
        assert!(
            sql.contains(&format!("'{}'", d.as_str())),
            "SQL CHECK missing canonical literal {d}"
        );
    }
}

#[test]
fn check_constraint_coverage_matches_wi_spec() {
    let sql = migration_sql_no_comments();
    // available_tokens >= 0.0
    assert!(sql.contains("CHECK (available_tokens >= 0.0)"));
    // burst_capacity >= 1
    assert!(sql.contains("CHECK (burst_capacity >= 1)"));
    // available_tokens <= burst_capacity
    assert!(sql.contains("CHECK (available_tokens <= burst_capacity)"));
    // refill_rate_per_sec >= 0.0
    assert!(sql.contains("CHECK (refill_rate_per_sec >= 0.0)"));
    // created_at_ms >= 0
    assert!(sql.contains("CHECK (created_at_ms >= 0)"));
    // updated_at_ms >= created_at_ms
    assert!(sql.contains("CHECK (updated_at_ms >= created_at_ms)"));
    // last_refill_at_ms >= created_at_ms
    assert!(sql.contains("CHECK (last_refill_at_ms >= created_at_ms)"));
}

#[test]
fn destructive_tokens_are_forbidden() {
    let sql_upper = MIGRATION_0010_RATELIMIT_BUCKETS.to_uppercase();
    // No DROP TABLE / DROP COLUMN / ALTER … DROP / RENAME / TRUNCATE.
    for forbidden in [
        "DROP TABLE",
        "DROP COLUMN",
        "DROP INDEX",
        "DROP CONSTRAINT",
        "TRUNCATE",
        "RENAME COLUMN",
        "RENAME TABLE",
        "RENAME TO",
    ] {
        assert!(
            !sql_upper.contains(forbidden),
            "migration must not contain destructive token: {forbidden}"
        );
    }
}

#[test]
fn no_begin_commit_block() {
    // `wrangler d1 migrations apply` uses implicit transaction; explicit
    // BEGIN/COMMIT would conflict.
    let sql_upper = MIGRATION_0010_RATELIMIT_BUCKETS.to_uppercase();
    assert!(!sql_upper.contains("BEGIN;"));
    assert!(!sql_upper.contains("COMMIT;"));
}

#[test]
fn tenant_id_text_column_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("tenant_id              TEXT     NOT NULL"));
}

#[test]
fn schema_version_pinned_at_10() {
    assert_eq!(ratelimit_schema_version(), 10);
}

#[test]
fn migration_carries_canonical_invariant_references() {
    // The migration MUST reference the canonical invariants in the
    // header comments so future drift is caught at code review.
    assert!(MIGRATION_0010_RATELIMIT_BUCKETS.contains("INV-RATE-LIMIT-PROPORTIONALITY"));
    assert!(MIGRATION_0010_RATELIMIT_BUCKETS.contains("INV-AVAIL-ISOLATION"));
    assert!(MIGRATION_0010_RATELIMIT_BUCKETS.contains("INV-TENANT-ISOLATION"));
}
