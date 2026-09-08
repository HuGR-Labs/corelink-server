//! Durable D1 `audit_outbox`-backed [`AuditSink`] for the native CAS + AC
//! data plane.
//!
//! # Why this exists (F1 / CAA-360)
//!
//! `R2CasHandler` / `R2AcHandler` emit an audit row BEFORE every data-plane
//! mutation and treat any sink error as fail-CLOSED (the route returns 503 on
//! `AuditFailed`, aborting the mutation). Until this module existed, the
//! deployed builders (`build_r2_cas_handler_from_env` /
//! `build_r2_ac_handler_from_env`) hardcoded the in-process
//! `InMemoryAuditSink`, so:
//!
//! * every CAS/AC data-plane audit event (`ReadAttempted`, `ReadDenied`,
//!   `WriteCommitted`, `CorrectnessViolation`, …) was written ONLY to RAM and
//!   lost on container restart — the "durable audit row before mutation"
//!   guarantee was unwired; and
//! * `InMemoryAuditSink::emit` only errors under a test-injected failure, so
//!   the route's fail-CLOSED `AuditFailed → 503` guard was dead code in prod.
//!
//! This sink appends each event as a PLAIN, UNCHAINED row to the D1
//! `audit_outbox` intake table (`migrations/d1/0001_blob_meta.sql`) — the
//! exact same trail the S-09 audit-drain (`routes/audit_drain.rs`) seals and
//! the DSR erasure sink (`routes/dsr/audit.rs`) already writes to. A D1
//! transport failure propagates as `Err`, which the handler treats as
//! fail-CLOSED — so the 503 guard is now reachable in production.
//!
//! ## Tamper-evidence posture (identical to the DSR sink)
//!
//! Rows land with `emitted_at = NULL` and carry NO BLAKE3 chain link. They are
//! ordinary mutable D1 rows until the S-09 drain seals them; do NOT describe
//! the live trail as "chained" / "sealed" / "tamper-evident".
//!
//! ## `region` column
//!
//! Tenant-scoped INSERTs set `region` from a correlated subquery on the
//! tenant's `primary_region`, NOT from the column default. The pre-tenant
//! pilot-signup path instead pins its canonical `_public` namespace to `wnam`.
//! The residency guard rejects a non-public row whose tenant is absent or whose
//! region disagrees; the result is deliberately allowed to fail CLOSED rather
//! than creating another unevaluable audit row.
//! See [`AUDIT_OUTBOX_INSERT_ONE_SQL`] for the incident this encodes.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::customer_d1::ms_to_iso8601;
use crate::storage::d1_http::D1HttpClient;

/// Durable audit sink that appends CAS/AC data-plane audit rows to the D1
/// `audit_outbox` table.
///
/// A single struct implements BOTH [`corelink_handler_cas::AuditSink`] and
/// [`corelink_handler_ac::AuditSink`]; the `source` field (`"corelink/cas"` /
/// `"corelink/ac"`) — set at construction — distinguishes the CloudEvents
/// `source` in the persisted envelope.
pub struct D1AuditOutboxSink {
    d1: Arc<D1HttpClient>,
    /// CloudEvents `source` — `"corelink/cas"` or `"corelink/ac"`.
    source: &'static str,
}

impl core::fmt::Debug for D1AuditOutboxSink {
    // Never surface the inner client's token; the struct NAME is the durable
    // marker the builder tests assert on (vs `InMemoryAuditSink`).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1AuditOutboxSink")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// The field values of ONE `audit_outbox` row, as derived by
/// [`D1AuditOutboxSink::build_row`] — the single place that decides what an
/// audit row contains.
///
/// Both write shapes in this module consume this and nothing else, so they
/// cannot drift on the idempotency keys or on the CloudEvents envelope an
/// auditor reads:
///
/// * [`D1AuditOutboxSink::build_insert`] → the single-row
///   `INSERT … VALUES (?1..?7)` (via [`AuditRow::into_params`]);
/// * [`D1AuditOutboxSink::build_batch_insert`] → the JSON1
///   `INSERT … SELECT … FROM json_each(?1)` (via
///   [`AuditRow::to_json_element`]).
#[derive(Clone, Debug, PartialEq, Eq)]
struct AuditRow {
    id: String,
    tenant: String,
    /// `None` ⇒ SQL `NULL` (pre-write events carry no blob hash).
    digest: Option<String>,
    request_id: String,
    event_type: String,
    payload_json: String,
    at_ms: i64,
}

impl AuditRow {
    /// Positional params for [`AUDIT_OUTBOX_INSERT_ONE_SQL`] (`?1`..`?7`).
    fn into_params(self) -> Vec<Value> {
        let digest_param = match self.digest {
            Some(d) => Value::String(d),
            None => Value::Null,
        };
        vec![
            json!(self.id),
            json!(self.tenant),
            digest_param,
            json!(self.request_id),
            json!(self.event_type),
            json!(self.payload_json),
            json!(self.at_ms),
        ]
    }

