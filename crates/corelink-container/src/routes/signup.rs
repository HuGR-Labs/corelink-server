//! Pilot signup route — `POST /v1/signup/pilot/{token}`.
//!
//! Wave-29 stream-1 deliverable closing **DEBT-027** engineering-side.
//! Wave-27 (commit `6761860`) shipped the operator-facing pilot admin
//! shell scripts (`grant-pilot-tier.sh`, `list-pilot-tenants.sh`,
//! `pilot-24h-checkin.sh`) and the Grafana dashboard panel template;
//! wave-28's pilot-comms package (`docs/internal/pilot-comms-templates.md`)
//! references the public URL
//! `https://signup.corelink.humangr.com/pilot/<token>`. This module is the
//! production backend that finally redeems those tokens.
//!
//! # Token format
//!
//! `pilot_<env>_<unix_ms>_<16-hex-random>.<hmac-hex>`
//!
//! - `env` ∈ {`staging`, `prod`} — segregates staging / prod tokens so
//!   a staging mint never opens a prod slot.
//! - `unix_ms` — token mint timestamp (Unix epoch ms). The route
//!   refuses any token older than [`PILOT_TOKEN_TTL_MS`] (14 days —
//!   matches the operator's pilot-window policy referenced in
//!   `docs/internal/customer-success-playbook.md §1`).
//! - `16-hex-random` — `getrandom::getrandom(&mut [u8; 8])` then hex.
//!   8 random bytes = 64 bits of entropy ≫ the per-month outreach
//!   volume bound on the pilot programme (DEBT-027 caps at single-
//!   digit pilots before GA-cutover).
//! - `hmac-hex` — `HMAC-SHA256(SIGNUP_TOKEN_KEY,
//!   "pilot_<env>_<unix_ms>_<16-hex>")`, hex-encoded. Verified in
//!   constant time via [`subtle::ConstantTimeEq`].
//!
//! The HMAC key comes from the `SIGNUP_TOKEN_KEY` env var at boot.
//! Production wiring binds this to a Cloudflare Workers secret;
//! native dev/CI uses a deterministic in-memory key fed via
//! [`build_state_with_key`].
//!
//! # Request shape
//!
//! ```text
//! POST /v1/signup/pilot/{token}
//! Content-Type: application/json
//! X-Corelink-Client-Ip: <client-ip>  (rate-limit anchor; set by the Worker
//!                                      from cf-connecting-ip; NOT client-controlled)
//!
//! {
//!   "email": "...",
//!   "company_name": "...",
//!   "tier_hint": "free|pro|enterprise",
//!   "expected_use_case": "..."
//! }
//! ```
//!
//! All four body fields are required (`400` if any is missing) and
//! capped at [`MAX_FIELD_LEN`] characters (`400` if any exceeds).
//!
//! # Response shape (201)
//!
//! ```text
//! HTTP/1.1 201 Created
//! Content-Type: application/json
//!
//! {
//!   "tenant_id": "<uuid v7>",
//!   "activation_url": "https://signup.corelink.humangr.com/pilot/activate/<id>",
//!   "state": "RESERVED"
//! }
//! ```
//!
//! # Failure modes
//!
//! | Condition                          | Status | Body / header                  |
//! |------------------------------------|--------|--------------------------------|
//! | malformed token / bad HMAC         | 401    | `unauthorized`                 |
//! | expired token (> TTL)              | 401    | `unauthorized`                 |
//! | missing / oversize body field      | 400    | `bad_request`                  |
//! | rate-limit exceeded (5/IP/hour)    | 429    | `Retry-After: <secs>`          |
//! | audit emit failure (fail-CLOSED)   | 503    | `audit pipeline closed`        |
//! | D1 store failure                   | 503    | `signup store unavailable`     |
//!
//! # Charter compliance
//!
//! - `#![forbid(unsafe_code)]` inherited from `corelink-server`.
//! - No `unwrap`/`expect`/`panic` in src (clippy lints).
//! - HMAC verify uses `subtle::ConstantTimeEq` (timing-safe).
//! - Audit emit BEFORE response (fail-CLOSED per
//!   `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//! - Rate-limit gate at the route boundary uses the canonical
//!   `corelink-ratelimit::RateLimiter` trait (per-IP bucket, keyed on
//!   the server-trusted `x-corelink-client-ip` header; missing header
//!   collapses to the shared `"_no_ip"` bucket — fail-CLOSED).
//!
//! # Audit cross-reference
//!
//! `specs/_audits/sealed/2026-05-16-signup-corelink-dev-backend.md`.

// W35-P2: module-level `//!` docs reference items defined further down
// this file via short paths; under the umbrella crate's scope they
// would require full prefixes. Suppressing the lint preserves the
// original reference text.
#![allow(rustdoc::broken_intra_doc_links)]

