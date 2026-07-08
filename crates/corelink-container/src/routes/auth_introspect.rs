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
//! - **Multiple consumers, isolated secrets:** additional consumers each carry
//!   their OWN dedicated key so a compromised consumer can never present (nor
//!   leak the blast radius of) another's credential. The primary
//!   `FABRIC_INTROSPECT_AUTH_KEY` is the corelink-runners fabric; optional
//!   `FABRIC_INTROSPECT_AUTH_KEY_HUGR` (≥ 32 chars, else ignored with a warn) is
//!   the HuGR toolkits fleet. The gate checks the header against EVERY configured
//!   key without short-circuiting, so the timing reveals no consumer identity.
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
//!   { "valid": true, "tenant_id": "<uuid>", "plan": "pro", "max_concurrency": 40, "max_vcpu_h": 240 }
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
//! `max_vcpu_h` (the per-tenant monthly vCPU-hour ceiling, migration 0072) is a
//! SECOND additive field on the SAME `runners_entitlement` lookup
//! (`SELECT max_concurrency, max_vcpu_h FROM runners_entitlement WHERE
//! tenant_id = ?1`). It carries the deliberate **asymmetry** vs
//! `max_concurrency`: an absent `max_vcpu_h` ⇒ **wall-off** (the fabric enforces
//! no monthly compute cap and lets the job through), NOT a reject. Present →
//! `Some(vcpu_h)`; the column is NULLABLE so existing rows back-fill to
//! `None`/absent (wall-off).
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
    /// Accepted shared secrets for the `X-Corelink-Internal-Auth` header — one per
    /// distinct CONSUMER, each an independently-rotatable secret so a compromised
    /// consumer can never present (or share the blast radius of) another's
    /// credential. Always non-empty: index 0 is the primary
    /// `FABRIC_INTROSPECT_AUTH_KEY` (the corelink-runners fabric); additional
    /// entries are other consumers (e.g. the HuGR toolkits fleet via
    /// `FABRIC_INTROSPECT_AUTH_KEY_HUGR`).
    internal_auth_keys: Vec<Arc<str>>,
    /// The full container-side PAT verification pipeline (Option B).
    verifier: Arc<PatVerifier>,
    /// D1 HTTP client used to resolve the tenant's effective tier.
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for AuthIntrospectRouteState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthIntrospectRouteState")
            // count only — never the values
            .field(
                "internal_auth_keys",
                &format_args!("[{} key(s), REDACTED]", self.internal_auth_keys.len()),
            )
            .field("verifier", &self.verifier)
            .field("d1", &"Arc<D1HttpClient>")
            .finish()
    }
}

impl AuthIntrospectRouteState {
    /// Construct from explicit collaborators (used by the production wiring and
    /// by tests). Seeds the accepted-key set with the single primary key; use
    /// [`with_auth_key`](Self::with_auth_key) to register additional consumers.
    #[must_use]
    pub fn new(
        internal_auth_key: Arc<str>,
        verifier: Arc<PatVerifier>,
        d1: Arc<D1HttpClient>,
    ) -> Self {
        Self {
            internal_auth_keys: vec![internal_auth_key],
            verifier,
            d1,
        }
    }

