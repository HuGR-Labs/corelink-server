//! Canonical-text regression tests for
//! `migrations/d1/0013_abuse_scores.sql` (WI-S08-004; per-tenant
//! durable score history; companion abuse_appeals +
//! abuse_response_actions + abuse_calibration_weights deferred to
//! WI-S08-006).
//!
//! The orchestrator already covers the algorithmic invariants. This
//! file pins down a small set of textual properties of the SQL artifact
//! so that an accidental delete of a `CREATE TABLE` / index / CHECK
//! clause turns the test red instead of silently shipping. A live-D1
//! roundtrip lands alongside WI-S08-006 (PRR ship gate).
//!
//! Coverage (mirror of `migration_canonical_0012.rs` / `_0011.rs`):
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` uses
//!    `IF NOT EXISTS`.
//! 2. **PRIMARY KEY** — composite `(tenant_id, computed_at)`
//!    tenant-leftmost (CTRL-ISO-005 + INV-TENANT-ISOLATION).
//! 3. **CHECK constraint coverage** — every constraint listed in
//!    WI §6 must be present.
//! 4. **decision CHECK lists the canonical 3 literals**
//!    (`Benign`, `Suspicious`, `Malicious`).
//! 5. **score envelope CHECK** — `[0.0, 1.0]` per WI §1 invariant 5.
//! 6. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden.
//! 7. **No BEGIN/COMMIT** — `wrangler d1 migrations apply` uses an
//!    implicit transaction.
//! 8. **Schema version bump to 13**.
//! 9. **Canonical invariant references in header comments** — the
//!    migration header MUST cite INV-TENANT-ISOLATION +
//!    INV-AUDIT-APPEND-ONLY + LGPD Art. 20 + GDPR Art. 22 so future
//!    drift is caught at code review.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_abuse::{abuse_schema_version, MIGRATION_0013_ABUSE_SCORES};

/// Strip `-- …` line comments before scanning.
fn migration_sql_no_comments() -> String {
    MIGRATION_0013_ABUSE_SCORES
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
fn migration_0013_is_non_empty() {
    assert!(!MIGRATION_0013_ABUSE_SCORES.is_empty());
}

#[test]
fn schema_version_is_thirteen() {
    assert_eq!(abuse_schema_version(), 13);
}

#[test]
fn create_table_uses_if_not_exists() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE TABLE IF NOT EXISTS abuse_score_history"),
        "main table must use IF NOT EXISTS"
    );
}

#[test]
fn create_index_uses_if_not_exists() {
    let sql = migration_sql_no_comments();
    let occurrences = sql.matches("CREATE INDEX IF NOT EXISTS").count();
    assert!(occurrences >= 3, "expected ≥ 3 indices; got {occurrences}");
}

#[test]
fn primary_key_is_tenant_leftmost_composite() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("PRIMARY KEY (tenant_id, computed_at)"),
        "PK must be composite tenant-leftmost"
    );
}

#[test]
fn decision_check_lists_canonical_three_literals() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("'Benign'"));
    assert!(sql.contains("'Suspicious'"));
    assert!(sql.contains("'Malicious'"));
}

#[test]
fn score_envelope_check_present() {
    let sql = migration_sql_no_comments();
    // The score CHECK lives in two halves (>= 0.0 AND <= 1.0).
    assert!(sql.contains("score >= 0.0 AND score <= 1.0"));
}

#[test]
fn cpu_wallclock_ratio_non_negative_check_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("cpu_wallclock_ratio >= 0.0"));
}

#[test]
fn egress_bytes_non_negative_check_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("egress_bytes_per_min >= 0"));
}

#[test]
fn entropy_non_negative_check_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("action_digest_entropy_bits >= 0.0"));
}

#[test]
fn concurrent_exec_count_non_negative_check_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("concurrent_exec_count >= 0"));
}

#[test]
fn window_bounds_check_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("window_end > window_start"));
}

#[test]
fn no_destructive_tokens() {
    let sql = MIGRATION_0013_ABUSE_SCORES;
    let banned = [
        "DROP TABLE",
        "DROP COLUMN",
        "DROP INDEX",
        "DROP CONSTRAINT",
        "ALTER COLUMN",
        "RENAME COLUMN",
        "RENAME TABLE",
        "TRUNCATE",
    ];
    for tok in banned {
        assert!(
            !sql.to_uppercase().contains(tok),
            "banned token {tok} present"
        );
    }
}

#[test]
fn no_begin_commit() {
    let sql = MIGRATION_0013_ABUSE_SCORES;
    assert!(
        !sql.to_uppercase().contains("BEGIN;"),
        "wrangler d1 migrations apply uses an implicit transaction"
    );
    assert!(!sql.to_uppercase().contains("COMMIT;"));
}

#[test]
fn header_cites_canonical_invariant_references() {
    // Header comments stay in MIGRATION_0013_ABUSE_SCORES (we deliberately
    // scan the WITH-comments form here).
    let sql = MIGRATION_0013_ABUSE_SCORES;
    assert!(sql.contains("INV-TENANT-ISOLATION"));
    assert!(sql.contains("INV-AUDIT-APPEND-ONLY"));
    assert!(sql.contains("LGPD Art. 20"));
    assert!(sql.contains("GDPR Art. 22"));
    assert!(sql.contains("CAP-ABUSE-001"));
    assert!(sql.contains("CAP-ABUSE-002"));
    assert!(sql.contains("R-S08-7"));
}

#[test]
fn header_cites_canonical_4_features() {
    let sql = MIGRATION_0013_ABUSE_SCORES;
    assert!(sql.contains("cpu_wallclock_ratio"));
    assert!(sql.contains("egress_bytes_per_min"));
    assert!(sql.contains("action_digest_entropy_bits"));
    assert!(sql.contains("concurrent_exec_count"));
}

#[test]
fn timestamp_columns_have_no_ms_suffix() {
    // Per Lote 10.7bis P0-3 column-drift lesson, timestamp columns
    // canonical store as Unix ms but the column NAME uses no `_ms`
    // suffix (`computed_at`, `window_start`, `window_end`).
    let sql = migration_sql_no_comments();
    assert!(sql.contains("computed_at"));
    assert!(sql.contains("window_start"));
    assert!(sql.contains("window_end"));
    assert!(
        !sql.contains("computed_at_ms"),
        "Lote 10.7bis P0-3: no `_ms` suffix"
    );
    assert!(
        !sql.contains("window_start_ms"),
        "Lote 10.7bis P0-3: no `_ms` suffix"
    );
}
