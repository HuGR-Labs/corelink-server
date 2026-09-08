//! Migration-level regression for pilot-signup audit writes.
//!
//! The pilot route is intentionally pre-tenant. This test executes the exact
//! INSERT built by [`D1AuditOutboxSink`] against the production-shaped D1
//! table/triggers, including both denied requests (no tenant id) and a
//! reservation carrying its future tenant id in the event payload.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use super::tests_support::stub_sink;
use super::*;
use crate::routes::signup::{
    SignupAuditRow, EVENT_TYPE_PILOT_RATE_LIMITED, EVENT_TYPE_PILOT_RESERVED,
    EVENT_TYPE_PILOT_TOKEN_REJECTED, PILOT_AUDIT_NAMESPACE, PILOT_AUDIT_REGION,
};
use rusqlite::{params_from_iter, types::Value as SqlValue, Connection};
use serde_json::Value;
use uuid::Uuid;

/// Minimal production-shaped schema: the `region` match trigger from 0023
/// and the explicit missing-tenant guard from migration 0107.
const SCHEMA: &str = r#"
CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY, primary_region TEXT);
CREATE TABLE audit_outbox (
  id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, digest TEXT, request_id TEXT NOT NULL,
  event_type TEXT NOT NULL, payload_json TEXT NOT NULL, enqueued_at INTEGER NOT NULL,
  emitted_at INTEGER, region TEXT NOT NULL DEFAULT 'wnam'
    CHECK (region IN ('wnam','enam','weur','sam','apac','afr')),
  UNIQUE (request_id, event_type));
CREATE TRIGGER trg_audit_outbox_region_match_insert BEFORE INSERT ON audit_outbox
  FOR EACH ROW WHEN NEW.region != (SELECT primary_region FROM tenant WHERE tenant_id = NEW.tenant_id)
  BEGIN SELECT RAISE(ABORT, 'residency_violation: audit_outbox.region must match tenant.primary_region'); END;
CREATE TRIGGER trg_audit_outbox_tenant_residency_required BEFORE INSERT ON audit_outbox
  FOR EACH ROW WHEN NEW.tenant_id != '_public'
    AND NOT EXISTS (SELECT 1 FROM tenant WHERE tenant_id = NEW.tenant_id AND primary_region = NEW.region)
    AND NOT EXISTS (SELECT 1 FROM audit_outbox AS existing WHERE existing.id = NEW.id)
  BEGIN SELECT RAISE(ABORT, 'residency_unprovable: audit_outbox tenant_id must resolve to matching tenant.primary_region'); END;
"#;

fn sqlite_value(value: &Value) -> SqlValue {
    match value {
        Value::Null => SqlValue::Null,
        Value::Bool(v) => SqlValue::Integer(i64::from(*v)),
        Value::Number(v) => v
            .as_i64()
            .map(SqlValue::Integer)
            .or_else(|| v.as_f64().map(SqlValue::Real))
            .expect("D1 test number must be representable by SQLite"),
        Value::String(v) => SqlValue::Text(v.clone()),
        Value::Array(_) | Value::Object(_) => panic!("D1 bind must be scalar"),
    }
}

fn run_insert(conn: &Connection, sink: &D1AuditOutboxSink, row: SignupAuditRow) -> usize {
    let (sql, params) = sink
        .build_signup_insert(row)
        .expect("signup audit INSERT builds");
    assert!(sql.contains("VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)"));
    assert!(!sql.contains("SELECT primary_region"));
    let params = params.iter().map(sqlite_value).collect::<Vec<_>>();
    conn.execute(&sql, params_from_iter(params))
        .expect("D1-shaped INSERT succeeds");
    1
}

#[test]
fn pre_tenant_pilot_audits_use_public_namespace_and_pass_0107() {
    let conn = Connection::open_in_memory().expect("sqlite");
    conn.execute_batch(SCHEMA)
        .expect("production-shaped schema");
    let sink = stub_sink();
    let future_tenant = Uuid::new_v4();

    // 401 and 429 arms have no tenant yet. They must not serialize the nil
    // UUID: migration 0107 rejects unknown tenant identities fail-CLOSED.
    run_insert(
        &conn,
        &sink,
        SignupAuditRow {
            event_type: EVENT_TYPE_PILOT_TOKEN_REJECTED.to_owned(),
            tenant_id: None,
            token_id_or_prefix: Some("pilot_prod_bad".to_owned()),
            exit_status: "rejected".to_owned(),
            payload: Some("signature_mismatch".to_owned()),
            emitted_at_ms: 1,
        },
    );
    run_insert(
        &conn,
        &sink,
        SignupAuditRow {
            event_type: EVENT_TYPE_PILOT_RATE_LIMITED.to_owned(),
            tenant_id: None,
            token_id_or_prefix: Some("pilot_prod_limited".to_owned()),
            exit_status: "rate_limited".to_owned(),
            payload: Some("retry_after_secs=720".to_owned()),
            emitted_at_ms: 2,
        },
    );

    // A successful pilot reservation also remains pre-tenant: its UUID is a
    // correlation value in pilot_signups until operator provisioning creates
    // tenant.primary_region. It therefore uses the same public namespace.
    run_insert(
        &conn,
        &sink,
        SignupAuditRow {
            event_type: EVENT_TYPE_PILOT_RESERVED.to_owned(),
            tenant_id: Some(future_tenant),
            token_id_or_prefix: Some("0123456789abcdef".to_owned()),
            exit_status: "reserved".to_owned(),
            payload: None,
            emitted_at_ms: 3,
        },
    );

    let mut stmt = conn
        .prepare("SELECT tenant_id, region, payload_json FROM audit_outbox ORDER BY enqueued_at")
        .expect("audit rows query");
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .expect("audit rows map")
        .collect::<Result<Vec<_>, _>>()
        .expect("audit rows decode");
    assert_eq!(rows.len(), 3);
    for (tenant, region, _) in &rows {
        assert_eq!(tenant, PILOT_AUDIT_NAMESPACE);
        assert_ne!(tenant, &Uuid::nil().to_string());
        assert_eq!(region, PILOT_AUDIT_REGION);
    }
    let reserved_payload: Value = serde_json::from_str(&rows[2].2).expect("CloudEvents JSON");
    assert_eq!(
        reserved_payload["data"]["tenant_id"],
        future_tenant.to_string(),
        "future tenant id remains available for pilot correlation"
    );
}
