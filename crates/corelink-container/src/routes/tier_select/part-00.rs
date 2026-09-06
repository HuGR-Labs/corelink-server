// `POST /v1/onboarding/tier-select` — server-side Stripe Checkout
// Session creation for self-serve tier upgrades (WI-S19-004 production
// wiring; the `corelink-tier-selection` charter deferred this to the
// PRR ship gate).
//
// # Security model (IRONCLAD — this is a payment + auth boundary)
//
// 1. **Not publicly reachable.** Mounted on the container's HTTP
//    listener (port 50051), reachable ONLY from the Cloudflare Durable
//    Object via `container.getTcpPort(50051)`. The DO/Worker forwards
//    only requests whose caller supplies `X-Corelink-Internal-Auth`
//    matching the dedicated secret (`CORELINK_TIER_SELECT_AUTH_KEY`), with
//    the documented `CORELINK_INTERNAL_AUTH_KEY` fallback. Absent
//    or mismatched → 401. **Constant-time** comparison (`subtle`).
//    Mirrors `internal_pat.rs`.
//
// 2. **Tenant identity is never trusted from the client.** `tenant_id`
//    is taken ONLY from the `x-corelink-tenant-id` header, which the
//    edge Worker sets AFTER verifying the Clerk session JWT (JWKS, via
//    `corelink-clerk`). The request body carries NO tenant field. Absent
//    or empty header → 401 (fail-CLOSED; there is deliberately NO
//    `_unknown` default — a missing verified tenant must never proceed
//    to a billing mutation).
//
// 3. **INV-ONBOARD-DPA-FIRST.** DPA acceptance is checked in D1 BEFORE
//    any Stripe API call, for ALL tiers (WI §6.5). Not accepted → 403.
//
// 4. **Durable double-checkout prevention.** A 60s row lock in
//    `tier_selection_locks` (`INSERT OR IGNORE`) is the cross-isolate
//    mutex held across the Stripe call; the UNIQUE partial index
//    `idx_tenant_active_subscription` is defense-in-depth against a
//    second active subscription (migration 0039). Concurrent caller →
//    409 `lock_held`.
//
// 5. **Stripe owns PCI.** We create a *hosted* Checkout Session and
//    return its URL; CoreLink never sees card data.
//
// 6. **Enterprise is not self-serve.** `enterprise` is rejected with
//    422 `use_inquiry_form` (the UI hint alone is bypassable).
//
// 7. **Audit before mutation.** Every state-mutating step emits its
//    audit record BEFORE the mutation (fail-CLOSED;
//    INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
//
// The pure-logic invariants are pinned by the 10k-iter property tests
// in `corelink-tier-selection`; this module is the production transport
// + durable-store wiring that must satisfy the same invariants.

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
use subtle::ConstantTimeEq;
use uuid::Uuid;

// ──────────────────────────────────────────────────────────────────────────────
// F6/F10: success_url / cancel_url host allowlist.
//
// The ONLY valid destination hosts for the Stripe Checkout redirect are the
// CoreLink-owned origins.  A scheme-only `starts_with("https://")` check is
// not a meaningful control: an authenticated user can supply any https URL and
// Stripe will redirect the post-payment browser there, leaking the Checkout
// Session id (Stripe appends `{CHECKOUT_SESSION_ID}` to the URL).
//
// Fix: parse the URL with the `url` crate (already a transitive dep via
// `reqwest`) and assert that:
//   • scheme is exactly "https"
//   • host is in ALLOWED_REDIRECT_HOSTS (exact match, case-insensitive)
//   • no userinfo/`@` component (embedded credentials)
//   • port is absent (standard 443 implied) — non-standard ports are rejected
// ──────────────────────────────────────────────────────────────────────────────

/// Exact set of CoreLink-owned hostnames that may appear in `success_url` /
/// `cancel_url`.  Must be kept in sync with the deployed admin-ui and app
/// origins.  Case-insensitive comparison (`.to_ascii_lowercase()` on the
/// parsed host).
const ALLOWED_REDIRECT_HOSTS: &[&str] = &["humangr.com"];

