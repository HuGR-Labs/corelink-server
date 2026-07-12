//! `GET /v1/admin/tenants/:tenant_id/{usage,billing,consents,dsr,pats}` —
//! operator-scoped, per-tenant READ enrichment for the Admin "Tenant
//! deep-dive" surface (`apps/admin-ui/src/components/admin/TenantDeepDive.tsx`).
//!
//! # Why this module exists
//!
//! The admin-ui deep-dive renders five cards (usage, billing, consents,
//! DSR queue, active PATs). Before this module, no per-tenant enrichment
//! endpoint existed, so every card rendered a permanent `EmptyState`.
//! These endpoints back those cards with real D1 rows.
//!
//! # Auth model — OPERATOR scope (NOT customer scope)
//!
//! Unlike [`super::customer`] — which trusts the Worker-injected
//! `x-corelink-tenant-id` header and can ONLY ever read the caller's OWN
//! tenant — this surface is OPERATOR-scoped: it targets an EXPLICIT
//! `:tenant_id` path parameter (any tenant) and is gated exactly like
//! [`super::admin`]'s read/mutate plane: the operator-only
//! `x-corelink-internal-auth` shared secret (`CORELINK_ADMIN_AUTH_KEY` →
//! shared `CORELINK_INTERNAL_AUTH_KEY` fallback). It is NOT reachable by
//! any authenticated tenant PAT — the Worker strips
//! `x-corelink-internal-auth` on the public `/v1/*` path, and the
//! constant-time gate below ([`super::admin::internal_auth_ok`]) is a
//! second independent layer. Absent/wrong/unconfigured secret → 403
//! BEFORE any storage access (fail-CLOSED).
//!
//! # Read-only
//!
//! Every route is a `GET`; no mutation happens here. Sensitive mutations
//! stay on [`super::admin`]'s dual-approval `POST /v1/admin/mutate`.
//!
//! # Data sources (all existing tables — no new migration)
//!
//! | Card     | Table(s) read                                   |
//! |----------|-------------------------------------------------|
//! | usage    | `tenant_storage_state`, `monthly_request_counts`|
//! | billing  | `tenant_billing`, `tier_selections`             |
//! | consents | `dpa_acceptances`                               |
//! | dsr      | `dsr_requested`                                 |
//! | pats     | `pat` (active only; NEVER the secret)           |
//!
//! Fields with no honest D1 source (usage `cas_hit_ratio` / `gb_egress`,
//! billing `mrr_usd` / `payment_method_last4`, consents `revoked`, DSR
//! `in_progress`) are returned as documented zero/absent values rather
//! than invented — see each handler.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};

use super::admin::{internal_auth_key_from_env, internal_auth_ok};
use crate::customer_d1::ms_to_iso8601;

#[cfg(not(target_arch = "wasm32"))]
use crate::storage::{d1_http::D1HttpClient, StorageEnv};

/// Bytes-per-gigabyte (decimal GB, matching the customer usage surface's
/// storage accounting).
const BYTES_PER_GB: f64 = 1_000_000_000.0;

/// Shared route state for the operator per-tenant read plane.
#[derive(Clone)]
pub struct AdminTenantDetailState {
    /// D1 HTTP client (real per-tenant rows). `None` in dev/CI when no
    /// storage credentials are configured — every handler then returns
    /// 503 (fail-CLOSED: an operator read surface never fabricates data).
    #[cfg(not(target_arch = "wasm32"))]
    db: Option<Arc<D1HttpClient>>,
    /// Operator-only shared secret for the `x-corelink-internal-auth`
    /// gate (sourced from `CORELINK_ADMIN_AUTH_KEY` → shared
    /// `CORELINK_INTERNAL_AUTH_KEY` fallback). `None` when unset at boot
    /// → every handler fails CLOSED (403).
    internal_auth_key: Option<Arc<str>>,
}

impl core::fmt::Debug for AdminTenantDetailState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AdminTenantDetailState")
            .finish_non_exhaustive()
    }
}

