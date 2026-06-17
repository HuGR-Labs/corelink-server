//! Axum handler for `GET /v1/audit/analytics/timeline`.
//!
//! Split from monolithic `audit_analytics.rs` (wave-33 stage 2.PRE-B.2.c).
//!
//! Security fix (`fix/sec-critical-public-exposure`): the authenticated
//! tenant is the DO-injected `x-corelink-tenant-id` header, bound by the
//! [`crate::auth_tenant::AuthTenant`] extractor (fail-CLOSED — the
//! handler cannot run without a concrete, non-sentinel tenant), mirroring
//! `routes/audit_export/handler.rs`. The previous implementation derived
//! the tenant from the CLIENT-forgeable `x-tenant-id` header via
//! `parse_tenant_header`, which the Worker never sets/strips — a forged
//! `x-tenant-id: <victim>` read any tenant's audit analytics. The tenant
//! is now SOLELY `auth.0`; `x-tenant-id` is no longer an authority.

#![forbid(unsafe_code)]

use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use uuid::Uuid;

use super::audit_sink::emit_or_503;
use super::rate_limit::rate_limit_check;
use super::shadow_factory::resolve_shadow_via_prelude;
use super::state::AuditAnalyticsRouteState;
use super::types::{
    AnalyticsAuditRow, RequestPrelude, TimelineEntry, TimelineQuery, TimelineResponse,
    EVENT_TYPE_ANALYTICS_QUERY, MAX_GRANULARITY_MS, MAX_TIMELINE_BUCKETS,
};

/// Internal handler for `/timeline`.
///
/// The authenticated tenant is the PAT-resolved `x-corelink-tenant-id`
/// header bound by the [`crate::auth_tenant::AuthTenant`] extractor (the
/// SOLE isolation key — the extractor already guarantees a non-empty,
/// non-sentinel value; parse it as a UUID, 400 on a non-UUID, mirroring
/// `audit_export`). Axum 0.7 extractor ordering: `State`, then
/// `AuthTenant` / `Option<Extension>` (both `FromRequestParts`), then
/// `Query` last (the sole `FromRequest`).
pub(super) async fn handle_timeline(
    State(state): State<AuditAnalyticsRouteState>,
    auth: crate::auth_tenant::AuthTenant,
    prelude: Option<Extension<RequestPrelude>>,
    Query(query): Query<TimelineQuery>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    let tenant = match Uuid::parse_str(auth.0.trim()) {
        Ok(t) => t,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "tenant: invalid uuid in header").into_response();
        }
    };
    // Native PAT possession gate (rt-nuclear #17) — see `handle_event_count`.
    if let Some(resp) = super::pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
    if query.from >= query.to {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "timeline".to_string(),
                query.from,
                query.to,
                0,
                "bad_request".to_string(),
            ),
            (StatusCode::BAD_REQUEST, "from must be < to").into_response(),
        );
    }
    let granularity_ms = query.granularity.unwrap_or(3_600_000);
    if granularity_ms == 0 || granularity_ms > MAX_GRANULARITY_MS {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "timeline".to_string(),
                query.from,
                query.to,
                0,
                "bad_request".to_string(),
            ),
            (
                StatusCode::BAD_REQUEST,
                "granularity must be in (0, 86_400_000]",
            )
                .into_response(),
        );
    }
    let span = query.to.saturating_sub(query.from);
    let bucket_count_estimate = span.div_ceil(granularity_ms);
    // The bucket-cardinality gate is a defense against *resource
    // exhaustion* (a 7-year span at 1ms granularity would compute
    // 220-billion buckets), NOT a rate-limit bypass guard. Wave-20
    // (B-P2-04 closure): a burst of 11 requests with `to - from = 1,
    // granularity = 1` (each request returns 1 bucket, passing this
    // cardinality cap) would consume 10 rate-limit tokens immediately,
    // BUT the underlying Postgres aggregate over a 1ms span is bounded
    // O(1) and the per-tenant rate-limit gate one line down enforces
    // the throughput cap. The residual attack surface is one burst of
    // `rate_limit.capacity` cheap aggregates — bounded by design.
    if bucket_count_estimate > MAX_TIMELINE_BUCKETS {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "timeline".to_string(),
                query.from,
                query.to,
                0,
                "bad_request".to_string(),
            ),
            (
                StatusCode::BAD_REQUEST,
                "(to - from) / granularity exceeds MAX_TIMELINE_BUCKETS",
            )
                .into_response(),
        );
    }
    if let Some(resp) = rate_limit_check(&state, tenant, "timeline", query.from, query.to) {
        return resp;
    }
    let (shadow, region_source) = match resolve_shadow_via_prelude(
        &state,
        prelude.as_ref().map(|Extension(p)| p),
        tenant,
        "timeline",
        query.from,
        query.to,
    ) {
        Ok(pair) => pair,
        Err(msg) => {
            return emit_or_503(
                &state,
                AnalyticsAuditRow::new(
                    EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                    Some(tenant),
                    "timeline".to_string(),
                    query.from,
                    query.to,
                    0,
                    "backend_error".to_string(),
                ),
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response(),
            );
        }
    };
    if shadow.tenant_id() != tenant {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "timeline".to_string(),
                query.from,
                query.to,
                0,
                "backend_error".to_string(),
            )
            .with_region_source(region_source),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "shadow sink tenant mismatch",
            )
                .into_response(),
        );
    }
    let buckets = match shadow.aggregate_timeline(query.from, query.to, granularity_ms) {
        Ok(v) => v,
        Err(_) => {
            return emit_or_503(
                &state,
                AnalyticsAuditRow::new(
                    EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                    Some(tenant),
                    "timeline".to_string(),
                    query.from,
                    query.to,
                    0,
                    "backend_error".to_string(),
                )
                .with_region_source(region_source),
                (StatusCode::INTERNAL_SERVER_ERROR, "shadow aggregate failed").into_response(),
            );
        }
    };
    let response = TimelineResponse {
        buckets: buckets.into_iter().map(TimelineEntry::from).collect(),
        granularity_ms,
    };
    let buckets_returned = response.buckets.len() as u64;
    emit_or_503(
        &state,
        AnalyticsAuditRow::new(
            EVENT_TYPE_ANALYTICS_QUERY.to_string(),
            Some(tenant),
            "timeline".to_string(),
            query.from,
            query.to,
            buckets_returned,
            "ok".to_string(),
        )
        .with_region_source(region_source),
        (StatusCode::OK, axum::Json(response)).into_response(),
    )
}
