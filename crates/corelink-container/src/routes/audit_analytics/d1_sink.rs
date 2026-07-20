//! D1-backed audit-analytics source (`#71` — retire the dead Neon "shadow"
//! scaffold; serve REAL aggregates).
//!
//! ## Why this exists
//!
//! The `/v1/audit/analytics/{event-count,timeline}` routes were designed
//! against a per-region Neon Postgres "analytics shadow" (`audit_events_shadow`)
//! that was **never wired in production**: the per-region DSN env
//! (`EnvVarResolver`) was never set and the real driver
//! (`TokioPgShadowSinkFactory`, feature `neon-real`) was never compiled into the
//! shipped container — so the boot path always fell back to
//! [`crate::routes::InMemoryShadowSinkFactory`], whose per-tenant
//! `InMemoryNeonShadowSink` holds ZERO buffered rows in a fresh container. Every
//! production analytics query therefore returned an **empty** aggregate.
//!
//! Meanwhile the customer-facing audit log already lives durably in the D1
//! `customer_audit_events` table (migration 0077) — written UNSKIPPABLE /
//! fail-CLOSED by the control-plane mutations (`keys create` → `pat.created`,
//! `team invite` → `team.invited`) and read by `/v1/customer/audit`
//! ([`crate::customer_d1::D1CustomerHandler::query`]). This module points the
//! analytics aggregates at that SAME table, so the endpoints answer with real
//! data instead of an empty shadow.
//!
//! ## Shape of the seam
//!
//! [`D1ShadowSinkFactory`] implements the existing
//! [`super::ShadowSinkFactory`] trait so the route wiring, rate-limit, native
//! PAT gate, audit-emit ordering, and handler code are all UNCHANGED — only the
//! data source is swapped. `for_tenant(tenant)` binds a [`D1AuditAnalyticsSink`]
//! to that tenant; the sink implements the `aggregate_*` half of
//! [`corelink_audit_chain::NeonShadowSink`] against D1.
//!
//! The `NeonShadowSink::sync_chunk` half (archive-chunk → shadow WRITE) is NOT a
//! customer-analytics concern — the durable audit WRITE path is the D1
//! `audit_outbox` sink (`storage::d1_audit_sink`, `#74`). This read-only sink
//! rejects `sync_chunk` with a typed error; the analytics handlers never call
//! it.
//!
//! ## Tenant isolation (INV-TENANT-ISOLATION)
//!
//! Every query is `WHERE tenant_id = ?1` with the sink's OWN bound tenant — a
//! value that flows from the DO-injected `x-corelink-tenant-id` header, never a
//! client parameter. The optional `event_type` filter is BOUND (`?N`), never
//! string-interpolated, so there is no injection surface. The handler
//! additionally asserts `sink.tenant_id() == authenticated_tenant` before
//! serving (defense in depth) — this sink returns exactly the tenant it was
//! bound to, so that check is authoritative.

#![forbid(unsafe_code)]

use std::sync::Arc;

use corelink_analytics::Region;
use corelink_audit_chain::{
    ArchiveReceipt, EventCountBucket, NeonShadowError, NeonShadowSink, ShadowEventRow,
    ShadowSyncReceipt, TimelineBucket,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::customer_d1::{CustomerD1, D1HttpCustomerDb};
use crate::storage::d1_http::D1Row;

use super::ShadowSinkFactory;

/// Defensive upper bound on the number of rows returned by a single aggregate
/// query. `event-count` is naturally tiny (bounded by the count of DISTINCT
/// `event_type` values — a handful). `timeline` is bounded by the count of
/// DISTINCT non-empty buckets, which is itself ≤ the tenant's row count; a
/// pathological `granularity=1ms` over a huge window can only ever emit as many
/// buckets as there are audit rows. The `LIMIT` is a belt-and-braces cap so a
/// tenant with a very large audit log cannot force an unbounded result set (a
/// memory/transport DoS) through the read-only analytics surface — it composes
/// with the per-tenant rate limit (10 queries / 60 s). A truncated timeline is
/// acceptable for an analytics convenience read (the compliance source of truth
/// is the R2 NDJSON archive, per the module disclaimer).
const AGGREGATE_ROW_LIMIT: u64 = 10_000;

/// Per-tenant factory that resolves a [`D1AuditAnalyticsSink`] over the shared
/// D1 row source. Wired by the production boot path (`main.rs`) in place of the
/// retired Neon `TokioPgShadowSinkFactory`.
pub struct D1ShadowSinkFactory {
    /// Shared sync D1 row source (the same seam `customer_d1` uses).
    db: Arc<dyn CustomerD1>,
}

impl D1ShadowSinkFactory {
    /// Wire the factory over a shared [`CustomerD1`] row source.
    #[must_use]
    pub fn new(db: Arc<dyn CustomerD1>) -> Self {
        Self { db }
    }

    /// Build the factory over a fresh D1-over-HTTP client from `StorageEnv`.
    /// `None` when the storage env is unset/invalid (dev/CI) — mirrors
    /// [`D1HttpCustomerDb::from_env`] and the customer-plane env-gates — so the
    /// caller falls back to the in-memory factory and the route surface still
    /// boots.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let db = D1HttpCustomerDb::from_env()?;
        Some(Self::new(Arc::new(db)))
    }
}

