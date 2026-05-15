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

use axum::Router;

/// Admin HTTP routes (R-prep wire-up; wave-11).
pub mod admin;
/// AC HTTP routes (R-prep wire-up; wave-11).
pub mod ac;
/// Customer-facing audit-export route (Wave-15.3 wiring of
/// WI-S09-008): `GET /v1/audit/export?from=&to=` streams NDJSON
/// audit events + inclusion proofs.
pub mod audit_export;
/// CAS HTTP routes (R-prep example wire-up; wave-8).
pub mod cas;

/// Build the composed handler router merging CAS, AC, and Admin
/// sub-routers. Each sub-router carries its own state.
pub fn build() -> Router {
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
    let audit_export_state = audit_export::build_state();
    Router::new()
        .merge(cas::router(cas_state))
        .merge(ac::router(ac_state))
        .merge(admin::router(admin_state))
        .merge(audit_export::router(audit_export_state))
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
}
