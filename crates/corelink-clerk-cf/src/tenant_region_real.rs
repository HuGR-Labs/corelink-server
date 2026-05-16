//! Wave-25 closure: CF Worker production wire for `D1TenantRegionResolver`.
//!
//! Wave-21 (`crates/corelink-audit-chain/src/neon_shadow/tenant_region.rs`)
//! shipped the `TenantConfigStore` + `D1TenantRegionResolver` trait
//! surface but left the CF Worker boot path on an
//! `InMemoryTenantRegionResolver` IAD fallback — the
//! `corelink-clerk-cf::prod_wiring` boot path "not yet updated to
//! construct `D1TenantRegionResolver`" per the wave-21 audit doc §7.
//!
//! This module closes that caveat by providing a `D1TenantConfigStore`
//! impl of [`corelink_audit_chain::TenantConfigStore`] that wraps a
//! [`corelink_cf_bindings::CfD1DatabaseReal`] (the tenant-prefix-
//! enforced + audit-fenced D1 wrapper from wave-15) against the
//! `tenant_config.region` column added by migration
//! `0052_tenant_config_region.sql`.
//!
//! ## Why a sync trait wrapping an async D1 call
//!
//! [`corelink_audit_chain::TenantConfigStore`] is a **sync** trait
//! because the wave-21 `ShadowSinkFactory::for_tenant` call site is
//! synchronous and synchronously routes through `resolve_region`. The
//! `worker::D1Database::prepare(...).first(...)` API is async, so the
//! production wasm32 impl cannot block-on-async inside the trait body
//! (CF Workers' wasm32 runtime forbids `block_on`). Resolution:
//! the production CF Worker boot path will warm a per-request cache
//! of resolved regions BEFORE entering the synchronous factory call
//! site, and this store consults the cache. The cache layer is
//! deferred to a future wave (it needs a per-Request `Arc<Mutex<…>>`
//! threaded through the handler chain); this module ships the
//! resolver wire with an in-memory cache constructor so the CF
//! Worker boot path can populate it from the JWT-validation step
//! (where the tenant-id is already resolved and a single D1 lookup
//! is the cheapest place to fetch the region pin).
//!
//! The wave-25 wire therefore consists of two halves:
//!
//! 1. [`D1TenantConfigStore`] — sync trait impl that reads from an
//!    in-memory `Arc<Mutex<HashMap<Uuid, Option<String>>>>` cache.
//! 2. [`D1TenantConfigStore::prefetch`] — async helper the CF Worker
//!    boot path calls (in the request-prelude, before the
//!    `ShadowSinkFactory::for_tenant` synchronous dispatch) to
//!    populate the cache by querying `SELECT region FROM tenant_config
//!    WHERE tenant_id = ?` via `CfD1DatabaseReal::scoped_query`.
//!
//! The cache MUST be re-populated per request (it carries a single
//! tenant entry per CF Worker invocation by construction — the
//! tenant-id is bound at request entry); long-lived caching across
//! requests is intentionally out of scope (TTL coherence on the
//! `tenant_config.region` column is the operator's responsibility
//! via deploy-time UPDATE statements).
//!
//! ## Cross-refs
//!
//! - `crates/corelink-audit-chain/src/neon_shadow/tenant_region.rs`
//!   — `TenantConfigStore` trait + `D1TenantRegionResolver` impl.
//! - `migrations/d1/0052_tenant_config_region.sql` — D1 column.
//! - `specs/_audits/2026-05-16-tenant-config-cf-prod-wire.md`
//!   — wave-25 closure note.
//! - `specs/_audits/2026-05-16-neon-shadow-real-driver.md` §7
//!   — wave-21 caveat being closed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use corelink_audit_chain::TenantConfigStore;
use corelink_cf_bindings::CfD1DatabaseReal;
use uuid::Uuid;

/// Canonical SQL for the `tenant_config.region` lookup. Pinned to
/// migration `0052_tenant_config_region.sql` column list.
///
/// Includes `WHERE tenant_id = ?` so the
/// [`CfD1DatabaseReal::scoped_query`] tenant-scope validator accepts it
/// (the validator rejects any SELECT lacking the canonical scope
/// clause — defense in depth against a tenant-isolation bypass).
pub const SQL_SELECT_TENANT_REGION: &str =
    "SELECT region FROM tenant_config WHERE tenant_id = ?";

