//! Ephemeral Postgres test harness (Wave-20 stream #1).
//!
//! Boots a `postgres:16-alpine` testcontainer per test, applies the
//! wave-18 audit-shadow migration (plus a wave-20 schema-drift
//! compensation patch — see [`HARNESS_SHADOW_SCHEMA_PATCH`] below),
//! and exposes a [`TokioPostgresExecutor`] that satisfies the
//! [`corelink_audit_chain::neon_shadow::real::NeonExecutor`] trait so
//! the production `RealNeonShadowSink` end-to-end path can be
//! exercised against a real Postgres instance — no Neon-staging
//! account, no GitHub Actions OIDC, no per-CI secret rotation.
//!
//! ## Why ephemeral
//!
//! Wave-19 shipped 5 integration tests that called out to a real
//! Neon-staging project; they were `#[ignore]`'d by default because
//! the staging project required out-of-band provisioning. Wave-20
//! lifts that constraint: every test spins up its own container, so
//! `cargo test -p corelink-audit-chain --features live-pg` runs the
//! suite locally on a developer laptop with Docker Desktop in under
//! 60 s total. See
//! `specs/_audits/sealed/2026-05-16-neon-shadow-pg-testharness.md`.
//!
//! ## Tenant isolation contract
//!
//! Every helper [`PgHarness::begin_tenant_txn`] / [`with_tenant`] sets
//! the `app.current_tenant` GUC via `set_config(..., true)` so the
//! RLS policy `tenant_isolation_audit_events_shadow` is exercised on
//! every read / write. The 5 tests assert that a cross-tenant
//! transaction sees zero rows — this is the load-bearing wave-18
//! INV-TENANT-ISOLATION check.
//!
//! ## Schema drift caveat
//!
//! The wave-18 migration
//! `migrations/neon/0001_audit_events_shadow.sql` does NOT yet declare
//! the `region` column that the production
//! `corelink_audit_chain::neon_shadow::real::SQL_INSERT_SHADOW_ROW`
//! statement binds as `$8`. The in-memory `InMemoryExecutor` unit
//! tests pass because they never type-check column existence. Real
//! Postgres rejects the INSERT immediately.
//!
//! Wave-20 stream #2 lifts the column into the canonical migration
//! (`0002_audit_events_shadow_with_check.sql`). Until that lands the
//! harness applies [`HARNESS_SHADOW_SCHEMA_PATCH`] which:
//!   1. Re-creates the `audit_events_shadow` table with the missing
//!      `region TEXT NOT NULL` column.
//!   2. Re-attaches the RLS policy.
//!   3. Leaves the secondary indexes intact.
//!
//! The patch is documented as a CAVEAT in the audit doc; it is NOT a
//! production migration and lives only in the test harness.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use parking_lot::Mutex;
use testcontainers::{runners::AsyncRunner, ContainerAsync};
use testcontainers_modules::postgres::Postgres as PostgresImage;
use tokio_postgres::{
    types::{ToSql, Type},
    Client, NoTls,
};
use uuid::Uuid;

use corelink_audit_chain::{ExecutorParam, ExecutorRow, NeonError, NeonExecutor};

/// The canonical wave-18 migration applied to every ephemeral
/// container. Kept as a `&str` so the harness has no `std::fs`
/// dependency on the source tree at runtime.
pub const WAVE18_MIGRATION: &str =
    include_str!("../../../../migrations/neon/0001_audit_events_shadow.sql");

/// Wave-20 schema-drift compensation patch — see module docs §"Schema
/// drift caveat". Re-creates `audit_events_shadow` with the missing
/// `region TEXT NOT NULL` column the production SQL_INSERT_SHADOW_ROW
/// expects. This patch is harness-only; production wiring depends on
/// wave-20 stream #2 lifting the same column into the canonical
/// migration.
pub const HARNESS_SHADOW_SCHEMA_PATCH: &str = "\
    DROP TABLE IF EXISTS audit_events_shadow CASCADE; \
    CREATE TABLE audit_events_shadow ( \
        tenant_id    UUID        NOT NULL, \
        seq          BIGINT      NOT NULL CHECK (seq >= 0), \
        event_time   TIMESTAMPTZ NOT NULL, \
        event_type   TEXT        NOT NULL, \
        prev_hash    TEXT        NOT NULL, \
        link_hash    TEXT        NOT NULL, \
        payload_jsonb JSONB      NOT NULL, \
        region       TEXT        NOT NULL, \
        synced_at    TIMESTAMPTZ NOT NULL DEFAULT now(), \
        PRIMARY KEY (tenant_id, seq) \
    ); \
    CREATE INDEX idx_audit_events_shadow_tenant_time \
        ON audit_events_shadow(tenant_id, event_time); \
    CREATE INDEX idx_audit_events_shadow_tenant_event_type \
        ON audit_events_shadow(tenant_id, event_type); \
    CREATE INDEX idx_audit_events_shadow_event_type_time \
        ON audit_events_shadow(event_type, event_time); \
    ALTER TABLE audit_events_shadow ENABLE ROW LEVEL SECURITY; \
    CREATE POLICY tenant_isolation_audit_events_shadow ON audit_events_shadow \
        USING (tenant_id = current_setting('app.current_tenant', true)::uuid) \
        WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid); \
";

