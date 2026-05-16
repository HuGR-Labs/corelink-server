//! Production `TokioPgShadowSinkFactory` — wave-29 closure of the
//! wave-27 caveat #2 ("ShadowSinkFactory consumer adoption — production
//! `TokioPgShadowSinkFactory` override").
//!
//! Wave-27 wired `RequestPrelude` into the `/v1/audit/analytics/*`
//! handlers and added the `ShadowSinkFactory::for_tenant_in_region`
//! trait method (default impl delegated back to `for_tenant`). The
//! caveat: the production factory in `apps/server/src/main.rs` kept
//! the default delegate, so prelude-present requests STILL paid the
//! per-request `TenantRegionResolver::resolve_region` round-trip.
//!
//! This module ships the wave-29 override: a public, unit-testable
//! `TokioPgShadowSinkFactory` whose `for_tenant_in_region` takes the
//! pre-resolved `(tenant_id, region)` from the prelude and dispatches
//! straight to the per-region executor pool. The fallback path is the
//! wave-21 behaviour — `for_tenant(tenant_id)` runs the resolver
//! round-trip as before. The fallback is exercised only on dev/CI
//! boot paths (no `RequestPrelude` extension) or when the prelude is
//! attached but bound to a different tenant (defense-in-depth).
//!
//! ## Audit-doc reference
//!
//! `specs/_audits/2026-05-16-shadow-sink-full-adoption.md` (this wave;
//! wave-29 closure) and `specs/_audits/2026-05-16-shadow-sink-consumer-adoption.md`
//! §9 caveat #2 → CLOSED-WAVE-29.
//!
//! ## Charter
//!
//! - `#![forbid(unsafe_code)]` — inherited from the crate root.
//! - No `unwrap` / `expect` / `panic` in src — every fallible step
//!   surfaces a `&'static str` error matching the `ShadowSinkFactory`
//!   trait contract.
//!
//! The module is gated by `#[cfg(feature = "neon-real")]` at the
//! crate root (`apps/server/src/lib.rs`); no inner attribute is
//! needed here.

use std::collections::BTreeMap;
use std::sync::Arc;

use corelink_analytics::Region;
use corelink_audit_chain::{
    NeonExecutor, NeonShadowSink, RealNeonShadowSink, ShadowSyncAuditSink, TenantRegionError,
    TenantRegionResolver,
};
use uuid::Uuid;

use crate::routes::audit_analytics::ShadowSinkFactory;

/// Production factory: resolves `(tenant_id) -> Arc<dyn NeonShadowSink>`
/// by routing through the wave-21 [`TenantRegionResolver`] (replacing
/// the wave-20 hard-coded `Region::Iad` default) and feeding the
/// per-region executor through [`RealNeonShadowSink`]. Each
/// `for_tenant` allocates a fresh `RealNeonShadowSink` so the tenant
/// pin stays per-request.
///
/// ## Wave-29 closure — `for_tenant_in_region` override
///
/// When the caller (the `/v1/audit/analytics/*` handler) extracted a
/// pre-resolved region from the wave-26 `RequestPrelude`, the route
/// dispatches through [`ShadowSinkFactory::for_tenant_in_region`].
/// This factory's override skips the
/// [`TenantRegionResolver::resolve_region`] round-trip entirely and
/// dispatches straight to the per-region executor pool — saving one
/// D1 round-trip per `/v1/audit/analytics/*` request when the prelude
/// is populated.
///
/// The pool lookup short-circuits as `region has no executor pool` if
/// the region was not wired at boot (the canonical wave-21
/// fail-CLOSED `&'static str` error contract).
#[derive(Debug)]
pub struct TokioPgShadowSinkFactory {
    executors: BTreeMap<&'static str, Arc<dyn NeonExecutor>>,
    audit_sink: Arc<dyn ShadowSyncAuditSink>,
    region_resolver: Arc<dyn TenantRegionResolver>,
}

impl TokioPgShadowSinkFactory {
    /// Construct the production factory from the per-region executor
    /// map, the shadow-sync audit sink (capture site for the
    /// `corelink.audit.neon_shadow_synced.v1` emits), and the
    /// wave-21 region resolver (the wave-29 override skips this on
    /// the prelude-present hot path).
    #[must_use]
    pub fn new(
        executors: BTreeMap<&'static str, Arc<dyn NeonExecutor>>,
        audit_sink: Arc<dyn ShadowSyncAuditSink>,
        region_resolver: Arc<dyn TenantRegionResolver>,
    ) -> Self {
        Self {
            executors,
            audit_sink,
            region_resolver,
        }
    }

    /// Internal helper — construct a `RealNeonShadowSink` against the
    /// already-resolved `(tenant_id, region)` pair. Returns
    /// `region has no executor pool` if the region was not wired at
    /// boot (matches the wave-21 error contract).
    fn build_sink(
        &self,
        tenant_id: Uuid,
        region: Region,
    ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        let exec = self
            .executors
            .get(region.as_str())
            .ok_or("region has no executor pool")?
            .clone();
        Ok(Arc::new(RealNeonShadowSink::new(
            tenant_id,
            region,
            exec,
            self.audit_sink.clone(),
        )))
    }
}

