//! Production durable DLQ store + billing-audit emitter (D1 over HTTP).
//!
//! # What this closes
//!
//! - **WP-D1 / webhook DLQ durability**: the Stripe-webhook dispatcher's
//!   DLQ sink was wired to the IN-MEMORY store — every container redeploy
//!   evaporated the quarantine, so a paying customer whose
//!   `checkout.session.completed` hit a transient materialize failure
//!   stayed locked out invisibly and irrecoverably. The F-008
//!   dedup-before-materialize order is DELIBERATE (Stripe retries hit
//!   `AlreadyProcessed`); the DLQ is the designed mitigation and it must
//!   survive restarts. This module implements the
//!   `corelink_stripe_real::dlq::WebhookDlqStore` trait against the REAL
//!   D1 table (`stripe_webhook_events_dlq`, migrations 0045 + 0094) with
//!   the same sync↔async bridge as [`crate::billing_d1_http`].
//!
//! - **MED-5 / billing-audit evidence**: the materializer emits a
//!   billing-audit record BEFORE every state mutation, but the only
//!   native emitter was in-memory — durable STATE rows with VOLATILE
//!   evidence. [`D1BillingAuditEmitter`] appends each record to
//!   `stripe_billing_audit_events` (migration 0095), mirroring what the
//!   wasm32 production binder archives to the audit chain.
//!
//! # Why the impls live HERE and not in `corelink-stripe-real`
//!
//! Dependency direction: `corelink-container` depends on
//! `corelink-stripe-real`; the reverse would drag the async
//! [`D1HttpClient`] into the shared wasm32-targeted crate. Same precedent
//! as [`crate::billing_d1_http`], which implements the materializer's
//! `BillingD1Writer` trait here for the same reason.
//!
//! # Idempotency contract (`DLQ_IDEMPOTENT_ON_EVENT_ID`)
//!
//! One row per `event_id`. Migration 0094 adds the UNIQUE index that lets
//! the UPSERT enforce it at the SQL layer: first quarantine INSERTS
//! (`RETURNING attempt_count = 1` → `Inserted`); re-quarantine of the
//! same event_id UPDATEs the SAME row (`attempt_count + 1` → `Updated`)
//! so `dlq_row_id` never splits.
//!
//! # SECURITY / fail-CLOSED
//!
//! Parameterised SQL only; transport errors map to `DlqError::Backend`
//! / `BillingAuditError::Transient` (the dispatcher turns an emitter
//! failure into HTTP 500 → Stripe retries). Raw body bytes live ONLY in
//! the `raw_body_hex` column — NEVER logged.

use std::sync::Arc;

use corelink_billing_stripe_materializer::{BillingAuditError, BillingAuditRecord};
use corelink_stripe_real::dlq::{
    DlqError, DlqQuarantineOutcome, DlqReplayOutcome, WebhookDlqRow, WebhookDlqStore,
};
use serde_json::{json, Value};

use crate::storage::d1_http::{D1HttpClient, D1Row};

// =========================================================================
// Canonical SQL — reconciled against migrations 0045 + 0094 + 0095.
// =========================================================================

/// Quarantine one event. Idempotent on `event_id` via ON CONFLICT:
/// first call inserts (RETURNING attempt_count=1 → `Inserted`), later
/// calls bump attempt_count + refresh last_seen/last_error on the SAME
/// row (`Updated`) — the dlq_row_id never splits. Binds ?1..?14.
pub const SQL_DLQ_QUARANTINE: &str = "INSERT INTO stripe_webhook_events_dlq \
    (event_id, dlq_row_id, event_type, raw_body_hex, correlation_id, attempt_count, \
     first_seen_at_ms, last_seen_at_ms, last_error, expires_at_ms, \
     replay_request_id, replayed_by, replayed_at_ms, replay_outcome) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) \
    ON CONFLICT(event_id) DO UPDATE SET \
      attempt_count = stripe_webhook_events_dlq.attempt_count + 1, \
      last_seen_at_ms = excluded.last_seen_at_ms, \
      last_error = excluded.last_error \
    RETURNING attempt_count";

/// Record a replay outcome. No-op when `event_id` is absent (trait
/// contract). Binds ?1..?5.
pub const SQL_DLQ_RECORD_REPLAY: &str = "UPDATE stripe_webhook_events_dlq SET \
    replay_request_id = ?2, replayed_by = ?3, replayed_at_ms = ?4, replay_outcome = ?5 \
    WHERE event_id = ?1";