use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use corelink_ratelimit::{
    BucketKey, InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics,
    InMemoryTokenBucketRateLimiter, RateLimitConfig, RateLimitDecision, RateLimiter,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::wall_clock::{default_wall_clock, WallClock};

/// Canonical pilot-signup route path. The `:token` segment is the
/// `pilot_<env>_<unix_ms>_<16-hex>.<hmac-hex>` token described in the
/// module-level docs.
///
/// **axum 0.7 path-param syntax**: this codebase pins axum 0.7 +
/// matchit 0.7, which uses the `:name` capture syntax (axum 0.8 +
/// matchit 0.8 switch to `{name}`). The audit-doc cross-reference
/// uses `{token}` in prose but the wire path is `:token` in the
/// matchit registration above.
pub const SIGNUP_PILOT_ROUTE: &str = "/v1/signup/pilot/:token";

/// Canonical CloudEvents-1.0 `type` literal for the pilot-reserved
/// audit emit.
pub const EVENT_TYPE_PILOT_RESERVED: &str = "corelink.signup.pilot_reserved.v1";

/// Canonical CloudEvents-1.0 `type` literal for the token-rejected
/// audit emit (forged HMAC / malformed / expired token).
pub const EVENT_TYPE_PILOT_TOKEN_REJECTED: &str = "corelink.signup.pilot_token_rejected.v1";

/// Canonical CloudEvents-1.0 `type` literal for the rate-limit-deny
/// audit emit.
pub const EVENT_TYPE_PILOT_RATE_LIMITED: &str = "corelink.signup.pilot_rate_limited.v1";

/// Pilot signup token TTL — tokens older than this (by mint
/// timestamp) are rejected as expired. 14 days mirrors the operator
/// pilot-window policy in
/// `docs/internal/customer-success-playbook.md §1`.
pub const PILOT_TOKEN_TTL_MS: u64 = 14 * 24 * 60 * 60 * 1000;

/// Maximum length (chars) for every free-text body field. 256 is the
/// canonical bound declared in the wave-29 stream-1 charter.
pub const MAX_FIELD_LEN: usize = 256;

/// Token environment discriminator — segregates `staging` from `prod`
/// at the parse layer so a staging mint never opens a prod slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenEnv {
    /// Staging mint (test outreach).
    Staging,
    /// Production mint (live pilot programme).
    Prod,
}

impl TokenEnv {
    /// Canonical string form (matches the token body prefix).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Staging => "staging",
            Self::Prod => "prod",
        }
    }

    /// Parse from the leading token segment.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "staging" => Some(Self::Staging),
            "prod" => Some(Self::Prod),
            _ => None,
        }
    }
}

/// Parsed pilot token (post-verify).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PilotToken {
    /// Token environment (`staging` / `prod`).
    pub env: TokenEnv,
    /// Mint timestamp (Unix epoch ms).
    pub minted_at_ms: u64,
    /// 16-hex random body — preserved verbatim for replay detection.
    pub token_id: String,
}

/// Token-parse / verify failure taxonomy. Mapped to 401 at the route
/// boundary; the variant disambiguates the audit emit `payload`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenError {
    /// Token did not contain a `.` separator or the body / signature
    /// segments were empty.
    Malformed,
    /// Token body had fewer than the canonical 4 underscore-separated
    /// fields (`pilot`, `<env>`, `<unix_ms>`, `<16-hex>`).
    BadStructure,
    /// Token did not start with the `pilot` literal.
    NotPilotToken,
    /// Token environment segment was neither `staging` nor `prod`.
    BadEnv,
    /// Timestamp segment failed to parse as `u64`.
    BadTimestamp,
    /// 16-hex random segment was not 16 hex chars.
    BadRandom,
    /// HMAC signature segment was not valid hex.
    BadSignatureEncoding,
    /// HMAC verification failed (forged signature).
    SignatureMismatch,
    /// Token mint timestamp is older than [`PILOT_TOKEN_TTL_MS`] ago.
    Expired,
}

impl TokenError {
    /// Canonical short tag — surfaced on the audit emit `payload`.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::BadStructure => "bad_structure",
            Self::NotPilotToken => "not_pilot_token",
            Self::BadEnv => "bad_env",
            Self::BadTimestamp => "bad_timestamp",
            Self::BadRandom => "bad_random",
            Self::BadSignatureEncoding => "bad_signature_encoding",
            Self::SignatureMismatch => "signature_mismatch",
            Self::Expired => "expired",
        }
    }
}