    /// One element of the JSON array bound as `?1` to
    /// [`AUDIT_OUTBOX_INSERT_MANY_SQL`]. The key names are the `$.…` paths
    /// that SQL's `json_extract` calls read — change one, change both.
    ///
    /// `digest: None` encodes as JSON `null`, and `json_extract` of a JSON
    /// `null` yields SQL `NULL` — the same value the single-row path binds.
    fn to_json_element(&self) -> Value {
        json!({
            "id": self.id,
            "tenant": self.tenant,
            "digest": self.digest,
            "request_id": self.request_id,
            "event_type": self.event_type,
            "payload": self.payload_json,
            "at": self.at_ms,
        })
    }
}

/// Why every `audit_outbox` INSERT in this module computes `region` from a
/// correlated subquery instead of taking the column default.
///
/// `region` MUST be the tenant's `primary_region`, NOT the `'wnam'` column
/// default. Migration 0107 adds a BEFORE-INSERT guard that rejects non-public
/// rows when the tenant is missing or mismatched. A missing tenant therefore
/// propagates D1 failure to the handler (503), instead of being disguised as a
/// valid US row. `_public` is the only explicit namespace exception and is
/// handled by the public-revocation sink.
const AUDIT_OUTBOX_INSERT_ONE_SQL: &str = "INSERT OR IGNORE INTO audit_outbox \
     (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at, region) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, \
             (SELECT primary_region FROM tenant WHERE tenant_id = ?2))";

/// The BATCH counterpart of [`AUDIT_OUTBOX_INSERT_ONE_SQL`]: the SAME table,
/// the SAME column list, the SAME `INSERT OR IGNORE` idempotency, the SAME
/// residency COALESCE — but N rows in ONE statement, and therefore ONE D1
/// round trip instead of N.
///
/// # Why JSON1 and not a multi-`VALUES` insert
///
/// D1 caps a statement at **100 bound parameters** (verified against prod:
/// `variable number must be between ?1 and ?100 … SQLITE_ERROR`). At 7 params
/// per row a multi-`VALUES` insert fits only 14 rows, so a `findMissingBlobs`
/// call at the 4096-digest cap would still need ~293 statements. D1 ships
/// SQLite's JSON1, so the whole batch rides in ONE parameter instead: `?1` is
/// a JSON array of [`AuditRow::to_json_element`] objects and `json_each`
/// expands it into rows.
///
/// The rows written are IDENTICAL to what the single-row path writes for the
/// same events — same event kind, same `id`/`request_id` derivation (both come
/// from [`D1AuditOutboxSink::build_row`]), same payload envelope, same
/// `INSERT OR IGNORE` dedupe. This changes how many network trips write the
/// rows, NOT what an auditor reads.
const AUDIT_OUTBOX_INSERT_MANY_SQL: &str = "INSERT OR IGNORE INTO audit_outbox \
     (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at, region) \
     SELECT json_extract(value,'$.id'), json_extract(value,'$.tenant'), json_extract(value,'$.digest'), \
            json_extract(value,'$.request_id'), json_extract(value,'$.event_type'), \
            json_extract(value,'$.payload'), json_extract(value,'$.at'), NULL, \
            (SELECT primary_region FROM tenant WHERE tenant_id = json_extract(value,'$.tenant')) \
     FROM json_each(?1)";

/// Maximum `audit_outbox` rows carried by ONE
/// [`AUDIT_OUTBOX_INSERT_MANY_SQL`] statement.
///
/// The 100-parameter cap is not the binding constraint any more (the batch
/// rides in a single param) — the **value size** is: D1 caps a single
/// string/BLOB/row at 2,000,000 bytes. A `ReadAttempted` element runs roughly
/// 600-800 bytes once the CloudEvents envelope is embedded, so 256 rows is
/// ~200 KB — an order of magnitude under the cap even for long principals and
/// tenant ids. At the `FIND_MISSING_BLOB_CAP` of 4096 digests this is 16
/// statements (dispatched concurrently by
/// [`D1AuditOutboxSink::append_batch_async`]) instead of 4096 serial ones.
const AUDIT_BATCH_ROWS_PER_STATEMENT: usize = 256;

impl D1AuditOutboxSink {
    /// Construct over a shared [`D1HttpClient`] with a fixed CloudEvents
    /// `source`.
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>, source: &'static str) -> Self {
        Self { d1, source }
    }

