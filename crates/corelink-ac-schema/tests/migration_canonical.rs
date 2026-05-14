#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]
//! Canonical-text regression tests for `migrations/d1/0002_ac_meta.sql`.
//!
//! The simulator already covers the algorithmic invariants. This file
//! pins down a small set of textual properties of the SQL artifact so
//! that an accidental delete of a `CREATE TABLE` / index / CHECK
//! clause will turn the test red instead of silently shipping. A
//! live-D1 roundtrip (via miniflare / wrangler-dev) lands in
//! WI-S04-006 alongside the REAPI conformance suite.
//!
//! Coverage:
//!
//! 1. **Idempotent DDL** — every `CREATE TABLE` / `CREATE INDEX` uses
//!    `IF NOT EXISTS`. `wrangler d1 migrations apply` is run on every
//!    deploy; a non-idempotent migration would explode on the second
//!    apply.
//! 2. **PRIMARY KEY direction** — the migration must declare
//!    `PRIMARY KEY (tenant_id, action_digest)` with `tenant_id`
//!    leftmost (per WI §1 + WI §9.1). A reverse PK
//!    `(action_digest, tenant_id)` would make tenant-scoped scans
//!    inefficient AND silently weaken the Layer-4 storage envelope.
//! 3. **CHECK constraint coverage** — every constraint listed in WI
//!    §1 must be present (defense-in-depth: handler validates AND
//!    schema CHECK catches handler-bypass refactors).
//! 4. **Region literal completeness** — the `chk_ac_region` IN-list
//!    must cite all 5 canonical regions in the order WI §1 freezes.
//! 5. **No destructive tokens** — `DROP TABLE` / `ALTER … DROP …` /
//!    `RENAME` / `TRUNCATE` are forbidden; the
//!    `scripts/check_migrations_additive.py` CI gate also runs over
//!    this file but the test pins the property at crate-test scope.
//! 6. **Three indices present** — `idx_ac_meta_tenant_expires`,
//!    `idx_ac_meta_tenant_last_hit`, `idx_ac_meta_region`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_ac_schema::{ac_schema_version, AcRegion, MIGRATION_0002_AC_META, REGION_LIST};

/// Strip `-- …` line comments before scanning so prose comments cannot
/// produce false positives on the destructive-token regression checks.
fn migration_sql_no_comments() -> String {
    MIGRATION_0002_AC_META
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
    assert!(!MIGRATION_0002_AC_META.is_empty());
    // The file is canonically named `0002_ac_meta.sql`; the rustdoc
    // header pins the migration number into the artefact prose.
    assert!(MIGRATION_0002_AC_META.contains("migration 0002"));
    assert_eq!(ac_schema_version(), 2);
}

#[test]
fn ac_meta_table_is_created_idempotently() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("CREATE TABLE IF NOT EXISTS ac_meta"),
        "ac_meta table must be CREATE TABLE IF NOT EXISTS"
    );
}

#[test]
fn primary_key_direction_is_tenant_first() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("PRIMARY KEY (tenant_id, action_digest)"),
        "PRIMARY KEY must be (tenant_id, action_digest) — tenant_id leftmost \
         per WI §1 + WI §9.1; reverse direction would weaken Layer-4 storage envelope"
    );
    assert!(
        !sql.contains("PRIMARY KEY (action_digest, tenant_id)"),
        "reverse PRIMARY KEY direction is forbidden by WI §7 anti-scope"
    );
}

#[test]
fn every_canonical_check_constraint_is_present() {
    // Mnemonics live inline as `-- chk_ac_*` annotations next to the
    // enforcing SQL clause, so we scan the raw migration (with
    // comments) here. The pure-SQL no-comments view is used for
    // destructive-token checks below.
    let sql = MIGRATION_0002_AC_META;
    let needles = [
        "chk_ac_action_digest_len",
        "chk_ac_result_hash_len",
        "chk_ac_blob_refs_size",
        "chk_ac_blob_refs_count",
        "chk_ac_result_size",
        "chk_ac_region",
        "chk_ac_sig_alg",
        "chk_ac_lifecycle",
        "chk_ac_tenant_prefix_len",
        "chk_ac_path_key_id_positive",
        "chk_ac_sig_key_id_positive",
    ];
    for n in needles {
        assert!(
            sql.contains(n),
            "missing CHECK constraint mnemonic: {n}"
        );
    }
}