impl ShadowSinkFactory for TokioPgShadowSinkFactory {
    fn for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        // Wave-21: route through the `TenantRegionResolver`. The
        // typed error variants surface as stable `&'static str`s the
        // route layer maps to 503 SERVICE_UNAVAILABLE.
        let region = match self.region_resolver.resolve_region(&tenant_id) {
            Ok(r) => r,
            Err(TenantRegionError::Unresolved { .. }) => {
                return Err("tenant_region: unresolved");
            }
            Err(TenantRegionError::BackendUnavailable { .. }) => {
                return Err("tenant_region: backend_unavailable");
            }
            // `TenantRegionError` is `#[non_exhaustive]`; a future
            // variant defaults to the same 503-equivalent terminal
            // state until the route layer is taught the new branch.
            Err(_) => return Err("tenant_region: unknown"),
        };
        self.build_sink(tenant_id, region)
    }

    /// Wave-29 override — skip the `TenantRegionResolver::resolve_region`
    /// round-trip entirely; the region was already resolved by the
    /// wave-26 CF Worker request-prelude prefetch.
    ///
    /// Net effect: -1 D1 round-trip per `/v1/audit/analytics/*`
    /// request when the prelude is populated. The fallback path
    /// (`for_tenant`) remains the wave-21 behaviour for dev / test /
    /// native gRPC boot paths or stale-prelude defense-in-depth.
    fn for_tenant_in_region(
        &self,
        tenant_id: Uuid,
        region: Region,
    ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        self.build_sink(tenant_id, region)
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

    use std::sync::Mutex;

    use corelink_audit_chain::{InMemoryExecutor, InMemoryShadowSyncAuditSink};

    /// Resolver fake that counts every `resolve_region` invocation so
    /// the wave-29 override can assert the round-trip is SKIPPED.
    #[derive(Debug)]
    struct CountingResolver {
        calls: Arc<Mutex<u32>>,
        region: Region,
    }

    impl CountingResolver {
        fn new(region: Region) -> Self {
            Self {
                calls: Arc::new(Mutex::new(0)),
                region,
            }
        }

        fn call_count(&self) -> u32 {
            *self.calls.lock().expect("counter lock")
        }
    }

    impl TenantRegionResolver for CountingResolver {
        fn resolve_region(
            &self,
            _tenant_id: &Uuid,
        ) -> Result<Region, TenantRegionError> {
            *self.calls.lock().expect("counter lock") += 1;
            Ok(self.region)
        }
    }

    fn build_factory(region: Region) -> (TokioPgShadowSinkFactory, Arc<CountingResolver>) {
        let resolver = Arc::new(CountingResolver::new(region));
        let mut executors: BTreeMap<&'static str, Arc<dyn NeonExecutor>> = BTreeMap::new();
        executors.insert(region.as_str(), Arc::new(InMemoryExecutor::new()));
        let audit_sink: Arc<dyn ShadowSyncAuditSink> =
            Arc::new(InMemoryShadowSyncAuditSink::new());
        let resolver_dyn: Arc<dyn TenantRegionResolver> = resolver.clone();
        let factory = TokioPgShadowSinkFactory::new(executors, audit_sink, resolver_dyn);
        (factory, resolver)
    }

    /// Wave-29 closure pin: `for_tenant_in_region` MUST NOT touch the
    /// `TenantRegionResolver` — the region is sourced from the
    /// pre-resolved `RequestPrelude` (wave-26) and the resolver
    /// round-trip is the round-trip this override eliminates.
    #[test]
    fn tokio_pg_shadow_sink_factory_for_tenant_in_region_skips_resolver_lookup() {
        let (factory, resolver) = build_factory(Region::Fra);
        let tenant = Uuid::now_v7();

        let sink = factory
            .for_tenant_in_region(tenant, Region::Fra)
            .expect("shadow sink resolves under wave-29 override");
        assert_eq!(sink.tenant_id(), tenant);
        assert_eq!(sink.region(), Region::Fra);
        assert_eq!(
            resolver.call_count(),
            0,
            "wave-29: for_tenant_in_region MUST NOT invoke the region resolver \
             (the prelude already carried the resolved region)"
        );
    }

    /// Symmetric pin: the legacy `for_tenant` path still routes
    /// through the resolver (wave-21 behaviour is unchanged on the
    /// fallback path).
    #[test]
    fn tokio_pg_shadow_sink_factory_for_tenant_still_invokes_resolver() {
        let (factory, resolver) = build_factory(Region::Iad);
        let tenant = Uuid::now_v7();

        let sink = factory
            .for_tenant(tenant)
            .expect("legacy fallback still resolves");
        assert_eq!(sink.tenant_id(), tenant);
        assert_eq!(sink.region(), Region::Iad);
        assert_eq!(
            resolver.call_count(),
            1,
            "wave-21 fallback: for_tenant MUST invoke the region resolver exactly once"
        );
    }

    /// Defense-in-depth: when the prelude carried a region whose
    /// executor pool was NEVER wired at boot (e.g. partial-region
    /// rollout, a region whose Neon project was misconfigured), the
    /// wave-29 override surfaces the canonical
    /// `region has no executor pool` error — the same stable
    /// `&'static str` the wave-21 `for_tenant` arm surfaces on the
    /// same condition. The route layer maps both to 503.
    #[test]
    fn tokio_pg_shadow_sink_factory_for_tenant_in_region_surfaces_missing_pool() {
        let (factory, _resolver) = build_factory(Region::Iad);
        let tenant = Uuid::now_v7();

        // Region::Fra was NEVER inserted into the executor map.
        let err = factory
            .for_tenant_in_region(tenant, Region::Fra)
            .expect_err("missing pool must surface");
        assert_eq!(err, "region has no executor pool");
    }
}
