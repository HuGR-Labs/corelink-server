//! Canonical-text regression tests for
//! `migrations/d1/0009_quota_reservations.sql` (WI-S07-003; Lote 10.7bis
//! R5 P0-2 size-proportional reservation TTL durable mirror).
//!
//! The simulator already covers the algorithmic invariants. This file
//! pins down a small set of textual properties of the SQL artifact so
//! that an accidental delete of a `CREATE TABLE` / index / CHECK clause
//! turns the test red instead of silently shipping. A live-D1 roundtrip
//! lands alongside WI-S07-005 (PRR ship gate).
//!
//! Coverage (mirror of `migration_canonical_0008.rs`):
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` uses
//!    `IF NOT EXISTS`.
//! 2. **PRIMARY KEY direction** — composite PK `(tenant_id,
//!    reservation_id)` tenant-leftmost.
//! 3. **CHECK constraint coverage** — every constraint listed in
//!    WI §6 must be present.
//! 4. **Region CHECK lists the canonical 5 literals**.
//! 5. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden.
//! 6. **No BEGIN/COMMIT** — `wrangler d1 migrations apply` uses an
//!    implicit transaction.
//! 7. **Tenant-leftmost** — the first PK column is `tenant_id`.
//! 8. **Schema version bump to 9**.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_eviction::EvictionRegion;
use corelink_quota::{
    quota_schema_version, MIGRATION_0009_QUOTA_RESERVATIONS,
};

/// Strip `-- …` line comments before scanning.
fn migration_sql_no_comments() -> String {
    MIGRATION_0009_QUOTA_RESERVATIONS
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
    assert!(!MIGRATION_0009_QUOTA_RESERVATIONS.is_empty());
    assert!(MIGRATION_0009_QUOTA_RESERVATIONS.contains("migration 0009"));
    assert_eq!(quota_schema_version(), 9);
}

#[test]
fn quota_reservations_table_is_created_idempotently() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE TABLE IF NOT EXISTS quota_reservations"),
        "quota_reservations must be CREATE TABLE IF NOT EXISTS"
    );
}

#[test]
fn primary_key_is_tenant_leftmost_composite() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("PRIMARY KEY (tenant_id, reservation_id)"),
        "PRIMARY KEY must be (tenant_id, reservation_id) tenant-leftmost"
    );
}

#[test]
fn region_check_lists_canonical_5_literals() {
    let sql = MIGRATION_0009_QUOTA_RESERVATIONS;
    assert!(sql.contains("region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')"));
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
    let sql = MIGRATION_0009_QUOTA_RESERVATIONS;
    let needles = [
        "chk_quota_reservations_region",
        "chk_quota_reservations_requested_bytes_non_negative",
        "chk_quota_reservations_ttl_after_create",
        "chk_quota_reservations_lifecycle_create_non_negative",
    ];
    for n in needles {
        assert!(sql.contains(n), "missing CHECK constraint mnemonic: {n}");
    }
}

#[test]
fn tenant_region_index_present() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains(
            "CREATE INDEX IF NOT EXISTS idx_quota_reservations_tenant_region"
        ),
        "idx_quota_reservations_tenant_region must be present"
    );
}

#[test]
fn expires_at_index_present() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains(
            "CREATE INDEX IF NOT EXISTS idx_quota_reservations_expires_at_ms"
        ),
        "idx_quota_reservations_expires_at_ms must be present (TTL sweep)"
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
fn reservation_id_is_not_null() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("reservation_id         TEXT     NOT NULL"));
}

#[test]
fn region_is_not_null() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("region                 TEXT     NOT NULL"));
}

#[test]
fn requested_bytes_is_not_null() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("requested_bytes        INTEGER  NOT NULL"));
}

#[test]
fn expires_at_ms_is_not_null() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("expires_at_ms          INTEGER  NOT NULL"));
}

#[test]
fn created_at_ms_is_not_null() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("created_at_ms          INTEGER  NOT NULL"));
}