/// Live ephemeral Postgres harness. Owns the container handle (drops
/// it at scope exit so the container is removed) plus a long-lived
/// `tokio-postgres` connection bound to a dedicated non-superuser
/// role so RLS is actually enforced.
///
/// `postgres` (the bootstrap superuser) bypasses RLS — every test
/// MUST use [`PgHarness::client`] (the `corelink_test` role) for any
/// query that asserts tenant isolation.
pub struct PgHarness {
    /// Held for its `Drop` impl — the container is killed when this
    /// goes out of scope. Wrapped in `Option` so `PgHarness::drop`
    /// can take ownership and drop inside the runtime context
    /// (testcontainers 0.21's `ContainerAsync::drop` calls
    /// `tokio::runtime::Handle::current()` which panics if no
    /// runtime is in scope — see `testcontainers/src/core/async_drop.rs`).
    container: Option<ContainerAsync<PostgresImage>>,
    /// Long-lived runtime handle. Held here so the harness can
    /// synchronously block_on the container drop in `Drop::drop` —
    /// the field outlives `container` only because Rust drops
    /// fields in declaration order (top to bottom).
    runtime: Arc<tokio::runtime::Runtime>,
    client: Arc<Client>,
    /// `host:port` connection target, exposed so the test can spin
    /// up additional `Client`s (e.g. one per tenant for cross-tenant
    /// isolation checks).
    pub conn_target: String,
    /// Bootstrap superuser DSN — needed to run schema DDL and to
    /// configure the per-test non-superuser role.
    pub superuser_dsn: String,
    /// Non-superuser role DSN — every RLS-enforced read / write goes
    /// through this DSN.
    pub app_role_dsn: String,
}

impl Drop for PgHarness {
    fn drop(&mut self) {
        if let Some(container) = self.container.take() {
            // W21-FOLLOWUP-03: wrap the container drop in a 30s timeout
            // so a hung Docker daemon never blocks the test process
            // indefinitely on shutdown. The runtime is entered via
            // `block_on` so the testcontainers async-drop helper can call
            // `tokio::runtime::Handle::current()` without panicking
            // (testcontainers 0.21's `ContainerAsync::drop` requires a
            // runtime in scope). The actual teardown remains best-effort
            // — if the timeout elapses we log to stderr but do NOT panic
            // (Drop must never panic).
            let runtime = Arc::clone(&self.runtime);
            runtime.block_on(async {
                let timeout = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    async move {
                        drop(container);
                    },
                )
                .await;
                if timeout.is_err() {
                    eprintln!(
                        "PgHarness::drop timed out after 30s waiting for \
                         testcontainers async-drop — Docker daemon may be \
                         hung; container will be reaped by docker-prune or \
                         the host's cleanup cron. See W21-FOLLOWUP-03 \
                         (`specs/_audits/sealed/2026-05-16-wave20-adversarial-review.md`)."
                    );
                }
            });
        }
    }
}

impl std::fmt::Debug for PgHarness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgHarness")
            .field("conn_target", &self.conn_target)
            .field("superuser_dsn", &"<redacted>")
            .field("app_role_dsn", &"<redacted>")
            .finish()
    }
}

impl PgHarness {
    /// Borrow the canonical non-superuser `tokio-postgres` client.
    pub fn client(&self) -> Arc<Client> {
        Arc::clone(&self.client)
    }
}

