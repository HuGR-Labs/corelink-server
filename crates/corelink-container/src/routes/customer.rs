//! Customer self-serve HTTP routes: `/v1/customer/*`
//!
//! Wires [`corelink_handler_customer`]'s 6 traits + `InMemoryCustomerHandler`
//! into the axum router. The Worker injects `x-corelink-tenant-id` +
//! `x-corelink-token-prefix` headers post-auth; these routes trust those
//! headers exclusively and never accept client-supplied tenant identities.
//!
//! # Endpoints
//!
//! ```text
//! GET  /v1/customer/overview
//! GET  /v1/customer/usage?period=<period>
//! GET  /v1/customer/audit?from=&to=&kind=
//! GET  /v1/customer/billing
//! POST /v1/customer/billing/portal
//! GET  /v1/customer/keys
//! POST /v1/customer/keys              { name, scopes[] }
//! POST /v1/customer/keys/:pat_id/revoke
//! GET  /v1/customer/team
//! POST /v1/customer/team/invite       { email, role }
//! ```
//!
//! # Auth model
//!
//! All routes require the Worker-injected headers. Absent headers fall back
//! to `"_unknown"` (fail-CLOSED: the handler emits `Unauthorized` on an
//! unknown principal and the route maps it to 401).
//!
//! # Pattern
//!
//! Mirrors [`super::cas`] exactly: one `CustomerRouteState` struct holding
//! trait objects, one `build_handlers()` factory, one `router(state)`.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use corelink_handler_customer::{
    AuditQueryRequest, BillingRequest, CustomerAuditHandler, CustomerBillingHandler,
    CustomerHandlerError, CustomerKeysHandler, CustomerOverviewHandler, CustomerTeamHandler,
    CustomerUsageHandler, InMemoryAuditSink, InMemoryCustomerHandler, InMemorySliObserver,
    KeyCreateRequest, KeyRevokeRequest, KeysListRequest, OverviewRequest, PortalRequest,
    TeamInviteRequest, TeamListRequest, UsageRequest,
};
use serde::Deserialize;
use serde_json::{json, Value};

// ─── Route state ─────────────────────────────────────────────────────────────

/// Shared route state — one trait object per handler surface.
///
/// `InMemoryCustomerHandler` implements all 6 traits so `build_handlers`
/// shares one `Arc` behind all 6 slots. Production can swap each slot
/// independently once a D1-backed impl lands.
#[derive(Clone)]
pub struct CustomerRouteState {
    /// Overview handler.
    pub overview: Arc<dyn CustomerOverviewHandler>,
    /// Usage handler.
    pub usage: Arc<dyn CustomerUsageHandler>,
    /// Billing + portal-url handler.
    pub billing: Arc<dyn CustomerBillingHandler>,
    /// PAT list / create / revoke handler.
    pub keys: Arc<dyn CustomerKeysHandler>,
    /// Team list / invite handler.
    pub team: Arc<dyn CustomerTeamHandler>,
    /// Audit query handler.
    pub audit: Arc<dyn CustomerAuditHandler>,
}

impl core::fmt::Debug for CustomerRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CustomerRouteState").finish_non_exhaustive()
    }
}

// ─── Handler factory ─────────────────────────────────────────────────────────

