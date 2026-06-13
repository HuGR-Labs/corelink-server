//! `POST /internal/v1/auth/introspect` — PAT introspection endpoint for the
//! **corelink-runners fabric** (M1).
//!
//! # Why this exists
//!
//! The runners fabric is handed an inbound `Bearer` PAT per job and must
//! resolve the owning **tenant** and the tenant's **plan** before it places
//! work onto a runner. Rather than re-implement PAT verification (HMAC +
//! Argon2id + D1 liveness) in the fabric, the fabric calls THIS endpoint and
//! receives a frozen, minimal introspection result. The container is the one
//! place that already owns the full verification pipeline
//! ([`crate::adapter_pat::PatVerifier`]).
//!
//! # Security model
//!
//! - This route is **NOT** reachable from the public internet. It is mounted
//!   on the container's HTTP listener (port 50051), reached only via the
//!   Cloudflare Durable Object / fabric forwarder.
//! - **Caller auth (fail-CLOSED):** every request must carry an
//!   `X-Corelink-Internal-Auth` header whose value matches a **DEDICATED**
//!   secret, [`build_state_from_env`]'s `FABRIC_INTROSPECT_AUTH_KEY` — NOT the
//!   Worker↔container `CORELINK_INTERNAL_AUTH_KEY`. A separate secret keeps the
//!   blast radius tight: a leak of the fabric secret cannot mint PATs, and a
//!   leak of the mint secret cannot introspect. The compare reuses the exact
//!   constant-time, length-padded gate from
//!   [`crate::routes::internal_pat::internal_auth_ok`] — it is NOT reinvented
//!   here.
//! - If `FABRIC_INTROSPECT_AUTH_KEY` is absent or shorter than 32 chars, the
//!   route is **NOT mounted** ([`build_state_from_env`] returns `None`, warn
//!   log) — the same fail-CLOSED posture as `internal_pat`.
//!
//! # Request shape
//!
//! ```text
//! POST /internal/v1/auth/introspect
//! X-Corelink-Internal-Auth: <FABRIC_INTROSPECT_AUTH_KEY>
//! Content-Type: application/json
//!
//! { "token": "corelink_pat_..." }
//! ```
//!
//! # Response shapes
//!
//! - Valid PAT (200):
//!   ```text
//!   { "valid": true, "tenant_id": "<uuid>", "plan": "<tier>" }
//!   ```
//! - Invalid PAT (200, uniform — NO oracle on *why* and NO tenant_id):
//!   ```text
//!   { "valid": false }
//!   ```
//! - Verifier backend fault / tier-query fault (503, no body contract). The
//!   fabric maps 503 → `Err(Unreachable)` and never serves a plan it could not
//!   resolve (fail-CLOSED — never serve a wrong plan).
//! - Missing / wrong service secret (401).
//!
//! # M1 scope
//!
//! `max_concurrency` and `rate_ceiling_per_min` are **omitted** at M1 — they are
//! net-new product data not yet decided; the fabric's `StaticPlans` supplies
//! caps until a cap table is agreed. The response struct carries them as
//! `Option` with `skip_serializing_if`, so they can be added later WITHOUT a
//! breaking wire change.
//!
//! # Hard rules
//!
//! - The `token` is NEVER logged.
//! - On `valid: false` no reason is given (uniform with `VerifyError`).
//! - `tenant_id` is present ONLY when `valid: true`.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::adapter_pat::{PatVerifier, VerifyError};
use crate::routes::internal_pat::internal_auth_ok;
use crate::storage::d1_http::D1HttpClient;

/// The canonical tier wire strings (snake_case) accepted from D1, mirroring
/// `corelink_tier_selection::tier::TierKind::as_str` and the worker
/// `isValidTier` allow-list in `worker/src/lib/quota.ts`. A D1 value not in
/// this set is treated as absent and falls through to the next lookup tier.
const VALID_TIERS: [&str; 8] = [
    "free",
    "solo",
    "starter",
    "team",
    "pro",
    "org",
    "max",
    "enterprise",
];

