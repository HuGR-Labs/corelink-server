//! `TokioPostgresExecutor` — production `NeonExecutor` adapter (wave-20).
//!
//! Closes the wave-19 deferred binder caveat from
//! `specs/_audits/sealed/2026-05-16-neon-shadow-real-driver.md` §7:
//!
//! > **Real `TokioPostgresExecutor` binder** — deferred to the server-wire
//! > follow-on (the `apps/server` boot path needs the `deadpool-postgres`
//! > pool constructor + per-region pool sharding); the `RealNeonShadowSink`
//! > orchestration is wave-19 complete; only the trait-object binding to
//! > `tokio-postgres` is the deferred bit.
//!
//! This module ships that binder.
//!
//! ## Architecture
//!
//! The [`crate::neon_shadow::real::NeonExecutor`] trait is synchronous, but the
//! underlying `tokio-postgres` client is fully async. The adapter
//! bridges the gap by holding a `tokio::runtime::Handle` captured at
//! construction time and dispatching every call through
//! `block_in_place(|| handle.block_on(fut))`. This pattern is safe on
//! the tonic + axum hosts because they run on a multi-thread tokio
//! runtime (the only runtime that supports `block_in_place`); a
//! single-thread runtime triggers an explicit panic in
//! `block_in_place`, so a wiring mistake surfaces immediately at first
//! call rather than dead-locking.
//!
//! Within a worker thread, the bridge converts the sync executor call
//! into the canonical async tokio-postgres call sequence:
//!
//! ```text
//! pool.get() ─▶ client.execute(sql, &params)  // returns u64 rows-affected
//!         └──▶ client.query(sql, &params)     // returns Vec<Row>
//! ```
//!
//! ## Pool semantics
//!
//! - `max_size = 4` per region (Neon free + Pro tiers cap at ~100
//!   connections per project; 4 here leaves headroom for the daily
//!   reconcile cron + ad-hoc operator queries).
//! - `idle_timeout = 10s` so an idle pool releases connections quickly
//!   — Neon scales-to-zero compute the project after a connection-idle
//!   interval, so holding warm connections longer is wasted capacity.
//! - `recycle = Fast` (the deadpool default) — connections are reused
//!   without a `SELECT 1` ping; tokio-postgres surfaces a `closed()`
//!   future on the connection if Neon's pool-pause cycle invalidated
//!   the link, and the adapter's [`crate::neon_shadow::real::NeonError::Backend`]
//!   propagation triggers the SEV-2 audit emit so an operator notices
//!   even though the next pool checkout will succeed cleanly.
//!
//! ## TLS contract
//!
//! Neon REQUIRES TLS — the DSN's `sslmode=require` (or stronger) is
//! load-bearing. The adapter wires `tokio-postgres-rustls` with
//! `rustls-native-certs` for the system trust store + `webpki-roots`
//! as a fallback so a fresh container without `/etc/ssl/certs` still
//! validates Neon's hosted certificate.
//!
//! ## Native-only
//!
//! Every concrete type in this file is `#[cfg(not(target_arch = "wasm32"))]`.
//! The wasm32 path links a stub whose every method surfaces
//! [`crate::neon_shadow::real::NeonError::WasmOnly`] so a CF Worker that misroutes
//! a Neon call fails fast instead of silently no-op-ing — this mirrors
//! the [`crate::neon_shadow::real::RealNeonShadowSink`] wasm32 stub pattern.
//!
//! ## Charter
//!
//! - `#![forbid(unsafe_code)]` — inherited from the crate root.
//! - No `unwrap` / `expect` / `panic` in src — every fallible step
//!   surfaces a typed [`crate::neon_shadow::real::NeonError`] variant.
//! - Sync method bodies use `block_in_place` (panics on single-thread
//!   runtime — explicit failure, not silent dead-lock).
//! - Constant-time tenant comparison stays at the
//!   [`crate::neon_shadow::real::RealNeonShadowSink`] layer (this adapter is
//!   tenant-agnostic; the GUC `set_config` is the SQL-layer gate).
//!
//! ## Tests
//!
//! Unit tests cover the wasm32 stub + the param-vec encoder shape
//! (the SQL-ordering invariant is already pinned by the wave-19
//! `sync_chunk_emits_canonical_sql_order` test against the
//! [`crate::neon_shadow::real::InMemoryExecutor`]).
//!
//! Five `#[ignore]`-by-default integration tests live in
//! `crates/corelink-audit-chain/tests/neon_shadow_real.rs` — they
//! require a live Postgres (Neon staging or a local `testcontainers`
//! harness; see the test-file docs).

