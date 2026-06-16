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
//! - Valid PAT, cache-only tenant (200):
//!   ```text
//!   { "valid": true, "tenant_id": "<uuid>", "plan": "<tier>" }
//!   ```
//! - Valid PAT, tenant with a Runners entitlement (200):
//!   ```text
//!   { "valid": true, "tenant_id": "<uuid>", "plan": "pro", "max_concurrency": 40 }
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
//! # M2 scope — `max_concurrency` (runners seam, ratified)
//!
//! `max_concurrency` is the per-tenant runner concurrency cap. It is a
//! **top-level** integer field, present ONLY when the tenant has a **Runners
//! entitlement**, and absent for cache-only tenants (`skip_serializing_if`).
//!
//! The cap is a **SEPARATE entitlement axis from the cache tier** (runners TL
//! ratified): it is read from the dedicated `runners_entitlement` D1 table by a
//! single keyed lookup (`SELECT max_concurrency FROM runners_entitlement WHERE
//! tenant_id = ?1`, migration 0070), **NOT** derived from the cache-plan ladder.
//! A row present → `Some(row.max_concurrency)`; absent (the table starts empty)
//! → `None`, the field is omitted, and the tenant is treated as cache-only with
//! no Runners entitlement (the fabric rejects the placement — empty table = no
//! cap = reject). The `plan` field stays = the cache tier (informational only)
//! and never feeds the cap. The runners `CoreLinkPlanStore` parses this exact
//! shape.
//!
//! `rate_ceiling_per_min` remains **omitted** (M1) — net-new product data not
//! yet decided; the fabric's `StaticPlans` supplies that cap. The response
//! struct carries it as `Option` with `skip_serializing_if`, so it can be added
//! later WITHOUT a breaking wire change.
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
///
/// CAA-360 #26 — intentional asymmetry vs `admin.rs::TIER_SELECTIONS_TIERS`:
/// this is the **resolve/read** set (8 tiers — it must accept every value the
/// `tenant.tier` CHECK allows, INCLUDING the back-compat `team`/`org` retained
/// by migration 0064 for existing rows). `admin.rs::TIER_SELECTIONS_TIERS` is
/// the narrower **settable/write** ladder (6 — the current product tiers; it
/// deliberately omits the deprecated `team`/`org` so they cannot be assigned
/// anew). settable ⊂ resolvable ⊂ D1-CHECK, so no tier is ever unresolvable.
/// Keep these two in that subset relationship if either changes.
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
    /// Per-tenant runner concurrency cap (M2, runners seam). Present ONLY when
    /// the tenant holds a Runners entitlement; absent for cache-only tenants
    /// (`skip_serializing_if`). Read from the dedicated `runners_entitlement` D1
    /// table ([`runner_concurrency_for_tenant`], migration 0070) — a SEPARATE
    /// entitlement axis from the cache tier, NOT derived from the plan. The
    /// runners `CoreLinkPlanStore` parses this exact field.
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
    ///
    /// `max_concurrency` is supplied by the caller — it is `Some(cap)` ONLY when
    /// the tenant holds a Runners entitlement (the `runners_entitlement` D1 row),
    /// and `None` (field omitted) for cache-only tenants. See
    /// [`runner_concurrency_for_tenant`].
    #[must_use]
    fn valid(tenant_id: String, plan: String, max_concurrency: Option<u32>) -> Self {
        Self {
            valid: true,
            tenant_id: Some(tenant_id),
            plan: Some(plan),
            max_concurrency,
            rate_ceiling_per_min: None,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Runner concurrency cap (M2 — runners seam, ratified)
// ──────────────────────────────────────────────────────────────────────────────

/// SQL: the tenant's runner concurrency cap from the dedicated
/// `runners_entitlement` table (migration 0070). The cap is a SEPARATE
/// entitlement axis from the cache tier — keyed on `tenant_id`, NOT derived
/// from the plan ladder. A row is present ONLY for a tenant that actually holds
/// a Runners entitlement; the table's `CHECK(max_concurrency > 0)` guarantees a
/// present row always carries a real positive cap.
const RUNNERS_ENTITLEMENT_SQL: &str =
    "SELECT max_concurrency FROM runners_entitlement WHERE tenant_id = ?1 LIMIT 1";

/// Resolve the per-tenant runner concurrency cap for the introspection
/// response by querying the `runners_entitlement` D1 table.
///
/// - Row present → `Ok(Some(cap))` (the tenant holds a Runners entitlement).
/// - No row → `Ok(None)` (cache-only tenant; the wire field is omitted and the
///   fabric treats the absent cap as "no Runners entitlement" → reject).
///
/// This is a SEPARATE axis from the cache tier ([`tier_for_tenant`]): the cap
/// comes ONLY from this table, never from the plan.
///
/// # Fail-CLOSED
///
/// A genuine D1 backend fault surfaces as `Err(String)` so the route maps it to
/// **503** rather than guessing a cap (consistent with [`tier_for_tenant`]):
/// the fabric must never be handed a WRONG entitlement. A non-fault "no row"
/// is `Ok(None)`, not an error.
///
/// A stored value outside `u32` (or `≤ 0`, which the table CHECK forbids) is
/// treated as a backend fault (`Err`) — the cap is a hard entitlement, so an
/// out-of-contract row is fail-CLOSED rather than silently truncated.
///
/// # Errors
///
/// Returns `Err(String)` only on a D1 backend fault or an out-of-contract
/// stored value (so the route can 503).
pub async fn runner_concurrency_for_tenant(
    d1: &D1HttpClient,
    tenant_id: &str,
) -> Result<Option<u32>, String> {
    let rows = d1
        .query(
            RUNNERS_ENTITLEMENT_SQL,
            &[serde_json::Value::String(tenant_id.to_owned())],
        )
        .await?;
    decode_runner_cap(&rows)
}

/// Pure decode of the `runners_entitlement` query result into the wire cap.
///
/// Split from [`runner_concurrency_for_tenant`] so the entitlement-row logic is
/// unit-testable without a network: the I/O wrapper does the keyed query, this
/// function maps rows → cap. See [`runner_concurrency_for_tenant`] for the
/// full contract.
///
/// # Errors
///
/// Returns `Err(String)` when the entitlement row carries a `max_concurrency`
/// value that is non-positive or outside `u32` range (out-of-contract — the
/// table's `CHECK(max_concurrency > 0)` should make this impossible, so it is
/// treated as a backend fault and fails CLOSED → 503).
fn decode_runner_cap(rows: &[crate::storage::d1_http::D1Row]) -> Result<Option<u32>, String> {
    let Some(raw) = rows.first().and_then(|row| row.get("max_concurrency")) else {
        // No entitlement row → cache-only tenant → field omitted.
        return Ok(None);
    };

    // The row exists; it MUST carry a positive, u32-range integer (the table's
    // CHECK(max_concurrency > 0) enforces positivity at write time). Anything
    // else is an out-of-contract row → fail-CLOSED (Err → 503), never a guess.
    let cap = raw
        .as_u64()
        .filter(|v| *v >= 1 && *v <= u64::from(u32::MAX))
        .ok_or_else(|| {
            format!("runners_entitlement.max_concurrency out of u32 range or non-positive: {raw}")
        })?;

    // The filter above already bounds `cap` to `1..=u32::MAX`, so this cast is
    // lossless.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "cap is range-checked to 1..=u32::MAX immediately above"
    )]
    Ok(Some(cap as u32))
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
                Ok(plan) => {
                    // M2 (runners seam): the cap is a SEPARATE entitlement axis
                    // — read from `runners_entitlement` (migration 0070), NOT
                    // derived from `plan`. Present ONLY for a tenant with a row;
                    // a cache-only tenant gets the field omitted. A D1 fault
                    // fails CLOSED (503 — never guess a cap).
                    match runner_concurrency_for_tenant(&state.d1, &tenant_id).await {
                        Ok(max_concurrency) => (
                            StatusCode::OK,
                            Json(IntrospectResponse::valid(tenant_id, plan, max_concurrency)),
                        )
                            .into_response(),
                        Err(e) => {
                            tracing::error!(error = %e, "auth_introspect: runner entitlement resolution failed");
                            StatusCode::SERVICE_UNAVAILABLE.into_response()
                        }
                    }
                }
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
        // The cache-only 200 success shape: { valid:true, tenant_id, plan } with
        // the cap fields OMITTED (skip_serializing_if; no Runners entitlement).
        let resp = IntrospectResponse::valid(
            "11111111-1111-1111-1111-111111111111".to_owned(),
            "pro".to_owned(),
            None,
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
            "cache-only: max_concurrency must be omitted"
        );
        assert!(
            !obj.contains_key("rate_ceiling_per_min"),
            "M1: cap field must be omitted"
        );
    }

    #[test]
    fn valid_response_with_entitlement_serialises_max_concurrency() {
        // The runners-entitlement 200 shape: max_concurrency present as a
        // top-level integer (the exact shape the runners CoreLinkPlanStore parses).
        let resp = IntrospectResponse::valid(
            "22222222-2222-2222-2222-222222222222".to_owned(),
            "pro".to_owned(),
            Some(40),
        );
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["valid"], serde_json::json!(true));
        assert_eq!(v["plan"], serde_json::json!("pro"));
        assert_eq!(
            v["max_concurrency"],
            serde_json::json!(40),
            "max_concurrency must be a top-level integer when entitled"
        );
        // rate_ceiling_per_min stays omitted (still M1).
        assert!(!v.as_object().unwrap().contains_key("rate_ceiling_per_min"));
    }

    /// Build a one-row `runners_entitlement` result set carrying the given
    /// `max_concurrency` JSON value (mirrors what `D1HttpClient::query` returns
    /// for `SELECT max_concurrency ...`).
    fn entitlement_rows(max_concurrency: serde_json::Value) -> Vec<crate::storage::d1_http::D1Row> {
        let mut row = serde_json::Map::new();
        row.insert("max_concurrency".to_owned(), max_concurrency);
        vec![row]
    }

    #[test]
    fn decode_runner_cap_present_row_yields_some() {
        // A `runners_entitlement` row → Some(cap), read from the TABLE value
        // (the separate entitlement axis), not any plan ladder.
        assert_eq!(
            decode_runner_cap(&entitlement_rows(serde_json::json!(40))).unwrap(),
            Some(40),
            "an entitlement row must yield the stored cap"
        );
        // Any positive cap passes through verbatim — it is NOT clamped to a
        // ladder. A bespoke per-tenant value is honoured.
        assert_eq!(
            decode_runner_cap(&entitlement_rows(serde_json::json!(7))).unwrap(),
            Some(7)
        );
    }

    #[test]
    fn decode_runner_cap_absent_row_yields_none() {
        // Empty result set (no entitlement) → None → field omitted → cache-only.
        assert_eq!(
            decode_runner_cap(&[]).unwrap(),
            None,
            "no entitlement row must yield None (cache-only; field omitted)"
        );
    }

    #[test]
    fn decode_runner_cap_out_of_contract_row_fails_closed() {
        // The table CHECK forbids these, but if one ever appears the decode
        // fails CLOSED (Err → 503), never a guessed/truncated cap.
        assert!(
            decode_runner_cap(&entitlement_rows(serde_json::json!(0))).is_err(),
            "zero cap is out-of-contract → Err"
        );
        assert!(
            decode_runner_cap(&entitlement_rows(serde_json::json!(-5))).is_err(),
            "negative cap is out-of-contract → Err"
        );
        assert!(
            decode_runner_cap(&entitlement_rows(serde_json::json!(u64::from(u32::MAX) + 1)))
                .is_err(),
            "cap beyond u32 range is out-of-contract → Err"
        );
        assert!(
            decode_runner_cap(&entitlement_rows(serde_json::json!("forty"))).is_err(),
            "non-integer cap is out-of-contract → Err"
        );
    }

    #[test]
    fn decode_runner_cap_accepts_u32_max() {
        assert_eq!(
            decode_runner_cap(&entitlement_rows(serde_json::json!(u32::MAX))).unwrap(),
            Some(u32::MAX),
            "u32::MAX is the inclusive upper bound and must round-trip"
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