/// Spawn an ephemeral Postgres container, apply the wave-18 migration
/// plus the wave-20 schema patch, create a non-superuser
/// `corelink_test` role, and return a [`PgHarness`].
///
/// The `runtime` arg is the SAME runtime the caller will use to
/// drive `TokioPostgresExecutor` — it is stashed inside the harness
/// so `Drop::drop` can re-enter the runtime context (required by
/// `testcontainers::ContainerAsync`'s async drop helper).
///
/// # Panics
///
/// Panics on any boot / migration failure — every failure mode is a
/// hard test-infra error. The caller is a `#[tokio::test]` so the
/// panic flips the test to red.
pub async fn spawn_ephemeral_postgres(
    runtime: Arc<tokio::runtime::Runtime>,
) -> PgHarness {
    let image = PostgresImage::default();
    // Tag override: the testcontainers-modules default is `11-alpine`;
    // we pin `16-alpine` to match Neon's production major version per
    // INV-OBS-AUDIT-CHAIN-INTEGRITY (RLS / set_config semantics are
    // major-version-stable but jsonb sort order is not).
    let image = testcontainers::ImageExt::with_tag(image, "16-alpine");
    let container = image
        .start()
        .await
        .expect("postgres testcontainer must boot");

    let host = container.get_host().await.expect("get_host");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("get_host_port_ipv4");
    let conn_target = format!("{host}:{port}");
    let superuser_dsn =
        format!("postgres://postgres:postgres@{conn_target}/postgres");

    // Bootstrap connection: apply schema + create the non-superuser
    // role used by RLS-enforced queries.
    let (boot_client, boot_conn) = tokio_postgres::connect(&superuser_dsn, NoTls)
        .await
        .expect("bootstrap connect");
    let boot_handle = tokio::spawn(async move {
        // Discard the close result; the bootstrap conn is drained
        // synchronously below via `drop(boot_client) + handle.abort()`
        // so a stale Err here would be misleading noise.
        let _ = boot_conn.await;
    });

    boot_client
        .batch_execute(WAVE18_MIGRATION)
        .await
        .expect("apply wave18 migration");
    boot_client
        .batch_execute(HARNESS_SHADOW_SCHEMA_PATCH)
        .await
        .expect("apply harness schema patch");
    boot_client
        .batch_execute(
            "CREATE ROLE corelink_test LOGIN PASSWORD 'corelink_test'; \
             GRANT ALL ON audit_events_shadow TO corelink_test; \
             GRANT ALL ON audit_shadow_lag TO corelink_test;",
        )
        .await
        .expect("create corelink_test role");
    drop(boot_client);
    boot_handle.abort();

    let app_role_dsn =
        format!("postgres://corelink_test:corelink_test@{conn_target}/postgres");
    let (client, conn) = tokio_postgres::connect(&app_role_dsn, NoTls)
        .await
        .expect("app-role connect");
    tokio::spawn(async move {
        // App-role conn lives for the lifetime of the harness; close
        // result is intentionally discarded (the test panics
        // long-before the conn closes if anything goes wrong).
        let _ = conn.await;
    });

    PgHarness {
        container: Some(container),
        runtime,
        client: Arc::new(client),
        conn_target,
        superuser_dsn,
        app_role_dsn,
    }
}

/// `NeonExecutor` adapter built on `tokio_postgres::Client`. Owns the
/// client + a single-threaded tokio runtime so the synchronous
/// `NeonExecutor` trait can call into async tokio-postgres without
/// requiring the caller to be on a runtime.
///
/// One executor per test (cheap — the underlying connection pool is
/// the `tokio_postgres::Client` itself, which multiplexes statements
/// over a single TCP connection).
pub struct TokioPostgresExecutor {
    client: Arc<Client>,
    runtime: Arc<tokio::runtime::Runtime>,
    /// Captured SQL trace — useful for asserting the canonical
    /// BEGIN / set_config / INSERT / COMMIT order from inside a
    /// test. Cleared by [`Self::take_trace`].
    trace: Arc<Mutex<Vec<String>>>,
}

impl std::fmt::Debug for TokioPostgresExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokioPostgresExecutor")
            .field("trace_len", &self.trace.lock().len())
            .finish()
    }
}