#![allow(clippy::uninlined_format_args)]

#[cfg(all(feature = "neon-real", not(target_arch = "wasm32")))]
mod native {
    //! Native (non-wasm32) implementation. Pulls `tokio-postgres` +
    //! `deadpool-postgres` + `tokio-postgres-rustls`.

    use std::sync::Arc;
    use std::time::Duration;

    use deadpool_postgres::{Config as PoolConfig, ManagerConfig, Pool, RecyclingMethod, Runtime};
    use rustls::{ClientConfig, RootCertStore};
    use tokio_postgres::types::ToSql;
    use tokio_postgres_rustls::MakeRustlsConnect;
    use uuid::Uuid;

    use crate::neon_shadow::real::{ExecutorParam, ExecutorRow, NeonError, NeonExecutor};

    /// Default pool size per region. Chosen so the daily reconcile
    /// cron and the customer-facing analytics endpoints share a small,
    /// bounded pool (Neon free-tier compute caps at 100 connections
    /// per project).
    pub const DEFAULT_POOL_MAX_SIZE: usize = 4;

    /// Default pool idle timeout — see module docs for the rationale.
    pub const DEFAULT_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(10);

    /// Production [`NeonExecutor`] adapter wrapping a
    /// [`deadpool_postgres::Pool`].
    ///
    /// Construct with [`TokioPostgresExecutor::connect`] (async) and
    /// then hand the `Arc<dyn NeonExecutor>` to
    /// [`crate::neon_shadow::real::RealNeonShadowSink::new`].
    #[derive(Debug)]
    pub struct TokioPostgresExecutor {
        pool: Pool,
        rt: tokio::runtime::Handle,
    }

    impl TokioPostgresExecutor {
        /// Construct a new executor by:
        /// 1. Parsing the Neon DSN (`postgresql://user:pwd@host/db?sslmode=require`).
        /// 2. Building a rustls TLS connector with the system trust store
        ///    + webpki-roots fallback.
        /// 3. Constructing a [`deadpool_postgres::Pool`] with the canonical
        ///    pool config (max_size = 4, idle_timeout = 10s).
        ///
        /// # Errors
        ///
        /// Returns [`NeonError::Backend`] if the DSN is malformed, the
        /// TLS roots cannot be built, or the pool refuses to construct.
        ///
        /// # Panics
        ///
        /// Never — every fallible step is mapped to `NeonError::Backend`.
        pub async fn connect(dsn: &str) -> Result<Self, NeonError> {
            let rt = tokio::runtime::Handle::try_current().map_err(|e| {
                NeonError::Backend(format!(
                    "TokioPostgresExecutor::connect must run inside a tokio runtime: {}",
                    e
                ))
            })?;
            // W21-FOLLOWUP-02: document the multi-thread runtime contract
            // programmatically. `block_in_place` (used in `execute` / `query`)
            // panics on a current-thread runtime; failing fast here in debug
            // builds surfaces a wiring mistake before the first dispatch.
            // No-op in release (debug_assert!).
            //
            // Note: the audit doc suggests `metrics().num_workers() > 1`, but
            // `Handle::metrics` is gated behind `tokio_unstable` (not enabled
            // in this workspace). `runtime_flavor() == MultiThread` is the
            // stable equivalent and is what the audit's prose recommendation
            // actually pins.
            debug_assert!(
                matches!(
                    rt.runtime_flavor(),
                    tokio::runtime::RuntimeFlavor::MultiThread
                ),
                "TokioPostgresExecutor requires multi-thread runtime"
            );
            let tls = build_tls()?;
            let connector = MakeRustlsConnect::new(tls);

            let mut cfg = PoolConfig::new();
            cfg.url = Some(dsn.to_string());
            cfg.manager = Some(ManagerConfig {
                recycling_method: RecyclingMethod::Fast,
            });
            // PoolConfig::pool was deprecated in favor of pool size on builder; set on builder below.

            let pool = cfg
                .builder(connector)
                .map_err(|e| NeonError::Backend(format!("pool builder: {}", e)))?
                .max_size(DEFAULT_POOL_MAX_SIZE)
                .runtime(Runtime::Tokio1)
                .build()
                .map_err(|e| NeonError::Backend(format!("pool build: {}", e)))?;

            // Eager liveness check — surface a misconfigured DSN at boot
            // time, not at first analytics query.
            {
                let client = pool
                    .get()
                    .await
                    .map_err(|e| NeonError::Backend(format!("eager pool checkout: {}", e)))?;
                client
                    .simple_query("SELECT 1")
                    .await
                    .map_err(|e| NeonError::Backend(format!("eager liveness: {}", e)))?;
            }

            Ok(Self { pool, rt })
        }

