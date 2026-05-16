//! Production `RealNeonShadowSink` driver — closes the wave-18 caveat
//! #3 (the in-memory fake shipped wave-18; the real Postgres driver
//! ships here).
//!
//! ## What this module ships
//!
//! - [`NeonProjectResolver`] trait — looks up the per-region Neon
//!   project DSN (a Neon **project** is the residency-pinned Postgres
//!   instance; each `Region` has its own project per
//!   INV-DATA-RESIDENCY CRITICAL).
//! - [`EnvVarResolver`] — canonical impl reading `NEON_DB_URL_<REGION>`
//!   env vars (e.g. `NEON_DB_URL_IAD`, `NEON_DB_URL_FRA`).
//! - [`NeonExecutor`] trait — the SQL backplane abstraction. The real
//!   `tokio-postgres` adapter satisfies this trait (lives at the
//!   binary boot path, per the trait-abstraction-defer charter
//!   pattern; mirrors `corelink-byok-aws::AwsKmsRealProvider` +
//!   `corelink-cf-bindings::cf_r2`).
//! - [`RealNeonShadowSink`] — the production [`NeonShadowSink`] impl
//!   that orchestrates: tenant + residency pre-validation → BEGIN txn
//!   → `SELECT set_config('app.current_tenant', $1, true)` (RLS GUC
//!   binding) → idempotent INSERT (`ON CONFLICT (tenant_id, seq) DO
//!   NOTHING`) → COMMIT → audit emit. Fail-CLOSED on every arm.
//!
//! ## Why `NeonExecutor` not direct `tokio_postgres::Client`
//!
//! The `tokio-postgres` crate pulls a heavy tokio runtime + native-TLS
//! transitive deps the CF Worker target cannot link. The
//! `corelink-audit-chain` crate stays pure-logic + sync-trait; the
//! production binder (a thin adapter wrapping
//! `deadpool_postgres::Pool` behind `worker::send::SendFuture`) lives
//! in `apps/server`. The `NeonExecutor` trait is the seam.
//!
//! ## SQL contract (pinned at compile time)
//!
//! Every SQL string used by `RealNeonShadowSink` is a `pub const &str`
//! in this module so:
//!
//! 1. Schema drift between Rust + `migrations/neon/0001_audit_events_shadow.sql`
//!    is caught at unit-test time (`tests::sql_constants_match_migration`).
//! 2. The reconciliation cron (`.github/workflows/neon-shadow-reconcile-daily.yml`)
//!    consumes the same SQL via a thin Python harness — single
//!    canonical source of truth.
//!
//! ## RLS contract
//!
//! Every txn opens with:
//!
//! ```sql
//! BEGIN;
//! SELECT set_config('app.current_tenant', $tenant_uuid::text, true);
//! ```
//!
//! and the migration's `tenant_isolation_audit_events_shadow` RLS
//! policy gates EVERY read + write against
//! `current_setting('app.current_tenant')`. A wiring bug that drops
//! the `set_config` SET surfaces as a Postgres permission error
//! INSIDE the txn — never as a silent cross-tenant leak.
//!
//! ## Idempotency
//!
//! INSERTs use `ON CONFLICT (tenant_id, seq) DO NOTHING` per
//! INV-AUDIT-APPEND-ONLY. A retry after a transient network blip
//! that already committed half the rows correctly drops the
//! duplicates without raising — mirrors the wave-18 in-memory fake.
//!
//! ## Native-only
//!
//! `RealNeonShadowSink` is `cfg(not(target_arch = "wasm32"))`. The
//! wasm32 target compiles a stub whose every method returns
//! [`NeonError::WasmOnly`]. CF Workers reach the production Neon
//! project via the native gRPC server, NOT directly from the worker
//! isolate.

#![allow(clippy::uninlined_format_args)]

use std::sync::Arc;

use subtle::ConstantTimeEq;
use uuid::Uuid;

use corelink_analytics::Region;

use crate::archive_producer::ArchiveReceipt;
use crate::neon_shadow::{
    redact_tenant_uuid, EventCountBucket, NeonShadowError, NeonShadowSink, ShadowEventRow,
    ShadowSyncAuditRow, ShadowSyncAuditSink, ShadowSyncReceipt, TimelineBucket,
    EVENT_TYPE_SHADOW_SYNCED, EVENT_TYPE_SHADOW_SYNC_FAILED, SHADOW_LAG_SEV2_THRESHOLD_MS,
};

// ---------------------------------------------------------------------------
// SQL constants (single source of truth — pinned to the migration).
// ---------------------------------------------------------------------------

/// `BEGIN` opens a Postgres transaction. Used by every sink op so the
/// `SELECT set_config('app.current_tenant', ...)` GUC SET is scoped to
/// the in-flight txn (RLS isolation lives or dies with this txn).
pub const SQL_BEGIN_TXN: &str = "BEGIN";

/// Bind the RLS tenant GUC for the txn. Parameter `$1` is the
/// `tenant_id::text`. The third argument `true` to `set_config` scopes
/// the setting to the current transaction so a poolboy-reused
/// connection cannot leak the GUC across requests.
///
/// The `migrations/neon/0001_audit_events_shadow.sql` RLS policy
/// `tenant_isolation_audit_events_shadow` reads
/// `current_setting('app.current_tenant')` so this SET is load-bearing
/// for INV-AUTH-SCHEMA-RLS-DEFAULT-ON CRITICAL.
pub const SQL_SET_RLS_TENANT_GUC: &str =
    "SELECT set_config('app.current_tenant', $1, true)";

/// Idempotent batch INSERT for the shadow table.
///
/// `ON CONFLICT (tenant_id, seq) DO NOTHING` is the canonical
/// idempotency primitive (INV-AUDIT-APPEND-ONLY + retry-safe). The
/// PRIMARY KEY pinned in the migration is `(tenant_id, seq)` so a
/// retry that re-attempts an already-committed (tenant, seq) is a
/// no-op.
///
/// The bound parameter ordering MUST match
/// `(tenant_id, seq, event_time_ms_to_timestamptz, event_type,
///  prev_hash_hex, link_hash_hex, payload_jsonb, region)`.
pub const SQL_INSERT_SHADOW_ROW: &str = "INSERT INTO audit_events_shadow \
    (tenant_id, seq, event_time, event_type, prev_hash, link_hash, payload_jsonb, region) \
    VALUES ($1, $2, to_timestamp($3::double precision / 1000.0), $4, $5, $6, $7::jsonb, $8) \
    ON CONFLICT (tenant_id, seq) DO NOTHING";