impl AdminTenantDetailState {
    /// Build the production state from process env: a [`D1HttpClient`]
    /// over `CONFIG_DB` (when `StorageEnv` is present) plus the operator
    /// admin auth key. Mirrors [`super::admin::build_handlers`]'s env-gate
    /// and fallback posture: absent credentials → `db = None` (dev/CI), so
    /// the handlers fail CLOSED (503) rather than serving fabricated rows.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn from_env() -> Self {
        let db = match StorageEnv::from_env() {
            Some(env) => match D1HttpClient::new(&env) {
                Ok(client) => {
                    tracing::info!("Admin tenant-detail: D1 (real storage)");
                    Some(Arc::new(client))
                }
                Err(e) => {
                    tracing::error!(error = %e, "Admin tenant-detail: D1 build failed → 503");
                    None
                }
            },
            None => {
                tracing::info!(
                    "Admin tenant-detail: no storage credentials configured (dev/CI → 503)"
                );
                None
            }
        };
        Self {
            db,
            internal_auth_key: internal_auth_key_from_env(),
        }
    }
}

/// Build the axum `Router` exposing the five operator per-tenant read
/// routes. Mounted by `routes.rs` alongside [`super::admin::router`].
pub fn router(state: AdminTenantDetailState) -> Router {
    Router::new()
        .route("/v1/admin/tenants/{tenant_id}/usage", get(handle_usage))
        .route("/v1/admin/tenants/{tenant_id}/billing", get(handle_billing))
        .route(
            "/v1/admin/tenants/{tenant_id}/consents",
            get(handle_consents),
        )
        .route("/v1/admin/tenants/{tenant_id}/dsr", get(handle_dsr))
        .route("/v1/admin/tenants/{tenant_id}/pats", get(handle_pats))
        .with_state(state)
}

/// Operator gate + storage-presence check shared by every handler.
///
/// Returns `Ok(db)` when the request cleared the constant-time
/// `x-corelink-internal-auth` gate AND a D1 client is wired; otherwise
/// `Err(resp)` carrying the canonical 403 (bad/absent secret) or 503
/// (storage unconfigured) fail-CLOSED response.
///
/// The `Err` payload is an axum `Response` (large); the
/// `result_large_err` waiver matches the sibling gate helpers
/// (`admin_pilot.rs`, `audit_analytics::rate_limit`) — the caller
/// immediately `return`s it, so no large value is threaded further.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::result_large_err)]
fn gate<'a>(
    state: &'a AdminTenantDetailState,
    headers: &HeaderMap,
) -> Result<&'a Arc<D1HttpClient>, axum::response::Response> {
    if !internal_auth_ok(state.internal_auth_key.as_ref(), headers) {
        tracing::warn!(
            event = "AdminTenantDetailUnauthorized",
            "admin tenant-detail read rejected: missing/invalid \
             x-corelink-internal-auth (fail-CLOSED)"
        );
        return Err((StatusCode::FORBIDDEN, "forbidden").into_response());
    }
    state.db.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "tenant-detail storage not configured",
        )
            .into_response()
    })
}

