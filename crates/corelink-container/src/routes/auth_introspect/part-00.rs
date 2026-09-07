// `POST /internal/v1/auth/introspect` — PAT introspection endpoint for the
// **corelink-runners fabric** (M1).
//
// # Why this exists
//
// The runners fabric is handed an inbound `Bearer` PAT per job and must
// resolve the owning **tenant** and the tenant's **plan** before it places
// work onto a runner. Rather than re-implement PAT verification (HMAC +
// Argon2id + D1 liveness) in the fabric, the fabric calls THIS endpoint and
// receives a frozen, minimal introspection result. The container is the one
// place that already owns the full verification pipeline
// ([`crate::adapter_pat::PatVerifier`]).
//
// # Security model
//
// - This route is **NOT** reachable from the public internet. It is mounted
//   on the container's HTTP listener (port 50051), reached only via the
//   Cloudflare Durable Object / fabric forwarder.
// - **Caller auth (fail-CLOSED):** every request must carry an
//   `X-Corelink-Internal-Auth` header whose value matches a **DEDICATED**
//   secret, [`build_state_from_env`]'s `FABRIC_INTROSPECT_AUTH_KEY` — NOT the
//   Worker↔container `CORELINK_INTERNAL_AUTH_KEY`. A separate secret keeps the
//   blast radius tight: a leak of the fabric secret cannot mint PATs, and a
//   leak of the mint secret cannot introspect. The compare reuses the exact
//   constant-time, length-padded gate from
//   [`crate::routes::internal_pat::internal_auth_ok`] — it is NOT reinvented
//   here.
// - If `FABRIC_INTROSPECT_AUTH_KEY` is absent or shorter than 32 chars, the
//   route is **NOT mounted** ([`build_state_from_env`] returns `None`, warn
//   log) — the same fail-CLOSED posture as `internal_pat`.
// - **Multiple consumers, isolated secrets:** additional consumers each carry
//   their OWN dedicated key so a compromised consumer can never present (nor
//   leak the blast radius of) another's credential. The primary
//   `FABRIC_INTROSPECT_AUTH_KEY` is the corelink-runners fabric; optional
//   `FABRIC_INTROSPECT_AUTH_KEY_HUGR` (≥ 32 chars, else ignored with a warn) is
//   the HuGR toolkits fleet. The gate checks the header against EVERY configured
//   key without short-circuiting, so the timing reveals no consumer identity.
//
// # Request shape
//
// ```text
// POST /internal/v1/auth/introspect
// X-Corelink-Internal-Auth: <FABRIC_INTROSPECT_AUTH_KEY>
// Content-Type: application/json
//
// { "token": "corelink_pat_..." }
// ```
//
// # Response shapes
//
// - Valid PAT, cache-only tenant (200):
//   ```text
//   { "valid": true, "tenant_id": "<uuid>", "plan": "<tier>" }
//   ```
// - Valid PAT, tenant with a Runners entitlement (200):
//   ```text
//   { "valid": true, "tenant_id": "<uuid>", "plan": "pro", "max_concurrency": 40, "max_vcpu_h": 240 }
//   ```
// - Invalid PAT (200, uniform — NO oracle on *why* and NO tenant_id):
//   ```text
//   { "valid": false }
//   ```
// - Verifier backend fault / tier-query fault (503, no body contract). The
//   fabric maps 503 → `Err(Unreachable)` and never serves a plan it could not
//   resolve (fail-CLOSED — never serve a wrong plan).
// - Missing / wrong service secret (401).
//
// # M2 scope — `max_concurrency` (runners seam, ratified)
//
// `max_concurrency` is the per-tenant runner concurrency cap. It is a
// **top-level** integer field, present ONLY when the tenant has a **Runners
// entitlement**, and absent for cache-only tenants (`skip_serializing_if`).
//
// The cap is a **SEPARATE entitlement axis from the cache tier** (runners TL
// ratified): it is read from the dedicated `runners_entitlement` D1 table by a
// single keyed lookup (`SELECT max_concurrency FROM runners_entitlement WHERE
// tenant_id = ?1`, migration 0070), **NOT** derived from the cache-plan ladder.
// A row present → `Some(row.max_concurrency)`; absent (the table starts empty)
// → `None`, the field is omitted, and the tenant is treated as cache-only with
// no Runners entitlement (the fabric rejects the placement — empty table = no
// cap = reject). The `plan` field stays = the cache tier (informational only)
// and never feeds the cap. The runners `CoreLinkPlanStore` parses this exact
// shape.
//
// `max_vcpu_h` (the per-tenant monthly vCPU-hour ceiling, migration 0072) is a
// SECOND additive field on the SAME `runners_entitlement` lookup
// (`SELECT max_concurrency, max_vcpu_h FROM runners_entitlement WHERE
// tenant_id = ?1`). It carries the deliberate **asymmetry** vs
// `max_concurrency`: an absent `max_vcpu_h` ⇒ **wall-off** (the fabric enforces
// no monthly compute cap and lets the job through), NOT a reject. Present →
// `Some(vcpu_h)`; the column is NULLABLE so existing rows back-fill to
// `None`/absent (wall-off).
//
// `rate_ceiling_per_min` remains **omitted** (M1) — net-new product data not
// yet decided; the fabric's `StaticPlans` supplies that cap. The response
// struct carries it as `Option` with `skip_serializing_if`, so it can be added
// later WITHOUT a breaking wire change.
//
// # Hard rules
//
// - The `token` is NEVER logged.
// - On `valid: false` no reason is given (uniform with `VerifyError`).
// - `tenant_id` is present ONLY when `valid: true`.

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
#[non_exhaustive]
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
#[non_exhaustive]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntrospectRequest {
    /// The PAT plaintext to introspect (e.g. `corelink_pat_...`). NEVER logged.
    pub token: String,
}

/// JSON response body. Forward-compatible: `tenant_id` / `plan` are present
/// only on `valid: true`, and the M1-omitted cap fields are skipped when
/// `None` so they can be added later without a breaking change.
#[non_exhaustive]
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

    // The filter above already bounds `cap` to `1..=u32::MAX`, so conversion
    // cannot fail; retain the explicit checked conversion for lint hygiene.
    Ok(Some(u32::try_from(cap).map_err(|_| {
        "runners_entitlement.max_concurrency conversion failed".to_owned()
    })?))
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

    // The filter above already bounds `ceiling` to `1..=u32::MAX`, so conversion
    // cannot fail; retain the explicit checked conversion for lint hygiene.
    Ok(Some(u32::try_from(ceiling).map_err(|_| {
        "runners_entitlement.max_vcpu_h conversion failed".to_owned()
    })?))
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