#[test]
fn every_canonical_check_clause_is_inline_in_create_table() {
    // CHECK constraints must be inline in CREATE TABLE per ADR-0036
    // Rule 1 (D1/SQLite has no ALTER TABLE ADD CONSTRAINT). The
    // (post-comment-strip) SQL must mention each enforcing predicate.
    let sql = migration_sql_no_comments();
    let predicates = [
        "length(action_digest) = 64",
        "length(result_hash) = 64",
        "length(blob_refs) <= 10240",
        "blob_refs_count >= 0",
        "blob_refs_count <= 4096",
        "result_size_bytes >= 0",
        "result_size_bytes <= 1048576",
        "sig_alg = 'hkdf-sha256'",
        "last_hit_at >= created_at",
        "length(tenant_prefix) = 16",
        "path_key_id >= 1",
        "sig_key_id >= 1",
    ];
    for p in predicates {
        assert!(
            sql.contains(p),
            "missing inline CHECK predicate: `{p}`"
        );
    }
}

#[test]
fn region_check_constraint_lists_all_five_canonical_regions() {
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
    assert!(
        sql.contains(in_list),
        "expected canonical region IN list `{in_list}`"
    );
}

#[test]
fn three_canonical_indices_exist_idempotently() {
    let sql = migration_sql_no_comments();
    let names = [
        "idx_ac_meta_tenant_expires",
        "idx_ac_meta_tenant_last_hit",
        "idx_ac_meta_region",
    ];
    for name in names {
        let needle = format!("CREATE INDEX IF NOT EXISTS {name}");
        assert!(sql.contains(&needle), "missing index DDL: {needle}");
    }
}

#[test]
fn tenant_expires_index_is_partial_on_not_null() {
    let sql = migration_sql_no_comments();
    // The TTL index must be partial — most rows have NULL expires_at
    // initially (free tier / no expiry); a full index would be ~2x
    // size and degrade INSERT throughput per WI §9.4.
    assert!(
        sql.contains("ON ac_meta(tenant_id, expires_at)"),
        "tenant_expires index must include both columns in left-to-right order"
    );
    assert!(
        sql.contains("WHERE expires_at IS NOT NULL"),
        "tenant_expires index must be PARTIAL on `expires_at IS NOT NULL`"
    );
}

#[test]
fn tenant_id_is_not_null() {
    let sql = migration_sql_no_comments();
    // tenant_id NULLABLE = INV-AC-TENANT-SCOPED CRITICAL violated.
    assert!(
        sql.contains("tenant_id           TEXT        NOT NULL"),
        "tenant_id must be NOT NULL (INV-AC-TENANT-SCOPED CRITICAL)"
    );
}

#[test]
fn tenant_prefix_is_blob_with_length_check() {
    let sql = migration_sql_no_comments();
    assert!(
        sql.contains("tenant_prefix       BLOB        NOT NULL"),
        "tenant_prefix must be BLOB NOT NULL (ADR-0035 H-3 materialization)"
    );
    assert!(
        sql.contains("length(tenant_prefix) = 16"),
        "tenant_prefix length CHECK must enforce 16-byte HMAC truncation"
    );
}

#[test]
fn sig_alg_default_is_hkdf_sha256() {
    let sql = migration_sql_no_comments();
    // ADR-0021 / WI-S04-004 freeze hkdf-sha256 as the v1 algorithm.
    assert!(
        sql.contains("sig_alg             TEXT        NOT NULL DEFAULT 'hkdf-sha256'"),
        "sig_alg default must be 'hkdf-sha256' per ADR-0021"
    );
    assert!(
        sql.contains("sig_alg = 'hkdf-sha256'"),
        "sig_alg CHECK must whitelist 'hkdf-sha256' only (v2+ via ADR + new migration)"
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
    // including BEGIN/COMMIT here would conflict + fail (WI §1 P0 fix).
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
fn region_check_list_matches_acregion_enum() {
    // Defensive cross-check: every variant in the Rust enum must
    // appear in the SQL CHECK list, and no extra literal is present
    // in the SQL CHECK list that the enum doesn't represent.
    for region in AcRegion::all() {
        assert!(
            REGION_LIST.contains(&region.as_str()),
            "AcRegion::{:?} missing from REGION_LIST",
            region
        );
    }
    assert_eq!(REGION_LIST.len(), AcRegion::all().len());
}