    /// Register an ADDITIONAL accepted service-auth key for a distinct consumer
    /// (e.g. the HuGR toolkits fleet). Each consumer holds its own secret, so a
    /// compromised consumer cannot authenticate as — nor leak the credential of —
    /// any other. Caller is responsible for the ≥ [`MIN_FABRIC_AUTH_KEY_LEN`]
    /// length floor (enforced by [`build_state_from_env`] for env-sourced keys).
    #[must_use]
    pub fn with_auth_key(mut self, key: Arc<str>) -> Self {
        self.internal_auth_keys.push(key);
        self
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
    /// Per-tenant monthly vCPU-hour ceiling (vCPU-hours; runners seam, migration
    /// 0072). Read from the SAME `runners_entitlement` D1 row as
    /// [`max_concurrency`](Self::max_concurrency) — a SEPARATE additive field on
    /// that lookup, NOT derived from the plan. Present ONLY when the entitlement
    /// row carries a (NULLABLE) `max_vcpu_h` value; absent
    /// (`skip_serializing_if`) when the column is NULL. Note the intentional
    /// asymmetry vs `max_concurrency`: an absent `max_vcpu_h` ⇒ **wall-off** (the
    /// fabric enforces no monthly compute cap), NOT a reject. The runners
    /// `CoreLinkPlanStore` parses this exact field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_vcpu_h: Option<u32>,
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
            max_vcpu_h: None,
            rate_ceiling_per_min: None,
        }
    }

    /// A `valid: true` response carrying the resolved tenant + plan.
    ///
    /// `max_concurrency` and `max_vcpu_h` are supplied by the caller — each is
    /// `Some(..)` ONLY when the tenant's `runners_entitlement` D1 row carries
    /// that (per-field NULLABLE) value, and `None` (field omitted) otherwise. See
    /// [`runner_concurrency_for_tenant`]. Note their asymmetric semantics on the
    /// wire: absent `max_concurrency` ⇒ reject, absent `max_vcpu_h` ⇒ wall-off.
    #[must_use]
    fn valid(
        tenant_id: String,
        plan: String,
        max_concurrency: Option<u32>,
        max_vcpu_h: Option<u32>,
    ) -> Self {
        Self {
            valid: true,
            tenant_id: Some(tenant_id),
            plan: Some(plan),
            max_concurrency,
            max_vcpu_h,
            rate_ceiling_per_min: None,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Runner entitlement: concurrency cap + monthly vCPU-hour ceiling
// (M2 — runners seam, ratified; vCPU-h added migration 0072)
// ──────────────────────────────────────────────────────────────────────────────

/// SQL: the tenant's runner entitlement from the dedicated `runners_entitlement`
/// table — both the `max_concurrency` cap (migration 0070) and the monthly
/// `max_vcpu_h` ceiling (migration 0072), read in one keyed lookup. The
/// entitlement is a SEPARATE axis from the cache tier — keyed on `tenant_id`,
/// NOT derived from the plan ladder. A row is present ONLY for a tenant that
/// actually holds a Runners entitlement; the table's `CHECK(max_concurrency > 0)`
/// guarantees a present row always carries a real positive concurrency cap.
/// `max_vcpu_h` is NULLABLE (it may be absent on a row that still has a
/// concurrency cap — the asymmetry: absent ⇒ wall-off).
const RUNNERS_ENTITLEMENT_SQL: &str =
    "SELECT max_concurrency, max_vcpu_h FROM runners_entitlement WHERE tenant_id = ?1 LIMIT 1";

/// Resolve the per-tenant runner entitlement for the introspection response by
/// querying the `runners_entitlement` D1 table — returns
/// `(max_concurrency, max_vcpu_h)`.
///
/// - Row present → `Ok((Some(cap), max_vcpu_h))` (the tenant holds a Runners
///   entitlement). `max_vcpu_h` is `Some(..)` only if that NULLABLE column is set.
/// - No row → `Ok((None, None))` (cache-only tenant; both wire fields omitted —
///   the fabric treats the absent concurrency cap as "no Runners entitlement"
///   → reject).
///
/// This is a SEPARATE axis from the cache tier ([`tier_for_tenant`]): the
/// entitlement comes ONLY from this table, never from the plan.
///
/// # Wire asymmetry
///
/// An absent `max_concurrency` ⇒ reject; an absent `max_vcpu_h` ⇒ wall-off
/// (the fabric enforces no monthly compute cap). Both map to `None`/omitted here.
///
/// # Fail-CLOSED
///
/// A genuine D1 backend fault surfaces as `Err(String)` so the route maps it to
/// **503** rather than guessing an entitlement (consistent with
/// [`tier_for_tenant`]): the fabric must never be handed a WRONG entitlement. A
/// non-fault "no row" is `Ok((None, None))`, not an error.
///
/// A stored value outside `u32` (or, for `max_concurrency`, `≤ 0` which the
/// table CHECK forbids) is treated as a backend fault (`Err`) — the entitlement
/// is hard, so an out-of-contract row is fail-CLOSED rather than truncated.
///
/// # Errors
///
/// Returns `Err(String)` only on a D1 backend fault or an out-of-contract
/// stored value (so the route can 503).
pub async fn runner_concurrency_for_tenant(
    d1: &D1HttpClient,
    tenant_id: &str,
) -> Result<(Option<u32>, Option<u32>), String> {
    let rows = d1
        .query(
            RUNNERS_ENTITLEMENT_SQL,
            &[serde_json::Value::String(tenant_id.to_owned())],
        )
        .await?;
    let max_concurrency = decode_runner_cap(&rows)?;
    let max_vcpu_h = decode_runner_vcpu_h(&rows)?;
    Ok((max_concurrency, max_vcpu_h))
}

/// Pure decode of the `runners_entitlement` query result into the concurrency cap.
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

/// Pure decode of the `runners_entitlement` query result into the monthly
/// vCPU-hour ceiling (`max_vcpu_h`, migration 0072).
///
/// Mirrors [`decode_runner_cap`] but reads the `max_vcpu_h` column, which is
/// NULLABLE (and has no CHECK). The asymmetry vs the concurrency cap is on the
/// WIRE, not in the decode: a missing column / SQL NULL maps to `Ok(None)` (⇒
/// the field is omitted ⇒ the fabric walls off — proceeds with no monthly cap).
/// `Ok(None)` therefore covers BOTH "no entitlement row at all" AND "row present
/// but `max_vcpu_h` is NULL".
///
/// A PRESENT, non-null value is still validated as a positive `u32` and fails
/// CLOSED (`Err` → 503) if out-of-contract — the wire type is `u32` (vCPU-hours)
/// and a stored garbage value must never be silently truncated or served. (A
/// zero is rejected as out-of-contract: a ceiling is provisioned positive; a
/// "no ceiling" is expressed by NULL/absence, not a zero.)
///
/// # Errors
///
/// Returns `Err(String)` when the entitlement row carries a PRESENT, non-null
/// `max_vcpu_h` value that is non-positive or outside `u32` range.
fn decode_runner_vcpu_h(rows: &[crate::storage::d1_http::D1Row]) -> Result<Option<u32>, String> {
    let Some(raw) = rows.first().and_then(|row| row.get("max_vcpu_h")) else {
        // No entitlement row, OR no such column in the result → field omitted.
        return Ok(None);
    };

    // The column is NULLABLE: a SQL NULL → JSON null → wall-off (None/omitted),
    // NOT an error. This is the intentional asymmetry vs `max_concurrency`.
    if raw.is_null() {
        return Ok(None);
    }

    // A present, non-null value MUST be a positive, u32-range integer (the wire
    // type). Anything else is out-of-contract → fail-CLOSED (Err → 503).
    let ceiling = raw
        .as_u64()
        .filter(|v| *v >= 1 && *v <= u64::from(u32::MAX))
        .ok_or_else(|| {
            format!("runners_entitlement.max_vcpu_h out of u32 range or non-positive: {raw}")
        })?;

    // The filter above already bounds `ceiling` to `1..=u32::MAX`, so this cast
    // is lossless.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "ceiling is range-checked to 1..=u32::MAX immediately above"
    )]
    Ok(Some(ceiling as u32))
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
        .route(
            "/internal/v1/auth/resolve-tenant",
            post(handle_resolve_tenant),
        )
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
    // Check the presented header against EVERY configured consumer key. We
    // OR-combine with `|=` (NOT short-circuiting `||`) so all keys are always
    // evaluated: the response time does not reveal WHICH consumer's key matched
    // (no consumer-identity oracle), and the number of constant-time comparisons
    // is independent of the outcome. A caller with no/ wrong key is rejected
    // identically regardless of how many consumers are configured.
    let mut auth_ok = false;
    for key in &state.internal_auth_keys {
        auth_ok |= internal_auth_ok(key.as_bytes(), &headers);
    }
    if !auth_ok {
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
                    // M2 (runners seam): the entitlement is a SEPARATE axis —
                    // read from `runners_entitlement` (migrations 0070 + 0072),
                    // NOT derived from `plan`. One lookup returns both the
                    // concurrency cap and the monthly vCPU-h ceiling. Present
                    // ONLY for a tenant with a row (vCPU-h only if that NULLABLE
                    // column is set); a cache-only tenant gets both omitted. A D1
                    // fault fails CLOSED (503 — never guess an entitlement).
                    match runner_concurrency_for_tenant(&state.d1, &tenant_id).await {
                        Ok((max_concurrency, max_vcpu_h)) => (
                            StatusCode::OK,
                            Json(IntrospectResponse::valid(
                                tenant_id,
                                plan,
                                max_concurrency,
                                max_vcpu_h,
                            )),
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
// Tenant-per-org resolution (githugr ADR-0007 Epic A primitive)
// ──────────────────────────────────────────────────────────────────────────────

/// JSON request body for `POST /internal/v1/auth/resolve-tenant`. The
/// `clerk_org_id` is the Clerk organization id (`org_...`) to resolve.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveTenantRequest {
    /// The Clerk organization id to resolve to its isolated CoreLink tenant.
    pub clerk_org_id: String,
}

/// JSON response body for a SUCCESSFUL (`200`) tenant resolution.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResolveTenantResponse {
    /// The isolated CoreLink tenant UUID the org maps to.
    pub tenant_id: String,
}

/// SQL: the isolated tenant for a Clerk org from the `tenant_org_map` table
/// (migration 0083). The table is written ONLY by the provisioning authority;
/// this read is LOOKUP-ONLY (never auto-creates a mapping). A missing row is a
/// non-fault miss (the org is not yet provisioned).
///
/// # Ordering contract (A1, Option 2 — ratified)
///
/// Provisioning is the SOLE `tenant_org_map` writer — the resolver stays
/// lookup-only ON PURPOSE. Provision-in-resolver (Option 1) was REJECTED: the
/// `clerk_org_id` is an unverified, attacker-influenceable string, so writing a
/// mapping here would risk creating a WRONG/attacker-chosen tenant. The two
/// provisioning authorities are:
/// - **CoreLink-Clerk:** the `user.created` webhook (signup-worker) writes the
///   row (`INSERT OR IGNORE`, idempotent).
/// - **githugr-Clerk:** the in-worker Option-B token exchange writes the row.
///
/// Because the write happens out-of-band, a `resolve-tenant` read may race
/// AHEAD of provisioning (Svix webhook delivery lag): the row is simply not
/// there yet. That miss (`Ok(None)` → 404 `org_not_mapped`) is TRANSIENT during
/// the provisioning window, NOT a permanent "no such tenant". See
/// [`resolve_tenant_for_org`] / [`handle_resolve_tenant`] for the retry contract.
const RESOLVE_TENANT_SQL: &str =
    "SELECT tenant_id FROM tenant_org_map WHERE clerk_org_id = ?1 LIMIT 1";

/// Resolve a Clerk org id to its isolated CoreLink tenant via `tenant_org_map`
/// (migration 0083).
///
/// - Row present → `Ok(Some(tenant_id))` (the org is provisioned).
/// - No row → `Ok(None)` (the org is NOT mapped — the caller falls back to its
///   own unmapped-org behaviour; this endpoint NEVER auto-provisions).
///
/// # Fail-CLOSED
///
/// A genuine D1 backend fault surfaces as `Err(String)` so the route maps it to
/// **503** rather than guessing a tenant — the caller must never be handed a
/// WRONG tenant (which would break tenant isolation). A non-fault "no row" is
/// `Ok(None)`, not an error.
///
/// # Errors
///
/// Returns `Err(String)` only on a D1 backend fault (so the route can 503).
pub async fn resolve_tenant_for_org(
    d1: &D1HttpClient,
    clerk_org_id: &str,
) -> Result<Option<String>, String> {
    let rows = d1
        .query(
            RESOLVE_TENANT_SQL,
            &[serde_json::Value::String(clerk_org_id.to_owned())],
        )
        .await?;
    Ok(decode_resolved_tenant(&rows))
}

/// Pure decode of the `tenant_org_map` query result into the resolved tenant.
///
/// Split from [`resolve_tenant_for_org`] so the row→tenant mapping is
/// unit-testable without a network (mirrors [`decode_runner_cap`]): the I/O
/// wrapper does the keyed query, this maps rows → `Option<tenant_id>`.
///
/// - A row carrying a non-empty `tenant_id` string → `Some(tenant_id)`.
/// - No row (the org is not provisioned) → `None` (the handler maps this to a
///   404 `org_not_mapped` — never an auto-provision).
/// - A row whose `tenant_id` is missing / non-string / empty → `None` (treated
///   as "no mapping": a malformed row must never resolve to a wrong/blank
///   tenant — fail to a clean 404, never serve a bad isolation boundary).
fn decode_resolved_tenant(rows: &[crate::storage::d1_http::D1Row]) -> Option<String> {
    rows.first()
        .and_then(|row| row.get("tenant_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// `POST /internal/v1/auth/resolve-tenant` handler.
///
/// Mirrors [`handle_introspect`]'s posture: the dedicated-secret auth gate runs
/// FIRST on raw [`Bytes`] (before the body is parsed), so an unauthenticated
/// caller is rejected with 401 without any JSON parse. A mapped org → 200
/// `{ "tenant_id": ... }`; an unmapped org → 404 `{ "error": "org_not_mapped" }`
/// (provisioning is a SEPARATE step — never auto-created here); a D1 fault
/// → 503 (fail-CLOSED: never serve a wrong tenant).
///
/// # Retry contract (A1 ordering, Option 2 — ratified)
///
/// Provisioning (the `user.created` webhook for CoreLink-Clerk; the in-worker
/// token exchange for githugr-Clerk) is the SOLE `tenant_org_map` writer; this
/// handler NEVER writes. A `404 org_not_mapped` therefore means
/// "not-yet-provisioned", which during Svix webhook-delivery lag is a
/// **TRANSIENT** condition, not a permanent answer. Consumers MUST treat 404 as
/// **retryable** and re-poll with bounded backoff until the provisioning write
/// lands (an arbitrary new user then resolves reliably). `503` is distinct: it
/// is the fail-CLOSED D1-fault signal (also retryable, but backend-fault, not
/// provisioning-lag). Endpoint behaviour is UNCHANGED — this is a contract
/// clarification only; see `docs/integrations/resolve-tenant-contract.md`.
async fn handle_resolve_tenant(
    State(state): State<AuthIntrospectRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // ── 1. Dedicated-secret gate (constant-time; the SAME key set introspect
    //       uses) — non-short-circuiting OR so timing reveals no consumer. ────
    let mut auth_ok = false;
    for key in &state.internal_auth_keys {
        auth_ok |= internal_auth_ok(key.as_bytes(), &headers);
    }
    if !auth_ok {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    // ── 1b. Parse the body — ONLY after the auth gate passed. ───────────────
    let req: ResolveTenantRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "resolve_tenant: invalid request body");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_body" })),
            )
                .into_response();
        }
    };

    // ── 2. D1 read of tenant_org_map (lookup-only, never auto-create). ──────
    match resolve_tenant_for_org(&state.d1, &req.clerk_org_id).await {
        Ok(Some(tenant_id)) => {
            (StatusCode::OK, Json(ResolveTenantResponse { tenant_id })).into_response()
        }
        // Unmapped org → 404 (the showcase-tenant fallback path is the caller's;
        // this endpoint NEVER provisions).
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "org_not_mapped" })),
        )
            .into_response(),
        // D1 fault → fail-CLOSED 503 (never serve a guessed/wrong tenant).
        Err(e) => {
            tracing::error!(error = %e, "resolve_tenant: D1 lookup failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tenant-per-installation resolution + repo allowlist (cf-multitenant WP3)
// ──────────────────────────────────────────────────────────────────────────────

/// SQL: the isolated tenant for a GitHub App installation from the
/// `tenant_gh_installation_map` table (migration 0084). The table is written
/// ONLY by the provisioning authority; this read is LOOKUP-ONLY (never
/// auto-creates a mapping). A missing row is a non-fault miss (the installation
/// is not yet mapped to a tenant).
///
/// # Ordering contract (mirrors [`RESOLVE_TENANT_SQL`])
///
/// Provisioning is the SOLE `tenant_gh_installation_map` writer — the fabric
/// resolver stays lookup-only ON PURPOSE. The fabric plane resolves a GitHub App
/// installation id → isolated tenant against the SAME D1 table the Worker mint
/// reads (single source of truth, no divergent copy). Because the mapping write
/// happens out-of-band, a resolve read may race AHEAD of provisioning: the row is
/// simply not there yet. That miss (`Ok(None)` → 404 `installation_not_mapped`)
/// is TRANSIENT during the provisioning window, NOT a permanent "no such tenant"
/// (analogous to the org resolver's `org_not_mapped`). See
/// [`resolve_tenant_for_installation`].
const RESOLVE_TENANT_FOR_INSTALLATION_SQL: &str =
    "SELECT tenant_id FROM tenant_gh_installation_map WHERE installation_id = ?1 LIMIT 1";

/// SQL: whether a repo is on a tenant's runner allowlist
/// (`runner_repo_allowlist`, migration 0085). A present row (`SELECT 1`) means
/// the `repo_full_name` is explicitly allowed for `tenant_id`; the absence of a
/// row means NOT allowed. This is the fabric-plane's allowlist check, reading the
/// SAME table the Worker mint reads (single source of truth). See
/// [`repo_on_tenant_allowlist`].
const REPO_ON_TENANT_ALLOWLIST_SQL: &str =
    "SELECT 1 FROM runner_repo_allowlist WHERE tenant_id = ?1 AND repo_full_name = ?2 LIMIT 1";

/// Resolve a GitHub App installation id to its isolated CoreLink tenant via
/// `tenant_gh_installation_map` (migration 0084).
///
/// - Row present → `Ok(Some(tenant_id))` (the installation is mapped).
/// - No row → `Ok(None)` (the installation is NOT mapped — the caller falls back
///   to its own unmapped behaviour; this NEVER auto-provisions).
///
/// # Fail-CLOSED
///
/// A genuine D1 backend fault surfaces as `Err(String)` so the caller maps it to
/// **503** rather than guessing a tenant — the fabric must never be handed a
/// WRONG tenant (which would break tenant isolation). A non-fault "no row" is
/// `Ok(None)`, not an error. Mirrors [`resolve_tenant_for_org`].
///
/// # Errors
///
/// Returns `Err(String)` only on a D1 backend fault (so the caller can 503).
pub async fn resolve_tenant_for_installation(
    d1: &D1HttpClient,
    installation_id: &str,
) -> Result<Option<String>, String> {
    let rows = d1
        .query(
            RESOLVE_TENANT_FOR_INSTALLATION_SQL,
            &[serde_json::Value::String(installation_id.to_owned())],
        )
        .await?;
    Ok(decode_resolved_installation_tenant(&rows))
}

/// Pure decode of the `tenant_gh_installation_map` query result into the
/// resolved tenant.
///
/// Split from [`resolve_tenant_for_installation`] so the row→tenant mapping is
/// unit-testable without a network (mirrors [`decode_resolved_tenant`]): the I/O
/// wrapper does the keyed query, this maps rows → `Option<tenant_id>`.
///
/// - A row carrying a non-empty `tenant_id` string → `Some(tenant_id)`.
/// - No row (the installation is not mapped) → `None` (the caller maps this to a
///   404 `installation_not_mapped` — never an auto-provision).
/// - A row whose `tenant_id` is missing / non-string / empty → `None` (treated
///   as "no mapping": a malformed row must never resolve to a wrong/blank tenant
///   — fail to a clean 404, never serve a bad isolation boundary).
fn decode_resolved_installation_tenant(rows: &[crate::storage::d1_http::D1Row]) -> Option<String> {
    rows.first()
        .and_then(|row| row.get("tenant_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Whether `repo_full_name` is on `tenant_id`'s runner allowlist
/// (`runner_repo_allowlist`, migration 0085) — the fabric-plane's allowlist
/// check, reading the SAME table the Worker mint reads.
///
/// - A row present → `Ok(true)` (the repo is explicitly allowed for the tenant).
/// - No row → `Ok(false)` (NOT allowed — the caller denies the mint).
///
/// # Fail-CLOSED
///
/// A genuine D1 backend fault surfaces as `Err(String)` so the caller denies the
/// mint (never grants on a backend fault). This is a lookup-only read; it NEVER
/// writes the allowlist.
///
/// # Errors
///
/// Returns `Err(String)` only on a D1 backend fault (so the caller can deny /
/// 503 fail-CLOSED).
pub async fn repo_on_tenant_allowlist(
    d1: &D1HttpClient,
    tenant_id: &str,
    repo_full_name: &str,
) -> Result<bool, String> {
    let rows = d1
        .query(
            REPO_ON_TENANT_ALLOWLIST_SQL,
            &[
                serde_json::Value::String(tenant_id.to_owned()),
                serde_json::Value::String(repo_full_name.to_owned()),
            ],
        )
        .await?;
    Ok(decode_repo_on_allowlist(&rows))
}

/// Pure decode of the `runner_repo_allowlist` existence query into a bool.
///
/// Split from [`repo_on_tenant_allowlist`] so the row→bool mapping is
/// unit-testable without a network (mirrors [`decode_resolved_tenant`]): the I/O
/// wrapper does the keyed query, this maps rows → presence. The `SELECT 1`
/// projection means a row's mere PRESENCE is the answer — `true` iff at least one
/// row came back.
fn decode_repo_on_allowlist(rows: &[crate::storage::d1_http::D1Row]) -> bool {
    !rows.is_empty()
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

    let mut state = AuthIntrospectRouteState::new(
        Arc::from(auth_key.as_str()),
        Arc::new(verifier),
        Arc::new(d1),
    );

    // Optional ADDITIONAL consumer keys — each a distinct, independently-rotatable
    // secret so a consumer never shares another's blast radius. Today: the HuGR
    // toolkits fleet (37 MCP Workers) authenticating to introspect with their OWN
    // key, NOT the corelink-runners `FABRIC_INTROSPECT_AUTH_KEY`. A present-but-
    // too-short value is a misconfiguration → warn + ignore (fail-CLOSED: that
    // consumer simply can't authenticate, rather than weakening the gate).
    if let Ok(hugr_key) = std::env::var("FABRIC_INTROSPECT_AUTH_KEY_HUGR") {
        if hugr_key.len() >= MIN_FABRIC_AUTH_KEY_LEN {
            state = state.with_auth_key(Arc::from(hugr_key.as_str()));
        } else {
            tracing::warn!(
                "FABRIC_INTROSPECT_AUTH_KEY_HUGR set but too short (< 32 chars); \
                 HuGR introspect consumer NOT enabled"
            );
        }
    }

    Some(state)
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

    #[tokio::test]
    async fn additional_consumer_key_passes_gate_and_unknown_key_rejected() {
        // The HuGR toolkits fleet authenticates with its OWN introspect key (not
        // the runners' primary `FABRIC_INTROSPECT_AUTH_KEY`). Both configured
        // consumer keys must pass the auth gate; an unconfigured key must 401.
        // We assert the GATE outcome only (!= 401 = passed), independent of the
        // downstream PAT pipeline, so the test isolates the multi-key gate.
        const HUGR_KEY: &str = "hugr-fabric-introspect-key-32-chars!";
        let mk_app = || {
            let verifier = Arc::new(PatVerifier::new(
                Arc::new(FakeLookup::with_row("x", row_for("h", "t"))),
                test_key(),
            ));
            let state =
                AuthIntrospectRouteState::new(Arc::from(TEST_AUTH_KEY), verifier, unreachable_d1())
                    .with_auth_key(Arc::from(HUGR_KEY));
            router(state)
        };
        let body = serde_json::json!({ "token": "corelink_pat_x" });

        // Primary (runners) key → gate passes.
        let resp = mk_app()
            .oneshot(introspect_request(Some(TEST_AUTH_KEY), body.clone()))
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "primary runners key must pass the gate"
        );

        // HuGR consumer key → gate passes identically (own secret).
        let resp = mk_app()
            .oneshot(introspect_request(Some(HUGR_KEY), body.clone()))
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "HuGR consumer key must pass the gate"
        );

        // An unconfigured key (right length, wrong value) → still 401.
        let resp = mk_app()
            .oneshot(introspect_request(
                Some("some-other-unconfigured-32char-key!!"),
                body,
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "an unconfigured key must be rejected"
        );
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
            !obj.contains_key("max_vcpu_h"),
            "cache-only: max_vcpu_h must be omitted"
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
            None,
        );
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["valid"], serde_json::json!(true));
        assert_eq!(v["plan"], serde_json::json!("pro"));
        assert_eq!(
            v["max_concurrency"],
            serde_json::json!(40),
            "max_concurrency must be a top-level integer when entitled"
        );
        // max_vcpu_h stays omitted when None (the asymmetric wall-off default).
        assert!(
            !v.as_object().unwrap().contains_key("max_vcpu_h"),
            "max_vcpu_h must be omitted when None"
        );
        // rate_ceiling_per_min stays omitted (still M1).
        assert!(!v.as_object().unwrap().contains_key("rate_ceiling_per_min"));
    }

    #[test]
    fn valid_response_with_vcpu_h_serialises_max_vcpu_h() {
        // The full runners-entitlement 200 shape: both caps present as top-level
        // integers — the exact byte-shape pinned in conformance/corelink-introspect.json
        // (pro/40 → max_vcpu_h 240) and mirrored by the runners CoreLinkPlanStore.
        let resp = IntrospectResponse::valid(
            "11111111-1111-4111-8111-111111111111".to_owned(),
            "pro".to_owned(),
            Some(40),
            Some(240),
        );
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["valid"], serde_json::json!(true));
        assert_eq!(v["max_concurrency"], serde_json::json!(40));
        assert_eq!(
            v["max_vcpu_h"],
            serde_json::json!(240),
            "max_vcpu_h must be a top-level integer (vCPU-hours) when provisioned"
        );
        assert!(!v.as_object().unwrap().contains_key("rate_ceiling_per_min"));
    }

    /// Build a one-row `runners_entitlement` result set carrying the given
    /// `max_concurrency` JSON value (mirrors what `D1HttpClient::query` returns
    /// for `SELECT max_concurrency, max_vcpu_h ...`). The `max_vcpu_h` column is
    /// omitted from the row, modelling a pre-0072 / NULL-vCPU-h row.
    fn entitlement_rows(max_concurrency: serde_json::Value) -> Vec<crate::storage::d1_http::D1Row> {
        let mut row = serde_json::Map::new();
        row.insert("max_concurrency".to_owned(), max_concurrency);
        vec![row]
    }

    /// Build a one-row `runners_entitlement` result set carrying BOTH columns
    /// (mirrors a post-0072 row with a provisioned `max_vcpu_h`).
    fn entitlement_rows_with_vcpu_h(
        max_concurrency: serde_json::Value,
        max_vcpu_h: serde_json::Value,
    ) -> Vec<crate::storage::d1_http::D1Row> {
        let mut row = serde_json::Map::new();
        row.insert("max_concurrency".to_owned(), max_concurrency);
        row.insert("max_vcpu_h".to_owned(), max_vcpu_h);
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

    // ── decode_runner_vcpu_h (migration 0072) ────────────────────────────────

    #[test]
    fn decode_runner_vcpu_h_present_row_yields_some() {
        // A row WITH max_vcpu_h surfaces the stored ceiling verbatim (vCPU-hours;
        // the separate additive axis — NOT a plan-derived value).
        assert_eq!(
            decode_runner_vcpu_h(&entitlement_rows_with_vcpu_h(
                serde_json::json!(40),
                serde_json::json!(240)
            ))
            .unwrap(),
            Some(240),
            "a row with max_vcpu_h must yield the stored ceiling"
        );
        // A bespoke (Enterprise) value is honoured, not clamped to the ladder.
        assert_eq!(
            decode_runner_vcpu_h(&entitlement_rows_with_vcpu_h(
                serde_json::json!(8),
                serde_json::json!(5000)
            ))
            .unwrap(),
            Some(5000)
        );
    }

    #[test]
    fn decode_runner_vcpu_h_null_column_yields_none_walloff() {
        // A row present but max_vcpu_h SQL NULL → None (field omitted ⇒ wall-off).
        // This is the intentional asymmetry vs max_concurrency: NULL is NOT an
        // error here.
        assert_eq!(
            decode_runner_vcpu_h(&entitlement_rows_with_vcpu_h(
                serde_json::json!(40),
                serde_json::Value::Null
            ))
            .unwrap(),
            None,
            "NULL max_vcpu_h must yield None (wall-off), not Err"
        );
    }

    #[test]
    fn decode_runner_vcpu_h_absent_column_yields_none() {
        // A row WITHOUT the max_vcpu_h column at all (pre-0072 result shape) → None.
        assert_eq!(
            decode_runner_vcpu_h(&entitlement_rows(serde_json::json!(40))).unwrap(),
            None,
            "a row missing the max_vcpu_h column must yield None (wall-off)"
        );
    }

    #[test]
    fn decode_runner_vcpu_h_no_row_yields_none() {
        // No entitlement row at all → None (field omitted).
        assert_eq!(
            decode_runner_vcpu_h(&[]).unwrap(),
            None,
            "no entitlement row must yield None for max_vcpu_h"
        );
    }

    #[test]
    fn decode_runner_vcpu_h_out_of_contract_value_fails_closed() {
        // A PRESENT, non-null value that is out-of-contract fails CLOSED (Err →
        // 503), never a guessed/truncated ceiling. (NULL is wall-off; these are
        // not NULL.)
        for bad in [
            serde_json::json!(0),
            serde_json::json!(-5),
            serde_json::json!(u64::from(u32::MAX) + 1),
            serde_json::json!("lots"),
        ] {
            assert!(
                decode_runner_vcpu_h(&entitlement_rows_with_vcpu_h(serde_json::json!(40), bad))
                    .is_err(),
                "out-of-contract max_vcpu_h must fail closed"
            );
        }
    }

    #[test]
    fn decode_runner_vcpu_h_accepts_u32_max() {
        assert_eq!(
            decode_runner_vcpu_h(&entitlement_rows_with_vcpu_h(
                serde_json::json!(40),
                serde_json::json!(u32::MAX)
            ))
            .unwrap(),
            Some(u32::MAX),
            "u32::MAX is the inclusive upper bound for max_vcpu_h"
        );
    }

    #[test]
    fn introspect_response_roundtrips_max_vcpu_h() {
        // Deserialize the full wire shape (the runners repo mirrors this) and
        // confirm max_vcpu_h round-trips as a top-level u32.
        let json = serde_json::json!({
            "valid": true,
            "tenant_id": "11111111-1111-4111-8111-111111111111",
            "plan": "pro",
            "max_concurrency": 40,
            "max_vcpu_h": 240
        });
        let parsed: IntrospectResponse = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.max_concurrency, Some(40));
        assert_eq!(parsed.max_vcpu_h, Some(240));
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

    // ── resolve-tenant: tenant-per-org primitive (migration 0083) ────────────

    /// Build a `POST /internal/v1/auth/resolve-tenant` request, with an optional
    /// `X-Corelink-Internal-Auth` header.
    fn resolve_request(auth: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder()
            .method(http::Method::POST)
            .uri("/internal/v1/auth/resolve-tenant")
            .header("content-type", "application/json");
        if let Some(a) = auth {
            b = b.header("x-corelink-internal-auth", a);
        }
        b.body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    /// A `verifier` that is never reached by the resolve route (it has its own
    /// D1-only path); the resolve tests only exercise the auth gate + D1 read.
    fn unused_verifier() -> Arc<PatVerifier> {
        Arc::new(PatVerifier::new(
            Arc::new(FakeLookup::backend("unused")),
            test_key(),
        ))
    }

    fn resolve_app() -> Router {
        router(state_with(unused_verifier(), unreachable_d1()))
    }

    #[test]
    fn decode_resolved_tenant_seeded_org_yields_some() {
        // A `tenant_org_map` row carrying the org's tenant → Some(tenant_id):
        // the resolution that backs githugr's per-org token exchange.
        let mut row = serde_json::Map::new();
        row.insert(
            "tenant_id".to_owned(),
            serde_json::json!("11111111-1111-4111-8111-111111111111"),
        );
        assert_eq!(
            decode_resolved_tenant(&[row]),
            Some("11111111-1111-4111-8111-111111111111".to_owned()),
            "a seeded org row must resolve to its mapped tenant"
        );
    }

    #[test]
    fn decode_resolved_tenant_unmapped_org_yields_none() {
        // Empty result set (GATED-INERT: no rows until provisioning) → None →
        // the handler returns 404 org_not_mapped (the showcase-tenant fallback
        // path is the caller's; this endpoint never auto-provisions).
        assert_eq!(
            decode_resolved_tenant(&[]),
            None,
            "an unmapped org must yield None (→ 404 org_not_mapped)"
        );
    }

    #[test]
    fn decode_resolved_tenant_malformed_row_yields_none() {
        // A row whose tenant_id is missing / non-string / empty must NOT resolve
        // to a wrong/blank tenant — it falls to None (clean 404), never a bad
        // isolation boundary.
        let mut missing = serde_json::Map::new();
        missing.insert("other".to_owned(), serde_json::json!("x"));
        assert_eq!(decode_resolved_tenant(&[missing]), None);

        let mut non_str = serde_json::Map::new();
        non_str.insert("tenant_id".to_owned(), serde_json::json!(42));
        assert_eq!(decode_resolved_tenant(&[non_str]), None);

        let mut empty = serde_json::Map::new();
        empty.insert("tenant_id".to_owned(), serde_json::json!(""));
        assert_eq!(decode_resolved_tenant(&[empty]), None);
    }

    #[tokio::test]
    async fn resolve_missing_service_secret_returns_401() {
        let resp = resolve_app()
            .oneshot(resolve_request(
                None,
                serde_json::json!({ "clerk_org_id": "org_123" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn resolve_wrong_service_secret_returns_401() {
        let resp = resolve_app()
            .oneshot(resolve_request(
                Some("wrong-secret-which-is-also-32-chars!!"),
                serde_json::json!({ "clerk_org_id": "org_123" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn resolve_unknown_field_returns_400() {
        // deny_unknown_fields: an extra field is rejected on shape (after the
        // auth gate passes, before any D1 read).
        let resp = resolve_app()
            .oneshot(resolve_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!({ "clerk_org_id": "org_123", "evil": true }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = body_json(resp).await;
        assert_eq!(v["error"], serde_json::json!("invalid_body"));
    }

    #[tokio::test]
    async fn resolve_d1_fault_returns_503_failclosed() {
        // A well-formed, authenticated request whose D1 lookup faults must 503 —
        // never a guessed tenant (which would break tenant isolation).
        let resp = resolve_app()
            .oneshot(resolve_request(
                Some(TEST_AUTH_KEY),
                serde_json::json!({ "clerk_org_id": "org_123" }),
            ))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "a D1 fault on resolve must fail CLOSED (503), never serve a guessed tenant"
        );
    }

    #[tokio::test]
    async fn resolve_tenant_for_org_errors_on_d1_fault() {
        // The I/O wrapper surfaces a D1 fault as Err (→ route 503), mirroring
        // tier_for_tenant's fail-CLOSED contract.
        let d1 = unreachable_d1();
        let err = resolve_tenant_for_org(&d1, "org_123").await;
        assert!(
            err.is_err(),
            "a D1 fault must surface as Err (fail-CLOSED → 503)"
        );
    }

    #[test]
    fn resolve_tenant_response_serialises() {
        let v = serde_json::to_value(ResolveTenantResponse {
            tenant_id: "11111111-1111-4111-8111-111111111111".to_owned(),
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "tenant_id": "11111111-1111-4111-8111-111111111111" })
        );
    }

    // ── cf-multitenant WP3: tenant-per-installation resolve + repo allowlist ──

    /// Build a `runner_repo_allowlist` `SELECT 1` result row (the column name is
    /// irrelevant — the decode keys off row PRESENCE, not a value).
    fn allowlist_hit_rows() -> Vec<crate::storage::d1_http::D1Row> {
        let mut row = serde_json::Map::new();
        row.insert("1".to_owned(), serde_json::json!(1));
        vec![row]
    }

    #[test]
    fn decode_resolved_installation_tenant_mapped_yields_some() {
        // A `tenant_gh_installation_map` row carrying the installation's tenant →
        // Some(tenant_id): the resolution that backs the fabric-plane runner mint.
        let mut row = serde_json::Map::new();
        row.insert(
            "tenant_id".to_owned(),
            serde_json::json!("22222222-2222-4222-8222-222222222222"),
        );
        assert_eq!(
            decode_resolved_installation_tenant(&[row]),
            Some("22222222-2222-4222-8222-222222222222".to_owned()),
            "a mapped installation row must resolve to its tenant"
        );
    }

    #[test]
    fn decode_resolved_installation_tenant_unmapped_yields_none() {
        // Empty result set (no row until provisioning) → None → the caller
        // returns 404 installation_not_mapped (never auto-provisions).
        assert_eq!(
            decode_resolved_installation_tenant(&[]),
            None,
            "an unmapped installation must yield None (→ 404 installation_not_mapped)"
        );
    }

    #[test]
    fn decode_resolved_installation_tenant_malformed_row_yields_none() {
        // A row whose tenant_id is missing / non-string / empty must NOT resolve
        // to a wrong/blank tenant — it falls to None (clean 404), never a bad
        // isolation boundary (mirrors decode_resolved_tenant).
        let mut missing = serde_json::Map::new();
        missing.insert("other".to_owned(), serde_json::json!("x"));
        assert_eq!(decode_resolved_installation_tenant(&[missing]), None);

        let mut non_str = serde_json::Map::new();
        non_str.insert("tenant_id".to_owned(), serde_json::json!(42));
        assert_eq!(decode_resolved_installation_tenant(&[non_str]), None);

        let mut empty = serde_json::Map::new();
        empty.insert("tenant_id".to_owned(), serde_json::json!(""));
        assert_eq!(decode_resolved_installation_tenant(&[empty]), None);
    }

    #[tokio::test]
    async fn resolve_tenant_for_installation_errors_on_d1_fault() {
        // The I/O wrapper surfaces a D1 fault as Err (→ caller 503), mirroring
        // resolve_tenant_for_org's fail-CLOSED contract.
        let d1 = unreachable_d1();
        let err = resolve_tenant_for_installation(&d1, "12345678").await;
        assert!(
            err.is_err(),
            "a D1 fault must surface as Err (fail-CLOSED → 503)"
        );
    }

    #[test]
    fn decode_repo_on_allowlist_hit_yields_true() {
        // A present `SELECT 1` row means the repo is explicitly allowed.
        assert!(
            decode_repo_on_allowlist(&allowlist_hit_rows()),
            "an allowlist hit (row present) must yield true"
        );
    }

    #[test]
    fn decode_repo_on_allowlist_miss_yields_false() {
        // No row → the repo is NOT allowlisted for the tenant → deny.
        assert!(
            !decode_repo_on_allowlist(&[]),
            "an allowlist miss (no row) must yield false"
        );
    }

    #[tokio::test]
    async fn repo_on_tenant_allowlist_errors_on_d1_fault() {
        // The I/O wrapper surfaces a D1 fault as Err so the caller denies the
        // mint fail-CLOSED (never grants on a backend fault).
        let d1 = unreachable_d1();
        let err = repo_on_tenant_allowlist(
            &d1,
            "22222222-2222-4222-8222-222222222222",
            "octocat/hello-world",
        )
        .await;
        assert!(
            err.is_err(),
            "a D1 fault on the allowlist read must surface as Err (fail-CLOSED)"
        );
    }
}