/// Validate a single redirect URL (success or cancel).  Fail-CLOSED: any
/// parse failure, wrong scheme, disallowed host, present userinfo, or
/// non-standard port is a hard `BadRequest`.  Emits a structured `tracing`
/// warning (never logs the full URL to avoid accidental credential exposure).
fn validate_redirect_url(raw: &str, field: &str) -> Result<(), TierSelectHttpError> {
    // Keep the existing https prefix check as an early fast-path (avoids a
    // full URL parse on obviously-bad inputs — the `url` parse below is the
    // authoritative gate).
    if !raw.starts_with("https://") {
        tracing::warn!(
            field,
            "redirect URL rejected: scheme is not https (fail-closed, F6/F10)"
        );
        return Err(TierSelectHttpError::BadRequest);
    }

    let parsed = match url::Url::parse(raw) {
        Ok(u) => u,
        Err(_) => {
            tracing::warn!(
                field,
                "redirect URL rejected: URL parse failed (fail-closed, F6/F10)"
            );
            return Err(TierSelectHttpError::BadRequest);
        }
    };

    // Scheme must be exactly "https" (the Url parser normalises to lowercase).
    if parsed.scheme() != "https" {
        tracing::warn!(
            field,
            "redirect URL rejected: non-https scheme (fail-closed, F6/F10)"
        );
        return Err(TierSelectHttpError::BadRequest);
    }

    // Reject userinfo (embedded credentials — `user:pass@host`).
    if !parsed.username().is_empty() || parsed.password().is_some() {
        tracing::warn!(
            field,
            "redirect URL rejected: userinfo present (fail-closed, F6/F10)"
        );
        return Err(TierSelectHttpError::BadRequest);
    }

    // Reject non-standard ports.  An explicit `:443` is also rejected to keep
    // the check simple and consistent with the admin-ui's server-built URLs
    // (which never specify a port).
    if parsed.port().is_some() {
        tracing::warn!(
            field,
            "redirect URL rejected: explicit port present (fail-closed, F6/F10)"
        );
        return Err(TierSelectHttpError::BadRequest);
    }

    // Host must be in the allowlist (case-insensitive).
    let host = parsed
        .host_str()
        .map(|h| h.to_ascii_lowercase())
        .unwrap_or_default();
    if !ALLOWED_REDIRECT_HOSTS
        .iter()
        .any(|&allowed| allowed == host)
    {
        tracing::warn!(
            field,
            host = %host,
            allowlist = ?ALLOWED_REDIRECT_HOSTS,
            "redirect URL rejected: host not in CoreLink allowlist (fail-closed, F6/F10)"
        );
        return Err(TierSelectHttpError::BadRequest);
    }

    Ok(())
}

/// Header carrying the internal shared secret (worker → container trust
/// boundary). Identical mechanism to `internal_pat.rs`.
pub const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Header carrying the edge-verified tenant id (set by the Worker AFTER
/// Clerk JWT verification). The canonical container tenant header.
pub const TENANT_HEADER: &str = "x-corelink-tenant-id";

/// Canonical self-serve paid tiers accepted by this route. `free` is an
/// instant activation (no Stripe); the cache tiers (`solo` / `starter` /
/// `pro` / `max`) and the runner SKUs (`runner_starter` / `runner_pro` /
/// `runner_team` / `runner_scale` / `runner_max`) require Stripe Checkout;
/// `enterprise` is rejected here and routed to the inquiry form. Kept in
/// lockstep with `corelink_tier_selection::tier::TierKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestedTier {
    /// Free tier — instant activation, no Checkout Session.
    Free,
    /// Solo paid tier.
    Solo,
    /// Starter paid tier.
    Starter,
    /// Pro paid tier.
    Pro,
    /// Max paid tier.
    Max,
    /// Runner Starter paid tier (runner SKU).
    RunnerStarter,
    /// Runner Pro paid tier (runner SKU).
    RunnerPro,
    /// Runner Team paid tier (runner SKU).
    RunnerTeam,
    /// Runner Scale paid tier (runner SKU).
    RunnerScale,
    /// Runner Max paid tier (runner SKU).
    RunnerMax,
}

