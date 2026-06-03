#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
//! Canonical-text regression tests for
//! `migrations/d1/0003_multipart_chunks_manifest.sql`.
//!
//! The simulator already covers the algorithmic invariants. This file
//! pins down a small set of textual properties of the SQL artifact so
//! that an accidental delete of a `CREATE TABLE` / index / CHECK
//! clause turns the test red instead of silently shipping. A live-D1
//! roundtrip (via miniflare / wrangler-dev) lands in WI-S05-006
//! alongside the multipart sweeper conformance suite.
//!
//! Coverage:
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` /
//!    `CREATE UNIQUE INDEX` uses `IF NOT EXISTS`.
//! 2. **PRIMARY KEY direction** — every PK is tenant-leftmost (`chunks`
//!    `(tenant_id, chunk_digest)`; `manifest_chunks` `(tenant_id,
//!    blob_digest, chunk_index)`; `multipart_sessions` `session_id` PK
//!    + `tenant_id` binding column).
//! 3. **Partial UNIQUE INDEX** — the
//!    `uq_multipart_sessions_in_progress` partial UNIQUE INDEX scoped
//!    to `WHERE state = 'in_progress'` (Lote 10.5bis P0 fix).
//! 4. **CHECK constraint coverage** — every constraint mnemonic listed
//!    in the migration header is present (defense-in-depth: handler
//!    validates AND schema CHECK catches handler-bypass refactors).
//! 5. **Region literal completeness** — the
//!    `chk_{chunks,multipart}_region` IN-list cites all 5 canonical
//!    regions in the order WI §1 freezes.
//! 6. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden; the
//!    `scripts/check_migrations_additive.py` CI gate also runs over
//!    this file but the test pins the property at crate-test scope.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_cas::multipart_schema::{
    multipart_schema_version, MultipartRegion, MIGRATION_0003_MULTIPART_CHUNKS_MANIFEST,
    REGION_LIST,
};

/// Strip `-- …` line comments before scanning so prose comments cannot
/// produce false positives on the destructive-token regression checks.
fn migration_sql_no_comments() -> String {
    MIGRATION_0003_MULTIPART_CHUNKS_MANIFEST
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
    assert!(!MIGRATION_0003_MULTIPART_CHUNKS_MANIFEST.is_empty());
    assert!(MIGRATION_0003_MULTIPART_CHUNKS_MANIFEST.contains("migration 0003"));
    assert_eq!(multipart_schema_version(), 3);
}

#[test]
fn three_canonical_tables_created_idempotently() {
    let sql = migration_sql_no_comments();
    for needle in [
        "CREATE TABLE IF NOT EXISTS chunks",
        "CREATE TABLE IF NOT EXISTS manifest_chunks",
        "CREATE TABLE IF NOT EXISTS multipart_sessions",
    ] {
        assert!(
            sql.contains(needle),
            "missing idempotent CREATE TABLE: {needle}"
        );
    }
}

#[test]
fn chunks_pk_is_tenant_leftmost() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("PRIMARY KEY (tenant_id, chunk_digest)"),
        "chunks PK must be (tenant_id, chunk_digest) — tenant_id leftmost \
         per WI §1 + WI §9.1; reverse direction would weaken Layer-4 storage envelope"
    );
    assert!(
        !sql.contains("PRIMARY KEY (chunk_digest, tenant_id)"),
        "reverse PRIMARY KEY direction is forbidden by WI §7 anti-scope"
    );
}

#[test]
fn manifest_chunks_pk_is_tenant_leftmost_then_blob_then_index() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("PRIMARY KEY (tenant_id, blob_digest, chunk_index)"),
        "manifest_chunks PK must be (tenant_id, blob_digest, chunk_index) — \
         tenant_id leftmost; ordered scan-by-chunk_index uses PK btree directly"
    );
}

#[test]
fn multipart_sessions_session_id_is_pk_with_tenant_id_binding_column() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("session_id          TEXT        PRIMARY KEY"),
        "multipart_sessions session_id must be the PRIMARY KEY"
    );
    assert!(
        sql.contains("tenant_id           TEXT        NOT NULL"),
        "multipart_sessions tenant_id must be NOT NULL (cross-tenant binding column)"
    );
}

