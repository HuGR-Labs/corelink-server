//! Axum handler for `GET /v1/audit/analytics/event-count`.
//!
//! Split from monolithic `audit_analytics.rs` (wave-33 stage 2.PRE-B.2.c).
//! Verbatim move; no behavioural change.

#![forbid(unsafe_code)]

use axum::{
    extract::{Extension, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};

use super::audit_sink::emit_or_503;
use super::rate_limit::{parse_tenant_header, rate_limit_check};
use super::shadow_factory::resolve_shadow_via_prelude;
use super::state::AuditAnalyticsRouteState;
use super::types::{
    AnalyticsAuditRow, EventCountEntry, EventCountQuery, EventCountResponse, RequestPrelude,
    EVENT_TYPE_ANALYTICS_QUERY,
};

/// Internal handler for `/event-count`.
pub(super) async fn handle_event_count(
    State(state): State<AuditAnalyticsRouteState>,
    prelude: Option<Extension<RequestPrelude>>,
    headers: HeaderMap,
    Query(query): Query<EventCountQuery>,
) -> axum::response::Response {
    let tenant = match parse_tenant_header(&headers) {
        Ok(t) => t,
        Err(resp) => return resp,
    };
    if query.from >= query.to {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "event_count".to_string(),
                query.from,
                query.to,
                0,
                "bad_request".to_string(),
            ),
            (StatusCode::BAD_REQUEST, "from must be < to").into_response(),
        );
    }
    if let Some(resp) = rate_limit_check(&state, tenant, "event_count", query.from, query.to) {
        return resp;
    }
    let (shadow, region_source) = match resolve_shadow_via_prelude(
        &state,
        prelude.as_ref().map(|Extension(p)| p),
        tenant,
        "event_count",
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
                    "event_count".to_string(),
                    query.from,
                    query.to,
                    0,
                    "backend_error".to_string(),
                ),
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response(),
            );
        }
    };
    // Verify the shadow sink is bound to this tenant (defense in
    // depth — the SQL-layer RLS is the authoritative gate, the trait
    // pin is the wiring-bug catch). Emit failure here surfaces as 503
    // per the fail-CLOSED contract — a silently-dropped tenant-isolation
    // violation row is a SEV-1 observability hole (B-P1-02 / B-P1-03).
    if shadow.tenant_id() != tenant {
        return emit_or_503(
            &state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                "event_count".to_string(),
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
    let buckets = match shadow.aggregate_event_count(
        query.from,
        query.to,
        query.event_type.as_deref(),
    ) {
        Ok(v) => v,
        Err(_) => {
            return emit_or_503(
                &state,
                AnalyticsAuditRow::new(
                    EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                    Some(tenant),
                    "event_count".to_string(),
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
    let response = EventCountResponse {
        buckets: buckets
            .into_iter()
            .map(|b| EventCountEntry {
                event_type: b.event_type,
                count: b.count,
            })
            .collect(),
    };
    let buckets_returned = response.buckets.len() as u64;
    emit_or_503(
        &state,
        AnalyticsAuditRow::new(
            EVENT_TYPE_ANALYTICS_QUERY.to_string(),
            Some(tenant),
            "event_count".to_string(),
            query.from,
            query.to,
            buckets_returned,
            "ok".to_string(),
        )
        .with_region_source(region_source),
        (StatusCode::OK, axum::Json(response)).into_response(),
    )
}