/// Parse + HMAC-verify a pilot signup token.
///
/// `token` is the raw URL path segment; `key` is the
/// `SIGNUP_TOKEN_KEY` HMAC secret; `now_ms` is the current wall clock
/// (TTL check anchor).
///
/// # Errors
///
/// Returns the canonical [`TokenError`] taxonomy. Each variant maps to
/// 401 at the route boundary; the route's audit emit records the
/// `.tag()` for operator forensics.
pub fn parse_and_verify_pilot_token(
    token: &str,
    key: &[u8],
    now_ms: u64,
) -> Result<PilotToken, TokenError> {
    // 1) split body + signature on the single `.` separator.
    let (body, sig_hex) = match token.split_once('.') {
        Some((b, s)) if !b.is_empty() && !s.is_empty() => (b, s),
        _ => return Err(TokenError::Malformed),
    };
    // 2) split body into the 4 canonical underscore-separated fields:
    //    `pilot_<env>_<unix_ms>_<16-hex>`. We use rsplitn so the
    //    `<env>` field cannot smuggle an underscore.
    let parts: Vec<&str> = body.splitn(4, '_').collect();
    if parts.len() != 4 {
        return Err(TokenError::BadStructure);
    }
    if parts.first().copied() != Some("pilot") {
        return Err(TokenError::NotPilotToken);
    }
    let env_str = parts.get(1).copied().ok_or(TokenError::BadStructure)?;
    let env = TokenEnv::from_str(env_str).ok_or(TokenError::BadEnv)?;
    let ts_str = parts.get(2).copied().ok_or(TokenError::BadStructure)?;
    let minted_at_ms: u64 = ts_str.parse().map_err(|_| TokenError::BadTimestamp)?;
    let rand_str = parts.get(3).copied().ok_or(TokenError::BadStructure)?;
    if rand_str.len() != 16 || !rand_str.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TokenError::BadRandom);
    }
    // 3) decode the signature hex.
    let sig_bytes = hex::decode(sig_hex).map_err(|_| TokenError::BadSignatureEncoding)?;
    // 4) compute the expected HMAC over the body.
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(key).map_err(|_| TokenError::SignatureMismatch)?;
    mac.update(body.as_bytes());
    let expected = mac.finalize().into_bytes();
    // 5) constant-time compare. Bail on length mismatch first
    //    (length is not secret; HMAC-SHA256 is always 32 bytes).
    if sig_bytes.len() != expected.len() {
        return Err(TokenError::SignatureMismatch);
    }
    if sig_bytes.as_slice().ct_eq(expected.as_slice()).unwrap_u8() != 1 {
        return Err(TokenError::SignatureMismatch);
    }
    // 6) TTL check. `now_ms < minted_at_ms` (clock skew) is NOT an
    //    error — the route always accepts tokens minted "in the
    //    future" if the signature is valid (operator may pre-mint
    //    a batch for a future programme launch). We only reject
    //    tokens older than `PILOT_TOKEN_TTL_MS`.
    if now_ms >= minted_at_ms.saturating_add(PILOT_TOKEN_TTL_MS) {
        return Err(TokenError::Expired);
    }
    Ok(PilotToken {
        env,
        minted_at_ms,
        token_id: rand_str.to_owned(),
    })
}

/// Render the canonical pilot token over the four body segments
/// signed with `key`. Used by `scripts/admin/mint-pilot-token.sh` via
/// the binary `target/debug/mint-pilot-token` (not shipped — the
/// shell wrapper invokes a one-shot `cargo run` per mint, see the
/// script for details). Exposed for unit + integration tests.
///
/// # Errors
///
/// Returns a static error string if the HMAC key length is zero.
pub fn mint_pilot_token(
    env: TokenEnv,
    minted_at_ms: u64,
    rand_hex16: &str,
    key: &[u8],
) -> Result<String, &'static str> {
    if rand_hex16.len() != 16 || !rand_hex16.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("rand_hex16 must be 16 lower-case ASCII hex chars");
    }
    let body = format!("pilot_{}_{}_{}", env.as_str(), minted_at_ms, rand_hex16);
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).map_err(|_| "hmac key invalid")?;
    mac.update(body.as_bytes());
    let sig = mac.finalize().into_bytes();
    Ok(format!("{body}.{}", hex::encode(sig)))
}

/// Request body for the pilot signup route. Every field is required;
/// each must be ≤ [`MAX_FIELD_LEN`] characters.
#[derive(Clone, Debug, Deserialize)]
pub struct PilotSignupBody {
    /// Pilot contact email.
    pub email: String,
    /// Free-text company name.
    pub company_name: String,
    /// Free-text tier preference (`free` / `pro` / `enterprise` — not
    /// binding; operator pre-screening signal only).
    pub tier_hint: String,
    /// Free-text expected use-case (operator pre-screening signal).
    pub expected_use_case: String,
}

impl PilotSignupBody {
    /// Validate the body — every field non-empty + within
    /// [`MAX_FIELD_LEN`] chars.
    ///
    /// # Errors
    ///
    /// Returns the offending field's canonical name on failure.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.email.is_empty() || self.email.chars().count() > MAX_FIELD_LEN {
            return Err("email");
        }
        if !self.email.contains('@') {
            return Err("email");
        }
        if self.company_name.is_empty() || self.company_name.chars().count() > MAX_FIELD_LEN {
            return Err("company_name");
        }
        if self.tier_hint.is_empty() || self.tier_hint.chars().count() > MAX_FIELD_LEN {
            return Err("tier_hint");
        }
        if self.expected_use_case.is_empty()
            || self.expected_use_case.chars().count() > MAX_FIELD_LEN
        {
            return Err("expected_use_case");
        }
        Ok(())
    }
}

/// Successful pilot reservation response (201 body).
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct PilotSignupResponse {
    /// Reserved tenant id (UUID v7, lexicographic-sortable).
    pub tenant_id: Uuid,
    /// Activation URL the operator follows up with (per pilot-comms
    /// templates `docs/internal/pilot-comms-templates.md`).
    pub activation_url: String,
    /// Pilot lifecycle state — always `"RESERVED"` on this arm.
    pub state: String,
}