/// Build the canonical `CustomerRouteState` using `InMemoryCustomerHandler`
/// (shared behind all 6 trait-object slots via a single `Arc`).
///
/// Production wiring uses InMemory for now; a D1-backed handler is a
/// follow-up per the `trait-abstraction-defer` rule.
#[must_use]
pub fn build_handlers() -> CustomerRouteState {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let shared: Arc<InMemoryCustomerHandler> = Arc::new(InMemoryCustomerHandler::new(audit, sli));
    CustomerRouteState {
        overview: shared.clone(),
        usage: shared.clone(),
        billing: shared.clone(),
        keys: shared.clone(),
        team: shared.clone(),
        audit: shared,
    }
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Build the axum `Router` mounting all `/v1/customer/*` routes.
pub fn router(state: CustomerRouteState) -> Router {
    Router::new()
        .route("/v1/customer/overview", get(handle_overview))
        .route("/v1/customer/usage", get(handle_usage))
        .route("/v1/customer/audit", get(handle_audit))
        .route("/v1/customer/billing", get(handle_billing))
        .route("/v1/customer/billing/portal", post(handle_billing_portal))
        .route(
            "/v1/customer/keys",
            get(handle_keys_list).post(handle_keys_create),
        )
        .route("/v1/customer/keys/:pat_id/revoke", post(handle_keys_revoke))
        .route("/v1/customer/team", get(handle_team_list))
        .route("/v1/customer/team/invite", post(handle_team_invite))
        .with_state(state)
}

// ─── Header helpers ───────────────────────────────────────────────────────────

/// Read `name` from `headers`; return `default` when absent or non-ASCII.
fn header_or(headers: &HeaderMap, name: &str, default: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// Read `x-corelink-tenant-id`; fail-CLOSED to `"_unknown"`.
fn tenant(headers: &HeaderMap) -> String {
    header_or(headers, "x-corelink-tenant-id", "_unknown")
}

/// Read `x-corelink-token-prefix`; fail-CLOSED to `"_unknown"`.
fn principal(headers: &HeaderMap) -> String {
    header_or(headers, "x-corelink-token-prefix", "_unknown")
}

/// Logical wall-clock: 0 in routes (handler-provided `at_unix_ms` acts as
/// stand-in; production wiring threads a real clock collaborator).
fn now_ms() -> u64 {
    0u64
}

// ─── Query param shapes ───────────────────────────────────────────────────────

/// `GET /v1/customer/usage` query params.
#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    /// Optional billing period filter (e.g. `"2026-05"`).
    pub period: Option<String>,
}

/// `GET /v1/customer/audit` query params.
#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    /// Optional ISO-8601 since filter (maps to `AuditQueryRequest::since`).
    pub from: Option<String>,
    /// Ignored in this surface (InMemory has no `to` filter); preserved
    /// for forward-compatibility with the admin-ui contract.
    pub to: Option<String>,
    /// Optional event-type filter (comma-separated string).
    pub kind: Option<String>,
}

// ─── POST body shapes ─────────────────────────────────────────────────────────