/// Active depth: un-expired AND not successfully resolved. Mirrors the
/// in-memory `is_active` predicate exactly (NULL | failed count;
/// succeeded | abandoned do not). Binds ?1 = now_ms.
pub const SQL_DLQ_DEPTH: &str = "SELECT COUNT(*) AS n FROM stripe_webhook_events_dlq \
    WHERE expires_at_ms > ?1 AND (replay_outcome IS NULL OR replay_outcome = 'failed')";

/// Oldest ACTIVE row's first_seen (NULL when empty). Same active filter
/// as [`SQL_DLQ_DEPTH`]. Binds ?1 = now_ms.
pub const SQL_DLQ_OLDEST_FIRST_SEEN: &str = "SELECT MIN(first_seen_at_ms) AS oldest \
    FROM stripe_webhook_events_dlq \
    WHERE expires_at_ms > ?1 AND (replay_outcome IS NULL OR replay_outcome = 'failed')";

/// TTL prune. Returns the removed rows' ids so the caller can count them
/// even though the D1 HTTP envelope does not expose `meta.changes`.
pub const SQL_DLQ_PRUNE_EXPIRED: &str = "DELETE FROM stripe_webhook_events_dlq \
    WHERE expires_at_ms <= ?1 RETURNING event_id";

/// Triage lookup. Binds ?1 = event_id.
pub const SQL_DLQ_GET: &str = "SELECT event_id, dlq_row_id, event_type, raw_body_hex, \
    correlation_id, attempt_count, first_seen_at_ms, last_seen_at_ms, last_error, \
    expires_at_ms, replay_request_id, replayed_by, replayed_at_ms, replay_outcome \
    FROM stripe_webhook_events_dlq WHERE event_id = ?1";

/// Append one durable billing-audit evidence row. Binds ?1..?8.
pub const SQL_BILLING_AUDIT_INSERT: &str = "INSERT INTO stripe_billing_audit_events \
    (event_name, stripe_event_id, stripe_event_type, tenant_id, stripe_object_id, \
     severity, ts_ms, payload_json) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

// =========================================================================
// Shared bridge
// =========================================================================

/// Sync→async bridge over the shared [`D1HttpClient`] — the documented
/// pattern from [`crate::billing_d1_http`] (the trait surfaces are sync
/// by charter; native D1 access is async-only).
fn run_d1(d1: &Arc<D1HttpClient>, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
    let d1 = Arc::clone(d1);
    let sql = sql.to_owned();
    tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move { d1.query(&sql, &binds).await })
    })
}

fn required_u64(row: &D1Row, col: &str) -> Result<u64, String> {
    row.get(col)
        .and_then(Value::as_i64)
        .map(|v| u64::try_from(v).unwrap_or(0))
        .ok_or_else(|| format!("d1-webhook-dlq: row missing/invalid `{col}`"))
}

fn required_str<'a>(row: &'a D1Row, col: &str) -> Result<&'a str, String> {
    row.get(col)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("d1-webhook-dlq: row missing/invalid `{col}`"))
}

// =========================================================================
// D1WebhookDlqStore
// =========================================================================

/// Durable [`WebhookDlqStore`] backed by `stripe_webhook_events_dlq`
/// over the D1 REST API. Wired in `main.rs` when the StorageEnv D1
/// config is present; dev/CI keep the in-memory store.
#[derive(Clone)]
pub struct D1WebhookDlqStore {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for D1WebhookDlqStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1WebhookDlqStore").finish_non_exhaustive()
    }
}

