//! Regression: the durable CAS/AC + DSR audit-outbox INSERT must satisfy the
//! migration-0023/0107 residency triggers for a non-`'wnam'` tenant.
//!
//! # Why this exists (prod incident 2026-07-17)
//!
//! `D1AuditOutboxSink` (`storage/d1_audit_sink.rs`) and the DSR erasure sink
//! (`routes/dsr/audit.rs`) originally INSERTed into `audit_outbox` **omitting the
//! `region` column**, relying on its `DEFAULT 'wnam'`. But migration 0023
//! installs a `BEFORE INSERT` trigger that `RAISE(ABORT)`s when
//! `NEW.region != (SELECT primary_region FROM tenant …)`, and tenants default to
//! `'enam'` (migration 0028). So once task #74 flipped the CAS/AC audit sink from
//! the in-memory fake to durable D1, EVERY CAS/AC op's audit INSERT aborted →
//! `AuditFailed` → **503 "audit closed"** on every tenant. The unit tests used a
//! mock D1 with no trigger, so they were green while prod was down
//! (test-passed / prod-broke). Migration 0107 also closes the SQLite
//! NULL-comparison hole: an unknown tenant is now rejected instead of creating
//! another unevaluable row.
//!
//! This test exercises the REAL schema (the exact `audit_outbox` DDL + trigger
//! from migrations 0001/0023/0107 + a minimal `tenant` with 0028's `primary_region`)
//! against bundled SQLite (byte-identical trigger/CHECK semantics to D1), so a
//! future edit that stops tagging `region` correctly reddens here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use rusqlite::Connection;

/// The `audit_outbox` schema as deployed: columns from migration 0001, plus the
/// `region` column + residency trigger from migration 0023, plus a minimal
/// `tenant(primary_region)` (migration 0028). Copied verbatim from the migrations
/// so the trigger/CHECK/DEFAULT semantics match prod exactly.
const SCHEMA: &str = concat!(
    "CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY, primary_region TEXT); ",
    "CREATE TABLE audit_outbox (",
    "  id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, digest TEXT, request_id TEXT NOT NULL, ",
    "  event_type TEXT NOT NULL, payload_json TEXT NOT NULL, enqueued_at INTEGER NOT NULL, ",
    "  emitted_at INTEGER, ",
    "  region TEXT NOT NULL DEFAULT 'wnam' CHECK (region IN ('wnam','enam','weur','sam','apac','afr')), ",
    "  UNIQUE (request_id, event_type)); ",
    "CREATE TRIGGER trg_audit_outbox_region_match_insert BEFORE INSERT ON audit_outbox ",
    "  FOR EACH ROW WHEN NEW.region != (SELECT primary_region FROM tenant WHERE tenant_id = NEW.tenant_id) ",
    "  BEGIN SELECT RAISE(ABORT, 'residency_violation: audit_outbox.region must match tenant.primary_region'); END; ",
    "CREATE TRIGGER trg_audit_outbox_tenant_residency_required BEFORE INSERT ON audit_outbox ",
    "  FOR EACH ROW WHEN NEW.tenant_id != '_public' ",
    "    AND NOT EXISTS (SELECT 1 FROM tenant WHERE tenant_id = NEW.tenant_id AND primary_region = NEW.region) ",
    "    AND NOT EXISTS (SELECT 1 FROM audit_outbox AS existing WHERE existing.id = NEW.id) ",
    "  BEGIN SELECT RAISE(ABORT, 'residency_unprovable: audit_outbox tenant_id must resolve to matching tenant.primary_region'); END;"
);

/// The CURRENT (fixed) sink INSERT — `region` tagged from a correlated subquery.
/// Mirrors `storage/d1_audit_sink.rs::append` / `routes/dsr/audit.rs`.
const FIXED_INSERT: &str = "INSERT OR IGNORE INTO audit_outbox \
    (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at, region) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, \
            (SELECT primary_region FROM tenant WHERE tenant_id = ?2))";