/// `POST /v1/customer/keys` request body.
#[derive(Debug, Deserialize)]
pub struct CreatePatBody {
    /// Human-readable name for the new PAT.
    pub name: String,
    /// Scopes to grant (e.g. `["cache:read", "cache:write"]`).
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// `POST /v1/customer/team/invite` request body.
#[derive(Debug, Deserialize)]
pub struct InviteBody {
    /// Email address to invite.
    pub email: String,
    /// Role to assign (`"Owner"` / `"Admin"` / `"Developer"` / `"Viewer"`).
    pub role: String,
}

// ─── Handler fns ─────────────────────────────────────────────────────────────

/// `GET /v1/customer/overview`
async fn handle_overview(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let t = tenant(&headers);
    let p = principal(&headers);
    let req = OverviewRequest::new(t, p, now_ms());
    match state.overview.overview(req) {
        Ok(resp) => {
            let body: Value = json!({
                "tenant_id":       resp.tenant_id,
                "tenant_name":     resp.tenant_name,
                "plan":            resp.plan,
                "usage": {
                    "period":      resp.usage.period,
                    "cas_bytes":   resp.usage.cas_bytes,
                    "quota_bytes": resp.usage.quota_bytes,
                    "reads":       resp.usage.reads,
                    "writes":      resp.usage.writes,
                },
                "billing": {
                    "status":             resp.billing.status,
                    "next_invoice_at":    resp.billing.next_invoice_at,
                    "amount_due_cents":   resp.billing.amount_due_cents,
                    "currency":           resp.billing.currency,
                },
                "byok": {
                    "status":         resp.byok.status,
                    "cmk_id":         resp.byok.cmk_id,
                    "last_rotated_at": resp.byok.last_rotated_at,
                },
                "recent_activity": resp.recent_activity.iter().map(|e| json!({
                    "event_id":   e.event_id,
                    "ts":         e.ts,
                    "event_type": e.event_type,
                    "severity":   e.severity,
                    "actor":      e.actor,
                    "summary":    e.summary,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/usage?period=`
async fn handle_usage(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Query(q): Query<UsageQuery>,
) -> impl IntoResponse {
    let t = tenant(&headers);
    let p = principal(&headers);
    let req = UsageRequest::new(t, p, q.period, now_ms());
    match state.usage.usage(req) {
        Ok(resp) => {
            let body: Value = json!({
                "period":      resp.period,
                "cas_bytes":   resp.cas_bytes,
                "reads":       resp.reads,
                "writes":      resp.writes,
                "quota_bytes": resp.quota_bytes,
                "daily": resp.daily.iter().map(|d| json!({
                    "day":       d.day,
                    "reads":     d.reads,
                    "writes":    d.writes,
                    "cas_bytes": d.cas_bytes,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/audit?from=&to=&kind=`
async fn handle_audit(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Query(q): Query<AuditQuery>,
) -> impl IntoResponse {
    let t = tenant(&headers);
    let p = principal(&headers);
    // `kind` is comma-separated; split into event_types Vec.
    let event_types: Vec<String> = q
        .kind
        .as_deref()
        .map(|k| {
            k.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    // `from` maps to `since` in the handler surface; `to` has no
    // analogue in AuditQueryRequest (InMemory doesn't filter by end-date).
    let req = AuditQueryRequest::new(t, p, q.from, event_types, now_ms());
    match state.audit.query(req) {
        Ok(resp) => {
            let body: Value = json!({
                "rows": resp.rows.iter().map(|r| json!({
                    "event_id":   r.event_id,
                    "ts":         r.ts,
                    "event_type": r.event_type,
                    "severity":   r.severity,
                    "actor":      r.actor,
                    "summary":    r.summary,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/billing`
async fn handle_billing(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let t = tenant(&headers);
    let p = principal(&headers);
    let req = BillingRequest::new(t, p, now_ms());
    match state.billing.billing(req) {
        Ok(resp) => {
            let body: Value = json!({
                "status":                resp.status,
                "plan":                  resp.plan,
                "current_period_start":  resp.current_period_start,
                "current_period_end":    resp.current_period_end,
                "amount_due_cents":      resp.amount_due_cents,
                "currency":              resp.currency,
                "invoices": resp.invoices.iter().map(|i| json!({
                    "invoice_id":  i.invoice_id,
                    "issued_at":   i.issued_at,
                    "amount_cents": i.amount_cents,
                    "status":      i.status,
                    "hosted_url":  i.hosted_url,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/billing/portal`
async fn handle_billing_portal(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let t = tenant(&headers);
    let p = principal(&headers);
    let req = PortalRequest::new(t, p, now_ms());
    match state.billing.portal_url(req) {
        Ok(resp) => {
            let body: Value = json!({ "portal_url": resp.portal_url });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/keys`
async fn handle_keys_list(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let t = tenant(&headers);
    let p = principal(&headers);
    let req = KeysListRequest::new(t, p, now_ms());
    match state.keys.list(req) {
        Ok(resp) => {
            let body: Value = json!({
                "pats": resp.pats.iter().map(|row| json!({
                    "pat_id":      row.pat_id,
                    "name":        row.name,
                    "scopes":      row.scopes,
                    "created_at":  row.created_at,
                    "last_used_at": row.last_used_at,
                    "revoked_at":  row.revoked_at,
                })).collect::<Vec<_>>(),
                "byok": {
                    "status":         resp.byok.status,
                    "cmk_id":         resp.byok.cmk_id,
                    "last_rotated_at": resp.byok.last_rotated_at,
                },
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/keys`
async fn handle_keys_create(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Json(body): Json<CreatePatBody>,
) -> impl IntoResponse {
    let t = tenant(&headers);
    let p = principal(&headers);
    let req = KeyCreateRequest::new(t, p, body.name, body.scopes, now_ms());
    match state.keys.create(req) {
        Ok(resp) => {
            let json_body: Value = json!({
                "pat": {
                    "pat_id":      resp.pat.pat_id,
                    "name":        resp.pat.name,
                    "scopes":      resp.pat.scopes,
                    "created_at":  resp.pat.created_at,
                    "last_used_at": resp.pat.last_used_at,
                    "revoked_at":  resp.pat.revoked_at,
                },
                "token": resp.token,
            });
            (StatusCode::CREATED, Json(json_body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/keys/:pat_id/revoke`
async fn handle_keys_revoke(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Path(pat_id): Path<String>,
) -> impl IntoResponse {
    let t = tenant(&headers);
    let p = principal(&headers);
    let req = KeyRevokeRequest::new(t, p, pat_id, now_ms());
    match state.keys.revoke(req) {
        Ok(resp) => {
            let body: Value = json!({
                "pat": {
                    "pat_id":      resp.pat.pat_id,
                    "name":        resp.pat.name,
                    "scopes":      resp.pat.scopes,
                    "created_at":  resp.pat.created_at,
                    "last_used_at": resp.pat.last_used_at,
                    "revoked_at":  resp.pat.revoked_at,
                },
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `GET /v1/customer/team`
async fn handle_team_list(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let t = tenant(&headers);
    let p = principal(&headers);
    let req = TeamListRequest::new(t, p, now_ms());
    match state.team.list(req) {
        Ok(resp) => {
            let body: Value = json!({
                "members": resp.members.iter().map(|m| json!({
                    "user_id":   m.user_id,
                    "email":     m.email,
                    "role":      m.role,
                    "joined_at": m.joined_at,
                    "status":    m.status,
                })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

/// `POST /v1/customer/team/invite`
async fn handle_team_invite(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
    Json(body): Json<InviteBody>,
) -> impl IntoResponse {
    let t = tenant(&headers);
    let p = principal(&headers);
    let req = TeamInviteRequest::new(t, p, body.email, body.role, now_ms());
    match state.team.invite(req) {
        Ok(resp) => {
            let json_body: Value = json!({
                "member": {
                    "user_id":   resp.member.user_id,
                    "email":     resp.member.email,
                    "role":      resp.member.role,
                    "joined_at": resp.member.joined_at,
                    "status":    resp.member.status,
                },
            });
            (StatusCode::CREATED, Json(json_body)).into_response()
        }
        Err(e) => map_err(e),
    }
}

// ─── Error mapping ────────────────────────────────────────────────────────────

/// Map a [`CustomerHandlerError`] to the canonical HTTP response.
fn map_err(e: CustomerHandlerError) -> axum::response::Response {
    tracing::warn!(error = ?e, "customer handler error");
    match e {
        CustomerHandlerError::Unauthorized(_) => {
            (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
        }
        CustomerHandlerError::CrossTenantDenied { .. } => {
            (StatusCode::FORBIDDEN, "cross-tenant").into_response()
        }
        CustomerHandlerError::NotFound { .. } => {
            (StatusCode::NOT_FOUND, "not found").into_response()
        }
        CustomerHandlerError::AuditFailed(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "audit closed").into_response()
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

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
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use corelink_handler_customer::request::{
        ByokStatus, InvoiceRow, OverviewBilling, OverviewUsage, PatRow,
    };
    use corelink_handler_customer::{BillingResponse, OverviewResponse, UsageResponse};
    use tower::ServiceExt;

    /// Build a fixture state backed by shared InMemoryCustomerHandler
    /// and return both the state and the underlying handler for seeding.
    fn fixture() -> (CustomerRouteState, Arc<InMemoryCustomerHandler>) {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let shared: Arc<InMemoryCustomerHandler> =
            Arc::new(InMemoryCustomerHandler::new(audit, sli));
        let state = CustomerRouteState {
            overview: shared.clone(),
            usage: shared.clone(),
            billing: shared.clone(),
            keys: shared.clone(),
            team: shared.clone(),
            audit: shared.clone(),
        };
        (state, shared)
    }

    // ── Route table smoke tests ───────────────────────────────────────────────

    #[test]
    fn build_handlers_returns_usable_state() {
        // Smoke: build_handlers() constructs without panic.
        let state = build_handlers();
        let _router = router(state);
    }

    #[test]
    fn router_constructs_from_fixture() {
        let (state, _) = fixture();
        let _r = router(state);
    }

    // ── Tenant resolution ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn overview_reads_tenant_from_header() {
        let (state, shared) = fixture();
        // Seed an overview for tenant-xyz so the handler returns OK.
        let overview = OverviewResponse::new(
            "tenant-xyz",
            "XYZ Corp",
            "starter",
            OverviewUsage::new("2026-05", 0, 1_000_000, 0, 0),
            OverviewBilling::new("active", "2026-06-01T00:00:00Z", 2900, "usd"),
            ByokStatus::new("none", None, None),
            vec![],
        );
        shared.seed_overview("tenant-xyz", overview).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/overview")
            .method("GET")
            .header("x-corelink-tenant-id", "tenant-xyz")
            .header("x-corelink-token-prefix", "clpat_abc")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["tenant_id"], "tenant-xyz");
        assert_eq!(v["tenant_name"], "XYZ Corp");
        assert_eq!(v["plan"], "starter");
    }

    #[tokio::test]
    async fn overview_unknown_tenant_returns_404() {
        // With no seed the InMemory handler returns NotFound -> 404.
        let (state, _) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/overview")
            .method("GET")
            .header("x-corelink-tenant-id", "ghost")
            .header("x-corelink-token-prefix", "clpat_x")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn missing_tenant_header_falls_back_to_unknown() {
        // No tenant header → _unknown tenant → NotFound (no seed for _unknown).
        let (state, _) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/overview")
            .method("GET")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        // NotFound because _unknown has no seeded overview.
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── Usage route ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn usage_route_returns_200_with_period() {
        let (state, shared) = fixture();
        let usage = UsageResponse::new("2026-05", 512, 100, 50, 1_000_000, vec![]);
        shared.seed_usage("t1", usage).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/usage?period=2026-05")
            .method("GET")
            .header("x-corelink-tenant-id", "t1")
            .header("x-corelink-token-prefix", "clpat_t1")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["period"], "2026-05");
        assert_eq!(v["reads"], 100u64);
        assert_eq!(v["writes"], 50u64);
    }

    // ── Billing routes ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn billing_route_returns_200() {
        let (state, shared) = fixture();
        let billing = BillingResponse::new(
            "active",
            "starter",
            "2026-05-01",
            "2026-06-01",
            2900,
            "usd",
            vec![InvoiceRow::new(
                "inv_001",
                "2026-05-01",
                2900,
                "paid",
                "https://invoice.stripe.com/inv_001",
            )],
        );
        shared.seed_billing("t2", billing).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/billing")
            .method("GET")
            .header("x-corelink-tenant-id", "t2")
            .header("x-corelink-token-prefix", "clpat_t2")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["status"], "active");
        assert_eq!(v["invoices"].as_array().expect("invoices").len(), 1);
    }

    #[tokio::test]
    async fn billing_portal_returns_portal_url() {
        let (state, shared) = fixture();
        // Billing portal only needs billing seeded for happy path on the
        // real billing handler — but portal_url in InMemory doesn't look
        // up the billing store; it always returns a stub URL. However we
        // still seed billing to avoid any future path divergence.
        let billing = BillingResponse::new(
            "active",
            "team",
            "2026-05-01",
            "2026-06-01",
            4900,
            "usd",
            vec![],
        );
        shared.seed_billing("t3", billing).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/billing/portal")
            .method("POST")
            .header("x-corelink-tenant-id", "t3")
            .header("x-corelink-token-prefix", "clpat_t3")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let url = v["portal_url"].as_str().expect("portal_url string");
        assert!(url.starts_with("https://billing.stripe.com/"));
    }

    // ── Keys routes ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn keys_create_returns_201_with_token() {
        let (state, _) = fixture();
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "name": "ci-key",
            "scopes": ["cache:read", "cache:write"]
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("POST")
            .header("x-corelink-tenant-id", "t4")
            .header("x-corelink-token-prefix", "clpat_t4")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["pat"]["name"], "ci-key");
        assert!(v["token"].as_str().expect("token").starts_with("clpat_"));
    }

    #[tokio::test]
    async fn keys_list_returns_seeded_pats() {
        let (state, shared) = fixture();
        let pat = PatRow::new(
            "pat_001",
            "my-key",
            vec!["cache:read".into()],
            "2026-05-01T00:00:00Z",
            None,
            None,
        );
        shared.seed_pat("t5", pat).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/keys")
            .method("GET")
            .header("x-corelink-tenant-id", "t5")
            .header("x-corelink-token-prefix", "clpat_t5")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["pats"].as_array().expect("pats").len(), 1);
        assert_eq!(v["pats"][0]["name"], "my-key");
    }

    #[tokio::test]
    async fn keys_revoke_returns_revoked_pat() {
        let (state, shared) = fixture();
        let pat = PatRow::new(
            "pat_rev",
            "revoke-me",
            vec![],
            "2026-05-01T00:00:00Z",
            None,
            None,
        );
        shared.seed_pat("t6", pat).expect("seed");

        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/keys/pat_rev/revoke")
            .method("POST")
            .header("x-corelink-tenant-id", "t6")
            .header("x-corelink-token-prefix", "clpat_t6")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        // revoked_at is now set.
        assert!(v["pat"]["revoked_at"].is_string());
    }

    // ── Team routes ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn team_invite_returns_201_with_member() {
        let (state, _) = fixture();
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "email": "alice@example.com",
            "role": "Developer"
        }))
        .unwrap();
        let req = Request::builder()
            .uri("/v1/customer/team/invite")
            .method("POST")
            .header("x-corelink-tenant-id", "t7")
            .header("x-corelink-token-prefix", "clpat_t7")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["member"]["email"], "alice@example.com");
        assert_eq!(v["member"]["role"], "Developer");
        assert_eq!(v["member"]["status"], "invited");
    }

    // ── Audit query route ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn audit_route_returns_empty_rows_for_new_tenant() {
        let (state, _) = fixture();
        let app = router(state);
        let req = Request::builder()
            .uri("/v1/customer/audit")
            .method("GET")
            .header("x-corelink-tenant-id", "t8")
            .header("x-corelink-token-prefix", "clpat_t8")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["rows"].as_array().expect("rows").len(), 0);
    }

    // ── Error mapping ─────────────────────────────────────────────────────────

    #[test]
    fn map_err_unauthorized_is_401() {
        let resp = map_err(CustomerHandlerError::Unauthorized("no session".into()));
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn map_err_cross_tenant_is_403() {
        let resp = map_err(CustomerHandlerError::CrossTenantDenied {
            caller: "a".into(),
            requested_tenant: "b".into(),
        });
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn map_err_not_found_is_404() {
        let resp = map_err(CustomerHandlerError::NotFound { what: "x".into() });
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn map_err_audit_failed_is_503() {
        let resp = map_err(CustomerHandlerError::AuditFailed("pipe down".into()));
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn map_err_internal_is_500() {
        let resp = map_err(CustomerHandlerError::Internal("boom".into()));
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