    /// Derive the field values of ONE audit row — **the single place that
    /// decides what a row contains** (id, `request_id`, payload envelope,
    /// digest nullability, timestamp). See [`AuditRow`].
    fn build_row(
        &self,
        event_type: &str,
        tenant: &str,
        digest: Option<&str>,
        principal: &str,
        at_unix_ms: u64,
    ) -> Result<AuditRow, String> {
        // Epoch ms never realistically exceeds i64::MAX; saturate defensively.
        let at_ms = i64::try_from(at_unix_ms).unwrap_or(i64::MAX);
        // Treat an empty digest (pre-write events supply no hash) as SQL NULL.
        let dig = digest.filter(|d| !d.is_empty());

        let id = format!(
            "{}:{tenant}:{event_type}:{}:{principal}:{at_ms}",
            self.source,
            dig.unwrap_or("-"),
        );
        let request_id = format!("{tenant}:{}:{principal}:{at_ms}", dig.unwrap_or("-"));

        let payload = json!({
            "specversion": "1.0",
            "type": event_type,
            "source": self.source,
            "id": id,
            "subject": tenant,
            "time": ms_to_iso8601(at_ms),
            "data": {
                "tenant_id": tenant,
                "digest": dig,
                "principal": principal,
            }
        });
        let payload_json =
            serde_json::to_string(&payload).map_err(|e| format!("audit payload serialize: {e}"))?;

        Ok(AuditRow {
            id,
            tenant: tenant.to_owned(),
            digest: dig.map(str::to_owned),
            request_id,
            event_type: event_type.to_owned(),
            payload_json,
            at_ms,
        })
    }

    /// Build the `(sql, params)` pair for a SINGLE audit-event row, shared by
    /// BOTH the sync [`Self::append`] (drives it through
    /// [`Self::write_blocking`]) and the batch writer. The row's CONTENT comes
    /// from [`Self::build_row`],
    /// so this and [`Self::build_batch_insert`] cannot drift.
    fn build_insert(
        &self,
        event_type: &str,
        tenant: &str,
        digest: Option<&str>,
        principal: &str,
        at_unix_ms: u64,
    ) -> Result<(String, Vec<Value>), String> {
        let row = self.build_row(event_type, tenant, digest, principal, at_unix_ms)?;
        Ok((AUDIT_OUTBOX_INSERT_ONE_SQL.to_owned(), row.into_params()))
    }

    /// Build the `(sql, params)` pair that writes MANY rows in ONE statement
    /// — see [`AUDIT_OUTBOX_INSERT_MANY_SQL`]. `params` is always exactly one
    /// element: the JSON array `?1`.
    fn build_batch_insert(rows: &[AuditRow]) -> Result<(&'static str, Vec<Value>), String> {
        let elements: Vec<Value> = rows.iter().map(AuditRow::to_json_element).collect();
        let encoded = serde_json::to_string(&Value::Array(elements))
            .map_err(|e| format!("audit batch payload serialize: {e}"))?;
        Ok((AUDIT_OUTBOX_INSERT_MANY_SQL, vec![Value::String(encoded)]))
    }