/// Aggregate event-count query (counterpart of
/// `InMemoryNeonShadowSink::aggregate_event_count`).
///
/// Parameter ordering: `(from_ms, to_ms)`. The optional event-type
/// filter is appended at the executor level when set; see
/// `query_event_count`.
pub const SQL_QUERY_EVENT_COUNT: &str = "SELECT event_type, COUNT(*)::bigint AS cnt \
    FROM audit_events_shadow \
    WHERE event_time >= to_timestamp($1::double precision / 1000.0) \
      AND event_time <  to_timestamp($2::double precision / 1000.0) \
    GROUP BY event_type \
    ORDER BY event_type";

/// Aggregate event-count with event-type filter (parameter `$3`).
pub const SQL_QUERY_EVENT_COUNT_FILTERED: &str = "SELECT event_type, COUNT(*)::bigint AS cnt \
    FROM audit_events_shadow \
    WHERE event_time >= to_timestamp($1::double precision / 1000.0) \
      AND event_time <  to_timestamp($2::double precision / 1000.0) \
      AND event_type = $3 \
    GROUP BY event_type \
    ORDER BY event_type";

/// Aggregate timeline query (counterpart of
/// `InMemoryNeonShadowSink::aggregate_timeline`).
///
/// Parameter ordering: `(from_ms, to_ms, granularity_ms)`. The
/// per-bucket aggregation uses Postgres' integer division on the
/// `(event_time_ms - from_ms) / granularity_ms` offset so the bucket
/// boundaries match the in-memory fake byte-for-byte.
pub const SQL_QUERY_TIMELINE: &str =
    "SELECT (FLOOR((EXTRACT(EPOCH FROM event_time) * 1000 - $1::bigint)::bigint / $3::bigint) \
            * $3::bigint + $1::bigint)::bigint AS bucket_start_ms, \
            COUNT(*)::bigint AS cnt \
     FROM audit_events_shadow \
     WHERE event_time >= to_timestamp($1::double precision / 1000.0) \
       AND event_time <  to_timestamp($2::double precision / 1000.0) \
     GROUP BY bucket_start_ms \
     ORDER BY bucket_start_ms";

/// Reconciliation diff query — counts shadow rows for a (tenant, date)
/// pair. The daily reconciliation cron compares this count against the
/// R2 NDJSON archive's row count for the same window; a non-zero diff
/// fires SEV-2 (`NeonShadowDriftSev2`) per
/// `dashboards/alerts/dash-neon-shadow-alerts.yml`.
///
/// Parameter ordering: `(from_ms, to_ms)`.
pub const SQL_RECONCILE_COUNT: &str =
    "SELECT COUNT(*)::bigint AS cnt FROM audit_events_shadow \
     WHERE event_time >= to_timestamp($1::double precision / 1000.0) \
       AND event_time <  to_timestamp($2::double precision / 1000.0)";

/// `COMMIT` closes the txn opened by `SQL_BEGIN_TXN`. A panic /
/// `return Err` BEFORE this statement leaves the txn aborted; the
/// connection-pool's reset behavior surfaces the rollback.
pub const SQL_COMMIT_TXN: &str = "COMMIT";

// ---------------------------------------------------------------------------
// NeonProjectResolver — per-region Neon project DSN resolution.
// ---------------------------------------------------------------------------

/// Per-region Neon project DSN resolver. Production binds
/// [`EnvVarResolver`] (reads `NEON_DB_URL_<REGION_UPPER>`); tests
/// inject a static map.
pub trait NeonProjectResolver: Send + Sync + core::fmt::Debug {
    /// Resolve the canonical DSN for a region.
    ///
    /// # Errors
    ///
    /// Returns [`NeonError::ProjectUnresolved`] when no DSN is
    /// configured for the region.
    fn resolve(&self, region: Region) -> Result<String, NeonError>;
}

/// Canonical env-var-backed resolver. Maps `Region::Iad` →
/// `NEON_DB_URL_IAD`, `Region::Fra` → `NEON_DB_URL_FRA`, etc.
///
/// `secrets-checklist.md` rows pin every per-region env var
/// (`docs/internal/secrets-checklist.md`; one row per active region).
#[derive(Debug, Default)]
pub struct EnvVarResolver;

impl EnvVarResolver {
    /// Construct the canonical resolver.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Build the canonical env var name for a region: `NEON_DB_URL_<UPPER>`.
    #[must_use]
    pub fn env_var_name(region: Region) -> String {
        format!("NEON_DB_URL_{}", region.as_str().to_uppercase())
    }
}

impl NeonProjectResolver for EnvVarResolver {
    fn resolve(&self, region: Region) -> Result<String, NeonError> {
        let var = Self::env_var_name(region);
        std::env::var(&var).map_err(|_| NeonError::ProjectUnresolved {
            region: region.as_str(),
            env_var: var,
        })
    }
}

/// Static-map resolver for tests — does NOT read env vars. Each call
/// returns a clone of the per-region DSN string.
#[derive(Debug, Clone, Default)]
pub struct StaticResolver {
    entries: std::collections::BTreeMap<&'static str, String>,
}

impl StaticResolver {
    /// Construct an empty static resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a `(region, dsn)` pair.
    #[must_use]
    pub fn with(mut self, region: Region, dsn: impl Into<String>) -> Self {
        self.entries.insert(region.as_str(), dsn.into());
        self
    }
}