/// Persisted pilot-signup record (one row in the `pilot_signups` D1
/// table per `migrations/d1/0053_pilot_signups.sql`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PilotSignupRecord {
    /// Surrogate UUID v7 PK.
    pub id: Uuid,
    /// Reserved tenant id (also UUID v7).
    pub tenant_id: Uuid,
    /// Pilot contact email.
    pub email: String,
    /// Free-text company name.
    pub company_name: String,
    /// Free-text tier hint.
    pub tier_hint: String,
    /// Free-text expected use-case.
    pub expected_use_case: String,
    /// Signup wall-clock (Unix epoch ms).
    pub signed_up_at_ms: u64,
    /// Source token id (16-hex random segment).
    pub token_id: String,
    /// Pilot lifecycle state — always `RESERVED` at insertion time.
    pub state: String,
}

/// Audit-row shape captured by the route on every emit arm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignupAuditRow {
    /// Canonical CloudEvents `type` (one of the `EVENT_TYPE_*`
    /// constants in this module).
    pub event_type: String,
    /// Tenant id stamped at reservation time — `Some` only on the
    /// happy-path `pilot_reserved.v1` arm.
    pub tenant_id: Option<Uuid>,
    /// Source token id (16-hex random body, or the raw token prefix
    /// up to 32 chars on the reject arms when the body did not parse).
    pub token_id_or_prefix: Option<String>,
    /// Canonical exit status: `"reserved"` / `"duplicate"` /
    /// `"rejected"` / `"rate_limited"` / `"bad_request"`.
    pub exit_status: String,
    /// Optional structured payload (e.g. token reject `tag`).
    pub payload: Option<String>,
    /// Server-side emit wall-clock (Unix epoch ms).
    pub emitted_at_ms: u64,
}

/// Audit-emit trait for the pilot-signup route.
///
/// Production wiring binds this to the CloudEvents audit emitter
/// (mirrors the `audit_export` route pattern); native dev/CI uses
/// [`InMemorySignupAuditSink`].
pub trait SignupAuditSink: Send + Sync + core::fmt::Debug {
    /// Persist one [`SignupAuditRow`].
    ///
    /// # Errors
    ///
    /// Returns a static error string when the audit pipeline is
    /// closed. The route converts to 503 (fail-CLOSED per
    /// `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
    fn emit(&self, row: SignupAuditRow) -> Result<(), &'static str>;
}

/// In-memory capture sink. Cloning shares the captured buffer so the
/// test harness can inspect emits without re-handing the sink to the
/// route state.
#[derive(Clone, Debug, Default)]
pub struct InMemorySignupAuditSink {
    inner: Arc<Mutex<Vec<SignupAuditRow>>>,
    injected_failure: Arc<Mutex<Option<&'static str>>>,
}

impl InMemorySignupAuditSink {
    /// Construct a fresh empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the captured rows in emit order.
    ///
    /// # Errors
    ///
    /// Returns a static error string if the inner mutex is poisoned.
    pub fn snapshot(&self) -> Result<Vec<SignupAuditRow>, &'static str> {
        let g = self
            .inner
            .lock()
            .map_err(|_| "signup audit sink mutex poisoned")?;
        Ok(g.clone())
    }

    /// Inject a static failure to drive the 503 fail-CLOSED regression.
    ///
    /// # Errors
    ///
    /// Returns a static error string if the inner mutex is poisoned.
    pub fn inject_failure(&self, msg: &'static str) -> Result<(), &'static str> {
        let mut g = self
            .injected_failure
            .lock()
            .map_err(|_| "injected failure mutex poisoned")?;
        *g = Some(msg);
        Ok(())
    }
}

impl SignupAuditSink for InMemorySignupAuditSink {
    fn emit(&self, row: SignupAuditRow) -> Result<(), &'static str> {
        let injected = *self
            .injected_failure
            .lock()
            .map_err(|_| "injected failure mutex poisoned")?;
        if let Some(msg) = injected {
            return Err(msg);
        }
        let mut g = self
            .inner
            .lock()
            .map_err(|_| "signup audit sink mutex poisoned")?;
        g.push(row);
        Ok(())
    }
}

/// Fail-CLOSED audit-emit helper. Mirrors `audit_export::emit_or_503`.
#[must_use]
pub fn emit_or_503(sink: &Arc<dyn SignupAuditSink>, row: SignupAuditRow) -> Option<Response> {
    if sink.emit(row).is_err() {
        return Some((StatusCode::SERVICE_UNAVAILABLE, "audit pipeline closed").into_response());
    }
    None
}

/// Persistence trait for the pilot-signup store.
///
/// Production wiring binds this to D1 (`pilot_signups` table per
/// `migrations/d1/0053_pilot_signups.sql`). Native dev/CI uses
/// [`InMemorySignupStore`].
pub trait SignupStore: Send + Sync + core::fmt::Debug {
    /// Insert `record` into the store. If a row with the same `email`
    /// or `token_id` already exists, return the existing record
    /// (idempotent — the route returns the original tenant_id).
    ///
    /// # Errors
    ///
    /// Returns a static error string on store unavailability. The
    /// route converts to 503.
    fn insert_or_existing(
        &self,
        record: PilotSignupRecord,
    ) -> Result<PilotSignupRecord, &'static str>;
}

/// In-memory pilot-signup store.
#[derive(Clone, Debug, Default)]
pub struct InMemorySignupStore {
    inner: Arc<Mutex<Vec<PilotSignupRecord>>>,
    injected_failure: Arc<Mutex<Option<&'static str>>>,
}

