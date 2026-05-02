//! Canonical-text regression tests for `migrations/d1/0006_gc_run.sql`.
//!
//! The simulator already covers the algorithmic invariants. This file
//! pins down a small set of textual properties of the SQL artifact so
//! that an accidental delete of a `CREATE TABLE` / index / CHECK
//! clause turns the test red instead of silently shipping. A live-D1
//! roundtrip (via miniflare / wrangler-dev) lands in WI-S06-006
//! alongside the TLA+ CI gate + property test 100k race.
//!
//! Coverage:
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` uses
//!    `IF NOT EXISTS`. `wrangler d1 migrations apply` is run on every
//!    deploy; a non-idempotent migration would explode on the second
//!    apply.
//! 2. **PRIMARY KEY direction** — `gc_run` PK is the worker-minted
//!    `run_id`; tenant_id is enforced via the partial UNIQUE on
//!    `(tenant_id, region) WHERE status='running'`.
//! 3. **CHECK constraint coverage** — every constraint listed in WI
//!    §1 must be present.
//! 4. **Region literal completeness** — the `region` IN-list cites
//!    all 5 canonical regions in the order WI §1 freezes (matches
//!    [`corelink_gc::REGION_LIST`]).
//! 5. **Status CHECK includes 'failed'** (Lote 10.6bis P0-4 fix).
//! 6. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden; the
//!    `scripts/check_migrations_additive.py` CI gate also runs over
//!    this file but the test pins the property at crate-test scope.
//! 7. **No BEGIN/COMMIT** — `wrangler d1 migrations apply` uses an
//!    implicit transaction (Lote 10.4bis lesson).
//! 8. **Partial UNIQUE INDEX present** — `WHERE status='running'`
//!    governs the per-(tenant, region) lock.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_gc::{gc_schema_version, GcRegion, MIGRATION_0006_GC_RUN, REGION_LIST};

/// Strip `-- …` line comments before scanning so prose comments cannot
/// produce false positives on the destructive-token regression checks.
fn migration_sql_no_comments() -> String {
    MIGRATION_0006_GC_RUN
        .lines()
        .map(|line| match line.find("--") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn migration_is_non_empty_and_versioned() {
    assert!(!MIGRATION_0006_GC_RUN.is_empty());
    assert!(MIGRATION_0006_GC_RUN.contains("migration 0006"));
    // WI-S06-002 advances the GC domain to schema 7 (gc_candidates).
    assert_eq!(gc_schema_version(), 7);
}

#[test]
fn gc_run_table_is_created_idempotently() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE TABLE IF NOT EXISTS gc_run"),
        "gc_run table must be CREATE TABLE IF NOT EXISTS"
    );
}

#[test]
fn primary_key_is_run_id() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("run_id              TEXT        NOT NULL PRIMARY KEY"),
        "PRIMARY KEY must be run_id (UUIDv7-derived; per-region lock via partial UNIQUE)"
    );
}

#[test]
fn region_check_constraint_lists_canonical_5_regions() {
    let sql = MIGRATION_0006_GC_RUN;
    assert!(sql.contains("region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')"));
    // Ensure parity with the Rust mirror.
    let canonical_list: Vec<&str> = REGION_LIST.to_vec();
    assert_eq!(canonical_list, vec!["sam", "iad", "lhr", "nrt", "syd"]);
    // Cross-reference with the Rust enum.
    let from_enum: Vec<&str> = GcRegion::all().iter().map(|r| r.as_str()).collect();
    assert_eq!(from_enum, canonical_list);
}

#[test]
fn phase_check_constraint_lists_canonical_7_phases() {
    let sql = MIGRATION_0006_GC_RUN;
    assert!(sql.contains(
        "phase IN ('idle', 'mark', 'sweep', 'physical_delete', 'reconcile', 'completed', 'failed')"
    ));
}

#[test]
fn status_check_constraint_includes_failed_per_lote_10_6bis() {
    // Lote 10.6bis P0-4 fix: 'failed' status variant added.
    let sql = MIGRATION_0006_GC_RUN;
    assert!(sql.contains(
        "status IN ('pending', 'running', 'succeeded', 'crashed', 'aborted', 'failed')"
    ));
}

#[test]
fn every_canonical_check_constraint_mnemonic_is_present() {
    let sql = MIGRATION_0006_GC_RUN;
    let needles = [
        "chk_gc_run_phase",
        "chk_gc_run_status",
        "chk_gc_run_region",
        "chk_gc_run_lifecycle_checkpoint",
        "chk_gc_run_lifecycle_completed",
        "chk_gc_run_lifecycle_failed",
        "chk_gc_run_mark_started",
        "chk_gc_run_counters_non_negative",
    ];
    for n in needles {
        assert!(sql.contains(n), "missing CHECK constraint mnemonic: {n}");
    }
}

#[test]
fn partial_unique_index_present_per_lote_10_5bis() {
    // Lote 10.5bis lesson: state-conditioned UNIQUE for per-(tenant,
    // region) lock that releases on transition to terminal status.
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE UNIQUE INDEX IF NOT EXISTS uq_gc_run_running"),
        "partial UNIQUE INDEX uq_gc_run_running must be present"
    );
    assert!(
        sql.contains("WHERE status = 'running'"),
        "partial UNIQUE must be conditioned WHERE status = 'running'"
    );
}

#[test]
fn stale_running_index_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_gc_run_stale_running"));
    assert!(sql.contains("WHERE status = 'running'"));
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
    // wrangler d1 migrations apply uses implicit transaction; explicit
    // BEGIN/COMMIT would fail (Lote 10.4bis lesson).
    let sql = migration_sql_no_comments();
    let upper = sql.to_uppercase();
    assert!(!upper.contains("BEGIN TRANSACTION"));
    assert!(!upper.contains("BEGIN;"));
    assert!(!upper.contains("COMMIT TRANSACTION"));
    assert!(!upper.contains("COMMIT;"));
}

#[test]
fn no_alter_table_add_constraint() {
    // SQLite/D1 does not support ALTER TABLE ADD CONSTRAINT chk_*.
    // CHECK MUST be inline in CREATE TABLE per ADR-0036 Rule 1.
    let sql = migration_sql_no_comments();
    let upper = sql.to_uppercase();
    assert!(!upper.contains("ALTER TABLE"));
    assert!(!upper.contains("ADD CONSTRAINT"));
}

#[test]
fn tenant_id_is_not_null() {
    // INV-TENANT-ISOLATION: cross-tenant gc_run impossible at storage
    // layer.
    let sql = migration_sql_no_comments();
    assert!(sql.contains("tenant_id           TEXT        NOT NULL"));
}

#[test]
fn mark_started_at_ms_is_nullable_anchor() {
    // INV-GC-MARK-STARTED-AT-IMMUTABLE: NULL until Mark phase begins;
    // captured ONCE then immutable.
    let sql = migration_sql_no_comments();
    assert!(sql.contains("mark_started_at_ms  INTEGER     NULL"));
}

#[test]
fn lifecycle_checkpoint_check_present() {
    // chk_gc_run_lifecycle_checkpoint: last_checkpoint_at_ms >= started_at_ms.
    let sql = MIGRATION_0006_GC_RUN;
    assert!(sql.contains("last_checkpoint_at_ms >= started_at_ms"));
}

#[test]
fn counters_non_negative_check_present() {
    let sql = MIGRATION_0006_GC_RUN;
    for col in [
        "blobs_marked_count",
        "blobs_swept_count",
        "blobs_physically_deleted_count",
        "bytes_reclaimed",
    ] {
        assert!(
            sql.contains(&format!("{col} >= 0")),
            "non-negative CHECK missing for {col}"
        );
    }
}