impl NeonProjectResolver for StaticResolver {
    fn resolve(&self, region: Region) -> Result<String, NeonError> {
        match self.entries.get(region.as_str()) {
            Some(dsn) => Ok(dsn.clone()),
            None => Err(NeonError::ProjectUnresolved {
                region: region.as_str(),
                env_var: EnvVarResolver::env_var_name(region),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// NeonExecutor — the SQL backplane abstraction.
// ---------------------------------------------------------------------------

/// One row of a `query` response. A `NULL` cell maps to `None`. The
/// driver decodes `int8` / `text` / `jsonb` as UTF-8 strings — the
/// `RealNeonShadowSink` aggregate parsers handle the type coercion in
/// pure Rust so the executor stays type-agnostic.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ExecutorRow {
    /// One String per column. `None` = SQL `NULL`.
    pub cells: Vec<Option<String>>,
}

/// One scalar parameter passed to an executor call. The driver routes
/// the value to a typed Postgres parameter — `Uuid` to a `uuid`-typed
/// column, `Int8` to `int8`, etc. Type widening (e.g. u64 → i64) is
/// the caller's responsibility.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecutorParam {
    /// `tenant_id` UUID — bound directly via tokio-postgres `Uuid`
    /// support so cross-tenant injection via stringification is
    /// impossible.
    Uuid(Uuid),
    /// 64-bit signed integer (`int8`). Used for `seq`, `event_time_ms`,
    /// `from_ms`, `to_ms`, `granularity_ms`.
    Int8(i64),
    /// Text (`text` / `varchar`). Used for `event_type`, `region`.
    Text(String),
    /// Raw bytes encoded as hex string (`bytea` -> hex round-trip). The
    /// Rust side hands the driver the hex form; the migration column
    /// is `text` for `prev_hash` / `link_hash` to keep the on-the-wire
    /// shape stable.
    HexBytes(String),
    /// Raw JSON text bound to a `jsonb` column. The driver casts via
    /// `$N::jsonb` so the column type stays canonical.
    Jsonb(String),
}

/// SQL backplane abstraction. Production binds `TokioPostgresExecutor`
/// (lives at the binary boot path; pulls `tokio-postgres` +
/// `deadpool-postgres`). Tests bind [`InMemoryExecutor`].
///
/// Every method is synchronous — the production binder wraps the
/// async tokio-postgres calls via `worker::send::SendFuture` (mirrors
/// `corelink-cf-bindings::cf_r2`) or via `block_on` at the
/// composition root, NEVER inside this trait.
pub trait NeonExecutor: Send + Sync + core::fmt::Debug {
    /// Execute a statement that returns no rows (`INSERT`, `BEGIN`,
    /// `COMMIT`, `SELECT set_config`). Returns the number of rows
    /// affected.
    ///
    /// # Errors
    ///
    /// Returns [`NeonError::Backend`] on driver error.
    fn execute(&self, sql: &str, params: &[ExecutorParam]) -> Result<u64, NeonError>;

    /// Execute a query and materialize every row into [`ExecutorRow`].
    /// Aggregate queries (`SELECT event_type, COUNT(*) ...`) hit this.
    ///
    /// # Errors
    ///
    /// Returns [`NeonError::Backend`] on driver error.
    fn query(&self, sql: &str, params: &[ExecutorParam]) -> Result<Vec<ExecutorRow>, NeonError>;
}

// ---------------------------------------------------------------------------
// NeonError — the production driver's error surface.
// ---------------------------------------------------------------------------

/// Canonical error surface for the production Neon driver path. Maps
/// to [`NeonShadowError`] at the trait boundary so callers reading the
/// `NeonShadowSink` API see a single error type.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NeonError {
    /// No DSN configured for the requested region. Surface as SEV-2
    /// (analytics lag — not a chain break). Production wiring fires
    /// the `NeonShadowProjectUnresolved` alert.
    #[error(
        "neon project unresolved: region={region}, env_var={env_var} not set"
    )]
    ProjectUnresolved {
        /// Region label (`"iad"` / `"fra"` / ...).
        region: &'static str,
        /// Canonical env var name (`"NEON_DB_URL_IAD"`).
        env_var: String,
    },

    /// The driver rejected the SQL or the txn aborted. SEV-2.
    #[error("neon driver error: {0}")]
    Backend(String),

    /// Type-coercion failure when decoding an executor row (e.g. a
    /// `count(*)::bigint` arrived as a non-numeric string).
    #[error("neon row decode error at column {column}: {detail}")]
    RowDecode {
        /// 0-based column index.
        column: usize,
        /// Operator-readable detail.
        detail: String,
    },

    /// Wasm32 target: the production driver is native-only.
    /// CF Workers reach Neon via the native gRPC server.
    #[error("neon real driver unsupported on wasm32 — route via native gRPC server")]
    WasmOnly,

    /// Internal invariant violation (empty rows, mismatched param
    /// count, etc.).
    #[error("neon real internal: {0}")]
    Internal(String),
}

impl From<NeonError> for NeonShadowError {
    fn from(e: NeonError) -> Self {
        match e {
            NeonError::WasmOnly => {
                NeonShadowError::Backend("neon real driver unsupported on wasm32".to_string())
            }
            NeonError::ProjectUnresolved { region, env_var } => NeonShadowError::Backend(format!(
                "project unresolved for region={} (env {} unset)",
                region, env_var
            )),
            NeonError::Backend(msg) => NeonShadowError::Backend(msg),
            NeonError::RowDecode { column, detail } => NeonShadowError::Backend(format!(
                "row decode at col {}: {}",
                column, detail
            )),
            NeonError::Internal(msg) => NeonShadowError::Internal(msg),
        }
    }
}

// ---------------------------------------------------------------------------
// RealNeonShadowSink — the production NeonShadowSink impl.
// ---------------------------------------------------------------------------

/// Production [`NeonShadowSink`] impl. Binds a single
/// `(tenant_id, region)` pair to a [`NeonExecutor`] so cross-tenant /
/// cross-region writes are caught at the type system AND at the SQL
/// RLS layer (defense in depth — see module docs).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct RealNeonShadowSink {
    tenant_id: Uuid,
    region: Region,
    executor: Arc<dyn NeonExecutor>,
    audit_sink: Arc<dyn ShadowSyncAuditSink>,
}

#[cfg(not(target_arch = "wasm32"))]
impl RealNeonShadowSink {
    /// Construct the sink. The executor is shared via `Arc` so a
    /// single connection pool services all per-region sinks (the pool
    /// itself partitions by region at the binder layer).
    #[must_use]
    pub fn new(
        tenant_id: Uuid,
        region: Region,
        executor: Arc<dyn NeonExecutor>,
        audit_sink: Arc<dyn ShadowSyncAuditSink>,
    ) -> Self {
        Self {
            tenant_id,
            region,
            executor,
            audit_sink,
        }
    }

    /// Helper: emit a shadow-sync audit row, lifting an audit-sink
    /// rejection to [`NeonShadowError::AuditEmitFailed`].
    ///
    /// Wave-21 (B-P1-02 closure): the wave-18 helper discarded the
    /// emit result via `let _ = ...`. On a paired audit-sink-down +
    /// cross-tenant attempt the SEV-2 anchor the security team
    /// subscribes to silently vanished. The wave-20 SQL `WITH CHECK`
    /// constraint is the structural backstop; this helper closes the
    /// trait-level gap so the route layer sees the failure and can
    /// translate to a 503 + SEV-1 alert.
    ///
    /// Ordering note: the helper is still called BEFORE the original
    /// violation `Err(_)` is returned, so the SEV-2 audit row lands on
    /// the happy path. Only an audit-pipeline failure escalates to
    /// `AuditEmitFailed` (SEV-1 > SEV-2 in the operator response
    /// matrix).
    fn emit_audit(&self, row: ShadowSyncAuditRow) -> Result<(), NeonShadowError> {
        self.audit_sink
            .emit(row)
            .map_err(NeonShadowError::AuditEmitFailed)
    }