impl D1WebhookDlqStore {
    /// Wire the store over a shared [`D1HttpClient`].
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    /// Map a full 0045 row into the typed shape.
    fn hydrate(row: &D1Row) -> Result<WebhookDlqRow, String> {
        let replay_outcome =
            match row.get("replay_outcome").and_then(Value::as_str) {
                None | Some("") => None,
                Some(s) => Some(DlqReplayOutcome::parse_sql(s).ok_or_else(|| {
                    format!("d1-webhook-dlq: out-of-taxonomy replay_outcome `{s}`")
                })?),
            };
        Ok(WebhookDlqRow::rehydrate(
            required_str(row, "event_id")?,
            required_str(row, "dlq_row_id")?,
            required_str(row, "event_type")?,
            required_str(row, "raw_body_hex")?,
            required_str(row, "correlation_id")?,
            u32::try_from(required_u64(row, "attempt_count")?).unwrap_or(1),
            required_u64(row, "first_seen_at_ms")?,
            required_u64(row, "last_seen_at_ms")?,
            required_str(row, "last_error")?,
            required_u64(row, "expires_at_ms")?,
            row.get("replay_request_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            row.get("replayed_by")
                .and_then(Value::as_str)
                .map(str::to_owned),
            row.get("replayed_at_ms")
                .and_then(Value::as_i64)
                .map(|v| u64::try_from(v).unwrap_or(0)),
            replay_outcome,
        ))
    }
}

impl WebhookDlqStore for D1WebhookDlqStore {
    fn try_quarantine(&self, row: WebhookDlqRow) -> Result<DlqQuarantineOutcome, DlqError> {
        let binds = vec![
            json!(row.event_id),
            json!(row.dlq_row_id),
            json!(row.event_type),
            json!(row.raw_body_hex),
            json!(row.correlation_id),
            json!(i64::from(row.attempt_count)),
            json!(i64::try_from(row.first_seen_at_ms).unwrap_or(0)),
            json!(i64::try_from(row.last_seen_at_ms).unwrap_or(0)),
            json!(row.last_error),
            json!(i64::try_from(row.expires_at_ms).unwrap_or(0)),
            json!(row.replay_request_id),
            json!(row.replayed_by),
            json!(row.replayed_at_ms.map(|v| i64::try_from(v).unwrap_or(0))),
            json!(row.replay_outcome.map(DlqReplayOutcome::as_str)),
        ];
        let rows = run_d1(&self.d1, SQL_DLQ_QUARANTINE, binds).map_err(DlqError::Backend)?;
        let attempt_count = rows
            .first()
            .ok_or_else(|| {
                DlqError::Backend(
                    "d1-webhook-dlq: quarantine RETURNING produced no rows".to_owned(),
                )
            })
            .and_then(|r| required_u64(r, "attempt_count").map_err(DlqError::Backend))?;
        if attempt_count <= 1 {
            Ok(DlqQuarantineOutcome::Inserted)
        } else {
            Ok(DlqQuarantineOutcome::Updated)
        }
    }

    fn record_replay(
        &self,
        event_id: &str,
        replay_request_id: &str,
        replayed_by: &str,
        replayed_at_ms: u64,
        outcome: DlqReplayOutcome,
    ) -> Result<(), DlqError> {
        let binds = vec![
            json!(event_id),
            json!(replay_request_id),
            json!(replayed_by),
            json!(i64::try_from(replayed_at_ms).unwrap_or(0)),
            json!(outcome.as_str()),
        ];
        run_d1(&self.d1, SQL_DLQ_RECORD_REPLAY, binds).map_err(DlqError::Backend)?;
        Ok(())
    }

    fn depth(&self, now_ms: u64) -> Result<u64, DlqError> {
        let rows = run_d1(
            &self.d1,
            SQL_DLQ_DEPTH,
            vec![json!(i64::try_from(now_ms).unwrap_or(0))],
        )
        .map_err(DlqError::Backend)?;
        rows.first()
            .and_then(|r| r.get("n").and_then(Value::as_i64))
            .map(|n| u64::try_from(n).unwrap_or(0))
            .ok_or_else(|| DlqError::Backend("d1-webhook-dlq: depth row missing `n`".to_owned()))
    }

    fn oldest_age_seconds(&self, now_ms: u64) -> Result<u64, DlqError> {
        let now = i64::try_from(now_ms).unwrap_or(0);
        let rows = run_d1(&self.d1, SQL_DLQ_OLDEST_FIRST_SEEN, vec![json!(now)])
            .map_err(DlqError::Backend)?;
        let oldest = rows
            .first()
            .and_then(|r| r.get("oldest").and_then(Value::as_i64));
        match oldest {
            None | Some(0) => Ok(0), // empty DLQ (MIN over no active rows → NULL)
            Some(first_seen) => {
                Ok(u64::try_from(now.saturating_sub(first_seen)).unwrap_or(0) / 1_000)
            }
        }
    }

    fn prune_expired(&self, now_ms: u64) -> Result<u64, DlqError> {
        let rows = run_d1(
            &self.d1,
            SQL_DLQ_PRUNE_EXPIRED,
            vec![json!(i64::try_from(now_ms).unwrap_or(0))],
        )
        .map_err(DlqError::Backend)?;
        Ok(rows.len() as u64)
    }

    fn get(&self, event_id: &str) -> Result<Option<WebhookDlqRow>, DlqError> {
        let rows =
            run_d1(&self.d1, SQL_DLQ_GET, vec![json!(event_id)]).map_err(DlqError::Backend)?;
        match rows.first() {
            None => Ok(None),
            Some(row) => Self::hydrate(row).map(Some).map_err(DlqError::Backend),
        }
    }
}

// =========================================================================
// D1BillingAuditEmitter (MED-5 closure)
// =========================================================================

/// Durable `BillingAuditEmitter` appending to `stripe_billing_audit_events`
/// (migration 0095). Fail-CLOSED: a transport error surfaces as
/// [`BillingAuditError::Transient`] so the dispatcher returns HTTP 500 and
/// Stripe retries (the dedup row prevents double-materialization).
#[derive(Clone)]
pub struct D1BillingAuditEmitter {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for D1BillingAuditEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1BillingAuditEmitter")
            .finish_non_exhaustive()
    }
}