/// Map a D1 transport/decode error string to a 500 (the operator surface
/// never leaks the raw D1 message to the client — it is logged instead).
#[cfg(not(target_arch = "wasm32"))]
fn d1_err(context: &str, e: &str) -> axum::response::Response {
    tracing::error!(context, error = %e, "admin tenant-detail D1 read failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response()
}

/// `GET /v1/admin/tenants/:tenant_id/usage`
///
/// FE shape (`TenantDeepDive` `usage`): `{ cas_hit_ratio, gb_stored,
/// gb_egress }`.
///
/// - `gb_stored` — REAL: `SUM(tenant_storage_state.bytes_used)` / 1e9
///   (the exact source the customer usage surface reads).
/// - `cas_hit_ratio` / `gb_egress` — returned as `0.0`: CoreLink does not
///   meter CAS hit/miss or per-tenant egress in D1 (BLOCKED — no source).
/// - `request_count` — extra honest field (`SUM(monthly_request_counts.
///   request_count)`); the card ignores it but it satisfies the
///   "cas_bytes + request counts" contract without inventing a ratio.
#[cfg(not(target_arch = "wasm32"))]
async fn handle_usage(
    State(state): State<AdminTenantDetailState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> impl IntoResponse {
    let db = match gate(&state, &headers) {
        Ok(db) => db,
        Err(resp) => return resp,
    };
    let bytes_used = match db
        .query(
            "SELECT COALESCE(SUM(bytes_used), 0) AS bytes_used \
             FROM tenant_storage_state WHERE tenant_id = ?1",
            &[json!(tenant_id)],
        )
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .next()
            .and_then(|r| r.get("bytes_used").and_then(Value::as_i64))
            .unwrap_or(0),
        Err(e) => return d1_err("usage.bytes_used", &e),
    };
    let request_count = match db
        .query(
            "SELECT COALESCE(SUM(request_count), 0) AS c \
             FROM monthly_request_counts WHERE tenant_id = ?1",
            &[json!(tenant_id)],
        )
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .next()
            .and_then(|r| r.get("c").and_then(Value::as_i64))
            .unwrap_or(0),
        // monthly_request_counts is optional accounting; a read fault here
        // must not blank the (real) gb_stored card — degrade to 0.
        Err(e) => {
            tracing::warn!(error = %e, "usage.request_count read failed; defaulting to 0");
            0
        }
    };
    #[allow(clippy::cast_precision_loss)]
    let gb_stored = bytes_used.max(0) as f64 / BYTES_PER_GB;
    let body = json!({
        // REAL source (tenant_storage_state).
        "gb_stored": gb_stored,
        // BLOCKED — CoreLink does not meter CAS hit/miss or egress in D1.
        "cas_hit_ratio": 0.0,
        "gb_egress": 0.0,
        // Honest extra (card ignores it): cas_bytes + request counts.
        "cas_bytes": bytes_used,
        "request_count": request_count,
    });
    (StatusCode::OK, Json(body)).into_response()
}

