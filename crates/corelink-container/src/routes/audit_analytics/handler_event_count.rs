//! Axum handler for `GET /v1/audit/analytics/event-count`.
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
    AnalyticsAuditRow, EventCountEntry, EventCountQuery, EventCountResponse, RequestPrelude,
    EVENT_TYPE_ANALYTICS_QUERY,
};

/// Internal handler for `/event-count`.
///
/// The authenticated tenant is the PAT-resolved `x-corelink-tenant-id`
/// header bound by the [`crate::auth_tenant::AuthTenant`] extractor (the
/// SOLE isolation key — the extractor already guarantees a non-empty,
/// non-sentinel value; parse it as a UUID, 400 on a non-UUID, mirroring
/// `audit_export`). Axum 0.7 extractor ordering: `State`, then
/// `AuthTenant` / `Option<Extension>` (both `FromRequestParts`), then
/// `Query` last (the sole `FromRequest`).
pub(super) async fn handle_event_count(
    State(state): State<AuditAnalyticsRouteState>,
    auth: crate::auth_tenant::AuthTenant,
    prelude: Option<Extension<RequestPrelude>>,
    Query(query): Query<EventCountQuery>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    let tenant = match Uuid::parse_str(auth.0.trim()) {
        Ok(t) => t,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "tenant: invalid uuid in header").into_response();
        }
    };
    // Audit-read scope gate (WP-B, fail-CLOSED): analytics over the security /
    // PII audit log requires a READ-capable PAT (any of `read-only` /
    // `read-write` / `cas:*` / `admin`) in the Worker-trusted `x-corelink-scope`
    // header. A credential with NO read capability (empty / missing scope, or a
    // `find-missing`-only / `billing`-only token) is rejected 403 BEFORE any
    // data access and BEFORE the PAT-possession gate below. Cross-tenant
    // isolation is the `x-corelink-tenant-id` binding's job, not this predicate's.
    if !crate::scope::requires_audit_read(crate::scope::scope_from_headers(&headers)) {
        return (StatusCode::FORBIDDEN, "insufficient scope").into_response();
    }
    // Native PAT possession gate (rt-nuclear #17 — defense-in-depth): re-verify the
    // bearer PAT resolves to the authenticated tenant BEFORE any data access, so a
    // leaked PAT_SIGNING_KEY cannot serve a forged tenant's analytics. Mirrors
    // `cas::pat_gate_reject`. `None` in dev/CI ⇒ skipped.
    if let Some(resp) = super::pat_gate_reject(&state, &auth.0, &headers).await {
        return resp;
    }
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
    let buckets =
        match shadow.aggregate_event_count(query.from, query.to, query.event_type.as_deref()) {
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
