//! Common rate-limit gate + tenant-header parser for the
//! `/v1/audit/analytics/*` routes.
//!
//! Split from monolithic `audit_analytics.rs` (wave-33 stage 2.PRE-B.2.c).
//! Verbatim move of `rate_limit_check` and `parse_tenant_header`.

#![forbid(unsafe_code)]

use axum::{
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use corelink_ratelimit::{BucketKey, RateLimitDecision};
use uuid::Uuid;

use super::audit_sink::emit_or_503;
use super::state::AuditAnalyticsRouteState;
use super::types::{AnalyticsAuditRow, EVENT_TYPE_ANALYTICS_QUERY, TENANT_ID_HEADER};

/// Extract + validate the `X-Tenant-Id` header.
///
/// Wave-20 (B-P3-03 closure): the `Err` variant carries a full
/// `axum::response::Response` (>100 bytes) which Clippy flags via
/// `result_large_err`. The waiver is deliberate — the caller chains
/// `let tenant = match parse_tenant_header(...) { Ok(t) => t, Err(r) =>
/// return r };` and the alternative (`ParseTenantError { resp: Response
/// }` newtype) would force every call site to unwrap the newtype before
/// returning. The size cost is bounded by `Response`'s stack-allocated
/// header map (one of the few large-on-stack types in axum), accepted
/// trade-off for call-site ergonomics. Tracked as cosmetic follow-on.
#[allow(clippy::result_large_err)]
pub(super) fn parse_tenant_header(
    headers: &HeaderMap,
) -> Result<Uuid, axum::response::Response> {
    match headers
        .get(TENANT_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Uuid::parse_str)
    {
        Some(Ok(t)) => Ok(t),
        _ => Err(
            (StatusCode::UNAUTHORIZED, "missing or invalid X-Tenant-Id").into_response(),
        ),
    }
}

/// Common rate-limit + emit-on-deny path. Returns `Some(response)` on
/// deny; `None` if the request may proceed.
///
/// **Wave-21 closure (`B-P2-03`):** the bucket `now_ms` is now anchored
/// to [`AuditAnalyticsRouteState::wall_clock`] rather than the query
/// window's `to_ms`. The symmetric closure at
/// [`super::super::audit_export`] (`A-P2-05`) lands the same
/// [`crate::wall_clock::WallClock`] collaborator-swap surface — both
/// routes share the trait so production wiring stays uniform.
///
/// **Wave-24 closure (symmetric `W21-R-P2-01`):** the previous
/// implementation fell back to the query-window `to_ms` when
/// `wall_clock.now_ms() == 0`. That coupled the bucket clock to
/// caller-controlled bytes on the structurally-unreachable pre-epoch
/// branch — the exact pathology `audit_export.rs` closed at wave-23
/// (commit `5203e8b`, audit
/// `specs/_audits/2026-05-16-wave23-cleanup.md` §W21-R-P2-01). The
/// analytics route was scoped OUT of wave-23 and explicitly flagged
/// for a follow-on hygiene sweep — this is that sweep. The saturating
/// branch now fail-CLOSES with HTTP 503 + an
/// `exit_status = "clock_unavailable"` audit row, mirroring the
/// audit-export discipline. The bucket clock NEVER couples to
/// caller-controlled bytes, even on the structurally-unreachable
/// pre-epoch branch.
///
/// Production reachability: `SystemWallClock` saturates to 0 only on
/// pre-1970 wall-clock instants — structurally impossible on any
/// production host (epoch is decades past). The fail-CLOSED 503
/// affects only (a) test fakes deliberately pinned at `unix_ms == 0`
/// and (b) exotic hosts with a pre-epoch system clock, which is an
/// operational failure the audit pipeline SHOULD surface.
pub(super) fn rate_limit_check(
    state: &AuditAnalyticsRouteState,
    tenant: Uuid,
    endpoint: &str,
    from_ms: u64,
    to_ms: u64,
) -> Option<axum::response::Response> {
    let bucket_key = BucketKey::per_tenant_per_endpoint(tenant, "audit.analytics");
    let wall_now_ms = state.wall_clock.now_ms();
    if wall_now_ms == 0 {
        // Fail-CLOSED: emit `clock_unavailable` audit row + 503.
        // The bucket clock MUST NEVER couple to caller-controlled
        // bytes (`to_ms`), even on the structurally-unreachable
        // pre-epoch branch. Mirrors the wave-23 closure of
        // `W21-R-P2-01` on `audit_export.rs`.
        let resp = (StatusCode::SERVICE_UNAVAILABLE, "wall clock unavailable")
            .into_response();
        return Some(emit_or_503(
            state,
            AnalyticsAuditRow::new(
                EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                Some(tenant),
                endpoint.to_string(),
                from_ms,
                to_ms,
                0,
                "clock_unavailable".to_string(),
            ),
            resp,
        ));
    }
    let now_ms = wall_now_ms;
    match state.rate_limiter.try_acquire(tenant, bucket_key, 1, now_ms) {
        Ok(outcome) => match outcome.decision {
            RateLimitDecision::Allow { .. } => None,
            RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                let body = format!("rate-limited; retry after {retry_after_secs}s");
                let mut resp = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                if let Ok(val) = format!("{retry_after_secs}").parse() {
                    resp.headers_mut()
                        .insert(axum::http::header::RETRY_AFTER, val);
                }
                Some(emit_or_503(
                    state,
                    AnalyticsAuditRow::new(
                        EVENT_TYPE_ANALYTICS_QUERY.to_string(),
                        Some(tenant),
                        endpoint.to_string(),
                        from_ms,
                        to_ms,
                        0,
                        "rate_limited".to_string(),
                    ),
                    resp,
                ))
            }
            // `RateLimitDecision` is `#[non_exhaustive]`; any future
            // non-Allow variant fail-CLOSED to 429.
            _ => Some(
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate-limit decision arm not handled",
                )
                    .into_response(),
            ),
        },
        Err(_) => Some(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "rate-limit pipeline failed",
            )
                .into_response(),
        ),
    }
}