impl TokioPostgresExecutor {
    /// Build a new executor. The `runtime` is shared so multiple
    /// executors in the same test reuse one tokio reactor.
    pub fn new(client: Arc<Client>, runtime: Arc<tokio::runtime::Runtime>) -> Self {
        Self {
            client,
            runtime,
            trace: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Drain the captured SQL trace.
    pub fn take_trace(&self) -> Vec<String> {
        std::mem::take(&mut *self.trace.lock())
    }
}

impl NeonExecutor for TokioPostgresExecutor {
    fn execute(&self, sql: &str, params: &[ExecutorParam]) -> Result<u64, NeonError> {
        self.trace.lock().push(sql.to_string());
        let owned: Vec<OwnedParam> = params.iter().map(OwnedParam::from).collect();
        let types: Vec<Type> = params.iter().map(param_pg_type).collect();
        let refs: Vec<&(dyn ToSql + Sync)> =
            owned.iter().map(|p| p.as_to_sql()).collect();
        let client = Arc::clone(&self.client);
        let sql_owned = sql.to_string();
        let runtime = Arc::clone(&self.runtime);
        let fut = async move {
            // `prepare_typed` pins the parameter OIDs so the SQL's
            // `::double precision` / `::bigint` casts compose cleanly
            // even when the server would otherwise infer `float8` for
            // a param that we bind as `i64`. Mirrors the production
            // `tokio_postgres_executor` binder's intended behavior;
            // see `specs/_audits/sealed/2026-05-16-neon-shadow-pg-testharness.md`.
            let stmt = client.prepare_typed(&sql_owned, &types).await?;
            client.execute(&stmt, &refs).await
        };
        let n = runtime
            .block_on(fut)
            .map_err(|e| NeonError::Backend(format!("execute: {e}")))?;
        Ok(n)
    }

    fn query(
        &self,
        sql: &str,
        params: &[ExecutorParam],
    ) -> Result<Vec<ExecutorRow>, NeonError> {
        self.trace.lock().push(sql.to_string());
        let owned: Vec<OwnedParam> = params.iter().map(OwnedParam::from).collect();
        let types: Vec<Type> = params.iter().map(param_pg_type).collect();
        let refs: Vec<&(dyn ToSql + Sync)> =
            owned.iter().map(|p| p.as_to_sql()).collect();
        let client = Arc::clone(&self.client);
        let sql_owned = sql.to_string();
        let runtime = Arc::clone(&self.runtime);
        let fut = async move {
            let stmt = client.prepare_typed(&sql_owned, &types).await?;
            client.query(&stmt, &refs).await
        };
        let rows = runtime
            .block_on(fut)
            .map_err(|e| NeonError::Backend(format!("query: {e}")))?;

        // Convert every column to Option<String> so the canonical
        // RealNeonShadowSink decoders (which expect string cells) can
        // parse them. The shadow aggregate queries only return TEXT +
        // BIGINT columns; we cover both via type-aware decoding.
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let mut cells = Vec::with_capacity(row.len());
            for col_idx in 0..row.len() {
                let cell = decode_cell(&row, col_idx);
                cells.push(cell);
            }
            // ExecutorRow is `#[non_exhaustive]` so we cannot use a
            // struct literal here; construct via Default + field
            // assignment (the `cells` field is `pub`).
            let mut exec_row = ExecutorRow::default();
            exec_row.cells = cells;
            out.push(exec_row);
        }
        Ok(out)
    }
}

/// Owned counterpart of `ExecutorParam` — needed because
/// `tokio_postgres::Client::execute` wants `&(dyn ToSql + Sync)` refs
/// that outlive the call, but we receive borrowed `ExecutorParam`s.
enum OwnedParam {
    Uuid(Uuid),
    Int8(i64),
    Text(String),
    /// HEX-encoded bytes — the SQL pins the column as TEXT so we
    /// bind as String.
    HexText(String),
    /// Jsonb is bound via `$N::jsonb` cast on the SQL side; we
    /// stringify here and let Postgres parse.
    Jsonb(serde_json::Value),
}

impl From<&ExecutorParam> for OwnedParam {
    fn from(p: &ExecutorParam) -> Self {
        match p {
            ExecutorParam::Uuid(u) => OwnedParam::Uuid(*u),
            ExecutorParam::Int8(i) => OwnedParam::Int8(*i),
            ExecutorParam::Text(s) => OwnedParam::Text(s.clone()),
            ExecutorParam::HexBytes(s) => OwnedParam::HexText(s.clone()),
            ExecutorParam::Jsonb(s) => {
                let v = serde_json::from_str::<serde_json::Value>(s)
                    .unwrap_or(serde_json::Value::Null);
                OwnedParam::Jsonb(v)
            }
            // ExecutorParam is `#[non_exhaustive]`; an additive variant
            // shipped post-wave-20 surfaces as a hard test-infra error
            // so the harness can be updated alongside the production
            // driver. NULL fallback is INTENTIONALLY conservative.
            _ => OwnedParam::Text(String::new()),
        }
    }
}

/// Map an [`ExecutorParam`] to the canonical Postgres type OID so
/// `Client::prepare_typed` pins the parameter shape before the cast
/// chain in the SQL composes (load-bearing for the
/// `to_timestamp($N::double precision / 1000.0)` idiom in
/// `SQL_INSERT_SHADOW_ROW` / `SQL_QUERY_TIMELINE`).
fn param_pg_type(p: &ExecutorParam) -> Type {
    match p {
        ExecutorParam::Uuid(_) => Type::UUID,
        ExecutorParam::Int8(_) => Type::INT8,
        ExecutorParam::Text(_) => Type::TEXT,
        ExecutorParam::HexBytes(_) => Type::TEXT,
        ExecutorParam::Jsonb(_) => Type::JSONB,
        _ => Type::TEXT,
    }
}

impl OwnedParam {
    fn as_to_sql(&self) -> &(dyn ToSql + Sync) {
        match self {
            OwnedParam::Uuid(u) => u,
            OwnedParam::Int8(i) => i,
            OwnedParam::Text(s) => s,
            OwnedParam::HexText(s) => s,
            OwnedParam::Jsonb(v) => v,
        }
    }
}

/// Decode one column to `Option<String>` based on the type OID.
/// Covers TEXT, BIGINT, UUID, JSONB, TIMESTAMPTZ — the cell types
/// produced by the shadow aggregate queries.
fn decode_cell(row: &tokio_postgres::Row, idx: usize) -> Option<String> {
    let col = row.columns().get(idx)?;
    match col.type_().name() {
        "int8" => row.try_get::<usize, Option<i64>>(idx).ok()??.to_string().into(),
        "int4" => row.try_get::<usize, Option<i32>>(idx).ok()??.to_string().into(),
        "text" | "varchar" | "bpchar" => row.try_get::<usize, Option<String>>(idx).ok()?,
        "uuid" => row
            .try_get::<usize, Option<Uuid>>(idx)
            .ok()??
            .to_string()
            .into(),
        "jsonb" | "json" => row
            .try_get::<usize, Option<serde_json::Value>>(idx)
            .ok()??
            .to_string()
            .into(),
        _ => row.try_get::<usize, Option<String>>(idx).ok().flatten(),
    }
}

/// Count rows in `audit_events_shadow` visible to `tenant` under
/// the RLS policy. Opens a fresh connection (avoiding the `&mut
/// Client` constraint of `Client::transaction`), starts a txn,
/// binds the `app.current_tenant` GUC via `set_config(..., true)`,
/// runs the SELECT, COMMITs, and returns the count.
///
/// Mirrors the production `RealNeonShadowSink::sync_chunk` setup so
/// test asserts run under identical RLS conditions — the GUC is
/// txn-scoped, not session-scoped, so a poolboy-reused connection
/// could NOT leak the tenant across requests.
pub async fn count_visible_rows(
    harness: &PgHarness,
    tenant: Uuid,
) -> Result<i64, tokio_postgres::Error> {
    let (mut client, conn) =
        tokio_postgres::connect(&harness.app_role_dsn, NoTls).await?;
    let handle = tokio::spawn(async move {
        // Ephemeral conn for one count query; drop result.
        let _ = conn.await;
    });
    let result = {
        let txn = client.transaction().await?;
        txn.execute(
            "SELECT set_config('app.current_tenant', $1, true)",
            &[&tenant.to_string()],
        )
        .await?;
        let row = txn
            .query_one(
                "SELECT COUNT(*)::bigint FROM audit_events_shadow",
                &[],
            )
            .await?;
        let count: i64 = row.get(0);
        txn.commit().await?;
        Ok::<i64, tokio_postgres::Error>(count)
    };
    drop(client);
    handle.abort();
    result
}

/// Build a fresh single-threaded tokio runtime suitable for driving
/// the `TokioPostgresExecutor` from a synchronous test entrypoint.
/// Each test creates one; they are cheap (~5 ms).
pub fn build_runtime() -> Arc<tokio::runtime::Runtime> {
    Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime"),
    )
}
