//! Per-tenant shadow-sink factory trait + wave-27 `resolve_shadow_via_prelude`
//! helper that prefers the wave-26 `RequestPrelude` over the legacy
//! per-request resolver round-trip.
//!
//! Split from monolithic `audit_analytics.rs` (wave-33 stage 2.PRE-B.2.c).
//! Verbatim move; no behavioural change.

#![forbid(unsafe_code)]

use std::sync::Arc;

use corelink_analytics::Region;
use corelink_audit_chain::NeonShadowSink;
use uuid::Uuid;

use super::state::AuditAnalyticsRouteState;
use super::types::{
    AnalyticsAuditRow, RequestPrelude, EVENT_TYPE_ANALYTICS_QUERY, REGION_SOURCE_FALLBACK,
    REGION_SOURCE_PRELUDE, REQUEST_PRELUDE_MISSING_EXIT,
};

/// Per-tenant shadow-sink factory. Production wiring binds this to
/// the per-region Neon project (see
/// `crates/corelink-audit-chain/src/neon_shadow.rs` module docs).
///
/// The factory is the swap point that lets test harnesses inject an
/// `InMemoryNeonShadowSink` per (tenant, region) while production wires
/// a `RealNeonShadowSink` per tenant's pinned region.
pub trait ShadowSinkFactory: Send + Sync + core::fmt::Debug {
    /// Resolve a shadow sink for `tenant_id`. Production wiring returns
    /// the `RealNeonShadowSink` for the tenant's pinned region; tests
    /// return an in-memory fake.
    ///
    /// # Errors
    ///
    /// Returns a static error string when the tenant has no pinned
    /// region recorded (production) or the test fake doesn't carry one.
    fn for_tenant(&self, tenant_id: Uuid) -> Result<Arc<dyn NeonShadowSink>, &'static str>;

    /// Wave-27 closure: resolve a shadow sink for `tenant_id` given a
    /// *pre-resolved* `region` — typically sourced from the
    /// [`RequestPrelude`] populated by the CF Worker request-prelude
    /// prefetch (wave-26). Production factories override this to skip
    /// the per-request `TenantRegionResolver::resolve_region` round-trip
    /// and dispatch straight to the per-region executor pool.
    ///
    /// The default implementation delegates back to [`Self::for_tenant`]
    /// so existing trait impls keep compiling without modification — the
    /// route layer falls back to this default only when the request was
    /// dispatched WITHOUT a `RequestPrelude` extension (dev/test boot
    /// paths or the CF Worker fetch-handler invoked the legacy path).
    ///
    /// # Errors
    ///
    /// Same error contract as [`Self::for_tenant`]: a static error
    /// string when the tenant has no pinned region recorded or the
    /// per-region executor pool is unavailable.
    fn for_tenant_in_region(
        &self,
        tenant_id: Uuid,
        _region: Region,
    ) -> Result<Arc<dyn NeonShadowSink>, &'static str> {
        self.for_tenant(tenant_id)
    }
}

