//! Canonical-text regression tests for
//! `migrations/d1/0011_edge_blocklist.sql` (WI-S08-002; per-IP edge
//! CIDR blocklist durable source-of-truth; CF List replica deferred to
//! WI-S08-006).
//!
//! The orchestrator already covers the algorithmic invariants. This
//! file pins down a small set of textual properties of the SQL artifact
//! so that an accidental delete of a `CREATE TABLE` / index / CHECK
//! clause turns the test red instead of silently shipping. A live-D1
//! roundtrip lands alongside WI-S08-006 (PRR ship gate).
//!
//! Coverage (mirror of `migration_canonical_0010.rs`):
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` uses
//!    `IF NOT EXISTS`.
//! 2. **PRIMARY KEY direction** — PK is `cidr_text` (canonical
//!    normalised form is the load-bearing identity at the edge layer;
//!    no tenant_id at the pre-auth boundary per WI §6.1.8 +
//!    INV-AVAIL-ISOLATION).
//! 3. **CHECK constraint coverage** — every constraint listed in
//!    WI §6 must be present.
//! 4. **reason CHECK lists the canonical 4 literals**
//!    (`Sustained4xx`, `DDoSPattern`, `ManualAdmin`, `Compliance`).
//! 5. **source CHECK lists the canonical 2 literals**
//!    (`Manual`, `AutomatedSuggestionApproved`).
//! 6. **cidr_family CHECK lists the canonical {4, 6} pair**.
//! 7. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden.
//! 8. **No BEGIN/COMMIT** — `wrangler d1 migrations apply` uses an
//!    implicit transaction.
//! 9. **Schema version bump to 11**.
//! 10. **Canonical invariant references in header comments** — the
//!     migration header MUST cite INV-AVAIL-ISOLATION + INV-AUDIT-APPEND-ONLY
//!     so future drift is caught at code review.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_edge::{
    edge_schema_version, MIGRATION_0011_EDGE_BLOCKLIST,
};

/// Strip `-- …` line comments before scanning.
fn migration_sql_no_comments() -> String {
    MIGRATION_0011_EDGE_BLOCKLIST
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
    assert!(!MIGRATION_0011_EDGE_BLOCKLIST.is_empty());
    assert!(MIGRATION_0011_EDGE_BLOCKLIST.contains("migration 0011"));
    assert_eq!(edge_schema_version(), 11);
}

#[test]
fn create_table_and_indexes_are_idempotent() {
    let sql = migration_sql_no_comments();
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS edge_blocklist"));
    assert!(sql.contains(
        "CREATE INDEX IF NOT EXISTS idx_edge_blocklist_active_family_prefix"
    ));
    assert!(
        sql.contains("CREATE INDEX IF NOT EXISTS idx_edge_blocklist_unsynced")
    );
}

#[test]
fn primary_key_is_cidr_text() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("PRIMARY KEY (cidr_text)"),
        "PK must be (cidr_text) — canonical normalised form is the edge-layer identity"
    );
}

#[test]
fn cidr_family_check_lists_canonical_pair() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CHECK (cidr_family IN (4, 6))"),
        "cidr_family CHECK must list {{4, 6}}"
    );
}

#[test]
fn reason_check_lists_canonical_4_literals() {
    let sql = migration_sql_no_comments();
    for canonical in ["Sustained4xx", "DDoSPattern", "ManualAdmin", "Compliance"] {
        assert!(
            sql.contains(&format!("'{canonical}'")),
            "SQL CHECK missing canonical reason literal {canonical}"
        );
    }
    assert!(sql.contains("CHECK (reason IN ("));
}

#[test]
fn source_check_lists_canonical_2_literals() {
    let sql = migration_sql_no_comments();
    for canonical in ["Manual", "AutomatedSuggestionApproved"] {
        assert!(
            sql.contains(&format!("'{canonical}'")),
            "SQL CHECK missing canonical source literal {canonical}"
        );
    }
    assert!(
        sql.contains("CHECK (source IN ('Manual', 'AutomatedSuggestionApproved'))")
    );
}

#[test]
fn check_constraint_coverage_matches_wi_spec() {
    let sql = migration_sql_no_comments();
    // cidr_prefix_len envelopes.
    assert!(sql.contains("CHECK (cidr_prefix_len >= 0)"));
    assert!(sql.contains("(cidr_family = 4 AND cidr_prefix_len <= 32)"));
    assert!(sql.contains("(cidr_family = 6 AND cidr_prefix_len <= 128)"));
    // Timestamp envelopes.
    assert!(sql.contains("CHECK (added_at_ms >= 0)"));
    assert!(sql.contains(
        "CHECK (expires_at_ms IS NULL OR expires_at_ms > added_at_ms)"
    ));
    assert!(sql.contains(
        "CHECK (deleted_at_ms IS NULL OR deleted_at_ms >= added_at_ms)"
    ));
    // cidr_text non-empty.
    assert!(sql.contains("CHECK (length(cidr_text) >= 1)"));
}

#[test]
fn destructive_tokens_are_forbidden() {
    let sql_upper = MIGRATION_0011_EDGE_BLOCKLIST.to_uppercase();
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
    let sql_upper = MIGRATION_0011_EDGE_BLOCKLIST.to_uppercase();
    assert!(!sql_upper.contains("BEGIN;"));
    assert!(!sql_upper.contains("COMMIT;"));
}

#[test]
fn schema_version_pinned_at_11() {
    assert_eq!(edge_schema_version(), 11);
}

#[test]
fn migration_carries_canonical_invariant_references() {
    assert!(MIGRATION_0011_EDGE_BLOCKLIST.contains("INV-AVAIL-ISOLATION"));
    assert!(MIGRATION_0011_EDGE_BLOCKLIST.contains("INV-AUDIT-APPEND-ONLY"));
}
