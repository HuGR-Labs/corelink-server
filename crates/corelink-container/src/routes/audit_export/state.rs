//! Route state, rate-limit config, router builder, and query
//! parameters for the `/v1/audit/export` route.
//!
//! Split from monolithic `audit_export.rs` (wave-33 stage 2.PRE-B.1.c).
//! Verbatim move of `AuditExportRouteState`, `build_state`,
//! `audit_export_rate_limit_config`, `router`, and `AuditExportQuery`.

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::{routing::get, Router};
use corelink_audit_chain::{AuditExporter, InMemoryAuditExporter};
use corelink_ratelimit::{
    InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics, InMemoryTokenBucketRateLimiter,
    RateLimitConfig, RateLimiter,
};
use serde::Deserialize;

use crate::wall_clock::{default_wall_clock, WallClock};

use super::audit_sink::{ExportAuditSink, InMemoryExportAuditSink};
use super::handler::handle_export;
use super::types::{AUDIT_EXPORT_ROUTE, R2_LIST_PAGE_SIZE};

/// Shared route state. `Arc<dyn ...>` keeps the binary-shape stable
/// across the native / wasm32 swap.
#[derive(Clone)]
pub struct AuditExportRouteState {
    /// Audit-export pure-logic primitive (see WI-R-PREP-AUDIT-EXPORT).
    pub exporter: Arc<dyn AuditExporter>,
    /// Per-tenant rate limiter (per WI-S08-001 framework).
    pub rate_limiter: Arc<dyn RateLimiter>,
    /// Route-level audit sink for the `export_request.v1` +
    /// security + verify-failed emits.
    pub audit_sink: Arc<dyn ExportAuditSink>,
    /// Wave-19 — R2 list page size override (rows per
    /// [`super::stream::R2ListPager::next_page`]). Defaults to
    /// [`R2_LIST_PAGE_SIZE`].
    /// `0` is clamped to the default by the pager constructor.
    /// Exposed on the route state so integration tests can pin the
    /// multi-page wire shape without seeding 1000+ rows.
    pub pager_page_size: usize,
    /// Wave-21 — wall-clock collaborator. The rate-limit gate uses
    /// `wall_clock.now_ms()` as its bucket `now_ms` so the bucket clock
    /// advances on real time rather than on the request window's
    /// `until_ms` (closes finding `A-P2-05`). Production wiring slots
    /// [`crate::wall_clock::SystemWallClock`] (the
    /// [`build_state`] default); tests inject
    /// [`crate::wall_clock::InMemoryFakeWallClock`] for deterministic
    /// rate-limit timing.
    pub wall_clock: Arc<dyn WallClock>,
}

/// Manual `Debug` impl (wave-20 A-P3-02 closure): the `dyn` trait-object
/// fields (`Arc<dyn AuditExporter>`, `Arc<dyn RateLimiter>`,
/// `Arc<dyn ExportAuditSink>`) do NOT require `Debug` on their trait
/// surface — adding a `: Debug` bound would couple every production
/// implementer to a `Debug` derive, which leaks internal state shape
/// (e.g. a real R2 client's auth headers). `finish_non_exhaustive`
/// renders a stable shape (`AuditExportRouteState { .. }`) that's safe
/// to surface in trace logs without redacting per-field. Tests inspect
/// the captured-emit Vec directly via the sink, NOT via this `Debug`.
impl core::fmt::Debug for AuditExportRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AuditExportRouteState").finish_non_exhaustive()
    }
}

/// Build the in-memory native-target route state.
///
/// Production wiring slots the durable R2-backed exporter +
/// `RateLimiter` DO singleton + CloudEvents audit sink here. The
/// trait-object surface keeps the route shape stable across the
/// swap.
#[must_use]
pub fn build_state() -> AuditExportRouteState {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let exporter: Arc<dyn AuditExporter> = Arc::new(InMemoryAuditExporter::new());
        let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
        let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
        let rate_limiter: Arc<dyn RateLimiter> = Arc::new(
            InMemoryTokenBucketRateLimiter::new(
                rl_audit,
                rl_metrics,
                audit_export_rate_limit_config(),
            ),
        );
        let audit_sink: Arc<dyn ExportAuditSink> = Arc::new(InMemoryExportAuditSink::new());
        let wall_clock = default_wall_clock();
        AuditExportRouteState {
            exporter,
            rate_limiter,
            audit_sink,
            pager_page_size: R2_LIST_PAGE_SIZE,
            wall_clock,
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        compile_error!(
            "wasm32 CF-Worker audit-export handler not implemented yet; \
             tracked as WI-S04-CF-WIRING"
        );
    }
}

/// Per-WI-S09-008 §7 Q2 rate-limit config: 1 export per tenant per
/// minute. Burst capacity = 1, refill = 1 token / 60s ≈ 1 tps int.
#[must_use]
pub fn audit_export_rate_limit_config() -> RateLimitConfig {
    // Refill rate is integer tokens per second per
    // `RateLimitConfig` shape; 1/60s rounds to 0 — we therefore
    // express the policy as `burst=1, refill=1, retry_floor=60s`.
    // The bucket starts full so the first request succeeds, after
    // which the `Retry-After` floor of 60s prevents a second
    // request inside the same minute. The `with_overrides`
    // constructor validates the canceled-tenant ≥ ceiling invariant
    // we preserve below.
    //
    // SAFETY (invariant): the override tuple satisfies
    // `with_overrides` validation (burst ≥ 1; floor < ceiling;
    // canceled ≥ ceiling). On a misconfiguration we fall back to
    // `canonical()` to keep the route construction infallible.
    RateLimitConfig::with_overrides(
        1, 1, 60, RETRY_AFTER_HARD_CEILING_SECS_FOR_EXPORT,
        RETRY_AFTER_CANCELED_FOR_EXPORT,
    )
    .unwrap_or_else(RateLimitConfig::canonical)
}

/// Live-tenant retry-after hard ceiling for the audit-export route.
/// 1 day = 86_400s mirrors the framework default; the route never
/// needs a longer hold for live tenants (the 60s floor is the
/// per-WI-S09-008 §7 Q2 anchor; the ceiling is only relevant if
/// the bucket gets adversarially drained).
const RETRY_AFTER_HARD_CEILING_SECS_FOR_EXPORT: u64 = 86_400;
/// Canceled-tenant retry-after — mirrors the framework canonical
/// (7d). Canceled tenants never get an audit-export window served.
const RETRY_AFTER_CANCELED_FOR_EXPORT: u64 = 7 * 86_400;

/// Build the axum router exposing the audit-export route.
pub fn router(state: AuditExportRouteState) -> Router {
    Router::new()
        .route(AUDIT_EXPORT_ROUTE, get(handle_export))
        .with_state(state)
}

/// `GET /v1/audit/export` query parameter surface.
#[derive(Clone, Debug, Deserialize)]
pub struct AuditExportQuery {
    /// Window lower bound (inclusive). Accept either ISO 8601 UTC
    /// (`YYYY-MM-DDTHH:MM:SSZ`) or raw Unix epoch milliseconds.
    pub from: String,
    /// Window upper bound (exclusive). Same accepted shapes as
    /// `from`.
    pub to: String,
    /// Attempted tenant id. Optional — supplied ONLY to surface a
    /// cross-tenant attempt (see module-level docs). When absent
    /// the route uses the `X-Tenant-Id` header value.
    pub tenant: Option<String>,
}
