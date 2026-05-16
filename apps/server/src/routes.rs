//! HTTP route surface for the CoreLink server.
//!
//! Wave-8 landed the CAS read end-to-end as the **example wire-up**
//! for the R-prep handler-crate skeleton (see
//! `specs/_audits/2026-05-14-slo-instrumentation-gaps.md §6`):
//! `apps/server::routes::cas` wires `corelink-handler-cas` + in-memory
//! fakes into an axum route, demonstrating where the
//! `#[cfg(target_arch = "wasm32")]` CF-Worker handler slots in.
//!
//! Wave-11 (this commit) extends the same pattern to the AC and
//! Admin handler crates:
//!
//! - [`ac`] — `GET/PUT /v1/ac/{tenant}/{action_digest}` against
//!   `corelink-handler-ac::{AcLookupHandler, AcUpdateHandler}`.
//!   Emits `Sli::AvailAcLookup` + `Sli::LatencyAcHitP99` on every
//!   route entry; enforces `INV-TENANT-ISOLATION` at the route
//!   boundary on top of handler-layer enforcement.
//! - [`admin`] — `GET /v1/admin/read/{resource}` +
//!   `POST /v1/admin/mutate` against
//!   `corelink-handler-admin::{AdminReadHandler, AdminMutateHandler}`.
//!   Emits `Sli::AvailControlPlane` on every route entry; the
//!   mutate route is gated by dual-approval (per
//!   `dual_approval.md §3`).
//!
//! # Router composition
//!
//! [`build`] returns a fully composed `axum::Router` merging all
//! three sub-routers (cas, ac, admin). Each sub-router owns its
//! own `State<...>` so the trait-object swap surface stays local
//! to each module.

use std::sync::Arc;

use axum::Router;
use corelink_analytics::Region;
use corelink_audit_chain::{
    InMemoryNeonShadowSink, InMemoryShadowSyncAuditSink, NeonShadowSink,
};
use uuid::Uuid;

use crate::routes::audit_analytics::ShadowSinkFactory;

/// Admin HTTP routes (R-prep wire-up; wave-11).
pub mod admin;
/// Pilot-admin HTTP routes (Wave-29 stream-3): replaces the wave-27
/// placeholder scripts (`grant-pilot-tier.sh`, `list-pilot-tenants.sh`,
/// `pilot-24h-checkin.sh`) with proper endpoints + audit-emit
/// fail-CLOSED ordering + 5-Layer Defense scope gating.
pub mod admin_pilot;
/// AC HTTP routes (R-prep wire-up; wave-11).
pub mod ac;
/// Customer-facing audit-analytics routes (Wave-18 wiring of the
/// Neon analytics shadow sync): `GET /v1/audit/analytics/event-count`
/// + `GET /v1/audit/analytics/timeline` over the per-tenant
/// `audit_events_shadow` Neon table. The shadow is analytics-only;
/// the canonical chain-integrity store is the R2 NDJSON archive
/// (Wave 15) — see `specs/_audits/2026-05-15-neon-analytics-shadow.md`.
pub mod audit_analytics;
/// Customer-facing audit-export route (Wave-15.3 wiring of
/// WI-S09-008): `GET /v1/audit/export?from=&to=` streams NDJSON
/// audit events + inclusion proofs.
pub mod audit_export;
/// CAS HTTP routes (R-prep example wire-up; wave-8).
pub mod cas;

/// Per-tenant in-memory shadow-sink factory. Production wiring
/// replaces this with a Neon-backed factory (see
/// `apps/server/src/main.rs` boot path); the in-memory factory
/// keeps the `/v1/audit/analytics/*` routes mounted in dev/CI so the
/// handler surface stays exercised end-to-end without a live Postgres
/// dependency.
///
/// Each `for_tenant` call returns a FRESH `InMemoryNeonShadowSink` so
/// the tenant's RLS-binding (the sink owns the `tenant_id`) is
/// preserved per request. The sink's in-memory state is per-instance —
/// the dev/CI shadow does NOT persist across requests (this is
/// intentional: the in-memory factory is a route-shape fixture, not a
/// stand-in for the production Neon backing store).
#[derive(Debug, Default)]
pub struct InMemoryShadowSinkFactory;