/// Wave-27 closure: resolve the per-tenant shadow sink, preferring the
/// CF Worker request-prelude (wave-26) when present.
///
/// When the axum `Extension<RequestPrelude>` carries a pre-resolved
/// region for `tenant_id`, this helper dispatches through
/// [`ShadowSinkFactory::for_tenant_in_region`] — skipping the legacy
/// per-request `TenantRegionResolver::resolve_region` round-trip. The
/// CF Worker `prefetch_request_prelude` (wave-26
/// `corelink_clerk_cf::prod_wiring`) is the canonical populator: it
/// runs the D1 `SELECT region FROM tenant_config WHERE tenant_id = ?`
/// in the request-prelude and bundles the resolved [`Region`] into the
/// extension.
///
/// When the extension is missing (dev / test / native gRPC boot paths
/// without the CF Worker prefetch), this helper:
///
/// 1. Emits a `tracing::warn!` line so production operators can detect
///    a CF Worker boot path regression. NEVER a silent IAD fallback —
///    the WARN line is the operator-visible signal.
/// 2. Emits a `request_prelude_missing` audit row through
///    `state.audit_sink.emit` so the row survives in the analytics
///    pipeline (Logpush + dashboard widgets union on `exit_status`).
/// 3. Falls back to the legacy `state.shadow_factory.for_tenant(tenant_id)`
///    dispatch which routes through the wave-21 `TenantRegionResolver`
///    chain — the same path consumers exercised before wave-26.
///
/// The fallback path is intentionally NOT a hard failure: the analytics
/// route MUST stay functional on the native gRPC boot path (which never
/// constructs a CF Worker prelude). The WARN + audit emit is the SEV-3
/// observability hook; SEV-2 alerting is the rate of
/// `request_prelude_missing` rows in the dashboard.
///
/// # Errors
///
/// Returns a static error string when the underlying factory dispatch
/// fails — propagated unchanged so the calling handler emits the
/// canonical `backend_error` audit row + 500.
///
/// # Tenant binding
///
/// When the prelude extension's `tenant_id` does NOT match the header
/// `tenant_id`, the prelude is IGNORED and the fallback path runs. This
/// is a defense-in-depth catch for a wiring bug where the CF Worker
/// dispatches with a stale prelude attached to a different tenant's
/// request — the rejection emits the `request_prelude_missing` marker
/// (the prelude is *functionally* missing for this tenant).
pub(super) fn resolve_shadow_via_prelude(
    state: &AuditAnalyticsRouteState,
    prelude: Option<&RequestPrelude>,
    tenant: Uuid,
    endpoint: &str,
    from_ms: u64,
    to_ms: u64,
) -> Result<(Arc<dyn NeonShadowSink>, &'static str), &'static str> {
    match prelude {
        Some(p) if p.tenant_id == tenant => {
            // Hot path — prelude present and bound to the same
            // tenant. Wave-29: the production `TokioPgShadowSinkFactory`
            // override skips the `TenantRegionResolver::resolve_region`
            // round-trip entirely and dispatches straight to the per-
            // region executor pool. The `region_source = "prelude"`
            // telemetry threads through to the canonical
            // `corelink.audit.analytics_query.v1` happy-path emit.
            let sink = state
                .shadow_factory
                .for_tenant_in_region(tenant, p.region)?;
            Ok((sink, REGION_SOURCE_PRELUDE))
        }
        Some(_) => {
            // Prelude attached BUT bound to a different tenant — the
            // CF Worker dispatched with a stale extension. Treat as
            // missing (emit the marker + fall back).
            tracing::warn!(
                endpoint,
                tenant = %tenant,
                "wave-27 audit-analytics: RequestPrelude tenant mismatch; falling back to factory.for_tenant — \
                 SEV-3 wiring observability"
            );
            let _ = state.audit_sink.emit(AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                endpoint.to_string(),
                from_ms,
                to_ms,
                0,
                REQUEST_PRELUDE_MISSING_EXIT.to_string(),
            ));
            let sink = state.shadow_factory.for_tenant(tenant)?;
            Ok((sink, REGION_SOURCE_FALLBACK))
        }
        None => {
            // Prelude extension absent — dev/test/native gRPC dispatch.
            tracing::warn!(
                endpoint,
                tenant = %tenant,
                "wave-27 audit-analytics: RequestPrelude extension missing; falling back to factory.for_tenant — \
                 SEV-3 wiring observability"
            );
            let _ = state.audit_sink.emit(AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                endpoint.to_string(),
                from_ms,
                to_ms,
                0,
                REQUEST_PRELUDE_MISSING_EXIT.to_string(),
            ));
            let sink = state.shadow_factory.for_tenant(tenant)?;
            Ok((sink, REGION_SOURCE_FALLBACK))
        }
    }
}