#[test]
fn partial_unique_index_on_in_progress_sessions() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE UNIQUE INDEX IF NOT EXISTS uq_multipart_sessions_in_progress"),
        "partial UNIQUE INDEX uq_multipart_sessions_in_progress must be declared idempotently"
    );
    assert!(
        sql.contains("ON multipart_sessions(tenant_id, blob_digest_expected)"),
        "partial UNIQUE must cover (tenant_id, blob_digest_expected) tuple"
    );
    assert!(
        sql.contains("WHERE state = 'in_progress'"),
        "partial UNIQUE must be scoped to state='in_progress' per Lote 10.5bis P0 fix"
    );
}

#[test]
fn every_canonical_check_constraint_is_present() {
    // Mnemonics live inline as `-- chk_*` annotations next to the
    // enforcing SQL clause, so we scan the raw migration (with
    // comments) here. The pure-SQL no-comments view is used for
    // destructive-token checks below.
    let sql = MIGRATION_0003_MULTIPART_CHUNKS_MANIFEST;
    let needles = [
        // chunks
        "chk_chunks_digest_len",
        "chk_chunks_tenant_prefix_len",
        "chk_chunks_path_key_id_positive",
        "chk_chunks_region",
        "chk_chunks_size_bytes",
        "chk_chunks_refcount_non_negative",
        "chk_chunks_lifecycle",
        // manifest_chunks
        "chk_manifest_blob_digest_len",
        "chk_manifest_chunk_digest_len",
        "chk_manifest_chunk_index_bounded",
        // multipart_sessions
        "chk_multipart_blob_digest_len",
        "chk_multipart_tenant_prefix_len",
        "chk_multipart_path_key_id_positive",
        "chk_multipart_region",
        "chk_multipart_state_domain",
        "chk_multipart_lifecycle_activity",
        "chk_multipart_lifecycle_expires",
        "chk_multipart_lifecycle_finalized",
        "chk_multipart_finalized_iff_terminal",
    ];
    for n in needles {
        assert!(sql.contains(n), "missing CHECK constraint mnemonic: {n}");
    }
}

#[test]
fn every_canonical_check_clause_is_inline_in_create_table() {
    // CHECK constraints must be inline in CREATE TABLE per ADR-0036
    // Rule 1 (D1/SQLite has no ALTER TABLE ADD CONSTRAINT).
    let sql = migration_sql_no_comments();
    let predicates = [
        // chunks
        "length(chunk_digest) = 64",
        "length(tenant_prefix) = 16",
        "path_key_id >= 1",
        "size_bytes >= 1 AND size_bytes <= 4194304",
        "refcount >= 0",
        "last_referenced_at >= created_at",
        // manifest_chunks
        "length(blob_digest) = 64",
        "chunk_index >= 0 AND chunk_index < 81920",
        // multipart_sessions
        "length(blob_digest_expected) = 64",
        "state IN ('in_progress', 'completed', 'aborted')",
        "last_activity_at >= started_at",
        "expires_at_ms >= started_at",
    ];
    for p in predicates {
        assert!(sql.contains(p), "missing inline CHECK predicate: `{p}`");
    }
}

#[test]
fn region_check_constraints_list_all_five_canonical_regions() {
    let sql = migration_sql_no_comments();
    // Lower-bound check: every region literal appears within the file.
    for r in REGION_LIST {
        assert!(
            sql.contains(&format!("'{r}'")),
            "region literal '{r}' missing from migration"
        );
    }
    // The CHECK constraint must enumerate all 5 regions in a single IN list.
    let in_list = "region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')";
    let occurrences = sql.matches(in_list).count();
    assert!(
        occurrences >= 2,
        "expected canonical region IN list `{in_list}` at least twice (chunks + multipart_sessions); found {occurrences}"
    );
}

#[test]
fn canonical_indices_exist_idempotently() {
    let sql = migration_sql_no_comments();
    let names = [
        "idx_chunks_tenant_last_ref",
        "idx_chunks_tenant_refcount_zero",
        "idx_manifest_chunks_tenant_chunk",
        "idx_multipart_sessions_orphan_sweep",
        "idx_multipart_sessions_tenant_state",
    ];
    for name in names {
        let needle = format!("CREATE INDEX IF NOT EXISTS {name}");
        assert!(sql.contains(&needle), "missing index DDL: {needle}");
    }
}

#[test]
fn orphan_sweep_index_is_partial_on_in_progress() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("idx_multipart_sessions_orphan_sweep\n  ON multipart_sessions(last_activity_at)\n  WHERE state = 'in_progress'"),
        "orphan sweep index must be PARTIAL on state='in_progress' (cheap scan over only the active sessions)"
    );
}