impl RequestedTier {
    /// Parse the wire `tier` string (case-insensitive). Returns `None`
    /// for `enterprise` (handled separately → 422) and unknown values.
    #[must_use]
    pub fn parse_self_serve(s: &str) -> ParsedTier {
        match s.trim().to_ascii_lowercase().as_str() {
            "free" => ParsedTier::Tier(Self::Free),
            "solo" => ParsedTier::Tier(Self::Solo),
            "starter" => ParsedTier::Tier(Self::Starter),
            "pro" => ParsedTier::Tier(Self::Pro),
            "max" => ParsedTier::Tier(Self::Max),
            "runner_starter" => ParsedTier::Tier(Self::RunnerStarter),
            "runner_pro" => ParsedTier::Tier(Self::RunnerPro),
            "runner_team" => ParsedTier::Tier(Self::RunnerTeam),
            "runner_scale" => ParsedTier::Tier(Self::RunnerScale),
            "runner_max" => ParsedTier::Tier(Self::RunnerMax),
            "enterprise" => ParsedTier::Enterprise,
            _ => ParsedTier::Invalid,
        }
    }

    /// `true` for tiers that require a Stripe Checkout Session.
    #[must_use]
    pub const fn is_paid(self) -> bool {
        !matches!(self, Self::Free)
    }

    /// `true` for the runner SKUs. Runner is a SEPARATE entitlement axis from
    /// the cache tiers: a runner purchase's subscription↔tenant mapping and
    /// entitlement are written by the signup-worker Stripe webhook into
    /// `runner_billing` / `runners_entitlement` (migrations 0087 / 0070), NOT
    /// the cache `tier_selections` / `stripe_checkout_sessions` tables. Those
    /// cache tables are one-row-per-tenant with a cache-only `tier` CHECK, so
    /// persisting a runner tier there would both violate the CHECK and CLOBBER
    /// the tenant's cache tier. The orchestration therefore skips the cache
    /// persist for a runner checkout and guards the runner axis independently.
    #[must_use]
    pub const fn is_runner(self) -> bool {
        matches!(
            self,
            Self::RunnerStarter
                | Self::RunnerPro
                | Self::RunnerTeam
                | Self::RunnerScale
                | Self::RunnerMax
        )
    }
}

/// Outcome of parsing the wire `tier` string. Distinguishes the
/// enterprise route (422) from genuinely invalid input (400).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedTier {
    /// A valid self-serve tier.
    Tier(RequestedTier),
    /// `enterprise` — must use the inquiry form (422).
    Enterprise,
    /// Unknown / malformed tier (400).
    Invalid,
}

// ──────────────────────────────────────────────────────────────────────────────
// Request / response shapes
// ──────────────────────────────────────────────────────────────────────────────

/// JSON request body. `deny_unknown_fields` rejects anything unexpected
/// (defense-in-depth: a client cannot smuggle e.g. a `tenant_id`).
///
/// NOTE the deliberate ABSENCE of any tenant field — the tenant is taken
/// only from the edge-verified header (see security model §2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierSelectRequest {
    /// Requested tier: `free` | `solo` | `starter` | `pro` | `max` |
    /// `runner_starter` | `runner_pro` | `runner_team` | `runner_scale` |
    /// `runner_max`. (`enterprise` → 422.)
    pub tier: String,
    /// Post-payment success redirect (Stripe appends the session id). The
    /// Worker builds this from a trusted origin — never user-supplied.
    pub success_url: String,
    /// Checkout-cancel redirect.
    pub cancel_url: String,
}

