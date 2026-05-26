//! Route state + rate-limit config + router builder + in-memory
//! `build_state` for the `/v1/audit/analytics/*` routes.
//!
//! Split from monolithic `audit_analytics.rs` (wave-33 stage 2.PRE-B.2.c).
//! Verbatim move; no behavioural change.

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::{routing::get, Router};
use corelink_ratelimit::{
    InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics, InMemoryTokenBucketRateLimiter,
    RateLimitConfig, RateLimiter,
};

use crate::wall_clock::{default_wall_clock, WallClock};

use super::audit_sink::{AnalyticsAuditSink, InMemoryAnalyticsAuditSink};
use super::handler_event_count::handle_event_count;
use super::handler_timeline::handle_timeline;
use super::shadow_factory::ShadowSinkFactory;
use super::types::{ROUTE_EVENT_COUNT, ROUTE_TIMELINE};

/// Shared route state.
#[derive(Clone)]
pub struct AuditAnalyticsRouteState {
    /// Shadow-sink factory (resolves per-tenant Neon binding).
    pub shadow_factory: Arc<dyn ShadowSinkFactory>,
    /// Per-tenant rate limiter.
    pub rate_limiter: Arc<dyn RateLimiter>,
    /// Audit emit sink.
    pub audit_sink: Arc<dyn AnalyticsAuditSink>,
    /// Wave-21 — wall-clock collaborator. The rate-limit gate uses
    /// `wall_clock.now_ms()` as its bucket `now_ms` so the bucket clock
    /// advances on real time rather than the query window's `to_ms`
    /// (closes finding `B-P2-03`). Production wiring slots
    /// [`crate::wall_clock::SystemWallClock`] (the [`build_state`]
    /// default); tests inject
    /// [`crate::wall_clock::InMemoryFakeWallClock`] for deterministic
    /// rate-limit timing.
    pub wall_clock: Arc<dyn WallClock>,
}

impl core::fmt::Debug for AuditAnalyticsRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AuditAnalyticsRouteState")
            .finish_non_exhaustive()
    }
}

/// Canonical rate-limit config: 10 queries per tenant per minute,
/// 60s retry-after floor. Looser than the export endpoint (analytics
/// is cheaper — Postgres aggregate over the shadow vs. NDJSON walk
/// over R2).
#[must_use]
pub fn audit_analytics_rate_limit_config() -> RateLimitConfig {
    // Args: (refill_rate_per_sec, burst_capacity, retry_after_floor_secs,
    //        retry_after_hard_ceiling_secs, retry_after_canceled_tenant_secs)
    RateLimitConfig::with_overrides(
        1,
        10,
        60,
        ANALYTICS_HARD_CEILING_SECS,
        ANALYTICS_CANCELED_SECS,
    )
    .unwrap_or_else(RateLimitConfig::canonical)
}

const ANALYTICS_HARD_CEILING_SECS: u64 = 86_400;
const ANALYTICS_CANCELED_SECS: u64 = 7 * 86_400;

/// Build the axum router exposing both analytics endpoints.
pub fn router(state: AuditAnalyticsRouteState) -> Router {
    Router::new()
        .route(ROUTE_EVENT_COUNT, get(handle_event_count))
        .route(ROUTE_TIMELINE, get(handle_timeline))
        .with_state(state)
}

/// Build a native in-memory route state for tests + local dev.
///
/// Production wiring swaps `shadow_factory` for a per-region Neon
/// binding (deferred follow-on WI — needs `tokio-postgres` driver +
/// `app.current_tenant` GUC SET inside every transaction).
#[must_use]
pub fn build_state(shadow_factory: Arc<dyn ShadowSinkFactory>) -> AuditAnalyticsRouteState {
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        audit_analytics_rate_limit_config(),
    ));
    let audit_sink: Arc<dyn AnalyticsAuditSink> = Arc::new(InMemoryAnalyticsAuditSink::new());
    let wall_clock = default_wall_clock();
    AuditAnalyticsRouteState {
        shadow_factory,
        rate_limiter,
        audit_sink,
        wall_clock,
    }
}