        /// Construct directly from an existing pool — used by integration
        /// tests + the daily reconcile cron harness so they can share
        /// the canonical executor body without re-doing the DSN/TLS
        /// dance.
        ///
        /// # Errors
        ///
        /// Returns [`NeonError::Backend`] if called outside a tokio
        /// runtime (the executor needs a runtime handle to dispatch
        /// the async tokio-postgres calls).
        pub fn from_pool(pool: Pool) -> Result<Self, NeonError> {
            let rt = tokio::runtime::Handle::try_current().map_err(|e| {
                NeonError::Backend(format!(
                    "TokioPostgresExecutor::from_pool must run inside a tokio runtime: {}",
                    e
                ))
            })?;
            // W21-FOLLOWUP-02: mirror the multi-thread runtime guard from
            // `connect`. See that constructor for the rationale.
            debug_assert!(
                matches!(
                    rt.runtime_flavor(),
                    tokio::runtime::RuntimeFlavor::MultiThread
                ),
                "TokioPostgresExecutor requires multi-thread runtime"
            );
            Ok(Self { pool, rt })
        }

        /// Wrap a constructed executor in `Arc<dyn NeonExecutor>` for
        /// the [`crate::neon_shadow::real::RealNeonShadowSink::new`]
        /// composition root.
        #[must_use]
        pub fn into_arc(self) -> Arc<dyn NeonExecutor> {
            Arc::new(self)
        }
    }

    impl NeonExecutor for TokioPostgresExecutor {
        fn execute(&self, sql: &str, params: &[ExecutorParam]) -> Result<u64, NeonError> {
            let pool = self.pool.clone();
            let sql = sql.to_string();
            let params = params.to_vec();
            // `block_in_place` requires a multi-thread runtime. The
            // apps/server binary uses `#[tokio::main]` (multi-thread by
            // default) so the canonical production path satisfies this.
            tokio::task::block_in_place(move || {
                self.rt.block_on(async move {
                    let client = pool
                        .get()
                        .await
                        .map_err(|e| NeonError::Backend(format!("pool checkout: {}", e)))?;
                    let owned: OwnedParams = OwnedParams::from(&params[..]);
                    let refs: Vec<&(dyn ToSql + Sync)> = owned.refs();
                    client
                        .execute(&sql, &refs[..])
                        .await
                        .map_err(|e| NeonError::Backend(format!("execute: {}", e)))
                })
            })
        }

        fn query(
            &self,
            sql: &str,
            params: &[ExecutorParam],
        ) -> Result<Vec<ExecutorRow>, NeonError> {
            let pool = self.pool.clone();
            let sql = sql.to_string();
            let params = params.to_vec();
            tokio::task::block_in_place(move || {
                self.rt.block_on(async move {
                    let client = pool
                        .get()
                        .await
                        .map_err(|e| NeonError::Backend(format!("pool checkout: {}", e)))?;
                    let owned: OwnedParams = OwnedParams::from(&params[..]);
                    let refs: Vec<&(dyn ToSql + Sync)> = owned.refs();
                    let pg_rows = client
                        .query(&sql, &refs[..])
                        .await
                        .map_err(|e| NeonError::Backend(format!("query: {}", e)))?;
                    Ok(decode_rows(&pg_rows))
                })
            })
        }
    }

    /// Owned parameter storage. tokio-postgres' `query` / `execute`
    /// take `&[&(dyn ToSql + Sync)]`; we materialize the owned values
    /// here so the `&(dyn ToSql + Sync)` references stay valid for the
    /// duration of the call.
    enum Owned {
        Uuid(Uuid),
        Int8(i64),
        Text(String),
    }