/// JSON success response (200). Mirrors the `TierSelectResponse` the
/// admin-ui `/api/checkout/session` bridge expects.
#[derive(Debug, Clone, Serialize)]
pub struct TierSelectResponse {
    /// The Stripe-hosted Checkout URL the client redirects to. Always
    /// `https://`. (For `free`, this is omitted — see [`FreeActivated`].)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkout_url: Option<String>,
    /// The Stripe Checkout Session id (`cs_...`), or the activation id for
    /// the free tier.
    pub session_id: String,
}

/// Typed error → HTTP status mapping for this route. Kept in lockstep
/// with `corelink_tier_selection::error::TierError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TierSelectHttpError {
    /// Missing/invalid internal-auth header → 401.
    Unauthenticated,
    /// Missing/empty verified tenant header → 401.
    NoVerifiedTenant,
    /// Malformed body / invalid tier → 400.
    BadRequest,
    /// `enterprise` requested → 422 (use inquiry form).
    UseInquiryForm,
    /// DPA not accepted → 403 (INV-ONBOARD-DPA-FIRST).
    DpaRequired,
    /// Concurrent tier-select in flight → 409.
    LockHeld,
    /// Tenant already has an active subscription → 409.
    AlreadyActive,
    /// Stripe transport / config failure → 502. Carries a sanitized `detail`
    /// (Stripe's own error text — a masked `authentication_error`, a
    /// `No such price` `invalid_request_error`, or a transport/DNS error;
    /// never the secret key), surfaced in the response body so a prod checkout
    /// failure is diagnosable without container-log access.
    StripeUnavailable(Option<String>),
    /// Audit-sink failure (fail-CLOSED) or other internal fault → 500.
    Internal,
}

