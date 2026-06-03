//! Wave-25 closure: integration tests for the CF Worker production
//! tenant-region resolver wire.
//!
//! These tests run on the native target (wasm32 build is verified
//! separately by the `cargo build --target wasm32-unknown-unknown
//! -p corelink-clerk-cf` gate). They pin the wire contract added by
//! `corelink_clerk_cf::prod_wiring::build_tenant_region_resolver`
//! against the wave-21 [`corelink_audit_chain::TenantRegionResolver`]
//! trait surface.
//!
//! Net-new in wave-25:
//!
//! - `build_tenant_region_resolver_with_store_constructs_d1_resolver`
//!   — when the D1 binding is present (`Some(store)`), the wire
//!   constructs a `D1TenantRegionResolver` that honours the configured
//!   fallback for cache-miss / legacy-tenant paths. Pins the
//!   "production wire on" branch.
//! - `build_tenant_region_resolver_without_store_falls_back_to_inmemory`
//!   — when the D1 binding is absent (`None`), the wire falls back
//!   to an `InMemoryTenantRegionResolver` configured with the
//!   supplied fallback region. Pins the "dev mode" branch.

#![cfg(feature = "tenant-region-real")]
#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "integration test code"
)]

use std::sync::Arc;

use corelink_audit_chain::{Region, TenantRegionResolver};
use corelink_clerk_cf::prod_wiring::build_tenant_region_resolver;
use corelink_clerk_cf::tenant_region_real::D1TenantConfigStore;
use uuid::Uuid;

/// Wave-25 net-new #1: production wire branch — supplied
/// `D1TenantConfigStore` constructs the `D1TenantRegionResolver`,
/// which routes `resolve_region` through the store. A pre-populated
/// `Some("FRA")` cache entry resolves to `Region::Fra`; an
/// uncached tenant falls back to the configured `Region::Iad`.
#[test]
fn build_tenant_region_resolver_with_store_constructs_d1_resolver() {
    let store = Arc::new(D1TenantConfigStore::new());
    let pinned_tenant = Uuid::now_v7();
    store
        .insert(pinned_tenant, Some("FRA".to_owned()))
        .expect("insert FRA");

    let resolver: Arc<dyn TenantRegionResolver> =
        build_tenant_region_resolver(Some(store.clone()), Region::Iad);

    // Pinned tenant resolves through the D1-backed cache to FRA.
    assert_eq!(resolver.resolve_region(&pinned_tenant), Ok(Region::Fra));

    // Uncached tenant => `Ok(None)` from the store => resolver applies
    // its `Region::Iad` fallback (the wave-21 audit-doc §7 production
    // fallback that prevents legacy tenants from 503-ing).
    let legacy_tenant = Uuid::now_v7();
    assert_eq!(resolver.resolve_region(&legacy_tenant), Ok(Region::Iad));

    // The store cache carries the explicit pin (used for operator
    // boot-log visibility in the production wire).
    assert_eq!(store.cache_size().expect("cache_size"), 1);
}

/// Wave-25 net-new #2: dev-mode wire branch — when the CF Worker
/// boot path runs WITHOUT a D1 binding (`None`), the wire constructs
/// an `InMemoryTenantRegionResolver` with the configured fallback
/// region. Every `resolve_region` lands on the fallback so the
/// `/v1/audit/analytics/*` routes stay reachable in `wrangler dev`
/// even without the `CLERK_DB` binding wired.
#[test]
fn build_tenant_region_resolver_without_store_falls_back_to_inmemory() {
    let resolver: Arc<dyn TenantRegionResolver> = build_tenant_region_resolver(None, Region::Iad);

    // Every tenant resolves to the fallback (no pins, no D1 store).
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    assert_eq!(resolver.resolve_region(&tenant_a), Ok(Region::Iad));
    assert_eq!(resolver.resolve_region(&tenant_b), Ok(Region::Iad));

    // Constructing with a different fallback region routes there
    // instead — pins that the wire honours the caller-supplied
    // fallback rather than hard-coding `Region::Iad`.
    let fra_wire = build_tenant_region_resolver(None, Region::Fra);
    assert_eq!(fra_wire.resolve_region(&tenant_a), Ok(Region::Fra));
}