impl InMemoryShadowSinkFactory {
    /// Construct the canonical in-memory factory.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ShadowSinkFactory for InMemoryShadowSinkFactory {
    fn for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        // Default region for the in-memory dev/CI sink is IAD — the
        // production factory resolves the per-tenant pinned region
        // from the tenant-config store.
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        Ok(Arc::new(InMemoryNeonShadowSink::new(
            tenant_id,
            Region::Iad,
            audit,
        )))
    }
}

/// Build the composed handler router merging CAS, AC, Admin,
/// audit-export, and (Wave-20 closure) audit-analytics sub-routers.
/// Each sub-router carries its own state.
///
/// The audit-analytics router is mounted unconditionally — the
/// [`InMemoryShadowSinkFactory`] is the default backing for dev/CI;
/// production deployments swap the factory at the boot path
/// (`apps/server/src/main.rs`) when the `neon-real` feature + per-
/// region `NEON_DB_URL_<REGION>` env vars are present.
pub fn build() -> Router {
    build_with_factory(Arc::new(InMemoryShadowSinkFactory::new()))
}

/// Build the composed router with an explicit
/// [`ShadowSinkFactory`] — used by the production boot path to swap
/// in a Neon-backed factory while keeping every other route shape
/// identical.
pub fn build_with_factory(shadow_factory: Arc<dyn ShadowSinkFactory>) -> Router {
    let cas_state = cas::CasRouteState {
        handler: cas::build_handler(),
    };
    let (ac_lookup, ac_update) = ac::build_handlers();
    let ac_state = ac::AcRouteState {
        lookup: ac_lookup,
        update: ac_update,
    };
    let (admin_read, admin_mutate) = admin::build_handlers();
    let admin_state = admin::AdminRouteState {
        read: admin_read,
        mutate: admin_mutate,
    };
    let (pilot_store, pilot_audit) = admin_pilot::build_handlers();
    let pilot_admin_state = admin_pilot::PilotAdminRouteState {
        store: pilot_store,
        audit_sink: pilot_audit,
        wall_clock: crate::wall_clock::default_wall_clock(),
    };
    let audit_export_state = audit_export::build_state();
    let audit_analytics_state = audit_analytics::build_state(shadow_factory);
    Router::new()
        .merge(cas::router(cas_state))
        .merge(ac::router(ac_state))
        .merge(admin::router(admin_state))
        .merge(admin_pilot::router(pilot_admin_state))
        .merge(audit_export::router(audit_export_state))
        .merge(audit_analytics::router(audit_analytics_state))
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

    #[test]
    fn build_returns_composed_router() {
        // Smoke test: the composed router is constructable on the
        // native target. Per-route behaviour is covered by the
        // module-level integration tests in cas.rs / ac.rs / admin.rs.
        let _router = build();
    }

    #[test]
    fn in_memory_shadow_sink_factory_resolves_per_tenant() {
        // Wave-20: the dev/CI default factory yields a fresh
        // InMemoryNeonShadowSink per tenant id, with the tenant pin
        // load-bearing for the route's RLS-like construction-time gate.
        let f = InMemoryShadowSinkFactory::new();
        let tenant = uuid::Uuid::now_v7();
        let sink = f.for_tenant(tenant).expect("resolves");
        assert_eq!(sink.tenant_id(), tenant);
        assert_eq!(sink.region(), Region::Iad);
    }

    #[test]
    fn build_with_factory_accepts_in_memory_factory() {
        // Wave-20 closure: the production boot path uses
        // build_with_factory() to swap in a TokioPgShadowSinkFactory
        // under --feature neon-real; tests + dev use the in-memory
        // factory. Both branches must construct without panic.
        let factory: Arc<dyn ShadowSinkFactory> = Arc::new(InMemoryShadowSinkFactory::new());
        let _router = build_with_factory(factory);
    }
}
