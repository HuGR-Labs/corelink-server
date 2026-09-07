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