impl D1BillingAuditEmitter {
    /// Wire the emitter over a shared [`D1HttpClient`].
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}

fn severity_str(severity: corelink_billing_stripe_materializer::AuditSeverity) -> &'static str {
    use corelink_billing_stripe_materializer::AuditSeverity as S;
    match severity {
        S::Info => "info",
        S::Notice => "notice",
        S::Sev1 => "sev1",
        _ => "info",
    }
}

impl corelink_billing_stripe_materializer::BillingAuditEmitter for D1BillingAuditEmitter {
    fn emit_billing(&self, record: &BillingAuditRecord) -> Result<(), BillingAuditError> {
        let payload_json = serde_json::to_string(&record.payload).map_err(|e| {
            BillingAuditError::Transient(format!("d1-billing-audit: payload serialize: {e}"))
        })?;
        let binds = vec![
            json!(record.event_name),
            json!(record.stripe_event_id),
            json!(record.stripe_event_type),
            json!(record.tenant_id),
            json!(record.stripe_object_id),
            json!(severity_str(record.severity)),
            json!(i64::try_from(record.ts_ms).unwrap_or(0)),
            json!(payload_json),
        ];
        run_d1(&self.d1, SQL_BILLING_AUDIT_INSERT, binds)
            .map(|_| ())
            .map_err(|e| BillingAuditError::Transient(format!("d1-billing-audit: {e}")))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    const DDL_0045: &str = include_str!("../../../migrations/d1/0045_stripe_webhook_dlq.sql");
    const DDL_0094: &str =
        include_str!("../../../migrations/d1/0104_webhook_dlq_event_id_unique.sql");
    const DDL_0095: &str =
        include_str!("../../../migrations/d1/0105_stripe_billing_audit_events.sql");

    fn db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory sqlite");
        conn.execute_batch(DDL_0045).expect("0045 applies");
        conn.execute_batch(DDL_0094).expect("0094 applies");
        conn.execute_batch(DDL_0095).expect("0095 applies");
        conn
    }

    fn quarantine_row(event_id: &str, error: &str) -> WebhookDlqRow {
        WebhookDlqRow::new_quarantine(
            event_id,
            format!("row-{event_id}"),
            "checkout.session.completed",
            "deadbeef",
            "corr-1",
            error,
            1_000,
        )
    }

    /// Bind a row into [`SQL_DLQ_QUARANTINE`] exactly like the store does.
    fn run_quarantine(conn: &rusqlite::Connection, row: &WebhookDlqRow) -> u64 {
        conn.query_row(
            SQL_DLQ_QUARANTINE,
            rusqlite::params![
                row.event_id,
                row.dlq_row_id,
                row.event_type,
                row.raw_body_hex,
                row.correlation_id,
                i64::from(row.attempt_count),
                i64::try_from(row.first_seen_at_ms).unwrap_or(0),
                i64::try_from(row.last_seen_at_ms).unwrap_or(0),
                row.last_error,
                i64::try_from(row.expires_at_ms).unwrap_or(0),
                row.replay_request_id,
                row.replayed_by,
                row.replayed_at_ms.and_then(|v| i64::try_from(v).ok()),
                row.replay_outcome.map(DlqReplayOutcome::as_str),
            ],
            |r| r.get::<_, i64>(0),
        )
        .map(|v| u64::try_from(v).unwrap_or(0))
        .expect("quarantine executes")
    }