#[test]
fn refcount_zero_index_is_partial() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("idx_chunks_tenant_refcount_zero"),
        "GC candidate index must exist"
    );
    assert!(
        sql.contains("WHERE refcount = 0"),
        "GC candidate index must be PARTIAL on refcount=0 (small + cheap to scan)"
    );
}

#[test]
fn tenant_id_is_not_null_everywhere() {
    let sql = migration_sql_no_comments();
    // tenant_id NULLABLE = INV-TENANT-ISOLATION CRITICAL violated.
    let occurrences = sql
        .matches("tenant_id           TEXT        NOT NULL")
        .count();
    assert!(
        occurrences >= 3,
        "tenant_id must be NOT NULL on every table that has it (chunks, manifest_chunks, multipart_sessions); found {occurrences}"
    );
}

#[test]
fn tenant_prefix_is_blob_with_length_check_in_chunks_and_sessions() {
    let sql = migration_sql_no_comments();
    let occurrences = sql
        .matches("tenant_prefix       BLOB        NOT NULL")
        .count();
    assert!(
        occurrences >= 2,
        "tenant_prefix must be BLOB NOT NULL on chunks AND multipart_sessions (ADR-0035 H-3); found {occurrences}"
    );
    let length_check_count = sql.matches("length(tenant_prefix) = 16").count();
    assert!(
        length_check_count >= 2,
        "length(tenant_prefix) = 16 CHECK must appear on chunks AND multipart_sessions; found {length_check_count}"
    );
}

#[test]
fn no_destructive_tokens_present() {
    let sql = migration_sql_no_comments();
    let upper = sql.to_uppercase();
    let banned = [
        "DROP TABLE",
        "DROP COLUMN",
        "DROP INDEX",
        "DROP CONSTRAINT",
        "ALTER COLUMN",
        "RENAME COLUMN",
        "RENAME TABLE",
        "RENAME TO",
        "TRUNCATE",
    ];
    for token in banned {
        assert!(
            !upper.contains(token),
            "destructive SQL token `{token}` present in migration; INV-AUTH-MIGRATION-ADDITIVE violated"
        );
    }
}

#[test]
fn no_begin_or_commit_wrapper() {
    let sql = migration_sql_no_comments();
    let upper = sql.to_uppercase();
    // wrangler d1 migrations apply uses an implicit transaction;
    // including BEGIN/COMMIT here would conflict + fail (lesson Lote
    // 10.4bis WI-S04-002).
    assert!(
        !upper.contains("BEGIN TRANSACTION") && !upper.contains("BEGIN;"),
        "explicit BEGIN forbidden — wrangler d1 wraps in implicit transaction"
    );
    assert!(
        !upper.contains("COMMIT;"),
        "explicit COMMIT forbidden — wrangler d1 wraps in implicit transaction"
    );
}

#[test]
fn no_alter_add_constraint_chk() {
    let sql = migration_sql_no_comments();
    let upper = sql.to_uppercase();
    // SQLite/D1 does NOT support `ALTER TABLE … ADD CONSTRAINT chk_*`.
    // CHECK clauses are inline in CREATE TABLE only (lesson Lote 10.4bis).
    assert!(
        !upper.contains("ADD CONSTRAINT"),
        "ALTER TABLE ADD CONSTRAINT forbidden — SQLite/D1 unsupported per ADR-0036 Rule 1"
    );
}

#[test]
fn region_check_list_matches_multipart_region_enum() {
    // Defensive cross-check: every variant in the Rust enum must
    // appear in the SQL CHECK list.
    for region in MultipartRegion::all() {
        assert!(
            REGION_LIST.contains(&region.as_str()),
            "MultipartRegion::{:?} missing from REGION_LIST",
            region
        );
    }
    assert_eq!(REGION_LIST.len(), MultipartRegion::all().len());
}

#[test]
fn finalized_iff_terminal_check_present() {
    let sql = migration_sql_no_comments();
    // Self-consistency invariant: in_progress ⇔ finalized_at_ms IS NULL.
    assert!(
        sql.contains("(state = 'in_progress') = (finalized_at_ms IS NULL)"),
        "chk_multipart_finalized_iff_terminal CHECK must enforce in_progress ⇔ NULL finalized_at_ms"
    );
}