/// Failure modes for [`D1TenantConfigStore::prefetch`].
#[non_exhaustive]
#[derive(Debug)]
pub enum PrefetchError {
    /// The wrapped [`CfD1DatabaseReal`] rejected the query (tenant
    /// scope validation, audit fence, or backend error). The
    /// underlying [`corelink_cf_bindings::D1Error`] is preserved as
    /// a string for the boot-path log line; the resolver layer
    /// surfaces it as [`corelink_audit_chain::TenantRegionError::BackendUnavailable`].
    Backend(String),
}

impl std::fmt::Display for PrefetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend(msg) => write!(f, "prefetch: {msg}"),
        }
    }
}

impl std::error::Error for PrefetchError {}

/// Production [`TenantConfigStore`] backed by a per-request cache
/// populated from the CF Worker D1 binding via
/// [`CfD1DatabaseReal::scoped_query`].
///
/// The cache is an `Arc<Mutex<HashMap<Uuid, Option<String>>>>`:
///
/// - `Some(label)` — D1 returned a `tenant_config.region` row.
/// - `None`        — D1 returned no row (legacy tenant pre-`0052`);
///   the [`corelink_audit_chain::D1TenantRegionResolver`] applies
///   its configured fallback region.
///
/// Construct via [`D1TenantConfigStore::new`] then call
/// [`D1TenantConfigStore::prefetch`] from the CF Worker fetch
/// handler's request-prelude (BEFORE entering the synchronous
/// `ShadowSinkFactory::for_tenant` dispatch) to populate the cache
/// for the current request's tenant.
#[derive(Clone, Debug)]
pub struct D1TenantConfigStore {
    cache: Arc<Mutex<HashMap<Uuid, Option<String>>>>,
}

impl Default for D1TenantConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

impl D1TenantConfigStore {
    /// Construct an empty store. The cache is populated lazily via
    /// [`Self::prefetch`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Number of cached `(tenant_id → Option<region_label>)` entries.
    /// Operator-visible boot log proxy.
    ///
    /// # Errors
    ///
    /// Returns an `Err(&'static str)` if the cache mutex is poisoned
    /// — never happens in normal operation but we surface the
    /// fail-CLOSED path explicitly rather than panicking.
    pub fn cache_size(&self) -> Result<usize, &'static str> {
        let guard = self.cache.lock().map_err(|_| "cache_size: mutex poisoned")?;
        Ok(guard.len())
    }

    /// Insert a `(tenant_id → Some(label))` pin directly into the
    /// cache without consulting D1. Used at request-prelude time
    /// when the CF Worker boot path has already resolved the region
    /// via the JWT principal claim (development / staging path) or
    /// when the test harness pre-populates the store.
    ///
    /// # Errors
    ///
    /// Returns `Err(&'static str)` if the cache mutex is poisoned.
    pub fn insert(&self, tenant_id: Uuid, label: Option<String>) -> Result<(), &'static str> {
        let mut guard = self
            .cache
            .lock()
            .map_err(|_| "insert: cache mutex poisoned")?;
        guard.insert(tenant_id, label);
        Ok(())
    }

    /// Prefetch the `tenant_config.region` row for `tenant_id` via
    /// the supplied [`CfD1DatabaseReal`] and populate the cache.
    ///
    /// The query is the canonical [`SQL_SELECT_TENANT_REGION`]
    /// string; the `tenant_id` is bound as the FIRST positional
    /// parameter so [`CfD1DatabaseReal::verify_first_bind`]'s
    /// constant-time anchor check passes.
    ///
    /// # Errors
    ///
    /// Returns [`PrefetchError::Backend`] if the wrapped wrapper
    /// rejects the query (tenant scope violation, audit fence,
    /// backend network error). The native-stub variant always
    /// returns `Backend("WasmOnly: …")` after running the tenant-
    /// scope + audit contract — the test harness inserts cache
    /// entries directly via [`Self::insert`] to avoid that path.
    ///
    /// # wasm32 vs native
    ///
    /// - `target_arch = "wasm32"` — actually queries D1.
    /// - other targets — runs the validation + audit contract via
    ///   `stub_for_native_tests` and returns `Backend(WasmOnly:…)`.
    #[cfg(target_arch = "wasm32")]
    pub async fn prefetch(
        &self,
        d1: &CfD1DatabaseReal,
        tenant_id: Uuid,
        tenant_id_str: &str,
    ) -> Result<(), PrefetchError> {
        let scoped = d1
            .scoped_query(SQL_SELECT_TENANT_REGION)
            .map_err(|e| PrefetchError::Backend(e.to_string()))?;
        let stmt = d1
            .prepare(&scoped)
            .map_err(|e| PrefetchError::Backend(e.to_string()))?;
        let bound = d1
            .bind(stmt, &[tenant_id_str])
            .map_err(|e| PrefetchError::Backend(e.to_string()))?;
        let label: Option<String> = d1
            .first(&bound, Some("region"))
            .await
            .map_err(|e| PrefetchError::Backend(e.to_string()))?;
        self.insert(tenant_id, label)
            .map_err(|m| PrefetchError::Backend(m.to_owned()))?;
        Ok(())
    }

    /// Native-stub `prefetch`. Runs the validation + audit contract
    /// against the supplied native-stub D1 wrapper and surfaces
    /// `Backend("WasmOnly: …")` per the canonical native-stub
    /// pattern. Used by `tests/tenant_region_wire.rs` to pin the
    /// wire contract without the wasm32 toolchain.
    ///
    /// # Errors
    ///
    /// Always returns `Err(PrefetchError::Backend(WasmOnly:…))`
    /// after the audit + tenant-scope validation has run.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn prefetch(
        &self,
        d1: &CfD1DatabaseReal,
        _tenant_id: Uuid,
        _tenant_id_str: &str,
    ) -> Result<(), PrefetchError> {
        // Run the audit + tenant-scope validation via the native
        // `audit_and_scope_for_tests` helper so the wire path runs
        // the same validation contract as wasm32 production. The
        // native stub `prepare` / `bind` / `first` paths all return
        // `WasmOnly: …` after the contract.
        let _scoped = d1
            .audit_and_scope_for_tests(
                corelink_cf_bindings::D1Op::Prepare,
                SQL_SELECT_TENANT_REGION,
            )
            .map_err(|e| PrefetchError::Backend(e.to_string()))?;
        Err(PrefetchError::Backend(
            "WasmOnly: prefetch tenant_config.region (native stub)".to_owned(),
        ))
    }
}