    #[test]
    fn quarantine_is_idempotent_on_event_id() {
        let conn = db();
        let first = quarantine_row("evt_1", "boom");
        assert_eq!(
            run_quarantine(&conn, &first),
            1,
            "first → Inserted (attempt=1)"
        );

        // Same event_id re-quarantined → SAME dlq_row_id kept, attempt bumped.
        let retry = quarantine_row("evt_1", "boom again");
        assert_eq!(
            run_quarantine(&conn, &retry),
            2,
            "second → Updated (attempt=2)"
        );

        let (dlq_row_id, attempt): (String, i64) = conn
            .query_row(
                "SELECT dlq_row_id, attempt_count FROM stripe_webhook_events_dlq WHERE event_id = 'evt_1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(attempt, 2);
        assert_eq!(dlq_row_id, first.dlq_row_id, "audit trail never splits");
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM stripe_webhook_events_dlq", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
    }

    #[test]
    fn unique_index_blocks_split_rows() {
        let conn = db();
        let _ = run_quarantine(&conn, &quarantine_row("evt_1", "a"));
        // A hypothetical non-idempotent writer must hit the UNIQUE index.
        let dup = conn.execute(
            "INSERT INTO stripe_webhook_events_dlq (event_id, dlq_row_id, event_type, raw_body_hex, correlation_id, attempt_count, first_seen_at_ms, last_seen_at_ms, last_error, expires_at_ms) VALUES ('evt_1','other','t','x','c',1,1,1,'e',1000)",
            [],
        );
        assert!(dup.is_err(), "UNIQUE(event_id) enforced at SQL level");
    }

    #[test]
    fn depth_counts_only_active_rows() {
        let conn = db();
        let mut active = quarantine_row("evt_active", "e");
        active.expires_at_ms = 10_000; // un-expired
        let _ = run_quarantine(&conn, &active);

        let mut expired = quarantine_row("evt_expired", "e");
        expired.first_seen_at_ms = 100;
        expired.last_seen_at_ms = 100;
        expired.expires_at_ms = 500; // already expired by t=5_000
        let _ = run_quarantine(&conn, &expired);

        let mut succeeded = quarantine_row("evt_done", "e");
        succeeded.expires_at_ms = 10_000;
        let _ = run_quarantine(&conn, &succeeded);
        conn.execute(
            SQL_DLQ_RECORD_REPLAY,
            rusqlite::params!["evt_done", "rq", "op", 2_000_i64, "succeeded"],
        )
        .unwrap();

        let n: i64 = conn
            .query_row(SQL_DLQ_DEPTH, rusqlite::params![5_000_i64], |r| r.get("n"))
            .unwrap();
        assert_eq!(n, 1, "only the un-expired, unresolved row counts");
    }

    #[test]
    fn oldest_age_and_prune_behave() {
        let conn = db();
        let mut old = quarantine_row("evt_old", "e");
        old.first_seen_at_ms = 1_000;
        old.last_seen_at_ms = 1_000;
        old.expires_at_ms = 90_000;
        let _ = run_quarantine(&conn, &old);

        let oldest: Option<i64> = conn
            .query_row(
                SQL_DLQ_OLDEST_FIRST_SEEN,
                rusqlite::params![61_000_i64],
                |r| r.get("oldest"),
            )
            .unwrap();
        assert_eq!(oldest, Some(1_000));

        // Prune removes only rows past expiry.
        let mut stale = quarantine_row("evt_stale", "e");
        stale.first_seen_at_ms = 2_000;
        stale.last_seen_at_ms = 2_000;
        stale.expires_at_ms = 3_000;
        let _ = run_quarantine(&conn, &stale);

        let pruned: Vec<String> = conn
            .prepare(SQL_DLQ_PRUNE_EXPIRED)
            .unwrap()
            .query_map(rusqlite::params![50_000_i64], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(pruned.len(), 1, "only evt_stale expires by t=50s");
        assert_eq!(pruned[0], "evt_stale");
    }

    #[test]
    fn record_replay_is_no_op_on_absent_event() {
        let conn = db();
        let changed = conn
            .execute(
                SQL_DLQ_RECORD_REPLAY,
                rusqlite::params!["evt_missing", "rq", "op", 2_000_i64, "succeeded"],
            )
            .unwrap();
        assert_eq!(changed, 0, "absent event_id → no-op per trait contract");
    }

    #[test]
    fn billing_audit_insert_accepts_canonical_severities_only() {
        let conn = db();
        for sev in ["info", "notice", "sev1"] {
            conn.execute(
                SQL_BILLING_AUDIT_INSERT,
                rusqlite::params![
                    "corelink.billing.invoice.materialized.v1",
                    "evt_1",
                    "invoice.paid",
                    "ten_1",
                    Option::<String>::None,
                    sev,
                    1_000_i64,
                    "{}"
                ],
            )
            .unwrap_or_else(|e| panic!("severity {sev} must be admitted: {e}"));
        }
        let bad = conn.execute(
            SQL_BILLING_AUDIT_INSERT,
            rusqlite::params![
                "corelink.billing.echo.v1",
                "evt_2",
                "echo",
                "ten_1",
                Option::<String>::None,
                "critical",
                1_000_i64,
                "{}"
            ],
        );
        assert!(bad.is_err(), "out-of-taxonomy severity rejected by CHECK");
    }
}