impl core::fmt::Debug for D1ShadowSinkFactory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1ShadowSinkFactory")
            .finish_non_exhaustive()
    }
}

impl ShadowSinkFactory for D1ShadowSinkFactory {
    fn for_tenant(&self, tenant_id: Uuid) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        Ok(Arc::new(D1AuditAnalyticsSink {
            tenant: tenant_id,
            db: Arc::clone(&self.db),
        }))
    }
    // `for_tenant_in_region` uses the trait default (delegates to `for_tenant`):
    // D1 is a single global binding with no per-region shard, so the resolved
    // `Region` carries no routing meaning here.
}

/// Read-only [`NeonShadowSink`] that answers the analytics aggregates from the
/// D1 `customer_audit_events` table (migration 0077), scoped to `tenant`.
struct D1AuditAnalyticsSink {
    /// The tenant this sink is bound to — the SOLE isolation key.
    tenant: Uuid,
    /// Shared sync D1 row source.
    db: Arc<dyn CustomerD1>,
}

impl core::fmt::Debug for D1AuditAnalyticsSink {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1AuditAnalyticsSink")
            .field("tenant", &self.tenant)
            .finish_non_exhaustive()
    }
}

impl D1AuditAnalyticsSink {
    /// Canonical tenant-id bind value. `customer_audit_events.tenant_id` is a
    /// TEXT column written with the caller's `x-corelink-tenant-id` header
    /// string; the handler parsed that header into `Uuid` before constructing
    /// this sink, so the canonical lowercase-hyphenated `to_string()` round-trips
    /// back to the stored value.
    fn tenant_bind(&self) -> Value {
        json!(self.tenant.to_string())
    }
}

/// Read `COUNT(*)`-style integer columns from a D1 row. D1 returns integers as
/// JSON numbers; a missing/non-numeric cell counts as 0 (never a panic).
fn col_u64(row: &D1Row, key: &str) -> u64 {
    row.get(key)
        .and_then(serde_json::Value::as_i64)
        .filter(|n| *n >= 0)
        .map(|n| n as u64)
        .unwrap_or(0)
}