/// The hard-default tier when no active subscription and no tenant tier row
/// resolves — the most restrictive plan (mirrors `getTierForTenant`'s default).
const DEFAULT_TIER: &str = "free";

// ──────────────────────────────────────────────────────────────────────────────
// State
// ──────────────────────────────────────────────────────────────────────────────

/// Route state injected at boot time.
#[derive(Clone)]
pub struct AuthIntrospectRouteState {
    /// Dedicated shared secret for the `X-Corelink-Internal-Auth` header —
    /// sourced from `FABRIC_INTROSPECT_AUTH_KEY` (NOT the mint secret).
    internal_auth_key: Arc<str>,
    /// The full container-side PAT verification pipeline (Option B).
    verifier: Arc<PatVerifier>,
    /// D1 HTTP client used to resolve the tenant's effective tier.
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for AuthIntrospectRouteState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthIntrospectRouteState")
            .field("internal_auth_key", &"[REDACTED]")
            .field("verifier", &self.verifier)
            .field("d1", &"Arc<D1HttpClient>")
            .finish()
    }
}

impl AuthIntrospectRouteState {
    /// Construct from explicit collaborators (used by the production wiring and
    /// by tests).
    #[must_use]
    pub fn new(
        internal_auth_key: Arc<str>,
        verifier: Arc<PatVerifier>,
        d1: Arc<D1HttpClient>,
    ) -> Self {
        Self {
            internal_auth_key,
            verifier,
            d1,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Request / response shapes
// ──────────────────────────────────────────────────────────────────────────────

/// JSON request body. The `token` is the inbound Bearer PAT plaintext.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntrospectRequest {
    /// The PAT plaintext to introspect (e.g. `corelink_pat_...`). NEVER logged.
    pub token: String,
}

/// JSON response body. Forward-compatible: `tenant_id` / `plan` are present
/// only on `valid: true`, and the M1-omitted cap fields are skipped when
/// `None` so they can be added later without a breaking change.
#[derive(Debug, Serialize, Deserialize)]
pub struct IntrospectResponse {
    /// Whether the PAT verified.
    pub valid: bool,
    /// Owning tenant UUID — present ONLY when `valid` is `true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// The tenant's effective plan (tier wire string) — present ONLY when
    /// `valid` is `true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// Per-tenant concurrency cap. OMITTED at M1 (the fabric's `StaticPlans`
    /// supplies caps until a cap table is agreed); reserved for forward
    /// compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<u32>,
    /// Per-tenant request rate ceiling (per minute). OMITTED at M1; reserved
    /// for forward compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_ceiling_per_min: Option<u32>,
}

impl IntrospectResponse {
    /// The uniform `valid: false` response — no tenant_id, no plan, no reason.
    #[must_use]
    fn invalid() -> Self {
        Self {
            valid: false,
            tenant_id: None,
            plan: None,
            max_concurrency: None,
            rate_ceiling_per_min: None,
        }
    }

    /// A `valid: true` response carrying the resolved tenant + plan.
    #[must_use]
    fn valid(tenant_id: String, plan: String) -> Self {
        Self {
            valid: true,
            tenant_id: Some(tenant_id),
            plan: Some(plan),
            max_concurrency: None,
            rate_ceiling_per_min: None,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tier resolution (additive Rust mirror of getTierForTenant)
// ──────────────────────────────────────────────────────────────────────────────

/// SQL: the tenant's tier from an ACTIVE subscription row. Mirrors the worker
/// `getTierForTenant` step 1 — only `subscription_state = 'active'` grants its
/// paid tier (a `pending_checkout` row must NOT be served paid quota).
const TIER_SELECTION_SQL: &str =
    "SELECT tier FROM tier_selections WHERE tenant_id = ?1 AND subscription_state = 'active' LIMIT 1";

/// SQL: the tenant's default tier column (migration 0057, `DEFAULT 'free'`).
const TENANT_TIER_SQL: &str = "SELECT tier FROM tenant WHERE tenant_id = ?1 LIMIT 1";

/// Resolve the effective tier (plan) for a tenant — an additive Rust mirror of
/// `worker/src/lib/quota.ts::getTierForTenant`.
///
/// Lookup order:
///   1. `tier_selections.tier` WHERE `subscription_state = 'active'`.
///   2. `tenant.tier` (default column).
///   3. Hard default `"free"`.
///
/// A D1 value not in the canonical tier set is ignored (fall through), mirroring
/// the worker's `isValidTier` filter.
///
/// # Fail-CLOSED difference from the worker
///
/// The worker fails *open* (returns `"free"` on a D1 error) because its DO has a
/// deeper quota FSM as a safety net. The fabric path has no such net and must
/// never be handed a WRONG plan, so this helper returns `Err` on a genuine D1
/// fault; the route maps that to **503** rather than serving a guessed tier.
///
/// # Errors
///
/// Returns `Err(String)` only on a D1 backend fault (so the route can 503).
pub async fn tier_for_tenant(d1: &D1HttpClient, tenant_id: &str) -> Result<String, String> {
    // ── 1. Active subscription (canonical) ──────────────────────────────────
    let active_rows = d1
        .query(
            TIER_SELECTION_SQL,
            &[serde_json::Value::String(tenant_id.to_owned())],
        )
        .await?;
    if let Some(tier) = active_rows
        .into_iter()
        .next()
        .and_then(|row| row.get("tier").and_then(|v| v.as_str()).map(str::to_owned))
    {
        if is_valid_tier(&tier) {
            return Ok(tier);
        }
    }

    // ── 2. tenant.tier default column ───────────────────────────────────────
    let tenant_rows = d1
        .query(
            TENANT_TIER_SQL,
            &[serde_json::Value::String(tenant_id.to_owned())],
        )
        .await?;
    if let Some(tier) = tenant_rows
        .into_iter()
        .next()
        .and_then(|row| row.get("tier").and_then(|v| v.as_str()).map(str::to_owned))
    {
        if is_valid_tier(&tier) {
            return Ok(tier);
        }
    }

    // ── 3. Hard default ─────────────────────────────────────────────────────
    Ok(DEFAULT_TIER.to_owned())
}

/// Whether a D1 tier string is one of the canonical wire tiers.
#[must_use]
fn is_valid_tier(value: &str) -> bool {
    VALID_TIERS.contains(&value)
}

// ──────────────────────────────────────────────────────────────────────────────
// Route handler
// ──────────────────────────────────────────────────────────────────────────────

/// Build the introspection router. Mount at the top level so
/// `/internal/v1/auth/introspect` is directly addressable.
pub fn router(state: AuthIntrospectRouteState) -> Router {
    Router::new()
        .route("/internal/v1/auth/introspect", post(handle_introspect))
        .with_state(state)
}

/// `POST /internal/v1/auth/introspect` handler.
///
/// The auth gate runs FIRST, on raw [`Bytes`] (the `HeaderMap` is a
/// `FromRequestParts` extractor, so it is evaluated before the body is
/// buffered/parsed): an unauthenticated caller is rejected with 401 WITHOUT the
/// body ever being JSON-parsed (denying parse CPU/heap to an attacker), exactly
/// as in `internal_pat`.
async fn handle_introspect(
    State(state): State<AuthIntrospectRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // ── 1. Dedicated-secret gate (constant-time; reused gate) ───────────────
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    // ── 1b. Parse the body — ONLY after the auth gate passed ────────────────
    let req: IntrospectRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            // Never log the token; the body is rejected on shape only.
            tracing::warn!(error = %e, "auth_introspect: invalid request body");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_body" })),
            )
                .into_response();
        }
    };

    // ── 2. Verify the PAT (HMAC + D1 liveness + Argon2id + scope) ───────────
    match state.verifier.verify(&req.token).await {
        Ok(tenant_id) => {
            // ── 3. Resolve the plan. A tier-query fault → 503 (fail-CLOSED:
            //       never serve a wrong plan). ──────────────────────────────
            match tier_for_tenant(&state.d1, &tenant_id).await {
                Ok(plan) => (
                    StatusCode::OK,
                    Json(IntrospectResponse::valid(tenant_id, plan)),
                )
                    .into_response(),
                Err(e) => {
                    tracing::error!(error = %e, "auth_introspect: tier resolution failed");
                    StatusCode::SERVICE_UNAVAILABLE.into_response()
                }
            }
        }
        // Uniform invalid — no tenant_id, no reason.
        Err(VerifyError::InvalidPat) => {
            (StatusCode::OK, Json(IntrospectResponse::invalid())).into_response()
        }
        // Genuine backend fault — the fabric maps 503 → Err(Unreachable).
        Err(VerifyError::Backend(e)) => {
            tracing::error!(error = %e, "auth_introspect: verifier backend fault");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// State builder
// ──────────────────────────────────────────────────────────────────────────────

/// Minimum length (chars) of the dedicated fabric introspection secret.
const MIN_FABRIC_AUTH_KEY_LEN: usize = 32;

/// Build the route state from env at binary boot.
///
/// - `FABRIC_INTROSPECT_AUTH_KEY` — DEDICATED shared secret for the auth-header
///   gate (NOT `CORELINK_INTERNAL_AUTH_KEY`). Must be ≥ 32 chars. Absent / too
///   short → returns `None` (route NOT mounted; warn log) — fail-CLOSED.
/// - The PAT verifier + D1 client are built from the same env as the cache
///   adapters via [`PatVerifier::from_env`] / [`D1HttpClient`]; absent →
///   `None`.
///
/// Returns `None` when any required input is missing/invalid; the caller logs a
/// warning and skips mounting the route (dev/CI without secrets).
#[must_use]
pub fn build_state_from_env() -> Option<AuthIntrospectRouteState> {
    let auth_key = std::env::var("FABRIC_INTROSPECT_AUTH_KEY").ok()?;
    if auth_key.len() < MIN_FABRIC_AUTH_KEY_LEN {
        tracing::warn!(
            "FABRIC_INTROSPECT_AUTH_KEY absent or too short (< 32 chars); \
             /internal/v1/auth/introspect route NOT mounted"
        );
        return None;
    }

    let verifier = PatVerifier::from_env()?;

    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = D1HttpClient::new(&storage_env)
        .map_err(|e| {
            tracing::warn!(error = %e, "auth_introspect: D1 client init failed; route NOT mounted");
        })
        .ok()?;

    Some(AuthIntrospectRouteState::new(
        Arc::from(auth_key.as_str()),
        Arc::new(verifier),
        Arc::new(d1),
    ))
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{self, Request};
    use corelink_pat::{
        mint, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId, SCOPE_CACHE_RW,
    };
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;
    use crate::adapter_pat::{PatRow, PatRowLookup};

    const TEST_AUTH_KEY: &str = "fabric-introspect-test-key-32-chars!";

    /// A fake `PatRowLookup` that returns a pre-seeded row (or a forced backend
    /// error) — drives the real crypto pipeline without a network.
    struct FakeLookup {
        rows: HashMap<String, PatRow>,
        backend_err: Option<String>,
    }

    impl FakeLookup {
        fn with_row(token_id: &str, row: PatRow) -> Self {
            let mut rows = HashMap::new();
            rows.insert(token_id.to_owned(), row);
            Self {
                rows,
                backend_err: None,
            }
        }
        fn backend(err: &str) -> Self {
            Self {
                rows: HashMap::new(),
                backend_err: Some(err.to_owned()),
            }
        }
    }

    #[async_trait]
    impl PatRowLookup for FakeLookup {
        async fn lookup(&self, token_id: &str) -> Result<Option<PatRow>, String> {
            if let Some(e) = &self.backend_err {
                return Err(e.clone());
            }
            Ok(self.rows.get(token_id).cloned())
        }
    }

    fn test_key() -> Arc<PatSigningKey> {
        Arc::new(PatSigningKey::from_bytes(vec![0x42u8; 32]).unwrap())
    }

    /// Mint a real PAT → `(plaintext, token_id, pat_hash, tenant_string)`.
    fn mint_pat(key: &PatSigningKey, tenant: u128) -> (String, String, String, String) {
        let tenant_id = TenantId(Uuid::from_u128(tenant));
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            tenant_id,
            PrincipalId(Uuid::from_u128(tenant + 1000)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            key,
            1,
        )
        .unwrap();
        (
            plaintext.into_string(),
            pat.token_id.as_str().to_owned(),
            pat.hash.as_str().to_owned(),
            pat.tenant_id.0.to_string(),
        )
    }

    /// A `PatRow` for a freshly-minted PAT.
    fn row_for(hash: &str, tenant: &str) -> PatRow {
        PatRow {
            tenant_id: tenant.to_owned(),
            pat_hash: hash.to_owned(),
            scope: "cas:rw".to_owned(),
        }
    }

    /// A D1 client that points at an unroutable host — every `query` fails,
    /// modelling a D1 backend fault for the 503 tier-query path.
    fn unreachable_d1() -> Arc<D1HttpClient> {
        let env = crate::storage::StorageEnv {
            r2_endpoint: "http://127.0.0.1:1".to_owned(),
            r2_access_key_id: "x".to_owned(),
            r2_secret_access_key: "x".to_owned(),
            cloudflare_account_id: "acct".to_owned(),
            cf_api_token: "tok".to_owned(),
            d1_database_id: "db".to_owned(),
        };
        Arc::new(D1HttpClient::new(&env).unwrap())
    }

    fn state_with(verifier: Arc<PatVerifier>, d1: Arc<D1HttpClient>) -> AuthIntrospectRouteState {
        AuthIntrospectRouteState::new(Arc::from(TEST_AUTH_KEY), verifier, d1)
    }

    fn introspect_request(auth: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder()
            .method(http::Method::POST)
            .uri("/internal/v1/auth/introspect")
            .header("content-type", "application/json");
        if let Some(a) = auth {
            b = b.header("x-corelink-internal-auth", a);
        }
        b.body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
        if bytes.is_empty() {
            return serde_json::Value::Null;
        }
        serde_json::from_slice(&bytes).unwrap()
    }

    // ── Caller auth ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn missing_service_secret_returns_401() {
        let verifier = Arc::new(PatVerifier::new(
            Arc::new(FakeLookup::with_row("x", row_for("h", "t"))),
            test_key(),
        ));
        let app = router(state_with(verifier, unreachable_d1()));
        let req = introspect_request(None, serde_json::json!({ "token": "corelink_pat_x" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_service_secret_returns_401() {
        let verifier =
            Arc::new(PatVerifier::new(Arc::new(FakeLookup::backend("unused")), test_key()));
        let app = router(state_with(verifier, unreachable_d1()));
        let req = introspect_request(
            Some("wrong-secret-which-is-also-32-chars!!"),
            serde_json::json!({ "token": "corelink_pat_x" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── Valid PAT → {valid, tenant_id, plan} ─────────────────────────────────

    #[tokio::test]
    async fn valid_pat_then_d1_fault_returns_503_failclosed() {
        // A PAT that VERIFIES (the Ok(tenant) arm) but whose plan cannot be
        // resolved (D1 unreachable) must 503 — never a guessed plan.
        let key = test_key();
        let (pt, tid, hash, tenant) = mint_pat(&key, 42);
        let lookup = Arc::new(FakeLookup::with_row(&tid, row_for(&hash, &tenant)));
        let verifier = Arc::new(PatVerifier::new(lookup, key));
        let app = router(state_with(verifier, unreachable_d1()));
        let req = introspect_request(Some(TEST_AUTH_KEY), serde_json::json!({ "token": pt }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "verified PAT + D1 tier-query fault must 503 (never serve a guessed plan)"
        );
    }

    #[test]
    fn valid_response_shape_serialises() {
        // The exact 200 success shape: { valid:true, tenant_id, plan } with the
        // M1 cap fields OMITTED (skip_serializing_if).
        let resp = IntrospectResponse::valid(
            "11111111-1111-1111-1111-111111111111".to_owned(),
            "pro".to_owned(),
        );
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["valid"], serde_json::json!(true));
        assert_eq!(
            v["tenant_id"],
            serde_json::json!("11111111-1111-1111-1111-111111111111")
        );
        assert_eq!(v["plan"], serde_json::json!("pro"));
        let obj = v.as_object().unwrap();
        assert!(
            !obj.contains_key("max_concurrency"),
            "M1: cap field must be omitted"
        );
        assert!(
            !obj.contains_key("rate_ceiling_per_min"),
            "M1: cap field must be omitted"
        );
    }

    // ── Invalid PAT → {valid:false} ──────────────────────────────────────────

    #[tokio::test]
    async fn invalid_pat_returns_valid_false_no_tenant() {
        // Forged token (bad HMAC) → InvalidPat → 200 { valid:false }, no oracle.
        let verifier =
            Arc::new(PatVerifier::new(Arc::new(FakeLookup::backend("unused")), test_key()));
        let app = router(state_with(verifier, unreachable_d1()));
        let req = introspect_request(
            Some(TEST_AUTH_KEY),
            serde_json::json!({ "token": "corelink_pat_not-a-real-token" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["valid"], serde_json::json!(false));
        let obj = v.as_object().unwrap();
        assert!(!obj.contains_key("tenant_id"), "no tenant_id on valid:false");
        assert!(!obj.contains_key("plan"), "no plan on valid:false");
    }

    #[test]
    fn invalid_response_shape_serialises() {
        let v = serde_json::to_value(IntrospectResponse::invalid()).unwrap();
        assert_eq!(v, serde_json::json!({ "valid": false }));
    }

    // ── Backend error → 503 ──────────────────────────────────────────────────

    #[tokio::test]
    async fn verifier_backend_error_returns_503() {
        // A valid-HMAC token that reaches D1, where the lookup faults → Backend
        // → 503 (the fabric maps this to Err(Unreachable)).
        let key = test_key();
        let (pt, _tid, _hash, _tenant) = mint_pat(&key, 7);
        let verifier = Arc::new(PatVerifier::new(
            Arc::new(FakeLookup::backend("d1 unreachable")),
            key,
        ));
        let app = router(state_with(verifier, unreachable_d1()));
        let req = introspect_request(Some(TEST_AUTH_KEY), serde_json::json!({ "token": pt }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // ── Route NOT mounted when the dedicated secret is absent ────────────────

    #[test]
    fn build_state_returns_none_when_secret_absent() {
        // FABRIC_INTROSPECT_AUTH_KEY is not set in the test process → None.
        // (Guard against a polluted env so the assertion is meaningful.)
        if std::env::var("FABRIC_INTROSPECT_AUTH_KEY").is_err() {
            assert!(build_state_from_env().is_none());
        }
    }

    // ── tier_for_tenant: D1 fault surfaces as Err (→ route 503) ──────────────

    #[tokio::test]
    async fn tier_for_tenant_errors_on_d1_fault() {
        let d1 = unreachable_d1();
        let err = tier_for_tenant(&d1, "11111111-1111-1111-1111-111111111111").await;
        assert!(
            err.is_err(),
            "a D1 fault must surface as Err (fail-CLOSED → 503)"
        );
    }

    #[test]
    fn is_valid_tier_matches_canonical_set() {
        for t in [
            "free", "solo", "starter", "team", "pro", "org", "max", "enterprise",
        ] {
            assert!(is_valid_tier(t), "{t} should be valid");
        }
        assert!(!is_valid_tier("platinum"));
        assert!(!is_valid_tier(""));
    }
}