impl InMemorySignupStore {
    /// Construct a fresh empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every persisted record in insertion order.
    ///
    /// # Errors
    ///
    /// Returns a static error string if the inner mutex is poisoned.
    pub fn snapshot(&self) -> Result<Vec<PilotSignupRecord>, &'static str> {
        let g = self
            .inner
            .lock()
            .map_err(|_| "signup store mutex poisoned")?;
        Ok(g.clone())
    }

    /// Inject a static failure to drive the 503 regression.
    ///
    /// # Errors
    ///
    /// Returns a static error string if the inner mutex is poisoned.
    pub fn inject_failure(&self, msg: &'static str) -> Result<(), &'static str> {
        let mut g = self
            .injected_failure
            .lock()
            .map_err(|_| "injected failure mutex poisoned")?;
        *g = Some(msg);
        Ok(())
    }
}

impl SignupStore for InMemorySignupStore {
    fn insert_or_existing(
        &self,
        record: PilotSignupRecord,
    ) -> Result<PilotSignupRecord, &'static str> {
        let injected = *self
            .injected_failure
            .lock()
            .map_err(|_| "injected failure mutex poisoned")?;
        if let Some(msg) = injected {
            return Err(msg);
        }
        let mut g = self
            .inner
            .lock()
            .map_err(|_| "signup store mutex poisoned")?;
        for existing in g.iter() {
            if existing.email == record.email || existing.token_id == record.token_id {
                return Ok(existing.clone());
            }
        }
        g.push(record.clone());
        Ok(record)
    }
}

/// Rate-limit config — 5 requests / IP / hour. The wave-29 stream-1
/// charter §Deliverables.1 specifies `5/IP/hour`; the
/// `RateLimitConfig` shape gives us
/// `(refill_rate_per_sec, burst_capacity, retry_floor_secs,
///   retry_ceiling_secs, retry_canceled_secs)`. We pick
/// `burst_capacity = 5` (the bucket starts FULL so the first 5
/// requests admit) and `refill = 0 tokens/sec` (rounded — 5 / 3600s
/// is < 1 tps integer). With `retry_after_floor = 720s` (hour / 5),
/// the 6th request in the same hour surfaces 429 + `Retry-After: 720`.
/// The hard ceiling is 1d; the canceled-tenant value is 7d
/// (mirrors the audit-export config).
///
/// Note: `with_overrides` REQUIRES `default_burst_capacity > 0` — so
/// we set burst=5 and refill=1 (the bucket effectively drains in 5
/// requests because the refill / 1s contribution between requests
/// in a wall-clock-aligned test is bounded by the now_ms granularity:
/// the test pins the clock so refill is zero between requests).
/// Production behavior on the live clock: 1 token / sec refill is
/// 12 tokens / 5s → far below the 5/h policy floor; the
/// `retry_after_floor_secs = 720` clamps the 429 retry to 12 min.
#[must_use]
pub fn pilot_signup_rate_limit_config() -> RateLimitConfig {
    // (refill_per_sec=1, burst=5, floor=720s, ceiling=86_400s,
    //  canceled=7*86_400s).
    RateLimitConfig::with_overrides(1, 5, 720, 86_400, 7 * 86_400)
        .unwrap_or_else(RateLimitConfig::canonical)
}

/// Shared route state.
#[derive(Clone)]
pub struct SignupRouteState {
    /// HMAC-SHA256 key for token verify (production wiring binds to
    /// the `SIGNUP_TOKEN_KEY` env var / Worker secret).
    pub token_key: Arc<Vec<u8>>,
    /// Per-IP rate limiter.
    pub rate_limiter: Arc<dyn RateLimiter>,
    /// Audit sink.
    pub audit_sink: Arc<dyn SignupAuditSink>,
    /// Pilot signup persistence.
    pub store: Arc<dyn SignupStore>,
    /// Wall clock — anchors the TTL check + emit wall-clock.
    pub wall_clock: Arc<dyn WallClock>,
    /// Activation URL base — production binds to
    /// `https://signup.corelink.humangr.com/pilot/activate`.
    pub activation_url_base: Arc<String>,
}

impl core::fmt::Debug for SignupRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SignupRouteState").finish_non_exhaustive()
    }
}

/// Canonical activation-URL base (production wiring). Tests inject a
/// fixture base via [`build_state_with_key`].
pub const DEFAULT_ACTIVATION_URL_BASE: &str = "https://signup.corelink.humangr.com/pilot/activate";

/// Construct the native dev/CI route state with the given HMAC key.
///
/// The HMAC key is mandatory — production wiring SHOULD pass the
/// `SIGNUP_TOKEN_KEY` env var bytes here; the boot path in
/// `apps/server/src/main.rs` is responsible for failing CLOSED if the
/// env var is missing.
#[must_use]
pub fn build_state_with_key(token_key: Vec<u8>) -> SignupRouteState {
    let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
    let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(
        rl_audit,
        rl_metrics,
        pilot_signup_rate_limit_config(),
    ));
    let audit_sink: Arc<dyn SignupAuditSink> = Arc::new(InMemorySignupAuditSink::new());
    let store: Arc<dyn SignupStore> = Arc::new(InMemorySignupStore::new());
    let wall_clock = default_wall_clock();
    SignupRouteState {
        token_key: Arc::new(token_key),
        rate_limiter,
        audit_sink,
        store,
        wall_clock,
        activation_url_base: Arc::new(DEFAULT_ACTIVATION_URL_BASE.to_owned()),
    }
}