impl TenantConfigStore for D1TenantConfigStore {
    fn region_label(&self, tenant_id: &Uuid) -> Result<Option<String>, String> {
        let guard = self
            .cache
            .lock()
            .map_err(|_| "d1: cache mutex poisoned".to_owned())?;
        // Cache miss is treated as `Ok(None)` so the resolver applies
        // its configured fallback — same shape as a D1 row missing
        // for a legacy tenant. This is fail-OPEN at the resolver-
        // level but fail-CLOSED at the route layer when the fallback
        // is `None` (strict mode). The CF Worker boot path always
        // configures a non-`None` fallback so a cache-miss does not
        // 503 the request.
        Ok(guard.get(tenant_id).cloned().unwrap_or(None))
    }
}

// ---------------------------------------------------------------------------
// Tests — native (host CI) target only. The wasm32 target is built by
// the canonical `cargo build --target wasm32-unknown-unknown` gate.
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: panics on assertion failure are the canonical signal"
)]
mod tests {
    use super::*;
    use corelink_cf_bindings::TenantId;

    #[test]
    fn store_returns_none_for_uncached_tenant() {
        let store = D1TenantConfigStore::new();
        let tenant = Uuid::now_v7();
        assert_eq!(store.region_label(&tenant).expect("region_label"), None);
    }

    #[test]
    fn store_returns_inserted_label() {
        let store = D1TenantConfigStore::new();
        let tenant = Uuid::now_v7();
        store
            .insert(tenant, Some("FRA".to_owned()))
            .expect("insert");
        assert_eq!(
            store.region_label(&tenant).expect("region_label"),
            Some("FRA".to_owned())
        );
        assert_eq!(store.cache_size().expect("cache_size"), 1);
    }

    #[tokio::test]
    async fn prefetch_native_stub_returns_wasm_only() {
        // The native stub exercises the audit + tenant-scope
        // validation contract then surfaces `WasmOnly:` per the
        // canonical native-stub pattern.
        let tenant_id_str = "0123456789abcdef";
        let tenant = Uuid::now_v7();
        let d1 = CfD1DatabaseReal::stub_for_native_tests(
            TenantId::new(tenant_id_str).expect("valid tenant"),
        );
        let store = D1TenantConfigStore::new();
        let outcome = store.prefetch(&d1, tenant, tenant_id_str).await;
        match outcome {
            Err(PrefetchError::Backend(msg)) => {
                assert!(msg.contains("WasmOnly"), "expected WasmOnly diagnostic: {msg}");
            }
            other => panic!("expected PrefetchError::Backend(WasmOnly), got {other:?}"),
        }
    }
}
