//! Canonical-text regression tests for
//! `migrations/d1/0012_quota_cas_attempts.sql` (WI-S08-003; canonical
//! 100% hard-block CAS audit ring; production CF DO + D1 atomic batch
//! land alongside WI-S08-006 PRR ship gate).
//!
//! The orchestrator already covers the algorithmic invariants. This
//! file pins down a small set of textual properties of the SQL artifact
//! so that an accidental delete of a `CREATE TABLE` / index / CHECK
//! clause turns the test red instead of silently shipping. A live-D1
//! roundtrip lands alongside WI-S08-006 (PRR ship gate).
//!
//! Coverage (mirror of `migration_canonical_0011.rs` + WI §6.1.8):
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` uses
//!    `IF NOT EXISTS`.
//! 2. **PRIMARY KEY** is `attempt_id` (canonical UUIDv7 surrogate;
//!    every CAS attempt emits its own row).
//! 3. **CHECK constraint coverage** — every constraint listed in WI §6
//!    must be present.
//! 4. **event_type CHECK lists the canonical 6 literals** (matches the
//!    `corelink-quota-cas::audit::QuotaCasEventType` enum 1:1).
//! 5. **region CHECK lists the canonical 5 literals** (matches
//!    `corelink-eviction::EvictionRegion` enum 1:1).
//! 6. **retry_after_secs CHECK envelope** lists the canonical bounds
//!    `[0, 31 × 86_400]` per `MAX_SECS_PER_MONTH`.
//! 7. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden.
//! 8. **No BEGIN/COMMIT** — `wrangler d1 migrations apply` uses an
//!    implicit transaction.
//! 9. **Schema version bump to 12**.
//! 10. **Canonical invariant references in header comments** — the
//!     migration header MUST cite INV-QUOTA-ENFORCEMENT +
//!     INV-AUDIT-APPEND-ONLY + ADR-0020 so future drift is caught at
//!     code review.
//! 11. **3 indices**: tenant scan, race-detected partial, denied 429
//!     partial (matching WI §6.1.8).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_quota_cas::{
    quota_cas_schema_version, MIGRATION_0012_QUOTA_CAS_ATTEMPTS,
};

/// Strip `-- …` line comments before scanning.
fn migration_sql_no_comments() -> String {
    MIGRATION_0012_QUOTA_CAS_ATTEMPTS
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
    assert!(!MIGRATION_0012_QUOTA_CAS_ATTEMPTS.is_empty());
    assert!(MIGRATION_0012_QUOTA_CAS_ATTEMPTS.contains("migration 0012"));
    assert_eq!(quota_cas_schema_version(), 12);
}

#[test]
fn create_table_and_indexes_are_idempotent() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS quota_cas_attempts"));
    assert!(sql.contains(
        "CREATE INDEX IF NOT EXISTS idx_quota_cas_attempts_tenant_now_ms"
    ));
    assert!(sql.contains(
        "CREATE INDEX IF NOT EXISTS idx_quota_cas_attempts_race_detected"
    ));
    assert!(sql.contains(
        "CREATE INDEX IF NOT EXISTS idx_quota_cas_attempts_denied_429"
    ));
}

#[test]
fn primary_key_is_attempt_id() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("PRIMARY KEY (attempt_id)"),
        "PK must be (attempt_id) — canonical UUIDv7 surrogate per CAS attempt"
    );
}

#[test]
fn event_type_check_lists_canonical_6_literals() {
    let sql = migration_sql_no_comments();
    for canonical in [
        "CasCheckPassed",
        "CasDenied429HardBlock",
        "CasRaceDetected",
        "CasCommitSucceeded",
        "CasReleaseIdempotent",
        "CasRetryAfterEmitted",
    ] {
        assert!(
            sql.contains(&format!("'{canonical}'")),
            "SQL CHECK missing canonical event_type literal {canonical}"
        );
    }
    assert!(sql.contains("CHECK (event_type IN ("));
}

#[test]
fn region_check_lists_canonical_5_literals() {
    let sql = migration_sql_no_comments();
    for canonical in ["sam", "iad", "lhr", "nrt", "syd"] {
        assert!(
            sql.contains(&format!("'{canonical}'")),
            "SQL CHECK missing canonical region literal {canonical}"
        );
    }
    assert!(sql.contains("CHECK (region IN ("));
}

#[test]
fn retry_after_check_envelope_is_canonical_bounds() {
    let sql = migration_sql_no_comments();
    // 31 days in seconds = 2_678_400.
    assert!(
        sql.contains("2678400"),
        "retry_after_secs CHECK must list canonical 31-day ceiling"
    );
}

#[test]
fn cas_attempt_has_minimum_check() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("cas_attempt >= 1"));
}

#[test]
fn cas_version_has_non_negative_check() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("cas_version >= 0"));
}

#[test]
fn bytes_has_non_negative_check() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("bytes >= 0"));
}

#[test]
fn now_ms_has_non_negative_check() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("now_ms >= 0"));
}

#[test]
fn no_destructive_tokens() {
    let sql = migration_sql_no_comments().to_uppercase();
    for forbidden in [
        "DROP TABLE",
        "DROP INDEX",
        "DROP COLUMN",
        "ALTER COLUMN",
        "RENAME COLUMN",
        "RENAME TABLE",
        "TRUNCATE",
    ] {
        assert!(
            !sql.contains(forbidden),
            "destructive token in migration: {forbidden}"
        );
    }
}

#[test]
fn no_explicit_transaction_block() {
    let sql_upper = migration_sql_no_comments().to_uppercase();
    assert!(!sql_upper.contains("BEGIN;"));
    assert!(!sql_upper.contains("COMMIT;"));
}

#[test]
fn header_cites_canonical_invariants_and_adr() {
    // Header comments are part of the original (commented) SQL.
    let raw = MIGRATION_0012_QUOTA_CAS_ATTEMPTS;
    assert!(raw.contains("INV-QUOTA-ENFORCEMENT"));
    assert!(raw.contains("INV-AUDIT-APPEND-ONLY"));
    assert!(raw.contains("ADR-0020"));
    assert!(raw.contains("WI-S08-003"));
}

#[test]
fn all_required_columns_present() {
    let sql = migration_sql_no_comments();
    for col in [
        "attempt_id",
        "tenant_id",
        "region",
        "event_type",
        "cas_version",
        "cas_attempt",
        "bytes",
        "retry_after_secs",
        "created_by_request_id",
        "now_ms",
    ] {
        assert!(
            sql.contains(col),
            "missing canonical column declaration: {col}"
        );
    }
}

#[test]
fn race_detected_index_is_partial() {
    let sql = migration_sql_no_comments();
    // The SEV-3 alert query filter is the partial WHERE clause.
    assert!(sql.contains("WHERE event_type = 'CasRaceDetected'"));
}

#[test]
fn denied_429_index_is_partial() {
    let sql = migration_sql_no_comments();
    // The SEV-1 alarm query filter is the partial WHERE clause.
    assert!(sql.contains("WHERE event_type = 'CasDenied429HardBlock'"));
}