fn col_str(row: &D1Row, key: &str) -> String {
    row.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl NeonShadowSink for D1AuditAnalyticsSink {
    fn tenant_id(&self) -> Uuid {
        self.tenant
    }

    fn region(&self) -> Region {
        // D1 has no per-region shard for this table; `region()` is unused by the
        // analytics handlers (they read only `tenant_id()` + the aggregates).
        // A fixed value keeps the trait total without implying a residency claim.
        Region::Iad
    }

    fn sync_chunk(
        &self,
        _receipt: &ArchiveReceipt,
        _rows: &[ShadowEventRow],
        _now_ms: u64,
    ) -> Result<ShadowSyncReceipt, NeonShadowError> {
        // The durable audit WRITE path is the D1 `audit_outbox` sink
        // (`storage::d1_audit_sink`, #74); this read-only analytics sink never
        // persists chunks and the analytics handlers never call `sync_chunk`.
        Err(NeonShadowError::Backend(
            "D1AuditAnalyticsSink is read-only (analytics aggregates); \
             the audit WRITE path is the D1 audit_outbox sink"
                .to_string(),
        ))
    }

    fn aggregate_event_count(
        &self,
        from_ms: u64,
        to_ms: u64,
        event_type_filter: Option<&str>,
    ) -> Result<Vec<EventCountBucket>, NeonShadowError> {
        // Tenant scope is fail-CLOSED (`WHERE tenant_id = ?1`, the sink's OWN
        // tenant). The `[from, to)` window bounds ts_ms. The optional event-type
        // filter is BOUND (`?4`), never interpolated.
        let mut sql = String::from(
            "SELECT event_type, COUNT(*) AS c \
             FROM customer_audit_events \
             WHERE tenant_id = ?1 AND ts_ms >= ?2 AND ts_ms < ?3",
        );
        let mut binds: Vec<Value> = vec![
            self.tenant_bind(),
            json!(clamp_i64(from_ms)),
            json!(clamp_i64(to_ms)),
        ];
        if let Some(et) = event_type_filter {
            binds.push(json!(et));
            sql.push_str(" AND event_type = ?4");
        }
        sql.push_str(&format!(
            " GROUP BY event_type ORDER BY event_type LIMIT {AGGREGATE_ROW_LIMIT}"
        ));

        let rows = self
            .db
            .query(&sql, binds)
            .map_err(NeonShadowError::Backend)?;
        Ok(rows
            .iter()
            .map(|row| EventCountBucket::new(col_str(row, "event_type"), col_u64(row, "c")))
            .collect())
    }

    fn aggregate_timeline(
        &self,
        from_ms: u64,
        to_ms: u64,
        granularity_ms: u64,
    ) -> Result<Vec<TimelineBucket>, NeonShadowError> {
        if granularity_ms == 0 {
            return Err(NeonShadowError::Backend(
                "granularity_ms must be > 0".to_string(),
            ));
        }
        let from_i = clamp_i64(from_ms);
        let gran_i = clamp_i64(granularity_ms);
        // Bucket index = floor((ts_ms - from) / granularity). Both operands are
        // bound integers, so `/` is SQLite integer division; `ts_ms >= from`
        // keeps the numerator non-negative. Bucket start is reconstructed as
        // `from + index*granularity` on the Rust side (never interpolated).
        let sql = format!(
            "SELECT ((ts_ms - ?2) / ?4) AS bucket, COUNT(*) AS c \
             FROM customer_audit_events \
             WHERE tenant_id = ?1 AND ts_ms >= ?2 AND ts_ms < ?3 \
             GROUP BY bucket ORDER BY bucket LIMIT {AGGREGATE_ROW_LIMIT}"
        );
        let binds: Vec<Value> = vec![
            self.tenant_bind(),
            json!(from_i),
            json!(clamp_i64(to_ms)),
            json!(gran_i),
        ];

        let rows = self
            .db
            .query(&sql, binds)
            .map_err(NeonShadowError::Backend)?;
        Ok(rows
            .iter()
            .map(|row| {
                let bucket_index = row
                    .get("bucket")
                    .and_then(serde_json::Value::as_i64)
                    .filter(|n| *n >= 0)
                    .unwrap_or(0);
                // from + index*granularity, saturating (all inputs are already
                // clamped into the non-negative i64 domain).
                let start = (from_i as i128) + (bucket_index as i128) * (gran_i as i128);
                let bucket_start_ms = u64::try_from(start).unwrap_or(from_ms);
                TimelineBucket::new(bucket_start_ms, col_u64(row, "c"))
            })
            .collect())
    }
}

/// Clamp a `u64` millis value into the non-negative `i64` domain D1/SQLite binds
/// as an integer. Real audit timestamps are far below `i64::MAX`; a caller that
/// passes an absurd `u64` gets a saturated bound rather than a wrapped negative.
fn clamp_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Hermetic [`CustomerD1`] mock: captures the last `(sql, binds)` and
    /// returns canned rows so the tests assert the QUERY shape (tenant binding,
    /// filter binding, bucket math) without a live D1.
    #[derive(Debug, Default)]
    struct MockD1 {
        last: Mutex<Option<(String, Vec<Value>)>>,
        rows: Vec<D1Row>,
        fail: bool,
    }

    impl MockD1 {
        fn with_rows(rows: Vec<D1Row>) -> Self {
            Self {
                last: Mutex::new(None),
                rows,
                fail: false,
            }
        }
        fn failing() -> Self {
            Self {
                last: Mutex::new(None),
                rows: Vec::new(),
                fail: true,
            }
        }
        fn last(&self) -> (String, Vec<Value>) {
            self.last.lock().unwrap().clone().expect("query was run")
        }
    }

    impl CustomerD1 for MockD1 {
        fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
            *self.last.lock().unwrap() = Some((sql.to_string(), binds));
            if self.fail {
                return Err("mock D1 transport failure".to_string());
            }
            Ok(self.rows.clone())
        }
    }

    fn row(pairs: &[(&str, Value)]) -> D1Row {
        pairs
            .iter()
            .cloned()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    fn sink(db: Arc<dyn CustomerD1>) -> D1AuditAnalyticsSink {
        D1AuditAnalyticsSink {
            tenant: Uuid::parse_str("d863fafb-0000-4000-8000-000000000001").unwrap(),
            db,
        }
    }

    #[test]
    fn event_count_binds_tenant_and_maps_rows() {
        let mock = Arc::new(MockD1::with_rows(vec![
            row(&[("event_type", json!("pat.created")), ("c", json!(3))]),
            row(&[("event_type", json!("team.invited")), ("c", json!(1))]),
        ]));
        let s = sink(mock.clone());
        let out = s.aggregate_event_count(1_000, 2_000, None).unwrap();

        let (sql, binds) = mock.last();
        assert!(sql.contains("FROM customer_audit_events"), "sql: {sql}");
        assert!(sql.contains("WHERE tenant_id = ?1"), "tenant scope: {sql}");
        assert!(sql.contains("GROUP BY event_type"), "sql: {sql}");
        // Tenant is bound as the canonical hyphenated string, NOT interpolated.
        assert_eq!(binds[0], json!("d863fafb-0000-4000-8000-000000000001"));
        assert_eq!(binds[1], json!(1_000_i64));
        assert_eq!(binds[2], json!(2_000_i64));

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].event_type, "pat.created");
        assert_eq!(out[0].count, 3);
        assert_eq!(out[1].count, 1);
    }

    #[test]
    fn event_count_filter_is_bound_not_interpolated() {
        // A hostile event_type must NOT reach the SQL string.
        let mock = Arc::new(MockD1::with_rows(vec![]));
        let s = sink(mock.clone());
        let hostile = "x'; DROP TABLE customer_audit_events;--";
        let _ = s.aggregate_event_count(0, 10, Some(hostile)).unwrap();

        let (sql, binds) = mock.last();
        assert!(
            sql.contains("AND event_type = ?4"),
            "filter must be bound: {sql}"
        );
        assert!(!sql.contains("DROP TABLE"), "injection reached SQL: {sql}");
        assert_eq!(binds[3], json!(hostile));
    }

    #[test]
    fn timeline_bucket_math_reconstructs_start() {
        // granularity 100ms, window from=1_000: bucket 0 → 1_000, bucket 2 → 1_200.
        let mock = Arc::new(MockD1::with_rows(vec![
            row(&[("bucket", json!(0)), ("c", json!(5))]),
            row(&[("bucket", json!(2)), ("c", json!(2))]),
        ]));
        let s = sink(mock.clone());
        let out = s.aggregate_timeline(1_000, 2_000, 100).unwrap();

        let (sql, binds) = mock.last();
        assert!(sql.contains("(ts_ms - ?2) / ?4"), "bucket expr: {sql}");
        assert!(sql.contains("WHERE tenant_id = ?1"), "tenant scope: {sql}");
        assert_eq!(binds[3], json!(100_i64), "granularity bound");

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].bucket_start_ms, 1_000);
        assert_eq!(out[0].count, 5);
        assert_eq!(out[1].bucket_start_ms, 1_200);
        assert_eq!(out[1].count, 2);
    }

    #[test]
    fn timeline_rejects_zero_granularity() {
        let mock = Arc::new(MockD1::with_rows(vec![]));
        let s = sink(mock);
        let err = s.aggregate_timeline(0, 10, 0).unwrap_err();
        assert!(matches!(err, NeonShadowError::Backend(_)));
    }

    #[test]
    fn aggregate_propagates_d1_error_as_backend() {
        let mock = Arc::new(MockD1::failing());
        let s = sink(mock);
        let err = s.aggregate_event_count(0, 10, None).unwrap_err();
        assert!(matches!(err, NeonShadowError::Backend(_)));
    }

    #[test]
    fn sync_chunk_is_unsupported_on_read_only_sink() {
        use corelink_audit_chain::ChainHash;
        let mock = Arc::new(MockD1::with_rows(vec![]));
        let s = sink(mock);
        // The method rejects BEFORE touching the receipt/rows; a minimal
        // receipt is enough to satisfy the signature.
        let receipt = ArchiveReceipt {
            r2_key: "audit/2026/07/20/00000001.ndjson".to_string(),
            tenant_id: s.tenant,
            first_event_time_ms: 0,
            last_event_time_ms: 0,
            first_sequence_number: 0,
            last_sequence_number: 0,
            prev_hash_anchor: ChainHash([0u8; 32]),
            chain_head_after: ChainHash([0u8; 32]),
            bytes_written: 0,
            events_written: 0,
        };
        let err = s.sync_chunk(&receipt, &[], 0).unwrap_err();
        assert!(matches!(err, NeonShadowError::Backend(_)));
    }

    #[test]
    fn factory_binds_requested_tenant() {
        let mock = Arc::new(MockD1::with_rows(vec![]));
        let factory = D1ShadowSinkFactory::new(mock);
        let t = Uuid::parse_str("d863fafb-0000-4000-8000-0000000000ff").unwrap();
        let sink = factory.for_tenant(t).unwrap();
        assert_eq!(
            sink.tenant_id(),
            t,
            "sink must be bound to the requested tenant"
        );
    }
}