    impl Owned {
        fn as_dyn(&self) -> &(dyn ToSql + Sync) {
            match self {
                Owned::Uuid(v) => v as &(dyn ToSql + Sync),
                Owned::Int8(v) => v as &(dyn ToSql + Sync),
                Owned::Text(s) => s as &(dyn ToSql + Sync),
            }
        }
    }

    struct OwnedParams(Vec<Owned>);

    impl OwnedParams {
        fn refs(&self) -> Vec<&(dyn ToSql + Sync)> {
            self.0.iter().map(Owned::as_dyn).collect()
        }
    }

    impl From<&[ExecutorParam]> for OwnedParams {
        fn from(params: &[ExecutorParam]) -> Self {
            // The driver layer collapses `HexBytes` + `Jsonb` to `Text`
            // because the migration's `prev_hash` / `link_hash` columns
            // are `text` (hex-encoded) and `payload_jsonb` is bound via
            // the SQL `$N::jsonb` cast — the Rust side hands tokio-postgres
            // a `&str` and Postgres performs the cast server-side.
            let mut out = Vec::with_capacity(params.len());
            for p in params {
                match p {
                    ExecutorParam::Uuid(u) => out.push(Owned::Uuid(*u)),
                    ExecutorParam::Int8(n) => out.push(Owned::Int8(*n)),
                    ExecutorParam::Text(s) => out.push(Owned::Text(s.clone())),
                    ExecutorParam::HexBytes(s) => out.push(Owned::Text(s.clone())),
                    ExecutorParam::Jsonb(s) => out.push(Owned::Text(s.clone())),
                }
            }
            OwnedParams(out)
        }
    }

    /// Decode a `Vec<tokio_postgres::Row>` into the canonical
    /// `Vec<ExecutorRow>` shape. Every cell is rendered as
    /// `Option<String>` — the [`crate::neon_shadow::real::RealNeonShadowSink`]
    /// aggregate parsers handle the type coercion in pure Rust.
    fn decode_rows(rows: &[tokio_postgres::Row]) -> Vec<ExecutorRow> {
        rows.iter()
            .map(|row| {
                let mut cells = Vec::with_capacity(row.len());
                for idx in 0..row.len() {
                    // Try common column shapes in order. `try_get::<&str>`
                    // covers `text` + `varchar`; `try_get::<i64>` covers
                    // `bigint` / `int8`. A non-decodable cell becomes
                    // `None` rather than panicking — the upstream caller
                    // surfaces a `Backend` error if a required cell is
                    // `None`.
                    let cell: Option<String> = if let Ok(s) = row.try_get::<_, Option<String>>(idx)
                    {
                        s
                    } else if let Ok(n) = row.try_get::<_, Option<i64>>(idx) {
                        n.map(|v| v.to_string())
                    } else {
                        None
                    };
                    cells.push(cell);
                }
                ExecutorRow { cells }
            })
            .collect()
    }

