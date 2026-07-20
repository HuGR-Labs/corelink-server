//! Regression: the durable CAS/AC + DSR audit-outbox INSERT must satisfy the
//! migration-0023 residency trigger for a non-`'wnam'` tenant.
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
//! (test-passed / prod-broke).
//!
//! This test exercises the REAL schema (the exact `audit_outbox` DDL + trigger
//! from migrations 0001/0023 + a minimal `tenant` with 0028's `primary_region`)
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
    "  BEGIN SELECT RAISE(ABORT, 'residency_violation: audit_outbox.region must match tenant.primary_region'); END;"
);

/// The CURRENT (fixed) sink INSERT — `region` tagged from a correlated subquery.
/// Mirrors `storage/d1_audit_sink.rs::append` / `routes/dsr/audit.rs`.
const FIXED_INSERT: &str = "INSERT OR IGNORE INTO audit_outbox \
    (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at, region) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, \
            COALESCE((SELECT primary_region FROM tenant WHERE tenant_id = ?2), 'wnam'))";

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
    // RAISE(ABORT). This is the exact failure that fail-closed the handler to 503.
    let err = conn.execute(BUGGY_INSERT, bind()).unwrap_err();
    assert!(
        err.to_string().contains("residency_violation"),
        "the omitted-region INSERT must abort on the residency trigger, got: {err}"
    );
}

#[test]
fn fixed_insert_tolerates_absent_tenant_row() {
    let conn = seeded();
    // Tenant not present → COALESCE 'wnam'; the trigger's `NEW.region != NULL` is
    // UNKNOWN → no abort → INSERT succeeds (mirrors a pre-provisioning / synthetic
    // caller, and keeps the audit path fail-safe rather than fail-closed).
    let params: [&dyn rusqlite::ToSql; 7] = [
        &"id-2",
        &"t-missing",
        &"blake3:bb",
        &"req-2",
        &"corelink.ac.read_attempted",
        &"{}",
        &2_i64,
    ];
    let n = conn
        .execute(FIXED_INSERT, params)
        .expect("absent-tenant INSERT must succeed");
    assert_eq!(n, 1);
}
