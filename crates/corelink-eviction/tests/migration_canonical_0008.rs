//! Canonical-text regression tests for
//! `migrations/d1/0008_tenant_storage_state.sql` (WI-S07-002 + WI-S07-003
//! dependency; Lote 10.7bis P0-2 NEW table).
//!
//! The simulator already covers the algorithmic invariants. This file
//! pins down a small set of textual properties of the SQL artifact so
//! that an accidental delete of a `CREATE TABLE` / index / CHECK clause
//! turns the test red instead of silently shipping. A live-D1 roundtrip
//! lands alongside WI-S07-005 (PRR ship gate).
//!
//! Coverage (mirror of `migration_canonical_0007.rs`):
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` uses
//!    `IF NOT EXISTS`.
//! 2. **PRIMARY KEY direction** — composite PK `(tenant_id, region)`
//!    tenant-leftmost.
//! 3. **CHECK constraint coverage** — every constraint listed in
//!    WI §6 must be present.
//! 4. **Region CHECK lists the canonical 5 literals**.
//! 5. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden.
//! 6. **No BEGIN/COMMIT** — `wrangler d1 migrations apply` uses an
//!    implicit transaction.
//! 7. **Tenant-leftmost** — the first PK column is `tenant_id`.
//! 8. **Schema version bump to 8**.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_eviction::{
    eviction_schema_version, EvictionRegion, MIGRATION_0008_TENANT_STORAGE_STATE,
};

/// Strip `-- …` line comments before scanning.
fn migration_sql_no_comments() -> String {
    MIGRATION_0008_TENANT_STORAGE_STATE
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
    assert!(!MIGRATION_0008_TENANT_STORAGE_STATE.is_empty());
    assert!(MIGRATION_0008_TENANT_STORAGE_STATE.contains("migration 0008"));
    assert_eq!(eviction_schema_version(), 8);
}

#[test]
fn tenant_storage_state_table_is_created_idempotently() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE TABLE IF NOT EXISTS tenant_storage_state"),
        "tenant_storage_state must be CREATE TABLE IF NOT EXISTS"
    );
}

#[test]
fn primary_key_is_tenant_leftmost_composite() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("PRIMARY KEY (tenant_id, region)"),
        "PRIMARY KEY must be (tenant_id, region) tenant-leftmost"
    );
}

#[test]
fn region_check_lists_canonical_5_literals() {
    let sql = MIGRATION_0008_TENANT_STORAGE_STATE;
    assert!(sql.contains("region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')"));
    // Cross-reference with the Rust mirror.
    let from_enum = [
        EvictionRegion::Sam.as_str(),
        EvictionRegion::Iad.as_str(),
        EvictionRegion::Lhr.as_str(),
        EvictionRegion::Nrt.as_str(),
        EvictionRegion::Syd.as_str(),
    ];
    for literal in from_enum {
        assert!(
            sql.contains(&format!("'{literal}'")),
            "region literal '{literal}' missing from CHECK list"
        );
    }
}

#[test]
fn every_canonical_check_constraint_mnemonic_is_present() {
    let sql = MIGRATION_0008_TENANT_STORAGE_STATE;
    let needles = [
        "chk_tenant_storage_state_region",
        "chk_tenant_storage_state_bytes_used_non_negative",
        "chk_tenant_storage_state_bytes_quota_non_negative",
        "chk_tenant_storage_state_bytes_reclaimed_non_negative",
        "chk_tenant_storage_state_lifecycle_updated_monotonic",
        "chk_tenant_storage_state_sync_monotonic",
        "chk_tenant_storage_state_evict_after_create",
    ];
    for n in needles {
        assert!(sql.contains(n), "missing CHECK constraint mnemonic: {n}");
    }
}

#[test]
fn region_evict_index_present() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE INDEX IF NOT EXISTS idx_tenant_storage_state_region_evict"),
        "idx_tenant_storage_state_region_evict must be present \
         (per-region eviction cron consumer)"
    );
}

#[test]
fn bytes_used_index_present() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE INDEX IF NOT EXISTS idx_tenant_storage_state_bytes_used"),
        "idx_tenant_storage_state_bytes_used must be present (analytics)"
    );
}

#[test]
fn no_destructive_sql_tokens() {
    let sql = migration_sql_no_comments();
    let upper = sql.to_uppercase();
    let forbidden = [
        "DROP TABLE",
        "DROP COLUMN",
        "DROP INDEX",
        "DROP CONSTRAINT",
        "TRUNCATE",
        "RENAME COLUMN",
        "RENAME TABLE",
        "RENAME TO",
    ];
    for tok in forbidden {
        assert!(
            !upper.contains(tok),
            "destructive token detected in migration: {tok}"
        );
    }
}

#[test]
fn no_begin_or_commit_in_migration() {
    let sql = migration_sql_no_comments();
    let upper = sql.to_uppercase();
    assert!(!upper.contains("BEGIN TRANSACTION"));
    assert!(!upper.contains("BEGIN;"));
    assert!(!upper.contains("COMMIT TRANSACTION"));
    assert!(!upper.contains("COMMIT;"));
}

#[test]
fn no_alter_table_add_constraint() {
    let sql = migration_sql_no_comments();
    let upper = sql.to_uppercase();
    assert!(!upper.contains("ALTER TABLE"));
    assert!(!upper.contains("ADD CONSTRAINT"));
}

#[test]
fn tenant_id_is_not_null() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("tenant_id              TEXT     NOT NULL"));
}

#[test]
fn region_is_not_null() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("region                 TEXT     NOT NULL"));
}

#[test]
fn bytes_used_default_zero() {
    let sql = MIGRATION_0008_TENANT_STORAGE_STATE;
    assert!(sql.contains("bytes_used             INTEGER  NOT NULL DEFAULT 0"));
}

#[test]
fn bytes_quota_default_zero() {
    let sql = MIGRATION_0008_TENANT_STORAGE_STATE;
    assert!(sql.contains("bytes_quota            INTEGER  NOT NULL DEFAULT 0"));
}

#[test]
fn last_evict_at_ms_nullable() {
    let sql = migration_sql_no_comments();
    // The column is nullable (no NOT NULL).
    assert!(sql.contains("last_evict_at_ms       INTEGER  NULL"));
}