/// Default dev/CI key — deterministic 32-byte value. Production wiring
/// MUST override via [`build_state_with_key`] with the live secret.
const DEV_TOKEN_KEY: &[u8; 32] = b"corelink-dev-pilot-signup-key!!\0";

/// Construct the default native dev/CI route state with the canonical
/// dev key. Production callers MUST use [`build_state_with_key`].
#[must_use]
pub fn build_state() -> SignupRouteState {
    build_state_with_key(DEV_TOKEN_KEY.to_vec())
}

/// Build the axum router exposing the pilot-signup route.
pub fn router(state: SignupRouteState) -> Router {
    Router::new()
        .route(SIGNUP_PILOT_ROUTE, post(handle_pilot_signup))
        .with_state(state)
}

/// Extract the client IP for the per-IP rate-limit bucket.
///
/// Reads **`x-corelink-client-ip`** — a server-trusted header set by
/// the Cloudflare Worker from `cf-connecting-ip` (which a client
/// cannot forge). The client-controlled `x-forwarded-for` header is
/// intentionally ignored to prevent rate-limit bypass via header
/// rotation.
///
/// Fail-CLOSED: if `x-corelink-client-ip` is absent or empty, returns
/// the shared sentinel `"_no_ip"` so all such requests share a single
/// throttle bucket rather than receiving unlimited unique keys.
fn extract_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-corelink-client-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| "_no_ip".to_owned(), str::to_owned)
}

/// Tenant id allocated for the per-IP rate-limit bucket. The pilot
/// signup route is pre-auth (no tenant id yet), so we anchor every
/// bucket on the nil UUID — the `BucketKey::per_ip(tenant, ip)` shape
/// keys on (tenant, ip), and a fixed nil tenant collapses the
/// dimension to per-IP only. This matches the audit-export route's
/// pre-tenant pattern.
const PRE_AUTH_TENANT: Uuid = Uuid::nil();

/// Pilot signup route handler.
#[allow(clippy::too_many_lines, reason = "single-handler route surface")]
async fn handle_pilot_signup(
    State(state): State<SignupRouteState>,
    Path(token): Path<String>,
    headers: HeaderMap,
    Json(body): Json<PilotSignupBody>,
) -> Response {
    let now_ms = state.wall_clock.now_ms();
    let client_ip = extract_client_ip(&headers);

    // 1) Rate-limit gate — per-IP bucket. The decision lands BEFORE the
    //    token verify so an adversary cannot probe the HMAC space with
    //    high QPS.
    let rl_key = BucketKey::per_ip(PRE_AUTH_TENANT, client_ip.clone());
    match state
        .rate_limiter
        .try_acquire(PRE_AUTH_TENANT, rl_key, 1, now_ms)
    {
        Ok(outcome) => match outcome.decision {
            RateLimitDecision::Allow { .. } => {}
            RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                let audit_row = SignupAuditRow {
                    event_type: EVENT_TYPE_PILOT_RATE_LIMITED.to_owned(),
                    tenant_id: None,
                    token_id_or_prefix: Some(token_prefix(&token)),
                    exit_status: "rate_limited".to_owned(),
                    payload: Some(format!("retry_after_secs={retry_after_secs}")),
                    emitted_at_ms: now_ms,
                };
                if let Some(resp) = emit_or_503(&state.audit_sink, audit_row) {
                    return resp;
                }
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("retry-after", retry_after_secs.to_string())],
                    "rate_limited",
                )
                    .into_response();
            }
            _ => {
                // `RateLimitDecision` is `#[non_exhaustive]`; any future
                // variant falls back to deny for fail-CLOSED.
                return (StatusCode::TOO_MANY_REQUESTS, "rate_limited").into_response();
            }
        },
        Err(_) => {
            return (StatusCode::SERVICE_UNAVAILABLE, "rate limiter unavailable").into_response();
        }
    }

    // 2) Token parse + HMAC verify.
    let parsed = match parse_and_verify_pilot_token(&token, &state.token_key, now_ms) {
        Ok(t) => t,
        Err(err) => {
            let audit_row = SignupAuditRow {
                event_type: EVENT_TYPE_PILOT_TOKEN_REJECTED.to_owned(),
                tenant_id: None,
                token_id_or_prefix: Some(token_prefix(&token)),
                exit_status: "rejected".to_owned(),
                payload: Some(err.tag().to_owned()),
                emitted_at_ms: now_ms,
            };
            if let Some(resp) = emit_or_503(&state.audit_sink, audit_row) {
                return resp;
            }
            return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
        }
    };

    // 3) Body validation.
    if let Err(field) = body.validate() {
        let audit_row = SignupAuditRow {
            event_type: EVENT_TYPE_PILOT_TOKEN_REJECTED.to_owned(),
            tenant_id: None,
            token_id_or_prefix: Some(parsed.token_id.clone()),
            exit_status: "bad_request".to_owned(),
            payload: Some(format!("invalid_field={field}")),
            emitted_at_ms: now_ms,
        };
        if let Some(resp) = emit_or_503(&state.audit_sink, audit_row) {
            return resp;
        }
        return (StatusCode::BAD_REQUEST, "bad_request").into_response();
    }

    // 4) Reserve the row in the store (idempotent on email / token_id).
    let signup_id = Uuid::now_v7();
    let tenant_id = Uuid::now_v7();
    let record = PilotSignupRecord {
        id: signup_id,
        tenant_id,
        email: body.email.clone(),
        company_name: body.company_name.clone(),
        tier_hint: body.tier_hint.clone(),
        expected_use_case: body.expected_use_case.clone(),
        signed_up_at_ms: now_ms,
        token_id: parsed.token_id.clone(),
        state: "RESERVED".to_owned(),
    };
    let stored = match state.store.insert_or_existing(record) {
        Ok(r) => r,
        Err(_) => {
            return (StatusCode::SERVICE_UNAVAILABLE, "signup store unavailable").into_response();
        }
    };

    // 5) Audit emit BEFORE we serialise the response body. The
    //    `pilot_reserved.v1` emit fails CLOSED.
    let exit_status = if stored.id == signup_id {
        "reserved"
    } else {
        "duplicate"
    }
    .to_owned();
    let audit_row = SignupAuditRow {
        event_type: EVENT_TYPE_PILOT_RESERVED.to_owned(),
        tenant_id: Some(stored.tenant_id),
        token_id_or_prefix: Some(stored.token_id.clone()),
        exit_status,
        payload: None,
        emitted_at_ms: now_ms,
    };
    if let Some(resp) = emit_or_503(&state.audit_sink, audit_row) {
        return resp;
    }

    // 6) Render the 201 response.
    let activation_url = format!("{}/{}", state.activation_url_base, stored.id);
    let resp_body = PilotSignupResponse {
        tenant_id: stored.tenant_id,
        activation_url,
        state: stored.state,
    };
    (StatusCode::CREATED, Json(resp_body)).into_response()
}