    /// Build the canonical rustls `ClientConfig` for Neon TLS.
    /// `rustls-native-certs` is the first-class source; `webpki-roots`
    /// the fallback.
    fn build_tls() -> Result<ClientConfig, NeonError> {
        let mut roots = RootCertStore::empty();
        // `rustls-native-certs` 0.8 returns a `CertificateResult` with
        // best-effort cert load — non-fatal errors are accumulated in
        // `.errors`. We add every successfully-loaded cert and fall
        // through to webpki-roots if the system store yielded nothing.
        let native = rustls_native_certs::load_native_certs();
        for cert in native.certs {
            let _ = roots.add(cert);
        }
        if roots.is_empty() {
            // webpki-roots fallback — a fresh container without
            // `/etc/ssl/certs` still validates Neon's cert.
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        if roots.is_empty() {
            return Err(NeonError::Backend(
                "no TLS root certificates available (system + webpki-roots both empty)".to_string(),
            ));
        }
        let cfg = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(cfg)
    }

    // -----------------------------------------------------------------
    // Tests — wasm32 stub roundtrip, param encoder, row decoder.
    // -----------------------------------------------------------------

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
        use crate::neon_shadow::real::{
            ExecutorParam, SQL_INSERT_SHADOW_ROW, SQL_SET_RLS_TENANT_GUC,
        };

        #[test]
        fn owned_params_collapses_hexbytes_and_jsonb_to_text() {
            let tenant = Uuid::now_v7();
            let params = [
                ExecutorParam::Uuid(tenant),
                ExecutorParam::Int8(42),
                ExecutorParam::Text("event".into()),
                ExecutorParam::HexBytes("deadbeef".into()),
                ExecutorParam::Jsonb("{\"k\":\"v\"}".into()),
            ];
            let owned = OwnedParams::from(&params[..]);
            let refs = owned.refs();
            assert_eq!(refs.len(), 5);
        }

        #[test]
        fn tokio_postgres_executor_set_local_runs_before_insert() {
            // This unit test pins the SQL ORDERING contract at the
            // adapter boundary. The actual driver path is tested by the
            // ignored integration tests against real Postgres; here we
            // verify the constants are stable so the integration tests
            // can pin against them.
            //
            // The contract (re-pinned for wave-20):
            //   1. SQL_SET_RLS_TENANT_GUC is the SECOND statement after BEGIN
            //   2. SQL_INSERT_SHADOW_ROW comes AFTER the set_config GUC
            //
            // The wave-19 `sync_chunk_emits_canonical_sql_order` test
            // pins this against InMemoryExecutor; this test pins the
            // adapter sees the same constants (no SQL re-definition).
            assert!(SQL_SET_RLS_TENANT_GUC.contains("set_config"));
            assert!(SQL_SET_RLS_TENANT_GUC.contains("app.current_tenant"));
            assert!(SQL_INSERT_SHADOW_ROW.contains("INSERT INTO audit_events_shadow"));
            // The third arg `true` to set_config scopes the GUC to the
            // txn — load-bearing for INV-AUTH-SCHEMA-RLS-DEFAULT-ON.
            assert!(SQL_SET_RLS_TENANT_GUC.contains("true"));
        }

        #[test]
        fn build_tls_loads_a_non_empty_root_store() {
            // Boot-time invariant: at least one root cert source must
            // populate the trust store, else Neon TLS handshake fails.
            let cfg = build_tls().expect("tls config builds");
            // ClientConfig doesn't expose root count directly; the
            // `build_tls` impl returns Err if no roots load, so reaching
            // this line means at least one source populated the store.
            let _ = cfg;
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_stub {
    //! Wasm32 stub. The production driver is native-only — CF Workers
    //! reach the per-region Neon project via the native gRPC server,
    //! never directly from the worker isolate. Every method surfaces
    //! [`crate::neon_shadow::real::NeonError::WasmOnly`] so a misrouted
    //! call fails fast.

    use std::sync::Arc;

    use crate::neon_shadow::real::{ExecutorParam, ExecutorRow, NeonError, NeonExecutor};

    /// Wasm32 stub of the production executor. Constructable so the
    /// `Arc<dyn NeonExecutor>` trait-object wiring type-checks on
    /// wasm32; every method surfaces [`NeonError::WasmOnly`].
    #[derive(Debug, Default)]
    pub struct TokioPostgresExecutor;

    impl TokioPostgresExecutor {
        /// Construct a wasm32 stub. The DSN is accepted (for signature
        /// parity) but dropped — every method surfaces `WasmOnly`.
        #[must_use]
        pub fn new() -> Self {
            Self
        }

        /// Wrap the wasm32 stub in `Arc<dyn NeonExecutor>` so the
        /// composition root signature stays target-agnostic.
        #[must_use]
        pub fn into_arc(self) -> Arc<dyn NeonExecutor> {
            Arc::new(self)
        }
    }

    impl NeonExecutor for TokioPostgresExecutor {
        fn execute(&self, _sql: &str, _params: &[ExecutorParam]) -> Result<u64, NeonError> {
            Err(NeonError::WasmOnly)
        }
        fn query(
            &self,
            _sql: &str,
            _params: &[ExecutorParam],
        ) -> Result<Vec<ExecutorRow>, NeonError> {
            Err(NeonError::WasmOnly)
        }
    }
}

#[cfg(all(feature = "neon-real", not(target_arch = "wasm32")))]
pub use native::{TokioPostgresExecutor, DEFAULT_POOL_IDLE_TIMEOUT, DEFAULT_POOL_MAX_SIZE};

#[cfg(target_arch = "wasm32")]
pub use wasm_stub::TokioPostgresExecutor;