impl TierSelectHttpError {
    /// The wire status + machine-readable error code.
    #[must_use]
    pub fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Unauthenticated => (StatusCode::UNAUTHORIZED, "unauthenticated"),
            Self::NoVerifiedTenant => (StatusCode::UNAUTHORIZED, "no_verified_tenant"),
            Self::BadRequest => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::UseInquiryForm => (StatusCode::UNPROCESSABLE_ENTITY, "use_inquiry_form"),
            Self::DpaRequired => (StatusCode::FORBIDDEN, "dpa_required"),
            Self::LockHeld => (StatusCode::CONFLICT, "lock_held"),
            Self::AlreadyActive => (StatusCode::CONFLICT, "already_active"),
            Self::StripeUnavailable(_) => (StatusCode::BAD_GATEWAY, "stripe_unavailable"),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for TierSelectHttpError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();
        let body = match self {
            Self::StripeUnavailable(Some(detail)) => {
                serde_json::json!({ "error": code, "detail": detail })
            }
            _ => serde_json::json!({ "error": code }),
        };
        (status, Json(body)).into_response()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Auth boundary (the ironclad core — fully implemented + unit-tested here)
// ──────────────────────────────────────────────────────────────────────────────

/// Verify the internal-auth shared secret in **constant time**. Returns
/// `Err(Unauthenticated)` on a missing or mismatched header — fail-CLOSED.
///
/// The compare pads the provided value to the expected length and runs a
/// single `ct_eq` over equal-length buffers, then folds in a length-equality
/// bit — so NEITHER the secret length NOR its content is leaked via an early
/// return / branch. Mirrors `admin.rs::internal_auth_ok` / `internal_pat.rs`
/// exactly (F2 fix: `subtle::ct_eq` short-circuits on unequal lengths, which
/// leaked the secret length via timing; the previous direct `ct_eq` here did
/// exactly that).
fn verify_internal_auth(
    headers: &HeaderMap,
    expected_secret: &str,
) -> Result<(), TierSelectHttpError> {
    // Treat a missing header as an empty presented value so the compare runs
    // on the SAME constant-time path (no early return distinguishes
    // missing-header from wrong-secret).
    let presented = headers
        .get(INTERNAL_AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let expected_bytes = expected_secret.as_bytes();
    let provided_bytes = presented.as_bytes();
    // Pad provided to expected length to run ct_eq on equal-length slices,
    // then fold in the real length-equality so a longer/shorter provided
    // value can never match. No branch short-circuits on the secret length.
    let provided_padded: Vec<u8> = if provided_bytes.len() >= expected_bytes.len() {
        provided_bytes
            .get(..expected_bytes.len())
            .unwrap_or(&[])
            .to_vec()
    } else {
        let mut v = provided_bytes.to_vec();
        v.resize(expected_bytes.len(), 0);
        v
    };
    let content_ok = expected_bytes.ct_eq(&provided_padded).unwrap_u8();
    let len_ok = u8::from(expected_bytes.len() == provided_bytes.len());
    if (content_ok & len_ok) == 1 {
        Ok(())
    } else {
        Err(TierSelectHttpError::Unauthenticated)
    }
}

/// Extract the edge-verified tenant id. Fail-CLOSED: a missing or empty
/// header is a hard 401 — a billing mutation MUST NOT proceed without a
/// verified tenant. Never falls back to a default.
fn extract_verified_tenant(headers: &HeaderMap) -> Result<String, TierSelectHttpError> {
    let raw = headers
        .get(TENANT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(TierSelectHttpError::NoVerifiedTenant)?;
    Ok(raw.to_owned())
}

// ──────────────────────────────────────────────────────────────────────────────
// Route state (collaborators wired at boot — see PR2)
// ──────────────────────────────────────────────────────────────────────────────

/// Route state injected at boot. Holds the constant-time internal-auth
/// secret plus the production collaborators wired by the build-state path:
/// the durable D1-over-HTTP store, the hosted-Stripe checkout creator, the
/// fail-CLOSED audit sink, and the current DPA version (for
/// INV-ONBOARD-DPA-FIRST). The collaborator method bodies are stubbed
/// (`todo!("WP-A/B/C")`) in this scaffold increment — the wiring + the
/// security-relevant shape (constant-time auth, secret redaction) are
/// frozen so WP-A/B/C fill the effects against a stable interface.
///
/// The adapters live in sibling modules
/// ([`crate::routes::tier_select_store`] / [`crate::routes::tier_select_checkout`] /
/// [`crate::routes::tier_select_audit`]) and each implements one of the three
/// trait seams below.
#[derive(Clone)]
pub struct TierSelectRouteState {
    /// Shared secret for `X-Corelink-Internal-Auth` (constant-time compare).
    /// NEVER logged (redacted in `Debug`).
    pub internal_auth_key: Arc<str>,
    /// Durable D1-over-HTTP store: lock / DPA / active-subscription /
    /// persist (WP-A).
    pub store: Arc<crate::routes::tier_select_store::D1HttpTierSelectStore>,
    /// Hosted Stripe Checkout creator (`StripeRealClient` via
    /// `spawn_blocking`) (WP-B).
    pub checkout: Arc<crate::routes::tier_select_checkout::StripeCheckoutCreator>,
    /// Fail-CLOSED audit sink — emit BEFORE every mutation (WP-C).
    pub audit: Arc<crate::routes::tier_select_audit::TierSelectAuditAdapter>,
    /// Current DPA version string checked by INV-ONBOARD-DPA-FIRST. Sourced
    /// at boot (env / config); the orchestration passes it to
    /// `store.is_dpa_accepted`.
    pub current_dpa_version: Arc<str>,
}

impl std::fmt::Debug for TierSelectRouteState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `internal_auth_key` is the trust-boundary secret and is ALWAYS
        // redacted. The collaborators carry their own credentials behind
        // their own redacting `Debug` impls (the D1 / Stripe tokens never
        // surface here).
        f.debug_struct("TierSelectRouteState")
            .field("internal_auth_key", &"[REDACTED]")
            .field("store", &self.store)
            .field("checkout", &self.checkout)
            .field("audit", &self.audit)
            .field("current_dpa_version", &self.current_dpa_version)
            .finish()
    }
}

/// Validate the request envelope: auth → verified tenant → typed tier.
/// This is the fully-implemented, side-effect-free security gate that
/// every request crosses BEFORE any D1/Stripe work. Returned to the
/// handler (PR2) which performs the durable orchestration.
fn authorize_and_validate(
    state: &TierSelectRouteState,
    headers: &HeaderMap,
    body: &TierSelectRequest,
) -> Result<(String, RequestedTier), TierSelectHttpError> {
    // (1) internal-auth boundary (constant-time).
    verify_internal_auth(headers, &state.internal_auth_key)?;
    // (2) verified tenant (fail-CLOSED; never from the body).
    let tenant_id = extract_verified_tenant(headers)?;
    // (3) tier parse + enterprise routing.
    let tier = match RequestedTier::parse_self_serve(&body.tier) {
        ParsedTier::Tier(t) => t,
        ParsedTier::Enterprise => return Err(TierSelectHttpError::UseInquiryForm),
        ParsedTier::Invalid => return Err(TierSelectHttpError::BadRequest),
    };
    // (4) F6/F10: redirect URLs must point to a CoreLink-owned origin.
    // The Worker normally builds these server-side from a trusted origin, but
    // an authenticated user can POST directly to the Worker bypassing the
    // admin-ui and supply arbitrary https:// URLs.  Validate both URLs against
    // the host allowlist (parse URL → assert host ∈ ALLOWED_REDIRECT_HOSTS,
    // reject userinfo/@/explicit ports).  Fail-CLOSED: any violation → 400,
    // structured tracing warning.  The https scheme check is also enforced
    // inside `validate_redirect_url`.
    if tier.is_paid() {
        validate_redirect_url(&body.success_url, "success_url")?;
        validate_redirect_url(&body.cancel_url, "cancel_url")?;
    }
    Ok((tenant_id, tier))
}

// ──────────────────────────────────────────────────────────────────────────────
// HTTP transport (WP-E) — axum handler + router.
//
// This layer is deliberately thin: every security + durability invariant
// lives in `authorize_and_validate` (the fail-CLOSED gate) and
// `orchestrate_tier_select` (the durable ordering). The handler only adapts
// bytes ⇄ typed values and the typed error ⇄ HTTP status, so it has no
// branch a test of the gate/orchestration doesn't already cover.
// ──────────────────────────────────────────────────────────────────────────────

/// `POST /v1/onboarding/tier-select`.
///
/// The body is taken as raw [`Bytes`] and parsed here rather than via the
/// `Json` extractor so that EVERY failure — malformed JSON, an unknown field
/// (`deny_unknown_fields`), or a typed orchestration error — returns the one
/// `{ "error": <code> }` envelope instead of axum's default rejection shape.
/// The route is internal-only (mounted behind the Durable Object + the
/// constant-time `X-Corelink-Internal-Auth` gate) and axum's default body
/// limit applies, so parsing the envelope before the auth check inside
/// `authorize_and_validate` is bounded; auth is still step 1 of that gate and
/// nothing downstream runs on a failed gate.
pub async fn handle(
    State(state): State<TierSelectRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Parse the envelope (`deny_unknown_fields` blocks a smuggled tenant_id);
    // any parse error is a 400 in the canonical shape.
    let req: TierSelectRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(_) => return TierSelectHttpError::BadRequest.into_response(),
    };