/// Truncate the raw token to its first 32 chars for the audit emit's
/// `token_id_or_prefix` field — the route MUST never log the full
/// signature (HMAC values are operator-internal forensic data).
fn token_prefix(token: &str) -> String {
    token.chars().take(32).collect()
}

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

    const TEST_KEY: &[u8; 16] = b"test-hmac-key!!!";

    fn fixed_now_ms() -> u64 {
        1_700_000_000_000
    }

    #[test]
    fn mint_then_verify_roundtrips() {
        let token =
            mint_pilot_token(TokenEnv::Prod, fixed_now_ms(), "0123456789abcdef", TEST_KEY).unwrap();
        let parsed =
            parse_and_verify_pilot_token(&token, TEST_KEY, fixed_now_ms()).expect("verify");
        assert_eq!(parsed.env, TokenEnv::Prod);
        assert_eq!(parsed.minted_at_ms, fixed_now_ms());
        assert_eq!(parsed.token_id, "0123456789abcdef");
    }

    #[test]
    fn forged_signature_rejected() {
        let token = mint_pilot_token(
            TokenEnv::Staging,
            fixed_now_ms(),
            "deadbeefcafef00d",
            TEST_KEY,
        )
        .unwrap();
        // Flip the last hex char of the signature.
        let mut bytes: Vec<char> = token.chars().collect();
        let last = bytes.len() - 1;
        bytes[last] = if bytes[last] == '0' { '1' } else { '0' };
        let forged: String = bytes.into_iter().collect();
        let err = parse_and_verify_pilot_token(&forged, TEST_KEY, fixed_now_ms())
            .expect_err("forged rejected");
        assert_eq!(err, TokenError::SignatureMismatch);
    }

    #[test]
    fn wrong_key_rejected() {
        let token =
            mint_pilot_token(TokenEnv::Prod, fixed_now_ms(), "ffffffff00000000", TEST_KEY).unwrap();
        let err = parse_and_verify_pilot_token(&token, b"other-key-zzzz!!", fixed_now_ms())
            .expect_err("wrong key rejected");
        assert_eq!(err, TokenError::SignatureMismatch);
    }

    #[test]
    fn expired_token_rejected() {
        let minted_at_ms = fixed_now_ms();
        let token =
            mint_pilot_token(TokenEnv::Prod, minted_at_ms, "0000000011111111", TEST_KEY).unwrap();
        // Advance well beyond the TTL.
        let later = minted_at_ms + PILOT_TOKEN_TTL_MS + 1;
        let err =
            parse_and_verify_pilot_token(&token, TEST_KEY, later).expect_err("expired rejected");
        assert_eq!(err, TokenError::Expired);
    }

    #[test]
    fn future_minted_token_accepted() {
        // Clock skew tolerance: a token minted 1h "in the future"
        // still verifies (operator may pre-mint for a launch).
        let minted_at_ms = fixed_now_ms() + 3_600_000;
        let token =
            mint_pilot_token(TokenEnv::Prod, minted_at_ms, "aaaaaaaabbbbbbbb", TEST_KEY).unwrap();
        let parsed = parse_and_verify_pilot_token(&token, TEST_KEY, fixed_now_ms())
            .expect("future-mint accepted");
        assert_eq!(parsed.minted_at_ms, minted_at_ms);
    }

    #[test]
    fn malformed_tokens_rejected() {
        let key = TEST_KEY;
        let now = fixed_now_ms();
        // No `.`.
        assert_eq!(
            parse_and_verify_pilot_token("pilot_prod_0_aaaaaaaaaaaaaaaa", key, now),
            Err(TokenError::Malformed),
        );
        // Bad env.
        let body = format!("pilot_dev_{}_aaaaaaaaaaaaaaaa", now);
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap();
        mac.update(body.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());
        let tok = format!("{body}.{sig}");
        assert_eq!(
            parse_and_verify_pilot_token(&tok, key, now),
            Err(TokenError::BadEnv),
        );
        // Not pilot.
        let body = format!("admin_prod_{}_aaaaaaaaaaaaaaaa", now);
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap();
        mac.update(body.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());
        let tok = format!("{body}.{sig}");
        assert_eq!(
            parse_and_verify_pilot_token(&tok, key, now),
            Err(TokenError::NotPilotToken),
        );
        // Bad random (not hex).
        let body = format!("pilot_prod_{}_zzzzzzzzzzzzzzzz", now);
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap();
        mac.update(body.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());
        let tok = format!("{body}.{sig}");
        assert_eq!(
            parse_and_verify_pilot_token(&tok, key, now),
            Err(TokenError::BadRandom),
        );
    }

    #[test]
    fn body_validation_catches_missing_and_oversize() {
        let mut body = PilotSignupBody {
            email: "a@b.com".to_owned(),
            company_name: "Co".to_owned(),
            tier_hint: "free".to_owned(),
            expected_use_case: "ci".to_owned(),
        };
        body.validate().unwrap();
        body.email = String::new();
        assert_eq!(body.validate(), Err("email"));
        body.email = "a@b.com".to_owned();
        body.tier_hint = "x".repeat(MAX_FIELD_LEN + 1);
        assert_eq!(body.validate(), Err("tier_hint"));
        body.tier_hint = "free".to_owned();
        body.email = "not-an-email".to_owned();
        assert_eq!(body.validate(), Err("email"));
    }

    #[test]
    fn in_memory_store_dedupes_on_email() {
        let store = InMemorySignupStore::new();
        let mk = |email: &str, token_id: &str| PilotSignupRecord {
            id: Uuid::now_v7(),
            tenant_id: Uuid::now_v7(),
            email: email.to_owned(),
            company_name: "X".to_owned(),
            tier_hint: "free".to_owned(),
            expected_use_case: "ci".to_owned(),
            signed_up_at_ms: 0,
            token_id: token_id.to_owned(),
            state: "RESERVED".to_owned(),
        };
        let r1 = mk("dup@example.com", "tok1");
        let r2 = mk("dup@example.com", "tok2"); // same email, different token
        let stored1 = store.insert_or_existing(r1.clone()).unwrap();
        let stored2 = store.insert_or_existing(r2).unwrap();
        assert_eq!(stored1.id, stored2.id, "duplicate email returns original");
        assert_eq!(stored1.tenant_id, stored2.tenant_id);
    }

    #[test]
    fn router_builds() {
        let _r = router(build_state());
    }

    // ── F1 regressions: extract_client_ip ─────────────────────────────────────

    /// F1: `x-corelink-client-ip` is read as the trusted rate-limit key.
    #[test]
    fn extract_client_ip_reads_trusted_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-corelink-client-ip", "203.0.113.42".parse().unwrap());
        assert_eq!(extract_client_ip(&headers), "203.0.113.42");
    }

    /// F1: a client-forged `x-forwarded-for` is completely ignored.
    #[test]
    fn extract_client_ip_ignores_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        // Only XFF is present; x-corelink-client-ip is absent.
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        // Must NOT return "1.2.3.4" (or any value from XFF).
        // Must return the shared no-ip sentinel.
        assert_eq!(
            extract_client_ip(&headers),
            "_no_ip",
            "forged x-forwarded-for must be ignored; missing trusted header must collapse to _no_ip"
        );
    }

    /// F1: both XFF and the trusted header are present — only the
    /// trusted header wins.
    #[test]
    fn extract_client_ip_trusted_header_wins_over_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "10.0.0.1, 10.0.0.2".parse().unwrap());
        headers.insert("x-corelink-client-ip", "203.0.113.99".parse().unwrap());
        assert_eq!(
            extract_client_ip(&headers),
            "203.0.113.99",
            "trusted header must win; XFF must not influence the bucket key"
        );
    }

    /// F1: absent `x-corelink-client-ip` → shared `"_no_ip"` sentinel
    /// (fail-CLOSED: one shared bucket, not unlimited unique keys).
    #[test]
    fn extract_client_ip_missing_header_returns_shared_no_ip_bucket() {
        let headers = HeaderMap::new();
        assert_eq!(
            extract_client_ip(&headers),
            "_no_ip",
            "missing trusted header must yield the shared _no_ip bucket"
        );
    }

    /// F1: empty-value `x-corelink-client-ip` → shared `"_no_ip"` sentinel.
    #[test]
    fn extract_client_ip_empty_header_returns_shared_no_ip_bucket() {
        let mut headers = HeaderMap::new();
        // An empty (whitespace-only) value must also collapse.
        headers.insert("x-corelink-client-ip", "   ".parse().unwrap());
        assert_eq!(
            extract_client_ip(&headers),
            "_no_ip",
            "empty trusted header must yield the shared _no_ip bucket"
        );
    }
}