    /// Append a single audit event to `audit_outbox`, synchronously (drives
    /// the write through [`Self::write_blocking`]'s `block_in_place` +
    /// `block_on` bridge). Every `AuditSink::emit` call routes through this.
    ///
    /// Keys (`id`, `request_id`) are deterministic per logical event so a
    /// re-emit collides on the PK / `UNIQUE(request_id, event_type)` and is an
    /// `INSERT OR IGNORE` no-op — idempotent on a handler retry (mirrors the
    /// DSR sink). Returns `Err` on serialize / D1 transport failure; the
    /// handler treats that as fail-CLOSED.
    fn append(
        &self,
        event_type: &str,
        tenant: &str,
        digest: Option<&str>,
        principal: &str,
        at_unix_ms: u64,
    ) -> Result<(), String> {
        let (sql, params) = self.build_insert(event_type, tenant, digest, principal, at_unix_ms)?;
        self.write_blocking(&sql, params)
    }

    /// Append MANY audit rows in ONE D1 round trip (or, past
    /// [`AUDIT_BATCH_ROWS_PER_STATEMENT`], a small fixed number of
    /// CONCURRENT ones) instead of one round trip per row.
    ///
    /// # Why this exists (the narrow batch seam)
    ///
    /// `AuditSink::emit` is, and stays, SYNC and one-row-at-a-time; this
    /// method is NOT part of that trait and is never reached through it. It
    /// exists solely for the Bazel REAPI `findMissingBlobs` batch seam
    /// (`R2CasHandler::exists_batch` in `storage/r2_s3.rs`), where a single
    /// request legitimately produces up to `FIND_MISSING_BLOB_CAP` (4096)
    /// `ReadAttempted` rows and the per-row round trip
    /// (~122 ms each, measured in prod) is the dominant cost of the endpoint.
    ///
    /// **This does not change the audit taxonomy.** N events still produce N
    /// rows, each byte-identical to what [`Self::append`] would have written
    /// for the same event (same [`Self::build_row`] derivation, same
    /// `INSERT OR IGNORE` idempotency key). Duplicate events inside one batch
    /// collide on the PK / `UNIQUE(request_id, event_type)` and dedupe
    /// exactly as a re-emit does today.
    ///
    /// Fail-CLOSED contract is unchanged: `Err` on serialize / D1 transport
    /// failure, and the caller must treat that as "no result may be served".
    /// It is ALL-or-nothing at the caller's level — any chunk failing fails
    /// the whole call.
    ///
    /// It deliberately opens NO [`crate::origin_timing::PhaseScope`]: the
    /// caller wraps the whole concurrent join in one `Phase::Store` scope so
    /// the overlapping window is attributed exactly once.
    async fn append_batch_async(&self, rows: Vec<AuditRow>) -> Result<(), String> {
        let statements = Self::build_batch_statements(&rows)?;
        // Chunks are independent `INSERT OR IGNORE`s over disjoint row sets,
        // so dispatching them together is safe and bounded: at the 4096
        // digest cap this is 16 in flight, never more.
        futures::future::try_join_all(
            statements
                .iter()
                .map(|(sql, params)| async move { self.d1.query(sql, params).await.map(|_| ()) }),
        )
        .await
        .map(|_| ())
    }

    /// Build exactly the D1 statements [`Self::append_batch_async`] dispatches.
    ///
    /// An empty slice deliberately produces an empty statement list: no
    /// `json_each('[]')` query is sent for a batch with no events.
    fn build_batch_statements(
        rows: &[AuditRow],
    ) -> Result<Vec<(&'static str, Vec<Value>)>, String> {
        rows.chunks(AUDIT_BATCH_ROWS_PER_STATEMENT)
            .map(Self::build_batch_insert)
            .collect()
    }