    // Fail-CLOSED gate: internal-auth (constant-time) → verified tenant
    // (header-only, never the body) → typed tier (+ https redirect check).
    let (tenant_id, tier) = match authorize_and_validate(&state, &headers, &req) {
        Ok(parts) => parts,
        Err(e) => return e.into_response(),
    };

    // Time-sortable correlation id threading the request through the audit
    // chain + durable store (mirrors the crate's `Uuid::now_v7` convention).
    let correlation_id = Uuid::now_v7().to_string();
    let now_ms = unix_millis_now();

    // The durable orchestration owns the load-bearing order (audit-before-
    // mutate → lock → DPA-first → active-sub → checkout → persist → release);
    // map its typed result to HTTP.
    match orchestrate_tier_select(
        &*state.store,
        &*state.checkout,
        &*state.audit,
        &tenant_id,
        tier,
        &req.success_url,
        &req.cancel_url,
        &state.current_dpa_version,
        now_ms,
        &correlation_id,
    )
    .await
    {
        Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Epoch-millisecond clock for lock expiry + audit timestamps. A pre-epoch
/// system clock is impossible on a deployed container; the saturating
/// fallback keeps the handler total rather than letting it panic.
fn unix_millis_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Build the tier-select router. `main.rs` mounts it via
/// [`build_state_from_env`] only when the internal-auth + D1 + Stripe
/// configuration is present (the same env gate as `internal_pat`), so the
/// route is never reachable without its production collaborators wired.
pub fn router(state: TierSelectRouteState) -> Router {
    Router::new()
        .route("/v1/onboarding/tier-select", post(handle))
        .with_state(state)
}

/// Dedicated environment name for the tier-select money-path credential.
const TIER_SELECT_AUTH_KEY_ENV: &str = "CORELINK_TIER_SELECT_AUTH_KEY";

/// Resolve the tier-select credential through the common 32-character
/// fail-closed gate. A deployment may use the shared key during migration, but
/// a valid dedicated key always wins and can therefore be rotated independently.
fn tier_select_auth_key_from_env() -> Option<Arc<str>> {
    crate::routes::admin::resolve_internal_auth_key(TIER_SELECT_AUTH_KEY_ENV)
}

/// Assemble the production [`TierSelectRouteState`] from the environment, or
/// `None` when the route must NOT be mounted. **Fail-safe:** the money path is
/// mounted ONLY when the dedicated internal-auth secret (or its documented
/// shared fallback), the D1 config, the Stripe config, AND the current DPA
/// version are all present; a missing/short secret or any client-init failure
/// leaves `/v1/onboarding/tier-select` unmounted (404) rather than half-wired.
/// Mirrors
/// [`crate::routes::internal_pat::build_state_from_env`].
#[must_use]
pub fn build_state_from_env() -> Option<TierSelectRouteState> {
    let auth_key = tier_select_auth_key_from_env()?;

    let Some(dpa_version) = std::env::var("CORELINK_DPA_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
    else {
        tracing::warn!("CORELINK_DPA_VERSION unset; /v1/onboarding/tier-select NOT mounted");
        return None;
    };

    // D1 config travels in `StorageEnv` (CF account / token / database id).
    // Shared as one `Arc<D1HttpClient>` between the store AND the audit
    // adapter so both durable collaborators ride the same connection.
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = match crate::storage::d1_http::D1HttpClient::new(&storage_env) {
        Ok(client) => Arc::new(client),
        Err(e) => {
            tracing::warn!(error = %e, "D1HttpClient init failed; tier-select NOT mounted");
            return None;
        }
    };

    let stripe = match corelink_stripe_real::StripeRealClient::from_env() {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!(error = %e, "StripeRealClient init failed; tier-select NOT mounted");
            return None;
        }
    };

    Some(TierSelectRouteState {
        internal_auth_key: auth_key,
        store: Arc::new(
            crate::routes::tier_select_store::D1HttpTierSelectStore::new(Arc::clone(&d1)),
        ),
        checkout: Arc::new(
            crate::routes::tier_select_checkout::StripeCheckoutCreator::new(Arc::new(stripe)),
        ),
        audit: Arc::new(crate::routes::tier_select_audit::TierSelectAuditAdapter::new(d1)),
        current_dpa_version: Arc::from(dpa_version),
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Durable orchestration (the ironclad heart) — store trait + fail-closed order.
//
// The store trait isolates the durable D1-over-HTTP effects so the
// orchestration ORDER (the load-bearing invariants) is testable natively
// against an in-memory store. The production `D1HttpTierSelectStore`
// (next increment) implements the same trait against D1 via
// `INSERT OR IGNORE ... RETURNING` (lock) + `SELECT` (DPA / active) +
// `UPDATE`/`INSERT` (persist). Generic (not `dyn`) to keep `async fn` in
// trait object-safety out of scope.
// ──────────────────────────────────────────────────────────────────────────────

/// The price/customer details returned by Stripe for a paid checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutCreated {
    /// Stripe-hosted Checkout URL (always `https://`).
    pub checkout_url: String,
    /// Stripe Checkout Session id (`cs_...`).
    pub session_id: String,
    /// Stripe customer id (`cus_...`).
    pub stripe_customer_id: String,
}

/// Durable effects backing the orchestration. Every method is fail-CLOSED:
/// an `Err` aborts the orchestration WITHOUT partial state (the caller
/// maps it to 500/502/409 as appropriate).
pub trait TierSelectStore {
    /// Acquire the 60s row lock for `tenant_id` (`INSERT OR IGNORE ...
    /// RETURNING`). Returns `Ok(true)` if acquired, `Ok(false)` if a live
    /// lock is already held (→ 409 `lock_held`).
    fn acquire_lock(
        &self,
        tenant_id: &str,
        now_ms: i64,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, String>> + Send;

    /// `true` iff `tenant_id` has accepted `dpa_version` (INV-ONBOARD-DPA-FIRST).
    fn is_dpa_accepted(
        &self,
        tenant_id: &str,
        dpa_version: &str,
    ) -> impl std::future::Future<Output = Result<bool, String>> + Send;

    /// `true` iff `tenant_id` already has an `active` (cache-axis) subscription.
    fn has_active_subscription(
        &self,
        tenant_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, String>> + Send;

    /// Repair mirror rows from the recoverable checkout ledger and expire
    /// abandoned pre-Stripe reservations before the active-subscription gate.
    fn reconcile_pending_checkout(
        &self,
        tenant_id: &str,
        now_ms: i64,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    /// `true` iff `tenant_id` already holds an active/trialing RUNNER
    /// subscription (`runner_billing`, migration 0087). The runner axis is
    /// SEPARATE from the cache [`has_active_subscription`](Self::has_active_subscription):
    /// a tenant may hold a cache tier AND a runner tier simultaneously, so the
    /// orchestration guards each axis independently. This guards ONLY against a
    /// second concurrent runner subscription.
    fn has_active_runner_subscription(
        &self,
        tenant_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, String>> + Send;

    /// Reserve the tenant's single payable slot before the network call. The
    /// reservation closes the lock-expiry window where Stripe could create a
    /// payable session before the D1 mirror is written.
    fn reserve_pending_checkout(
        &self,
        tenant_id: &str,
        tier: RequestedTier,
        now_ms: i64,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    /// Persist a paid-tier pending-checkout through the ownership ledger and
    /// recoverable `tier_selections`/`stripe_checkout_sessions` mirrors.
    fn persist_pending_checkout(
        &self,
        tenant_id: &str,
        tier: RequestedTier,
        created: &CheckoutCreated,
        now_ms: i64,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    /// Remove only this request's uncompleted reservation after a failed
    /// Stripe/mirror attempt; an activated row is never rolled back.
    fn abandon_pending_checkout(
        &self,
        tenant_id: &str,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    /// Persist an instant free-tier activation.
    fn persist_free_active(
        &self,
        tenant_id: &str,
        now_ms: i64,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;

    /// Release only the row lock owned by this correlation (best-effort; lock
    /// also self-expires at 60s).
    fn release_lock(
        &self,
        tenant_id: &str,
        correlation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send;
}

/// Create a Stripe Checkout Session for a paid tier. Isolated so the
/// orchestration is testable without real HTTP. The production adapter
/// calls `corelink_stripe_real::StripeRealClient` (sync, `reqwest::blocking`)
/// via `tokio::task::spawn_blocking`.
pub trait CheckoutCreator {
    /// Create the hosted Checkout Session, or `Err` (→ 502 `stripe_unavailable`).
    fn create(
        &self,
        tenant_id: &str,
        tier: RequestedTier,
        success_url: &str,
        cancel_url: &str,
    ) -> impl std::future::Future<Output = Result<CheckoutCreated, String>> + Send;
}
