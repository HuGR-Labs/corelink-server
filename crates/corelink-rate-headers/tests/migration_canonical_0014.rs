#![allow(clippy::doc_lazy_continuation)]

//! Canonical-text regression tests for
//! `migrations/d1/0014_global_circuit_state.sql` (WI-S08-005;
//! per-region durable circuit state + append-only trips_history;
//! companion DASH-RATE widget deferred to WI-S08-006).
//!
//! The orchestrator already covers the algorithmic invariants. This
//! file pins down a small set of textual properties of the SQL artifact
//! so that an accidental delete of a `CREATE TABLE` / index / CHECK
//! clause turns the test red instead of silently shipping. A live-D1
//! roundtrip lands alongside WI-S08-006 (PRR ship gate).
//!
//! Coverage (mirror of `migration_canonical_0013.rs` /
//! `migration_canonical_0012.rs`):
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` uses
//!    `IF NOT EXISTS`.
//! 2. **PRIMARY KEY** — single-row-per-region PK on `global_circuit_state`
//!    + composite `(region, tripped_at)` region-leftmost on
//!    `global_circuit_trips_history` (CTRL-ISO-005 + region-leftmost
//!    forensic scan).
//! 3. **CHECK constraint coverage** — every constraint listed in
//!    WI §6 must be present.
//! 4. **state CHECK lists the canonical 3 literals** (`closed`, `open`,
//!    `half_open`).
//! 5. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden.
//! 6. **No BEGIN/COMMIT** — `wrangler d1 migrations apply` uses an
//!    implicit transaction.
//! 7. **Schema version bump to 14**.
//! 8. **Canonical invariant references in header comments** — the
//!    migration header MUST cite INV-AVAIL-ISOLATION +
//!    INV-AUDIT-APPEND-ONLY + INV-TENANT-ISOLATION + CAP-RATE-004 +
//!    R-S08-4 + R-S08-8 + R-S08-9 so future drift is caught at code
//!    review.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_rate_headers::{
    rate_headers_schema_version, MIGRATION_0014_GLOBAL_CIRCUIT_STATE,
};

/// Strip `-- …` line comments before scanning.
fn migration_sql_no_comments() -> String {
    MIGRATION_0014_GLOBAL_CIRCUIT_STATE
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
fn migration_0014_is_non_empty() {
    assert!(!MIGRATION_0014_GLOBAL_CIRCUIT_STATE.is_empty());
}

#[test]
fn schema_version_is_fourteen() {
    assert_eq!(rate_headers_schema_version(), 14);
}

#[test]
fn create_state_table_uses_if_not_exists() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE TABLE IF NOT EXISTS global_circuit_state"),
        "global_circuit_state table must use IF NOT EXISTS"
    );
}

#[test]
fn create_trips_history_table_uses_if_not_exists() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains(
            "CREATE TABLE IF NOT EXISTS global_circuit_trips_history"
        ),
        "global_circuit_trips_history table must use IF NOT EXISTS"
    );
}

#[test]
fn create_indices_use_if_not_exists() {
    let sql = migration_sql_no_comments();
    let occurrences = sql.matches("CREATE INDEX IF NOT EXISTS").count();
    assert!(occurrences >= 3, "expected ≥ 3 indices; got {occurrences}");
}

#[test]
fn state_table_pk_is_region() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("region                      TEXT     NOT NULL PRIMARY KEY")
            || sql.contains("region TEXT NOT NULL PRIMARY KEY"),
        "global_circuit_state PK must be the region column"
    );
}

#[test]
fn trips_history_pk_is_region_leftmost_composite() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("PRIMARY KEY (region, tripped_at)"),
        "PK must be composite region-leftmost"
    );
}

#[test]
fn state_check_lists_canonical_three_literals() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("'closed'"));
    assert!(sql.contains("'open'"));
    assert!(sql.contains("'half_open'"));
}

#[test]
fn signal_5xx_rate_envelope_check_present() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("signal_5xx_rate >= 0.0 AND signal_5xx_rate <= 1.0")
    );
}

#[test]
fn signal_do_error_rate_envelope_check_present() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("signal_do_error_rate >= 0.0 AND signal_do_error_rate <= 1.0")
    );
}

#[test]
fn signal_p99_us_non_negative_check_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("signal_p99_us >= 0"));
}

#[test]
fn recovered_after_tripped_check_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("recovered_at IS NULL OR recovered_at >= tripped_at"));
}

#[test]
fn manual_override_consistent_check_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("trip_reason != 'ManualOverride'"));
    assert!(sql.contains("manual_override_admin_id IS NOT NULL"));
}

#[test]
fn no_destructive_tokens() {
    let sql = MIGRATION_0014_GLOBAL_CIRCUIT_STATE;
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
    let sql = MIGRATION_0014_GLOBAL_CIRCUIT_STATE;
    assert!(
        !sql.to_uppercase().contains("BEGIN;"),
        "wrangler d1 migrations apply uses an implicit transaction"
    );
    assert!(!sql.to_uppercase().contains("COMMIT;"));
}

#[test]
fn header_cites_canonical_invariant_references() {
    let sql = MIGRATION_0014_GLOBAL_CIRCUIT_STATE;
    assert!(sql.contains("INV-AVAIL-ISOLATION"));
    assert!(sql.contains("INV-AUDIT-APPEND-ONLY"));
    assert!(sql.contains("INV-TENANT-ISOLATION"));
    assert!(sql.contains("CAP-RATE-004"));
    assert!(sql.contains("R-S08-4"));
    assert!(sql.contains("R-S08-8"));
    assert!(sql.contains("R-S08-9"));
}

#[test]
fn header_cites_3_state_lifecycle() {
    let sql = MIGRATION_0014_GLOBAL_CIRCUIT_STATE;
    assert!(sql.contains("3-state"));
    assert!(sql.contains("Closed"));
    assert!(sql.contains("Open"));
    assert!(sql.contains("HalfOpen"));
}

#[test]
fn timestamp_columns_have_no_ms_suffix() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("state_since"));
    assert!(sql.contains("snapshotted_at"));
    assert!(sql.contains("tripped_at"));
    assert!(sql.contains("recovered_at"));
    assert!(
        !sql.contains("state_since_ms"),
        "Lote 10.7bis P0-3: no `_ms` suffix"
    );
    assert!(
        !sql.contains("snapshotted_at_ms"),
        "Lote 10.7bis P0-3: no `_ms` suffix"
    );
    assert!(
        !sql.contains("tripped_at_ms"),
        "Lote 10.7bis P0-3: no `_ms` suffix"
    );
}

#[test]
fn region_non_empty_check_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.matches("length(region) >= 1").count() >= 2);
}

#[test]
fn trip_reason_non_empty_check_present() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("length(trip_reason) >= 1"));
}

#[test]
fn ongoing_open_partial_index_present() {
    let sql = MIGRATION_0014_GLOBAL_CIRCUIT_STATE;
    assert!(sql.contains("WHERE recovered_at IS NULL"));
}

#[test]
fn manual_override_partial_index_present() {
    let sql = MIGRATION_0014_GLOBAL_CIRCUIT_STATE;
    assert!(sql.contains("WHERE trip_reason = 'ManualOverride'"));
}

#[test]
fn header_cites_lote_lessons() {
    let sql = MIGRATION_0014_GLOBAL_CIRCUIT_STATE;
    assert!(sql.contains("Lote 10.4bis"));
    assert!(sql.contains("Lote 10.6bis"));
    assert!(sql.contains("Lote 10.7bis P0-3"));
}