/// `GET /v1/admin/tenants/:tenant_id/billing`
///
/// FE shape (`TenantDeepDive` `billing`): `{ plan, mrr_usd, next_invoice,
/// payment_method_last4? }`.
///
/// - `plan` — REAL: `tier_selections.tier` (human label, e.g. `pro`),
///   falling back to `tenant_billing.plan` (Stripe price id) when no tier
///   row exists.
/// - `next_invoice` — REAL: `tenant_billing.current_period_end_ms` → ISO
///   date (empty string when unset / no billing row).
/// - `mrr_usd` — `0.0`: no MRR/amount column exists in `tenant_billing`
///   (BLOCKED — Stripe holds the amount; not mirrored to D1).
/// - `payment_method_last4` — OMITTED (optional): the PAN last4 is never
///   mirrored to D1 (Stripe-only). BLOCKED.
#[cfg(not(target_arch = "wasm32"))]
async fn handle_billing(
    State(state): State<AdminTenantDetailState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> impl IntoResponse {
    let db = match gate(&state, &headers) {
        Ok(db) => db,
        Err(resp) => return resp,
    };
    // tier_selections carries the human-readable tier; tenant_billing the
    // Stripe plan id + period end. Read both (LEFT-ish: either may be absent).
    let tier = match db
        .query(
            "SELECT tier FROM tier_selections WHERE tenant_id = ?1 LIMIT 1",
            &[json!(tenant_id)],
        )
        .await
    {
        Ok(rows) => rows
            .into_iter()
            .next()
            .and_then(|r| r.get("tier").and_then(Value::as_str).map(str::to_owned)),
        Err(e) => return d1_err("billing.tier", &e),
    };
    let (plan_from_billing, period_end_ms) = match db
        .query(
            "SELECT plan, current_period_end_ms \
             FROM tenant_billing WHERE tenant_id = ?1 LIMIT 1",
            &[json!(tenant_id)],
        )
        .await
    {
        Ok(rows) => rows.into_iter().next().map_or((None, None), |r| {
            let plan = r.get("plan").and_then(Value::as_str).map(str::to_owned);
            let end = r.get("current_period_end_ms").and_then(Value::as_i64);
            (plan, end)
        }),
        Err(e) => return d1_err("billing.tenant_billing", &e),
    };
    let plan = tier.or(plan_from_billing).unwrap_or_default();
    let next_invoice = period_end_ms.map(ms_to_iso8601).unwrap_or_default();
    let body = json!({
        "plan": plan,               // REAL (tier_selections / tenant_billing)
        "next_invoice": next_invoice, // REAL (tenant_billing.current_period_end_ms)
        "mrr_usd": 0.0,             // BLOCKED — no amount column in D1
        // payment_method_last4 intentionally omitted (Stripe-only; BLOCKED)
    });
    (StatusCode::OK, Json(body)).into_response()
}

/// `GET /v1/admin/tenants/:tenant_id/consents`
///
/// FE shape (`TenantDeepDive` `consents`): `{ granted, revoked,
/// last_capture }`, from `dpa_acceptances`.
///
/// - `granted` — REAL: `COUNT(*)` of the tenant's DPA acceptances.
/// - `last_capture` — REAL: `MAX(accepted_at)` → ISO (empty when none).
/// - `revoked` — `0`: `dpa_acceptances` is append-only; CoreLink has no
///   consent-revocation record (BLOCKED — no revoke source).
#[cfg(not(target_arch = "wasm32"))]
async fn handle_consents(
    State(state): State<AdminTenantDetailState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> impl IntoResponse {
    let db = match gate(&state, &headers) {
        Ok(db) => db,
        Err(resp) => return resp,
    };
    let (granted, last_ms) = match db
        .query(
            "SELECT COUNT(*) AS granted, COALESCE(MAX(accepted_at), 0) AS last_capture \
             FROM dpa_acceptances WHERE tenant_id = ?1",
            &[json!(tenant_id)],
        )
        .await
    {
        Ok(rows) => rows.into_iter().next().map_or((0, 0), |r| {
            let granted = r.get("granted").and_then(Value::as_i64).unwrap_or(0);
            let last = r.get("last_capture").and_then(Value::as_i64).unwrap_or(0);
            (granted, last)
        }),
        Err(e) => return d1_err("consents", &e),
    };
    let last_capture = if last_ms > 0 {
        ms_to_iso8601(last_ms)
    } else {
        String::new()
    };
    let body = json!({
        "granted": granted,          // REAL (dpa_acceptances COUNT)
        "revoked": 0,                // BLOCKED — no revocation record exists
        "last_capture": last_capture, // REAL (MAX accepted_at)
    });
    (StatusCode::OK, Json(body)).into_response()
}

/// `GET /v1/admin/tenants/:tenant_id/dsr`
///
/// FE shape (`TenantDeepDive` `dsr`): `{ pending, in_progress, completed }`,
/// from `dsr_requested` (`status IN ('requested','verified')`).
///
/// - `pending` — REAL: rows with `status = 'requested'`.
/// - `completed` — REAL: rows with `status = 'verified'`.
/// - `in_progress` — `0`: `dsr_requested` has no intermediate status
///   (BLOCKED — the lifecycle is requested → verified only).
#[cfg(not(target_arch = "wasm32"))]
async fn handle_dsr(
    State(state): State<AdminTenantDetailState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> impl IntoResponse {
    let db = match gate(&state, &headers) {
        Ok(db) => db,
        Err(resp) => return resp,
    };
    let rows = match db
        .query(
            "SELECT status, COUNT(*) AS c FROM dsr_requested \
             WHERE tenant_id = ?1 GROUP BY status",
            &[json!(tenant_id)],
        )
        .await
    {
        Ok(rows) => rows,
        Err(e) => return d1_err("dsr", &e),
    };
    let (mut pending, mut completed) = (0i64, 0i64);
    for r in rows {
        let count = r.get("c").and_then(Value::as_i64).unwrap_or(0);
        match r.get("status").and_then(Value::as_str) {
            Some("requested") => pending = count,
            Some("verified") => completed = count,
            _ => {}
        }
    }
    let body = json!({
        "pending": pending,      // REAL (status = 'requested')
        "in_progress": 0,        // BLOCKED — no such status in dsr_requested
        "completed": completed,  // REAL (status = 'verified')
    });
    (StatusCode::OK, Json(body)).into_response()
}

/// `GET /v1/admin/tenants/:tenant_id/pats`
///
/// FE shape (`TenantDeepDive` `pats`): `Array<{ pat_id, scope,
/// created_at }>` — ACTIVE PATs only (`revoked_at_ms IS NULL`).
///
/// SECURITY: only `pat_id` / `name` / `scope` / `created_ms` /
/// `last_used_at` columns are read — the PAT SECRET (Argon2id hash /
/// HMAC) is NEVER selected or returned.
#[cfg(not(target_arch = "wasm32"))]
async fn handle_pats(
    State(state): State<AdminTenantDetailState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> impl IntoResponse {
    let db = match gate(&state, &headers) {
        Ok(db) => db,
        Err(resp) => return resp,
    };
    let rows = match db
        .query(
            "SELECT pat_id, name, scope, created_ms FROM pat \
             WHERE tenant_id = ?1 AND revoked_at_ms IS NULL \
             ORDER BY created_ms DESC",
            &[json!(tenant_id)],
        )
        .await
    {
        Ok(rows) => rows,
        Err(e) => return d1_err("pats", &e),
    };
    let pats: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let pat_id = r
                .get("pat_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let scope = r
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let created_at = r
                .get("created_ms")
                .and_then(Value::as_i64)
                .map(ms_to_iso8601)
                .unwrap_or_default();
            // name is read as an honest extra (card renders pat_id + scope +
            // created_at); the secret is never touched.
            let name = r
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            json!({ "pat_id": pat_id, "name": name, "scope": scope, "created_at": created_at })
        })
        .collect();
    (StatusCode::OK, Json(json!({ "pats": pats }))).into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{self, Request};
    use tower::ServiceExt;

    /// A state with a configured auth key but NO db — exercises the router
    /// construction + the gate ordering (403 before the 503 storage check).
    fn state_no_db() -> AdminTenantDetailState {
        AdminTenantDetailState {
            db: None,
            internal_auth_key: Some(Arc::from("test-internal-auth-key-32-bytes-x")),
        }
    }

    #[test]
    fn router_builds() {
        let _ = router(state_no_db());
    }

    /// Fail-CLOSED: a request with NO internal-auth header is 403, and the
    /// gate fires BEFORE the storage-presence (503) check.
    #[tokio::test]
    async fn unauthenticated_is_403() {
        let app = router(state_no_db());
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/v1/admin/tenants/t1/usage")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// Wrong secret → 403 (constant-time gate rejects), never 503/200.
    #[tokio::test]
    async fn wrong_secret_is_403() {
        let app = router(state_no_db());
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/v1/admin/tenants/t1/pats")
            .header("x-corelink-internal-auth", "wrong")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// Correct secret but db unwired → 503 (fail-CLOSED, never fabricates).
    #[tokio::test]
    async fn authed_without_storage_is_503() {
        let app = router(state_no_db());
        for card in ["usage", "billing", "consents", "dsr", "pats"] {
            let req = Request::builder()
                .method(http::Method::GET)
                .uri(format!("/v1/admin/tenants/t1/{card}"))
                .header(
                    "x-corelink-internal-auth",
                    "test-internal-auth-key-32-bytes-x",
                )
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::SERVICE_UNAVAILABLE,
                "card {card} must fail CLOSED (503) when storage is unwired"
            );
        }
    }

    /// Unconfigured auth key → every request fails CLOSED (403) regardless
    /// of the header supplied.
    #[tokio::test]
    async fn unconfigured_key_fails_closed() {
        let state = AdminTenantDetailState {
            db: None,
            internal_auth_key: None,
        };
        let app = router(state);
        let req = Request::builder()
            .method(http::Method::GET)
            .uri("/v1/admin/tenants/t1/dsr")
            .header("x-corelink-internal-auth", "anything")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