/// The OLD (buggy) INSERT — `region` omitted ⇒ `DEFAULT 'wnam'`. Kept only to
/// PROVE the regression (it aborts for an `'enam'` tenant).
const BUGGY_INSERT: &str = "INSERT OR IGNORE INTO audit_outbox \
    (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)";

fn seeded() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    // A tenant with the DEFAULT residency region (migration 0028 backfills 'enam').
    conn.execute(
        "INSERT INTO tenant (tenant_id, primary_region) VALUES ('t-enam', 'enam')",
        [],
    )
    .unwrap();
    conn
}

fn bind() -> [&'static dyn rusqlite::ToSql; 7] {
    [
        &"id-1",
        &"t-enam",
        &"blake3:aa",
        &"req-1",
        &"corelink.cas.read_attempted",
        &"{}",
        &1_i64,
    ]
}

#[test]
fn fixed_insert_satisfies_residency_trigger_for_enam_tenant() {
    let conn = seeded();
    // The fix: region is tagged from tenant.primary_region ('enam') → trigger
    // condition `'enam' != 'enam'` is false → INSERT succeeds.
    let n = conn
        .execute(FIXED_INSERT, bind())
        .expect("fixed INSERT must succeed");
    assert_eq!(n, 1, "the audit row must be persisted");
    let region: String = conn
        .query_row("SELECT region FROM audit_outbox WHERE id='id-1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        region, "enam",
        "row must be tagged with the tenant's residency region"
    );
}

#[test]
fn buggy_insert_aborts_on_the_residency_trigger_fail_before() {
    let conn = seeded();
    // The prod incident: region omitted ⇒ 'wnam' ⇒ trigger `'wnam' != 'enam'` ⇒
    // RAISE(ABORT). SQLite runs same-table BEFORE triggers in reverse creation
    // order, so migration 0107 may report `residency_unprovable` before the
    // older 0023 `residency_violation`; both are the real fail-closed guards.
    let err = conn.execute(BUGGY_INSERT, bind()).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("residency_unprovable") || message.contains("residency_violation"),
        "the omitted-region INSERT must abort on a residency guard, got: {message}"
    );
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_outbox WHERE id='id-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        rows, 0,
        "a rejected audit INSERT must not leave a row behind"
    );
}

#[test]
fn unknown_tenant_is_rejected_instead_of_becoming_unevaluable() {
    let conn = seeded();
    // Tenant not present used to pass because `NEW.region != NULL` is UNKNOWN.
    // Migration 0107 makes the missing-tenant arm explicit and fails CLOSED.
    let params: [&dyn rusqlite::ToSql; 7] = [
        &"id-2",
        &"t-missing",
        &"blake3:bb",
        &"req-2",
        &"corelink.ac.read_attempted",
        &"{}",
        &2_i64,
    ];
    let err = conn.execute(FIXED_INSERT, params).unwrap_err();
    assert!(
        err.to_string().contains("residency_unprovable"),
        "unknown tenant must be rejected, got: {err}"
    );
}

#[test]
fn public_namespace_is_explicitly_allowed() {
    let conn = seeded();
    let params: [&dyn rusqlite::ToSql; 7] = [
        &"id-public",
        &"_public",
        &"digest",
        &"req-public",
        &"public.revoke",
        &"{}",
        &3_i64,
    ];
    let n = conn
        .execute(
            "INSERT OR IGNORE INTO audit_outbox \
             (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at, region) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 'wnam')",
            params,
        )
        .expect("public namespace must remain writable");
    assert_eq!(n, 1);
}

#[test]
fn deterministic_retry_of_retained_row_is_idempotent_after_erasure() {
    let conn = seeded();
    let params = bind();
    conn.execute(FIXED_INSERT, params).unwrap();
    conn.execute("DELETE FROM tenant WHERE tenant_id = 't-enam'", [])
        .unwrap();
    let n = conn
        .execute(FIXED_INSERT, params)
        .expect("retry of an existing retained row must remain idempotent");
    assert_eq!(n, 0);
}
