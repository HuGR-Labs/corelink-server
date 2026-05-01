//! Canonical-text regression tests for `migrations/002_auth_tables.sql`.
//!
//! The simulator already covers the algorithmic invariants. This file
//! pins down a small set of textual properties of the SQL artifact so
//! that an accidental delete of a `CREATE TABLE` / RLS clause will
//! turn the test red instead of silently shipping. A live-Postgres
//! roundtrip lives in WI-S03-005 §10.5.1 and is gated on staging-Neon
//! credentials.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use corelink_auth_schema::{schema_version, MIGRATION_002_AUTH_TABLES};

/// Strip `-- …` line comments before scanning so prose comments cannot
/// produce false positives in the destructive-token / banned-default
/// regression checks.
fn migration_sql_no_comments() -> String {
    MIGRATION_002_AUTH_TABLES
        .lines()
        .map(|line| match line.find("--") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const TABLES: &[&str] = &[
    "account",
    "tenant",
    "user_account",
    "membership",
    "pat",
    "webauthn_credentials",
    "revocation_log",
];

const ENUM_TYPES: &[&str] = &[
    "account_type",
    "region_t",
    "tier_t",
    "role_t",
    "pat_env_t",
    "revocation_reason_t",
];

const RLS_TABLES: &[&str] = &[
    "account",
    "tenant",
    "user_account",
    "membership",
    "pat",
    "webauthn_credentials",
    "revocation_log",
];

#[test]
fn schema_version_constant_matches_migration_id() {
    // Sanity: the embedded migration mentions schema_version=2.
    assert_eq!(schema_version(), 2);
    assert!(MIGRATION_002_AUTH_TABLES.contains("INSERT INTO schema_version"));
    assert!(MIGRATION_002_AUTH_TABLES.contains("(2,"));
}

#[test]
fn every_canonical_table_is_created_idempotently() {
    for table in TABLES {
        let needle = format!("CREATE TABLE IF NOT EXISTS {table} ");
        assert!(
            MIGRATION_002_AUTH_TABLES.contains(&needle),
            "migration is missing CREATE TABLE IF NOT EXISTS for {table}"
        );
    }
}

#[test]
fn every_canonical_enum_type_is_declared() {
    for enum_name in ENUM_TYPES {
        let needle = format!("CREATE TYPE {enum_name} AS ENUM");
        assert!(
            MIGRATION_002_AUTH_TABLES.contains(&needle),
            "migration is missing CREATE TYPE for {enum_name}"
        );
    }
}

#[test]
fn rls_is_enabled_on_every_auth_table() {
    // Normalise multi-space sequences so the alignment whitespace in
    // the canonical migration text doesn't gate the check.
    let normalised: String = MIGRATION_002_AUTH_TABLES
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for table in RLS_TABLES {
        let needle = format!("ALTER TABLE {table} ENABLE ROW LEVEL SECURITY");
        assert!(
            normalised.contains(&needle),
            "migration is missing ENABLE ROW LEVEL SECURITY for {table}"
        );
    }
}

#[test]
fn rls_policies_consume_app_current_tenant_setting() {
    // INV-AUTH-SCHEMA-RLS-DEFAULT-ON: tenant-scoped tables filter via
    // current_setting('app.current_tenant', …). The canonical policy
    // names for the four tenant-scoped tables are recorded here.
    let canonical_policies = [
        "tenant_isolation_tenant",
        "tenant_isolation_membership",
        "tenant_isolation_pat",
        "tenant_isolation_revocation",
    ];
    for policy in canonical_policies {
        assert!(
            MIGRATION_002_AUTH_TABLES.contains(policy),
            "missing tenant-isolation policy: {policy}"
        );
    }
    assert!(MIGRATION_002_AUTH_TABLES.contains("current_setting('app.current_tenant'"));
}

#[test]
fn pgcrypto_extension_and_email_hash_guard_present() {
    assert!(MIGRATION_002_AUTH_TABLES.contains("CREATE EXTENSION IF NOT EXISTS pgcrypto"));
    assert!(MIGRATION_002_AUTH_TABLES.contains("app.email_hash_key"));
    assert!(MIGRATION_002_AUTH_TABLES.contains("RAISE EXCEPTION"));
}

#[test]
fn no_gen_random_uuid_default_anywhere() {
    // P0 Lote 10.3bis: UUIDs are minted app-side; no DDL may declare
    // `DEFAULT gen_random_uuid()` (which produces v4 instead of v7).
    // The check ignores `--` line comments so the rationale block can
    // mention the banned function.
    let sql = migration_sql_no_comments();
    assert!(
        !sql.contains("gen_random_uuid()"),
        "migration DDL leaks DEFAULT gen_random_uuid() — UUIDv7 must be \
         minted app-side via uuid::Uuid::now_v7() per WI-S03-005 §1"
    );
}

#[test]
fn cascade_chains_are_declared() {
    // INV-AUTH-CASCADE-DSR-COMPLETE: account → tenant → membership/pat;
    // user_account → membership/webauthn. Whitespace-normalised so the
    // alignment columns in the canonical text don't gate the check.
    let normalised: String = MIGRATION_002_AUTH_TABLES
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cascades = [
        "REFERENCES account(account_id) ON DELETE CASCADE",
        "REFERENCES tenant(tenant_id) ON DELETE CASCADE",
        "REFERENCES user_account(user_id) ON DELETE CASCADE",
    ];
    for cascade in cascades {
        assert!(
            normalised.contains(cascade),
            "missing FK cascade clause: {cascade}"
        );
    }
}

#[test]
fn additive_only_no_drop_in_canonical_migration() {
    // The canonical 002 migration is greenfield; no DROP / TRUNCATE /
    // RENAME tokens are allowed in actual DDL (rationale comments are
    // stripped before the check). Mirrors
    // scripts/check_migrations_additive.py.
    let banned = [
        "DROP TABLE",
        "DROP COLUMN",
        "DROP INDEX",
        "TRUNCATE",
        "RENAME COLUMN",
    ];
    let upper = migration_sql_no_comments().to_ascii_uppercase();
    for tok in banned {
        assert!(
            !upper.contains(tok),
            "migration DDL contains banned token {tok}"
        );
    }
}