    /// Drive the async [`D1HttpClient::query`] to completion from the sync
    /// `AuditSink::emit` surface. MUST run on a multi-thread tokio worker (the
    /// axum handler path) — identical bridge + safety envelope as
    /// `routes/dsr/d1util::d1_query_blocking`.
    ///
    /// This is the choke point every SYNC CAS/AC/signup `emit`/`append` call
    /// routes through, so it is where the blocking D1-over-HTTP write is timed
    /// into `crate::origin_timing::Phase::Audit` (`oaudit`) — a single
    /// [`crate::origin_timing::PhaseScope`] here covers every caller rather
    /// than wrapping each `self.audit.emit(...)` call site individually.
    /// Silent pass-through with no ledger in scope, exactly like every other
    /// `PhaseScope` use: instrumentation can never change this method's
    /// result.
    ///
    /// The batch writer is the ONE exception: it drives the same SQL through
    /// `self.d1.query` directly (no `block_in_place`, no `PhaseScope` here)
    /// so its caller can attribute the joined audit/probe window itself.
    fn write_blocking(&self, sql: &str, params: Vec<Value>) -> Result<(), String> {
        let _scope = crate::origin_timing::PhaseScope::enter(crate::origin_timing::Phase::Audit);
        let d1 = Arc::clone(&self.d1);
        let sql = sql.to_owned();
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move { d1.query(&sql, &params).await })
        })
        .map(|_| ())
    }
}

impl corelink_handler_cas::AuditSink for D1AuditOutboxSink {
    fn emit(&self, event: corelink_handler_cas::AuditEvent) -> Result<(), String> {
        self.append(
            event.kind.slug(),
            &event.tenant,
            Some(event.hash.as_str()),
            &event.principal,
            event.at_unix_ms,
        )
    }
}

impl corelink_handler_ac::AuditSink for D1AuditOutboxSink {
    fn emit(&self, event: corelink_handler_ac::AuditEvent) -> Result<(), String> {
        self.append(
            event.kind.slug(),
            &event.tenant,
            Some(event.action_digest.as_str()),
            &event.principal,
            event.at_unix_ms,
        )
    }
}

