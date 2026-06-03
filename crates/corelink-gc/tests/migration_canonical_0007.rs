//! Canonical-text regression tests for `migrations/d1/0007_gc_candidates.sql`
//! (WI-S06-002).
//!
//! The simulator already covers the algorithmic invariants. This file
//! pins down a small set of textual properties of the SQL artifact so
//! that an accidental delete of a `CREATE TABLE` / index / CHECK
//! clause turns the test red instead of silently shipping. A live-D1
//! roundtrip (via miniflare / wrangler-dev) lands in WI-S06-007
//! alongside the PRR ship gate.
//!
//! Coverage (mirror of `migration_canonical.rs` for 0006):
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` uses
//!    `IF NOT EXISTS`.
//! 2. **PRIMARY KEY direction** — composite PK `(tenant_id, digest,
//!    mark_run_id)`; tenant-leftmost.
//! 3. **CHECK constraint coverage** — every constraint listed in WI
//!    §1 must be present.
//! 4. **Status CHECK lists the canonical 4 literals** — `candidate`,
//!    `swept`, `physically_deleted`, `protected_re_ref`.
//! 5. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden.
//! 6. **No BEGIN/COMMIT** — `wrangler d1 migrations apply` uses an
//!    implicit transaction (Lote 10.4bis lesson).
//! 7. **Tenant-leftmost** — the first PK column is `tenant_id` so the
//!    Layer 4 envelope is enforced at storage level.
//! 8. **Partial UNIQUE absent** — `gc_candidates` does not need a
//!    state-conditioned UNIQUE (composite PK already enforces); the
//!    test pins the absence so a future drift cannot silently
//!    introduce a different lock semantic.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_gc::{gc_schema_version, CandidateStatus, MIGRATION_0007_GC_CANDIDATES};

/// Strip `-- …` line comments before scanning so prose comments cannot
/// produce false positives on the destructive-token regression checks.
fn migration_sql_no_comments() -> String {
    MIGRATION_0007_GC_CANDIDATES
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
    assert!(!MIGRATION_0007_GC_CANDIDATES.is_empty());
    assert!(MIGRATION_0007_GC_CANDIDATES.contains("migration 0007"));
    // Schema version bumped to 7 by this migration.
    assert_eq!(gc_schema_version(), 7);
}

#[test]
fn gc_candidates_table_is_created_idempotently() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE TABLE IF NOT EXISTS gc_candidates"),
        "gc_candidates table must be CREATE TABLE IF NOT EXISTS"
    );
}

#[test]
fn primary_key_is_tenant_leftmost_composite() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("PRIMARY KEY (tenant_id, digest, mark_run_id)"),
        "PRIMARY KEY must be (tenant_id, digest, mark_run_id) tenant-leftmost"
    );
}

#[test]
fn status_check_lists_canonical_4_literals() {
    let sql = MIGRATION_0007_GC_CANDIDATES;
    assert!(
        sql.contains("status IN ('candidate', 'swept', 'physically_deleted', 'protected_re_ref')")
    );
    // Cross-reference with the Rust mirror.
    let from_enum = [
        CandidateStatus::Candidate.as_str(),
        CandidateStatus::Swept.as_str(),
        CandidateStatus::PhysicallyDeleted.as_str(),
        CandidateStatus::ProtectedReRef.as_str(),
    ];
    for literal in from_enum {
        assert!(
            sql.contains(&format!("'{literal}'")),
            "status literal '{literal}' missing from CHECK list"
        );
    }
}

#[test]
fn every_canonical_check_constraint_mnemonic_is_present() {
    let sql = MIGRATION_0007_GC_CANDIDATES;
    let needles = [
        "chk_gc_candidates_status",
        "chk_gc_candidates_blob_size_non_negative",
        "chk_gc_candidates_mark_anchor_monotonic",
        "chk_gc_candidates_swept_at_ms_monotonic",
        "chk_gc_candidates_physical_delete_after_swept",
    ];
    for n in needles {
        assert!(sql.contains(n), "missing CHECK constraint mnemonic: {n}");
    }
}

#[test]
fn run_status_index_present() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE INDEX IF NOT EXISTS idx_gc_candidates_run_status"),
        "idx_gc_candidates_run_status must be present (sweep-phase consumer)"
    );
}

#[test]
fn tenant_status_index_present() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE INDEX IF NOT EXISTS idx_gc_candidates_tenant_status"),
        "idx_gc_candidates_tenant_status must be present (analytics)"
    );
}

#[test]
fn protected_partial_index_present() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE INDEX IF NOT EXISTS idx_gc_candidates_protected"),
        "idx_gc_candidates_protected partial index must be present"
    );
    assert!(
        sql.contains("WHERE status = 'protected_re_ref'"),
        "protected index must be partial WHERE status = 'protected_re_ref'"
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
fn digest_is_not_null() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("digest                 TEXT     NOT NULL"));
}

#[test]
fn mark_started_at_ms_is_not_null() {
    // INV-GC-MARK-STARTED-AT-IMMUTABLE: anchor is denormalised + NOT
    // NULL on the candidate row (sweep needs every row to have a
    // canonical anchor for the protect-if-`>=` decision).
    let sql = migration_sql_no_comments();
    assert!(sql.contains("mark_started_at_ms     INTEGER  NOT NULL"));
}

#[test]
fn mark_run_id_is_not_null() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("mark_run_id            TEXT     NOT NULL"));
}

#[test]
fn no_partial_unique_present() {
    // `gc_candidates` does not need a state-conditioned UNIQUE — the
    // composite PK `(tenant_id, digest, mark_run_id)` already enforces
    // per-(tenant, digest, run) uniqueness. Pin the absence so a future
    // drift cannot silently introduce a different lock semantic.
    let sql = migration_sql_no_comments();
    assert!(!sql.contains("CREATE UNIQUE INDEX"));
}

#[test]
fn lifecycle_partial_order_check_present() {
    // physical_delete >= swept (when both set); chk encodes the partial
    // order across the lifecycle.
    let sql = MIGRATION_0007_GC_CANDIDATES;
    assert!(sql.contains("physically_deleted_at_ms >= swept_at_ms"));
    assert!(sql.contains("swept_at_ms >= created_at_ms"));
}