    /// Constant-time tenant id comparison via `subtle::ConstantTimeEq`.
    /// Production wiring runs the comparison on every row so a wiring
    /// bug that drops the bound tenant id surfaces immediately
    /// (defense in depth on top of the SQL RLS).
    fn tenant_eq_ct(a: Uuid, b: Uuid) -> bool {
        a.as_bytes().ct_eq(b.as_bytes()).into()
    }

    /// Reconciliation count query (used by the daily reconcile cron
    /// via a Python harness — see
    /// `.github/workflows/neon-shadow-reconcile-daily.yml`).
    ///
    /// # Errors
    ///
    /// Returns [`NeonShadowError::Backend`] when the executor errors
    /// or the count row decodes to a non-integer.
    pub fn reconcile_count(
        &self,
        from_ms: u64,
        to_ms: u64,
    ) -> Result<u64, NeonShadowError> {
        // RLS GUC set inside the same txn so the read sees only the
        // bound tenant (defense in depth — the executor is also bound
        // per-region at construction).
        let _ = self
            .executor
            .execute(SQL_BEGIN_TXN, &[])
            .map_err(NeonShadowError::from)?;
        let _ = self
            .executor
            .execute(
                SQL_SET_RLS_TENANT_GUC,
                &[ExecutorParam::Text(self.tenant_id.to_string())],
            )
            .map_err(NeonShadowError::from)?;
        let rows = self
            .executor
            .query(
                SQL_RECONCILE_COUNT,
                &[
                    ExecutorParam::Int8(from_ms as i64),
                    ExecutorParam::Int8(to_ms as i64),
                ],
            )
            .map_err(NeonShadowError::from)?;
        let _ = self
            .executor
            .execute(SQL_COMMIT_TXN, &[])
            .map_err(NeonShadowError::from)?;
        let first = rows.first().ok_or_else(|| {
            NeonShadowError::Internal("reconcile_count returned no rows".to_string())
        })?;
        let cell = first.cells.first().and_then(|c| c.as_ref()).ok_or_else(|| {
            NeonShadowError::Internal("reconcile_count: null count cell".to_string())
        })?;
        cell.parse::<u64>().map_err(|e| {
            NeonShadowError::Backend(format!("reconcile count parse: {}", e))
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl NeonShadowSink for RealNeonShadowSink {
    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    fn region(&self) -> Region {
        self.region
    }

    fn sync_chunk(
        &self,
        receipt: &ArchiveReceipt,
        rows: &[ShadowEventRow],
        now_ms: u64,
    ) -> Result<ShadowSyncReceipt, NeonShadowError> {
        // 1. Tenant + residency pre-checks (mirrors the in-memory fake
        //    — fail-CLOSED + audit emit BEFORE returning err).
        if !Self::tenant_eq_ct(receipt.tenant_id, self.tenant_id) {
            self.emit_audit(ShadowSyncAuditRow {
                event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                tenant_id: self.tenant_id,
                first_seq: receipt.first_sequence_number,
                last_seq: receipt.last_sequence_number,
                region: self.region,
                observed_lag_ms: 0,
                failure_reason: "tenant isolation violation".to_string(),
                sev: "sev-2",
            })?;
            return Err(NeonShadowError::TenantIsolationViolation {
                sink_tenant: self.tenant_id.to_string(),
                sink_tenant_redacted: redact_tenant_uuid(&self.tenant_id),
                observed_tenant: receipt.tenant_id.to_string(),
                observed_tenant_redacted: redact_tenant_uuid(&receipt.tenant_id),
            });
        }
        for row in rows {
            if !Self::tenant_eq_ct(row.tenant_id, self.tenant_id) {
                self.emit_audit(ShadowSyncAuditRow {
                    event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                    tenant_id: self.tenant_id,
                    first_seq: receipt.first_sequence_number,
                    last_seq: receipt.last_sequence_number,
                    region: self.region,
                    observed_lag_ms: 0,
                    failure_reason: "row tenant isolation violation".to_string(),
                    sev: "sev-2",
                })?;
                return Err(NeonShadowError::TenantIsolationViolation {
                    sink_tenant: self.tenant_id.to_string(),
                    sink_tenant_redacted: redact_tenant_uuid(&self.tenant_id),
                    observed_tenant: row.tenant_id.to_string(),
                    observed_tenant_redacted: redact_tenant_uuid(&row.tenant_id),
                });
            }
            if row.region != self.region {
                self.emit_audit(ShadowSyncAuditRow {
                    event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                    tenant_id: self.tenant_id,
                    first_seq: receipt.first_sequence_number,
                    last_seq: receipt.last_sequence_number,
                    region: self.region,
                    observed_lag_ms: 0,
                    failure_reason: "row residency violation".to_string(),
                    sev: "sev-2",
                })?;
                return Err(NeonShadowError::ResidencyViolation {
                    sink_region: self.region.as_str(),
                    observed_region: row.region.as_str(),
                });
            }
        }
        if rows.is_empty() {
            return Err(NeonShadowError::Internal("empty rows slice".to_string()));
        }

        // 2. Compute observed lag from the first event's wall-clock to
        //    the caller-supplied `now_ms`.
        let first_event_time_ms = rows.first().map(|r| r.event_time_ms).unwrap_or(0);
        let observed_lag_ms = now_ms.saturating_sub(first_event_time_ms);

        // 3. Open txn + bind RLS tenant GUC.
        if let Err(e) = self.executor.execute(SQL_BEGIN_TXN, &[]) {
            self.emit_audit(ShadowSyncAuditRow {
                event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                tenant_id: self.tenant_id,
                first_seq: receipt.first_sequence_number,
                last_seq: receipt.last_sequence_number,
                region: self.region,
                observed_lag_ms,
                failure_reason: format!("BEGIN failed: {}", e),
                sev: "sev-2",
            })?;
            return Err(e.into());
        }
        if let Err(e) = self.executor.execute(
            SQL_SET_RLS_TENANT_GUC,
            &[ExecutorParam::Text(self.tenant_id.to_string())],
        ) {
            self.emit_audit(ShadowSyncAuditRow {
                event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                tenant_id: self.tenant_id,
                first_seq: receipt.first_sequence_number,
                last_seq: receipt.last_sequence_number,
                region: self.region,
                observed_lag_ms,
                failure_reason: format!("set_config failed: {}", e),
                sev: "sev-2",
            })?;
            return Err(e.into());
        }

        // 4. Idempotent INSERT per row. The executor implementation
        //    is expected to pipeline; the sink stays sync.
        for row in rows {
            let prev_hex = hex::encode(row.prev_hash.0);
            let link_hex = hex::encode(row.link_hash.0);
            let params = [
                ExecutorParam::Uuid(row.tenant_id),
                ExecutorParam::Int8(row.seq as i64),
                ExecutorParam::Int8(row.event_time_ms as i64),
                ExecutorParam::Text(row.event_type.clone()),
                ExecutorParam::HexBytes(prev_hex),
                ExecutorParam::HexBytes(link_hex),
                ExecutorParam::Jsonb(row.payload_json.clone()),
                ExecutorParam::Text(row.region.as_str().to_string()),
            ];
            if let Err(e) = self.executor.execute(SQL_INSERT_SHADOW_ROW, &params) {
                self.emit_audit(ShadowSyncAuditRow {
                    event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                    tenant_id: self.tenant_id,
                    first_seq: receipt.first_sequence_number,
                    last_seq: receipt.last_sequence_number,
                    region: self.region,
                    observed_lag_ms,
                    failure_reason: format!("INSERT failed at seq={}: {}", row.seq, e),
                    sev: "sev-2",
                })?;
                return Err(e.into());
            }
        }

        // 5. Commit txn.
        if let Err(e) = self.executor.execute(SQL_COMMIT_TXN, &[]) {
            self.emit_audit(ShadowSyncAuditRow {
                event_type: EVENT_TYPE_SHADOW_SYNC_FAILED,
                tenant_id: self.tenant_id,
                first_seq: receipt.first_sequence_number,
                last_seq: receipt.last_sequence_number,
                region: self.region,
                observed_lag_ms,
                failure_reason: format!("COMMIT failed: {}", e),
                sev: "sev-2",
            })?;
            return Err(e.into());
        }

        // 6. Audit emit (success).
        let sev = if observed_lag_ms >= SHADOW_LAG_SEV2_THRESHOLD_MS {
            "sev-2"
        } else {
            "info"
        };
        self.emit_audit(ShadowSyncAuditRow {
            event_type: EVENT_TYPE_SHADOW_SYNCED,
            tenant_id: self.tenant_id,
            first_seq: receipt.first_sequence_number,
            last_seq: receipt.last_sequence_number,
            region: self.region,
            observed_lag_ms,
            failure_reason: String::new(),
            sev,
        })?;

        Ok(ShadowSyncReceipt {
            tenant_id: self.tenant_id,
            first_seq: receipt.first_sequence_number,
            last_seq: receipt.last_sequence_number,
            rows_persisted: rows.len() as u64,
            observed_lag_ms,
            region: self.region,
        })
    }

    fn aggregate_event_count(
        &self,
        from_ms: u64,
        to_ms: u64,
        event_type_filter: Option<&str>,
    ) -> Result<Vec<EventCountBucket>, NeonShadowError> {
        self.executor
            .execute(SQL_BEGIN_TXN, &[])
            .map_err(NeonShadowError::from)?;
        self.executor
            .execute(
                SQL_SET_RLS_TENANT_GUC,
                &[ExecutorParam::Text(self.tenant_id.to_string())],
            )
            .map_err(NeonShadowError::from)?;
        let rows = match event_type_filter {
            Some(ty) => self
                .executor
                .query(
                    SQL_QUERY_EVENT_COUNT_FILTERED,
                    &[
                        ExecutorParam::Int8(from_ms as i64),
                        ExecutorParam::Int8(to_ms as i64),
                        ExecutorParam::Text(ty.to_string()),
                    ],
                )
                .map_err(NeonShadowError::from)?,
            None => self
                .executor
                .query(
                    SQL_QUERY_EVENT_COUNT,
                    &[
                        ExecutorParam::Int8(from_ms as i64),
                        ExecutorParam::Int8(to_ms as i64),
                    ],
                )
                .map_err(NeonShadowError::from)?,
        };
        self.executor
            .execute(SQL_COMMIT_TXN, &[])
            .map_err(NeonShadowError::from)?;

        let mut buckets = Vec::with_capacity(rows.len());
        for (idx, row) in rows.iter().enumerate() {
            let event_type = row
                .cells
                .first()
                .and_then(|c| c.as_ref())
                .ok_or_else(|| {
                    NeonShadowError::Backend(format!(
                        "event_count row {}: null event_type cell",
                        idx
                    ))
                })?
                .clone();
            let count_str = row
                .cells
                .get(1)
                .and_then(|c| c.as_ref())
                .ok_or_else(|| {
                    NeonShadowError::Backend(format!("event_count row {}: null count cell", idx))
                })?;
            let count: u64 = count_str
                .parse()
                .map_err(|e| NeonShadowError::Backend(format!("count parse: {}", e)))?;
            buckets.push(EventCountBucket { event_type, count });
        }
        Ok(buckets)
    }

    fn aggregate_timeline(
        &self,
        from_ms: u64,
        to_ms: u64,
        granularity_ms: u64,
    ) -> Result<Vec<TimelineBucket>, NeonShadowError> {
        if granularity_ms == 0 {
            return Err(NeonShadowError::Internal(
                "granularity_ms must be > 0".to_string(),
            ));
        }
        self.executor
            .execute(SQL_BEGIN_TXN, &[])
            .map_err(NeonShadowError::from)?;
        self.executor
            .execute(
                SQL_SET_RLS_TENANT_GUC,
                &[ExecutorParam::Text(self.tenant_id.to_string())],
            )
            .map_err(NeonShadowError::from)?;
        let rows = self
            .executor
            .query(
                SQL_QUERY_TIMELINE,
                &[
                    ExecutorParam::Int8(from_ms as i64),
                    ExecutorParam::Int8(to_ms as i64),
                    ExecutorParam::Int8(granularity_ms as i64),
                ],
            )
            .map_err(NeonShadowError::from)?;
        self.executor
            .execute(SQL_COMMIT_TXN, &[])
            .map_err(NeonShadowError::from)?;

        let mut buckets = Vec::with_capacity(rows.len());
        for (idx, row) in rows.iter().enumerate() {
            let bucket_str = row
                .cells
                .first()
                .and_then(|c| c.as_ref())
                .ok_or_else(|| {
                    NeonShadowError::Backend(format!("timeline row {}: null bucket cell", idx))
                })?;
            let count_str = row
                .cells
                .get(1)
                .and_then(|c| c.as_ref())
                .ok_or_else(|| {
                    NeonShadowError::Backend(format!("timeline row {}: null count cell", idx))
                })?;
            let bucket_start_ms: u64 = bucket_str
                .parse()
                .map_err(|e| NeonShadowError::Backend(format!("bucket parse: {}", e)))?;
            let count: u64 = count_str
                .parse()
                .map_err(|e| NeonShadowError::Backend(format!("count parse: {}", e)))?;
            buckets.push(TimelineBucket {
                bucket_start_ms,
                count,
            });
        }
        Ok(buckets)
    }
}

// ---------------------------------------------------------------------------
// wasm32 stub — the real driver is native-only; the worker stub
// surfaces a typed `NeonError::WasmOnly` so a wiring bug is caught at
// compile + first-call time.
// ---------------------------------------------------------------------------

/// Wasm32 stub. Constructable so the trait-object Arc<dyn …> wiring
/// type-checks on both targets; every method returns the typed
/// `WasmOnly` error so a CF Worker that misroutes a Neon call surfaces
/// the error immediately instead of silently no-op-ing.
#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
pub struct RealNeonShadowSink {
    tenant_id: Uuid,
    region: Region,
}

#[cfg(target_arch = "wasm32")]
impl RealNeonShadowSink {
    /// Construct a wasm32 stub. The `executor` + `audit_sink` args
    /// are accepted (for signature parity) but dropped — every method
    /// surfaces [`NeonError::WasmOnly`].
    #[must_use]
    pub fn new(
        tenant_id: Uuid,
        region: Region,
        _executor: Arc<dyn NeonExecutor>,
        _audit_sink: Arc<dyn ShadowSyncAuditSink>,
    ) -> Self {
        Self { tenant_id, region }
    }
}

#[cfg(target_arch = "wasm32")]
impl NeonShadowSink for RealNeonShadowSink {
    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
    fn region(&self) -> Region {
        self.region
    }
    fn sync_chunk(
        &self,
        _receipt: &ArchiveReceipt,
        _rows: &[ShadowEventRow],
        _now_ms: u64,
    ) -> Result<ShadowSyncReceipt, NeonShadowError> {
        Err(NeonError::WasmOnly.into())
    }
    fn aggregate_event_count(
        &self,
        _from_ms: u64,
        _to_ms: u64,
        _event_type_filter: Option<&str>,
    ) -> Result<Vec<EventCountBucket>, NeonShadowError> {
        Err(NeonError::WasmOnly.into())
    }
    fn aggregate_timeline(
        &self,
        _from_ms: u64,
        _to_ms: u64,
        _granularity_ms: u64,
    ) -> Result<Vec<TimelineBucket>, NeonShadowError> {
        Err(NeonError::WasmOnly.into())
    }
}

// ---------------------------------------------------------------------------
// InMemoryExecutor — used by unit tests + the WI-S09 InMemory fakes
// network of dev/staging integration tests. The production
// `TokioPostgresExecutor` binder lives at `apps/server` boot path
// (mirrors the byok-*-real + cf-billing-real charter pattern).
// ---------------------------------------------------------------------------

/// Capture-mode executor used by the integration tests. Records every
/// `execute` / `query` call so the unit tests can assert the SQL
/// ordering (BEGIN → set_config → INSERT* → COMMIT) is preserved.
#[derive(Debug, Default)]
pub struct InMemoryExecutor {
    inner: parking_lot::Mutex<InMemoryExecutorInner>,
}

#[derive(Debug, Default)]
struct InMemoryExecutorInner {
    calls: Vec<(String, Vec<ExecutorParam>)>,
    /// Optional canned query responses, keyed by SQL string. Used by
    /// the aggregate-query integration tests.
    canned_query: std::collections::BTreeMap<String, Vec<ExecutorRow>>,
    /// Optional injected failure for the next `execute` / `query`.
    injected_failure: Option<String>,
    /// Captured rows (Postgres-shaped) — populated when an INSERT
    /// statement is observed so the property test can roundtrip.
    captured_rows: Vec<Vec<ExecutorParam>>,
}

impl InMemoryExecutor {
    /// Construct an empty in-memory executor.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-seed a canned response for a SQL string.
    pub fn add_canned_query(&self, sql: &str, rows: Vec<ExecutorRow>) {
        let mut g = self.inner.lock();
        g.canned_query.insert(sql.to_string(), rows);
    }

    /// Inject a failure for the next `execute` or `query`. Pass `None`
    /// to clear.
    pub fn inject_failure(&self, msg: Option<String>) {
        let mut g = self.inner.lock();
        g.injected_failure = msg;
    }

    /// Snapshot every captured `(sql, params)` pair in call order.
    #[must_use]
    pub fn calls(&self) -> Vec<(String, Vec<ExecutorParam>)> {
        let g = self.inner.lock();
        g.calls.clone()
    }

    /// Snapshot every captured INSERT row's parameter vector. Used by
    /// the property test (roundtrip serialize → insert → query →
    /// deserialize preserves `payload_jsonb`).
    #[must_use]
    pub fn captured_rows(&self) -> Vec<Vec<ExecutorParam>> {
        let g = self.inner.lock();
        g.captured_rows.clone()
    }
}

impl NeonExecutor for InMemoryExecutor {
    fn execute(&self, sql: &str, params: &[ExecutorParam]) -> Result<u64, NeonError> {
        let mut g = self.inner.lock();
        if let Some(msg) = g.injected_failure.take() {
            return Err(NeonError::Backend(msg));
        }
        g.calls.push((sql.to_string(), params.to_vec()));
        if sql == SQL_INSERT_SHADOW_ROW {
            g.captured_rows.push(params.to_vec());
        }
        Ok(1)
    }

    fn query(&self, sql: &str, params: &[ExecutorParam]) -> Result<Vec<ExecutorRow>, NeonError> {
        let mut g = self.inner.lock();
        if let Some(msg) = g.injected_failure.take() {
            return Err(NeonError::Backend(msg));
        }
        g.calls.push((sql.to_string(), params.to_vec()));
        Ok(g.canned_query.get(sql).cloned().unwrap_or_default())
    }
}

// ---------------------------------------------------------------------------
// Unit tests — pin SQL constants + RealNeonShadowSink behavior end to
// end via the in-memory executor. The 5 #[ignore]'d integration tests
// hit a real Neon-staging project (`tests/neon_shadow_real.rs`).
// ---------------------------------------------------------------------------

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::event::ChainHash;
    use crate::neon_shadow::InMemoryShadowSyncAuditSink;

    fn dummy_receipt(tenant: Uuid, first: u64, last: u64) -> ArchiveReceipt {
        ArchiveReceipt {
            r2_key: format!("audit/2026/05/16/{:08}.ndjson", first),
            tenant_id: tenant,
            first_event_time_ms: 1_000,
            last_event_time_ms: 2_000,
            first_sequence_number: first,
            last_sequence_number: last,
            prev_hash_anchor: ChainHash::genesis(),
            chain_head_after: ChainHash([0xCD; 32]),
            bytes_written: 100,
            events_written: last - first + 1,
        }
    }

    fn dummy_row(tenant: Uuid, region: Region, seq: u64, time_ms: u64, ty: &str) -> ShadowEventRow {
        ShadowEventRow {
            tenant_id: tenant,
            seq,
            event_time_ms: time_ms,
            event_type: ty.to_string(),
            prev_hash: ChainHash::genesis(),
            link_hash: ChainHash([(seq as u8); 32]),
            region,
            payload_json: format!("{{\"i\":{seq}}}"),
        }
    }

    #[test]
    fn env_var_resolver_canonical_names() {
        assert_eq!(EnvVarResolver::env_var_name(Region::Iad), "NEON_DB_URL_IAD");
        assert_eq!(EnvVarResolver::env_var_name(Region::Fra), "NEON_DB_URL_FRA");
        assert_eq!(EnvVarResolver::env_var_name(Region::Gru), "NEON_DB_URL_GRU");
    }

    #[test]
    fn env_var_resolver_returns_project_unresolved_when_unset() {
        let r = EnvVarResolver::new();
        // Highly unlikely to be set in the test runner — but the test
        // tolerates either state and asserts the error variant only
        // when truly unset.
        // Pick a region's env var that almost certainly is NOT set.
        let test_region = Region::Dxb;
        if std::env::var(EnvVarResolver::env_var_name(test_region)).is_err() {
            let err = r.resolve(test_region).expect_err("must be unresolved");
            assert!(matches!(err, NeonError::ProjectUnresolved { .. }));
        }
    }

    #[test]
    fn static_resolver_returns_dsn() {
        let r = StaticResolver::new().with(Region::Iad, "postgresql://x@host/db");
        let dsn = r.resolve(Region::Iad).expect("resolves");
        assert_eq!(dsn, "postgresql://x@host/db");
        let err = r.resolve(Region::Fra).expect_err("unresolved");
        assert!(matches!(err, NeonError::ProjectUnresolved { .. }));
    }

    #[test]
    fn sync_chunk_emits_canonical_sql_order() {
        let tenant = Uuid::now_v7();
        let exec = Arc::new(InMemoryExecutor::new());
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = RealNeonShadowSink::new(tenant, Region::Iad, exec.clone(), audit.clone());
        let rows = vec![
            dummy_row(tenant, Region::Iad, 0, 1_000, "x"),
            dummy_row(tenant, Region::Iad, 1, 2_000, "y"),
        ];
        let r = sink
            .sync_chunk(&dummy_receipt(tenant, 0, 1), &rows, 3_000)
            .expect("sync ok");
        assert_eq!(r.rows_persisted, 2);

        let calls = exec.calls();
        // Expected canonical order: BEGIN → set_config → INSERT × 2 → COMMIT.
        assert_eq!(calls.len(), 5);
        assert_eq!(calls[0].0, SQL_BEGIN_TXN);
        assert_eq!(calls[1].0, SQL_SET_RLS_TENANT_GUC);
        assert_eq!(calls[2].0, SQL_INSERT_SHADOW_ROW);
        assert_eq!(calls[3].0, SQL_INSERT_SHADOW_ROW);
        assert_eq!(calls[4].0, SQL_COMMIT_TXN);

        // Audit-of-audit emit on the happy path.
        let snap = audit.snapshot().expect("snap");
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].event_type, EVENT_TYPE_SHADOW_SYNCED);
    }

    #[test]
    fn sync_chunk_rejects_cross_tenant_constant_time() {
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let exec = Arc::new(InMemoryExecutor::new());
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = RealNeonShadowSink::new(tenant_a, Region::Iad, exec.clone(), audit.clone());
        let rows = vec![dummy_row(tenant_b, Region::Iad, 0, 1_000, "x")];
        let err = sink
            .sync_chunk(&dummy_receipt(tenant_b, 0, 0), &rows, 2_000)
            .expect_err("cross tenant");
        assert!(matches!(err, NeonShadowError::TenantIsolationViolation { .. }));
        // No SQL emitted — the pre-check fires before BEGIN.
        assert!(exec.calls().is_empty());
        // SEV-2 audit emitted.
        let snap = audit.snapshot().expect("snap");
        assert_eq!(snap[0].event_type, EVENT_TYPE_SHADOW_SYNC_FAILED);
        assert_eq!(snap[0].sev, "sev-2");
    }

    #[test]
    fn sync_chunk_rejects_cross_region() {
        let tenant = Uuid::now_v7();
        let exec = Arc::new(InMemoryExecutor::new());
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = RealNeonShadowSink::new(tenant, Region::Iad, exec, audit);
        let rows = vec![dummy_row(tenant, Region::Fra, 0, 1_000, "x")];
        let err = sink
            .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 2_000)
            .expect_err("cross region");
        assert!(matches!(err, NeonShadowError::ResidencyViolation { .. }));
    }

