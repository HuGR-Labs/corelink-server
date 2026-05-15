//! Production D1 database adapter.
//!
//! Unlike R2/KV (which have crate-level trait abstractions in
//! `corelink-worker`), D1 access in CoreLink is currently raw —
//! `corelink-clerk-cf::health` calls `D1Database::prepare(...).bind(...).run()`
//! directly. We expose [`CfD1DatabaseAdapter`] as a thin wrapper that:
//!
//! 1. Owns a `worker::D1Database` and surfaces `prepare` + `exec` for
//!    callers that want a stable trait-like façade.
//! 2. Documents the CTRL-PRIV-001 contract — D1 query results are NEVER
//!    logged at this layer. Callers redact by hashing PII columns before
//!    emitting `tracing::*` events.
//!
//! When the canonical D1 trait surface is added (post-R2-10), this
//! adapter will grow `impl D1Backend for CfD1DatabaseAdapter` and the
//! `prepare / exec` accessors will be deprecated in favor of trait
//! methods. For now the raw passthrough is the minimum viable wiring.

use std::sync::Arc;
use worker::{D1Database, D1PreparedStatement, Result as WorkerResult};

/// Production D1 adapter wrapping a `worker::D1Database`.
///
/// Construct via [`CfD1DatabaseAdapter::new`] from a database obtained
/// via `env.d1("CONFIG_DB")`. The adapter is cheaply clonable — the
/// `D1Database` from workers-rs 0.8 does NOT implement `Clone`, so we
/// hold it behind an `Arc` to retain the trait-abstraction-defer
/// pattern's "cheap Clone" invariant. CF Workers are single-threaded,
/// so `Arc` here is just for `&Self`-via-clone ergonomics, not real
/// cross-thread sharing.
#[derive(Clone, Debug)]
pub struct CfD1DatabaseAdapter {
    db: Arc<D1Database>,
}

impl CfD1DatabaseAdapter {
    /// Wrap a `worker::D1Database` binding.
    #[must_use]
    pub fn new(db: D1Database) -> Self {
        Self { db: Arc::new(db) }
    }

    /// Borrow the underlying `worker::D1Database` (escape hatch for
    /// callers that need the raw API surface, e.g. batch transactions).
    #[must_use]
    pub fn inner(&self) -> &D1Database {
        &self.db
    }

    /// Prepare a parameterized SQL statement.
    ///
    /// Equivalent to `inner().prepare(sql)`; provided for callers that
    /// want to depend on the adapter type rather than the raw
    /// `D1Database` for testability / mocking ergonomics.
    ///
    /// # CTRL-PRIV-001
    ///
    /// The `sql` parameter MAY be logged by the caller (queries are
    /// schema, not data). Bound parameter values (`?1`, `?2`, ...)
    /// MUST NOT be logged beyond a stable hash/correlation_id.
    pub fn prepare(&self, sql: &str) -> D1PreparedStatement {
        self.db.prepare(sql)
    }

    /// Execute a raw SQL statement (no bound params, no result rows).
    ///
    /// # Errors
    ///
    /// Returns `worker::Error` if the underlying CF binding rejects the
    /// statement. Callers should map to `COR_SERVICE_DEGRADED`.
    pub async fn exec(&self, sql: &str) -> WorkerResult<()> {
        self.db.exec(sql).await.map(|_| ())
    }
}