impl D1AuditOutboxSink {
    /// Batch audit writer for the `findMissingBlobs` seam. This is the only
    /// native read path that intentionally overlaps its durable audit write
    /// with storage probes; no result is returned until the batch audit
    /// future has succeeded.
    /// `findMissingBlobs` seam only — see [`Self::append_batch_async`].
    ///
    /// Preserves the taxonomy exactly: one row per event, in request order,
    /// each derived by the SAME [`Self::build_row`] the single-row path uses.
    pub(crate) async fn emit_cas_batch_async(
        &self,
        events: &[corelink_handler_cas::AuditEvent],
    ) -> Result<(), String> {
        let rows = events
            .iter()
            .map(|event| {
                self.build_row(
                    event.kind.slug(),
                    &event.tenant,
                    Some(event.hash.as_str()),
                    &event.principal,
                    event.at_unix_ms,
                )
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.append_batch_async(rows).await
    }
}

/// [`crate::routes::signup::SignupAuditSink`] over the SAME durable
/// `audit_outbox` table + bridge as the CAS/AC impls above — this is a NEW
/// trait impl on the EXISTING durable sink, not a new audit mechanism.
///
/// `SignupAuditRow` doesn't carry a `digest` (pilot signups have no blob
/// hash); the token id/prefix rides in the `digest` column slot instead (same
/// "nullable, non-blob events" contract the column doc already states) and
/// the full row (`tenant_id`, `token_id_or_prefix`, `exit_status`, `payload`)
/// is preserved verbatim in the CloudEvents `data` envelope, unlike the
/// CAS/AC `append()` helper which only carries `(tenant, digest, principal)`.
///
/// The pilot route is pre-tenant for *every* arm. Its rejected/rate-limited
/// requests have no tenant, and a successful reservation's placeholder id is
/// not present in `tenant` until operator provisioning. The audit row itself
/// therefore uses the canonical `_public` namespace, pinned to `wnam` by the
/// writer. A nil UUID is never a D1 audit identity.
impl crate::routes::signup::SignupAuditSink for D1AuditOutboxSink {
    fn emit(&self, row: crate::routes::signup::SignupAuditRow) -> Result<(), &'static str> {
        let (sql, params) = self
            .build_signup_insert(row)
            .map_err(|_| "signup audit sink: payload serialize failed")?;
        self.write_blocking(&sql, params)
            .map_err(|_| "signup audit sink: D1 write failed")
    }
}

impl D1AuditOutboxSink {
    /// Build the production pilot-signup audit INSERT without sending it.
    ///
    /// Kept as one helper so the live sink and the migration-level regression
    /// exercise exactly the same SQL/parameters. The route remains fail-CLOSED
    /// because callers still send the returned statement through
    /// [`Self::write_blocking`], and any D1 error is propagated by `emit`.
    fn build_signup_insert(
        &self,
        row: crate::routes::signup::SignupAuditRow,
    ) -> Result<(String, Vec<Value>), String> {
        let at_ms = i64::try_from(row.emitted_at_ms).unwrap_or(i64::MAX);
        // `_public` is a canonical residency-guard exception (migration
        // 0107), unlike the nil UUID previously used for pre-auth rows.
        // Keep `row.tenant_id` only in the event data below; it is a pilot
        // reservation correlation id, not yet a tenant row.
        let tenant = crate::routes::signup::PILOT_AUDIT_NAMESPACE;
        let token = row.token_id_or_prefix.clone().unwrap_or_default();
        let id = format!(
            "{}:{tenant}:{}:{}:{at_ms}",
            self.source,
            row.event_type,
            if token.is_empty() { "-" } else { &token },
        );
        let request_id = format!(
            "{tenant}:{}:{}:{at_ms}",
            row.event_type,
            if token.is_empty() { "-" } else { &token },
        );
        let payload = json!({
            "specversion": "1.0",
            "type": row.event_type,
            "source": self.source,
            "id": id,
            "subject": tenant,
            "time": ms_to_iso8601(at_ms),
            "data": {
                "tenant_id": row.tenant_id.map(|u| u.to_string()),
                "token_id_or_prefix": row.token_id_or_prefix,
                "exit_status": row.exit_status,
                "payload": row.payload,
            }
        });
        let payload_json = serde_json::to_string(&payload)
            .map_err(|e| format!("signup audit sink: payload serialize failed: {e}"))?;
        let digest_param = if token.is_empty() {
            Value::Null
        } else {
            json!(token)
        };
        // Pilot audit rows are intentionally pre-tenant. Pinning the public
        // namespace to its canonical region keeps the row compatible with
        // both the NOT NULL region column and migration 0107's residency
        // guard; tenant-scoped rows continue to use the correlated lookup in
        // `append` above.
        let sql = "INSERT OR IGNORE INTO audit_outbox \
             (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at, region) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)";
        let params = vec![
            json!(id),
            json!(tenant),
            digest_param,
            json!(request_id),
            json!(row.event_type),
            json!(payload_json),
            json!(at_ms),
            json!(crate::routes::signup::PILOT_AUDIT_REGION),
        ];
        Ok((sql.to_owned(), params))
    }
}

/// Yield a DURABLE CAS audit sink from a D1-client construction result, or
/// REFUSE (fail-CLOSED) when the D1 client could not be built.
///
/// The `Result` seam makes the refusal deterministically testable (a durable
/// sink can't force `reqwest` to fail). The production builder
/// (`build_r2_cas_handler_from_env`) now calls
/// [`cas_audit_sink_from_d1_concrete`] directly (it needs the concrete type
/// for the batch seam) — this type-erased wrapper is kept for tests that
/// only care about the `AuditSink` trait-object shape, hence `#[cfg(test)]`.
///
/// # Errors
///
/// Returns `Err(String)` when `d1` is `Err` (fail-CLOSED — see
/// [`cas_audit_sink_from_d1_concrete`]).
#[cfg(test)]
pub(crate) fn cas_audit_sink_from_d1(
    d1: Result<D1HttpClient, String>,
) -> Result<Arc<dyn corelink_handler_cas::AuditSink>, String> {
    Ok(cas_audit_sink_from_d1_concrete(d1)? as Arc<dyn corelink_handler_cas::AuditSink>)
}

/// Concrete-typed counterpart of [`cas_audit_sink_from_d1`], for callers
/// that need the narrow batch seam and not just the type-erased `AuditSink`
/// trait object — currently only `build_r2_cas_handler_from_env`, which passes
/// this SAME `Arc` to both `R2CasHandler::new` (coerced to `Arc<dyn AuditSink>`)
/// and `R2CasHandler::with_async_audit` (kept concrete).
///
/// # Errors
///
/// Returns `Err(String)` when `d1` is `Err` (fail-CLOSED — see
/// [`cas_audit_sink_from_d1`]).
pub(crate) fn cas_audit_sink_from_d1_concrete(
    d1: Result<D1HttpClient, String>,
) -> Result<Arc<D1AuditOutboxSink>, String> {
    Ok(Arc::new(D1AuditOutboxSink::new(
        durable_client(d1)?,
        "corelink/cas",
    )))
}

/// AC counterpart of [`cas_audit_sink_from_d1`] — also `#[cfg(test)]` for
/// the same reason (see there).
///
/// # Errors
///
/// Returns `Err(String)` when `d1` is `Err` (fail-CLOSED — see
/// [`cas_audit_sink_from_d1_concrete`]).
#[cfg(test)]
pub(crate) fn ac_audit_sink_from_d1(
    d1: Result<D1HttpClient, String>,
) -> Result<Arc<dyn corelink_handler_ac::AuditSink>, String> {
    Ok(ac_audit_sink_from_d1_concrete(d1)? as Arc<dyn corelink_handler_ac::AuditSink>)
}

/// Concrete-typed counterpart of [`ac_audit_sink_from_d1`] — see
/// [`cas_audit_sink_from_d1_concrete`] for why this exists (used by
/// `build_r2_ac_handler_from_env`).
///
/// # Errors
///
/// Returns `Err(String)` when `d1` is `Err` (fail-CLOSED — see
/// [`cas_audit_sink_from_d1`]).
pub(crate) fn ac_audit_sink_from_d1_concrete(
    d1: Result<D1HttpClient, String>,
) -> Result<Arc<D1AuditOutboxSink>, String> {
    Ok(Arc::new(D1AuditOutboxSink::new(
        durable_client(d1)?,
        "corelink/ac",
    )))
}

/// Pilot-signup counterpart of [`cas_audit_sink_from_d1`] — same
/// durable `audit_outbox` seam, wired for
/// `routes::signup::build_state_from_env()`.
///
/// # Errors
///
/// Returns `Err(String)` when `d1` is `Err` (fail-CLOSED — see
/// [`cas_audit_sink_from_d1`]).
pub(crate) fn signup_audit_sink_from_d1(
    d1: Result<D1HttpClient, String>,
) -> Result<Arc<dyn crate::routes::signup::SignupAuditSink>, String> {
    Ok(Arc::new(D1AuditOutboxSink::new(
        durable_client(d1)?,
        "corelink/signup",
    )))
}

/// Wrap the D1-client result into a shared handle, or fail CLOSED with a loud
/// message.
fn durable_client(d1: Result<D1HttpClient, String>) -> Result<Arc<D1HttpClient>, String> {
    d1.map(Arc::new)
        .map_err(|e| format!("durable audit sink unavailable: D1 client build failed: {e}"))
}

// ---------------------------------------------------------------------------
// Tests live in the sibling `d1_audit_sink/` directory, one file per PROPERTY:
//
//   tests_wiring            — the deployed builder seam is binary: a DURABLE
//                             sink or a loud Err, never a silent InMemory
//                             fallback.
//   tests_batch_shape       — batching changes how many round trips carry the
//                             rows, never what an auditor reads.
//   tests_batch_limits      — the statement carrying them stays legal at both
//                             boundaries: never oversized, never degenerate.
//   tests_phase_attribution — the blocking D1 write is timed into
//                             `Phase::Audit`, not the `oother` residue.
//   tests_support           — the shared fixtures, here rather than in a
//                             sibling so no test file looks load-bearing for
//                             the others.

#[cfg(test)]
mod tests_support;

#[cfg(test)]
mod tests_wiring;

#[cfg(test)]
mod tests_batch_shape;

#[cfg(test)]
mod tests_batch_limits;

#[cfg(test)]
mod tests_phase_attribution;

#[cfg(test)]
mod tests_signup;