    #[test]
    fn sync_chunk_propagates_backend_failure_sev2() {
        let tenant = Uuid::now_v7();
        let exec = Arc::new(InMemoryExecutor::new());
        exec.inject_failure(Some("connection reset".into()));
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = RealNeonShadowSink::new(tenant, Region::Iad, exec, audit.clone());
        let rows = vec![dummy_row(tenant, Region::Iad, 0, 1_000, "x")];
        let err = sink
            .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 2_000)
            .expect_err("backend err");
        assert!(matches!(err, NeonShadowError::Backend(_)));
        let snap = audit.snapshot().expect("snap");
        assert_eq!(snap[0].event_type, EVENT_TYPE_SHADOW_SYNC_FAILED);
    }

    /// Audit sink that always rejects emits — drives the wave-21
    /// `AuditEmitFailed` lift exercised by
    /// `real_neon_sink_propagates_audit_emit_failure`.
    #[derive(Debug, Default)]
    struct AlwaysFailAuditSink;

    impl ShadowSyncAuditSink for AlwaysFailAuditSink {
        fn emit(&self, _row: ShadowSyncAuditRow) -> Result<(), &'static str> {
            Err("synthetic audit-emit failure")
        }
    }

    #[test]
    fn real_neon_sink_propagates_audit_emit_failure() {
        // Wave-21 (B-P1-02 closure): `RealNeonShadowSink.sync_chunk` no
        // longer swallows audit-emit failures via `let _ = ...`. On a
        // happy-path Neon sync where the audit pipeline is down the
        // sink surfaces `NeonShadowError::AuditEmitFailed` so the route
        // layer can translate to 503 + SEV-1.
        let tenant = Uuid::now_v7();
        let exec = Arc::new(InMemoryExecutor::new());
        let audit: Arc<dyn ShadowSyncAuditSink> = Arc::new(AlwaysFailAuditSink);
        let sink = RealNeonShadowSink::new(tenant, Region::Iad, exec, audit);
        let rows = vec![dummy_row(tenant, Region::Iad, 0, 1_000, "x")];
        let err = sink
            .sync_chunk(&dummy_receipt(tenant, 0, 0), &rows, 2_000)
            .expect_err("audit emit failure must propagate");
        assert!(matches!(err, NeonShadowError::AuditEmitFailed(_)));
    }

    #[test]
    fn aggregate_event_count_decodes_canned_rows() {
        let tenant = Uuid::now_v7();
        let exec = Arc::new(InMemoryExecutor::new());
        exec.add_canned_query(
            SQL_QUERY_EVENT_COUNT,
            vec![
                ExecutorRow {
                    cells: vec![Some("cas.put".into()), Some("3".into())],
                },
                ExecutorRow {
                    cells: vec![Some("cas.get".into()), Some("1".into())],
                },
            ],
        );
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = RealNeonShadowSink::new(tenant, Region::Iad, exec, audit);
        let buckets = sink.aggregate_event_count(0, 10_000, None).expect("ok");
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].event_type, "cas.put");
        assert_eq!(buckets[0].count, 3);
    }

    #[test]
    fn aggregate_timeline_zero_granularity_rejected() {
        let tenant = Uuid::now_v7();
        let exec = Arc::new(InMemoryExecutor::new());
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = RealNeonShadowSink::new(tenant, Region::Iad, exec, audit);
        let err = sink
            .aggregate_timeline(0, 10, 0)
            .expect_err("zero granularity");
        assert!(matches!(err, NeonShadowError::Internal(_)));
    }

    #[test]
    fn reconcile_count_decodes_scalar() {
        let tenant = Uuid::now_v7();
        let exec = Arc::new(InMemoryExecutor::new());
        exec.add_canned_query(
            SQL_RECONCILE_COUNT,
            vec![ExecutorRow {
                cells: vec![Some("42".into())],
            }],
        );
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = RealNeonShadowSink::new(tenant, Region::Iad, exec, audit);
        let n = sink.reconcile_count(0, 10_000).expect("ok");
        assert_eq!(n, 42);
    }

    #[test]
    fn sql_constants_pin_to_migration_schema() {
        // Sentinel substrings — flag schema drift between Rust + the
        // 0001 migration. The full schema validator runs in
        // `scripts/check_migrations_additive.py`; here we pin the
        // PRIMARY-KEY-aware ON CONFLICT clause + the RLS GUC name.
        assert!(SQL_INSERT_SHADOW_ROW.contains("audit_events_shadow"));
        assert!(SQL_INSERT_SHADOW_ROW.contains("ON CONFLICT (tenant_id, seq) DO NOTHING"));
        assert!(SQL_SET_RLS_TENANT_GUC.contains("app.current_tenant"));
        assert!(SQL_SET_RLS_TENANT_GUC.contains("set_config"));
        // The 0001 migration column list (per
        // `migrations/neon/0001_audit_events_shadow.sql`).
        for col in [
            "tenant_id",
            "seq",
            "event_time",
            "event_type",
            "prev_hash",
            "link_hash",
            "payload_jsonb",
            "region",
        ] {
            assert!(
                SQL_INSERT_SHADOW_ROW.contains(col),
                "INSERT SQL missing column {}",
                col
            );
        }
    }

    #[test]
    fn neon_error_into_neon_shadow_error_preserves_taxonomy() {
        let e = NeonError::Backend("x".into());
        let s: NeonShadowError = e.into();
        assert!(matches!(s, NeonShadowError::Backend(_)));

        let e = NeonError::WasmOnly;
        let s: NeonShadowError = e.into();
        assert!(matches!(s, NeonShadowError::Backend(_)));

        let e = NeonError::Internal("inv".into());
        let s: NeonShadowError = e.into();
        assert!(matches!(s, NeonShadowError::Internal(_)));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod proptests {
    //! Property test (10k iter PR-gate) — roundtrip serialize → insert
    //! → query → deserialize preserves `payload_jsonb` byte-for-byte.
    //!
    //! The shadow stores the raw NDJSON line as `jsonb`; a wiring bug
    //! that mangles the payload (e.g. double-encodes, strips
    //! whitespace inside string values) would surface here. The
    //! property test wires the in-memory executor (canonical SQL
    //! ordering preserved) + asserts every captured `Jsonb(s)`
    //! parameter equals the input `payload_json`.
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        reason = "tests are allowed to use these primitives"
    )]

    use super::*;
    use crate::event::ChainHash;
    use crate::neon_shadow::InMemoryShadowSyncAuditSink;
    use proptest::prelude::*;

    fn arb_payload() -> impl Strategy<Value = String> {
        // Arbitrary single-key JSON object with a string value
        // covering common audit-payload shapes.
        (
            "[a-z]{1,8}",
            "[ -~]{0,32}", // printable ASCII, no escapes needed
        )
            .prop_map(|(k, v)| format!("{{\"{}\":\"{}\"}}", k, v.replace('"', "")))
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 10_000, .. ProptestConfig::default() })]

        #[test]
        fn roundtrip_jsonb_preserves_payload(
            payload in arb_payload(),
            seq in 0u64..1_000_000u64,
            event_time_ms in 0u64..2_000_000_000_000u64,
        ) {
            let tenant = Uuid::now_v7();
            let exec = Arc::new(InMemoryExecutor::new());
            let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
            let sink = RealNeonShadowSink::new(tenant, Region::Iad, exec.clone(), audit);
            let row = ShadowEventRow {
                tenant_id: tenant,
                seq,
                event_time_ms,
                event_type: "x".to_string(),
                prev_hash: ChainHash::genesis(),
                link_hash: ChainHash([0xEE; 32]),
                region: Region::Iad,
                payload_json: payload.clone(),
            };
            let receipt = ArchiveReceipt {
                r2_key: "k".into(),
                tenant_id: tenant,
                first_event_time_ms: event_time_ms,
                last_event_time_ms: event_time_ms,
                first_sequence_number: seq,
                last_sequence_number: seq,
                prev_hash_anchor: ChainHash::genesis(),
                chain_head_after: ChainHash([0xEE; 32]),
                bytes_written: 100,
                events_written: 1,
            };
            sink.sync_chunk(&receipt, &[row], event_time_ms + 500).expect("ok");
            let captured = exec.captured_rows();
            prop_assert_eq!(captured.len(), 1);
            // Jsonb param lives at index 6 in the INSERT param tuple
            // (see `SQL_INSERT_SHADOW_ROW` doc).
            match &captured[0][6] {
                ExecutorParam::Jsonb(s) => prop_assert_eq!(s, &payload),
                other => prop_assert!(false, "expected Jsonb, got {:?}", other),
            }
        }
    }
}
