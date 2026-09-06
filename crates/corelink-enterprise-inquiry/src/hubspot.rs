//! HubSpot CRM real client (`R2-5`).
//!
//! Production wiring for the [`CrmClient`] trait. Talks to the HubSpot
//! REST API (`/crm/v3/objects/{contacts, companies, deals, tickets}`)
//! using a HubSpot Private App access token (env `HUBSPOT_PRIVATE_APP_TOKEN`,
//! token shape `pat-na1-...`).
//!
//! # Architecture
//!
//! Per the `trait-abstraction-defer` charter pattern (and the
//! load-bearing "no tokio in src" rule of this crate), HTTP transport
//! is abstracted via the [`HubSpotHttp`] trait — a SYNC request /
//! response surface. The production binary wires a sync transport
//! (e.g. `ureq` or a `reqwest::blocking::Client`); the Cloudflare
//! Worker boundary wires a `cf-worker fetch` shim that bridges to a
//! oneshot channel. Tests in this module use a built-in
//! [`RecordingHubSpotHttp`] mock whose script of responses pins every
//! algorithmic invariant the production wiring relies on. The trait is
//! also adapter-compatible with `wiremock` when a caller provides an
//! external async runtime — the test recorder is preferred here so
//! the crate stays runtime-agnostic.
//!
//! # Operations
//!
//! - `create_contact(name, email, company)` — POST `/crm/v3/objects/contacts`
//! - `find_or_create_company(legal_name, tax_id)` — POST `/crm/v3/objects/companies/search`
//!   followed by POST `/crm/v3/objects/companies` if no hit (idempotent
//!   via `hs_unique_creation_key`).
//! - `create_deal(contact_id, company_id, stage, amount)` — POST
//!   `/crm/v3/objects/deals` with associations + initial stage
//!   `enterprise_inquiry`.
//! - `create_ticket(subject, content, owner_id)` — POST
//!   `/crm/v3/objects/tickets`.
//!
//! # Residency routing
//!
//! HubSpot portals are bound to either the US (`api.hubapi.com` — us1)
//! or EU (`api.hubapi.eu` — eu1) data centre. The client is constructed
//! with an explicit [`HubSpotRegion`] taken from the env var
//! `HUBSPOT_REGION` (`us1` | `eu1`). When a [`crate::form::ResidencyKind::Eu`]
//! inquiry is routed against a `us1` portal (or any other mismatch
//! covered by [`is_residency_routable`]), the client rejects the
//! payload with [`CrmError::Rejected`] tagged `residency-mismatch` per
//! `CTRL-PRIV-RES-001` BEFORE any HubSpot request is initiated. This
//! is the only point where the inquiry handoff fails closed for
//! residency policy.
//!
//! # PII boundary (R2-11; S-19 P1-NEW-3 CLOSED)
//!
//! As of R2-11, the [`crate::ledger::EnterpriseInquiryLedger`] in-
//! memory mirror holds ONLY the BYOK-sealed [`SealedInquiry`] — never
//! plaintext PII. The [`HubSpotCrmClient`] is the permitted decryption
//! boundary on the dispatch path: [`HubSpotCrmClient::create_entry`]
//! verifies the tenant AAD binding, unseals the payload via the
//! supplied [`InquiryPayloadEncryptor`], constructs the HubSpot REST
//! request from the unsealed PII, and lets the unsealed value go out
//! of scope as soon as the wire request returns. Cross-border transfer
//! of the plaintext PII to HubSpot is permitted ONLY under the HubSpot
//! DPA; the `HUBSPOT_REGION=eu1` constraint ensures EU-resident PII
//! does not transit the US Hub. Plaintext PII is NEVER logged at this
//! boundary (CTRL-PRIV-001).
//!
//! # Retry policy
//!
//! Per `PAT-RETRY-EXPBACK-001`:
//! - 5xx (502/503/504) and 429 → retry with exponential backoff
//!   (base 250ms, jitter-free for determinism in tests).
//! - 429 with `Retry-After` header → honour the server hint (capped at
//!   30s to bound saga latency).
//! - 401/403 (auth failure) → hard-fail immediately, NO retry, surface
//!   as [`CrmError::Rejected`] tagged `auth`. Operators MUST rotate
//!   the Private App token.
//! - 4xx (validation) → hard-fail, surface as [`CrmError::Rejected`].
//! - Max retries: 5. Beyond that the call surfaces as
//!   [`CrmError::Transport`] and escalates to Slack (`crm-push-failed`)
//!   via the saga rollback path.
//!
//! # Token handling
//!
//! The access token is held opaquely behind a [`HubSpotToken`] newtype
//! whose `Debug` redacts the value. The token is NEVER logged: error
//! messages reference `"token=<redacted>"` and the `Authorization`
//! header is sent over TLS only (caller responsibility — the trait
//! surface is transport-agnostic).

use std::sync::{Arc, Mutex};

use crate::crm::{CrmClient, CrmEntryId, CrmError};
use crate::encryption::{InquiryPayloadEncryptor, SealedInquiry, UnsealedInquiryPii};
use crate::form::{InquiryId, ResidencyKind};

// ----------------------------------------------------------------------
// HubSpot region + token + URL helpers
// ----------------------------------------------------------------------

/// HubSpot portal region. Determines the API base URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum HubSpotRegion {
    /// US Hub — `api.hubapi.com`. EU-resident PII MUST NOT land here.
    Us1,
    /// EU Hub — `api.hubapi.eu`. Required for `ResidencyKind::Eu`.
    Eu1,
}

impl HubSpotRegion {
    /// Canonical wire string (matches the env var `HUBSPOT_REGION`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Us1 => "us1",
            Self::Eu1 => "eu1",
        }
    }

    /// Base URL for HubSpot REST API calls.
    #[must_use]
    pub const fn base_url(self) -> &'static str {
        match self {
            Self::Us1 => "https://api.hubapi.com",
            Self::Eu1 => "https://api.hubapi.eu",
        }
    }

    /// Parse from a `HUBSPOT_REGION` env-var-style string.
    ///
    /// # Errors
    ///
    /// Returns [`HubSpotConfigError::InvalidRegion`] when the value is
    /// neither `"us1"` nor `"eu1"` (case-sensitive — env values MUST
    /// be canonical lowercase).
    pub fn parse(s: &str) -> Result<Self, HubSpotConfigError> {
        match s {
            "us1" => Ok(Self::Us1),
            "eu1" => Ok(Self::Eu1),
            other => Err(HubSpotConfigError::InvalidRegion(other.to_string())),
        }
    }
}

/// Opaque HubSpot Private App access token. Debug-redacted; NEVER
/// logged.
#[derive(Clone)]
pub struct HubSpotToken(String);

impl HubSpotToken {
    /// Wrap a raw token string after validating its `pat-na1-...`
    /// prefix.
    ///
    /// # Errors
    ///
    /// Returns [`HubSpotConfigError::InvalidTokenShape`] when the
    /// token does not match the canonical HubSpot Private App shape
    /// (`pat-na1-` prefix + at least 24 chars total).
    pub fn new(raw: impl Into<String>) -> Result<Self, HubSpotConfigError> {
        let raw = raw.into();
        if !raw.starts_with("pat-na1-") {
            return Err(HubSpotConfigError::InvalidTokenShape);
        }
        if raw.len() < 24 {
            return Err(HubSpotConfigError::InvalidTokenShape);
        }
        Ok(Self(raw))
    }

    /// Authorization header value (`Bearer <token>`). The caller is
    /// responsible for delivering this over TLS.
    #[must_use]
    pub fn bearer(&self) -> String {
        format!("Bearer {}", self.0)
    }
}

impl core::fmt::Debug for HubSpotToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("HubSpotToken(<redacted>)")
    }
}

/// Configuration error for the HubSpot client.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum HubSpotConfigError {
    /// Token does not match the canonical `pat-na1-...` shape.
    #[error("hubspot token shape invalid (expected `pat-na1-...`)")]
    InvalidTokenShape,
    /// Env var `HUBSPOT_REGION` was set to a non-canonical value.
    #[error("hubspot region invalid: {0} (expected `us1` or `eu1`)")]
    InvalidRegion(String),
    /// Required env var was missing.
    #[error("hubspot env var missing: {0}")]
    EnvMissing(String),
}

/// Check whether a [`ResidencyKind`] inquiry is routable against a
/// [`HubSpotRegion`]. EU-resident PII routes ONLY to `eu1`; US-resident
/// PII routes ONLY to `us1`. Other residencies (`None`, `Sam`, `Apac`,
/// `Specific`) are allowed against either region pending legal review —
/// the conservative policy is to permit `us1` for `None` (US Hub is
/// the default) and require explicit operator approval (out-of-band)
/// for the rest. Here we encode the strict EU/US bidirectional check;
/// the caller treats the remaining residencies as legal-review-pending.
#[must_use]
pub fn is_residency_routable(residency: ResidencyKind, region: HubSpotRegion) -> bool {
    match (residency, region) {
        (ResidencyKind::Eu, HubSpotRegion::Eu1) => true,
        (ResidencyKind::Eu, HubSpotRegion::Us1) => false,
        (ResidencyKind::Us, HubSpotRegion::Us1) => true,
        (ResidencyKind::Us, HubSpotRegion::Eu1) => false,
        // `None` / `Sam` / `Apac` / `Specific`: permitted on either
        // region; operator legal review (DPA scope) gates the actual
        // production deployment per-tenant.
        _ => true,
    }
}

// ----------------------------------------------------------------------
// HubSpot HTTP transport trait
// ----------------------------------------------------------------------

/// HTTP method for a HubSpot REST call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HubSpotMethod {
    /// HTTP POST.
    Post,
    /// HTTP GET.
    Get,
    /// HTTP PATCH.
    Patch,
}

impl HubSpotMethod {
    /// Canonical wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Post => "POST",
            Self::Get => "GET",
            Self::Patch => "PATCH",
        }
    }
}

/// HubSpot HTTP request — JSON body + `Authorization: Bearer <token>`
/// header value pre-computed by [`HubSpotCrmClient`]. The transport
/// adapter copies `authorization` verbatim onto the wire as the
/// `Authorization` header; the token is NEVER logged from this struct
/// (the `Debug` impl redacts it).
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HubSpotRequest {
    /// HTTP method.
    pub method: HubSpotMethod,
    /// Absolute URL.
    pub url: String,
    /// JSON body (already serialised; empty string for GET).
    pub body: String,
    /// `Authorization` header value (`Bearer pat-na1-...`). Redacted
    /// in `Debug`.
    pub authorization: String,
}

impl core::fmt::Debug for HubSpotRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HubSpotRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("body", &self.body)
            .field("authorization", &"<redacted>")
            .finish()
    }
}

/// HubSpot HTTP response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HubSpotResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body (raw bytes as UTF-8).
    pub body: String,
    /// Optional `Retry-After` header value (seconds).
    pub retry_after_s: Option<u64>,
}

impl HubSpotResponse {
    /// True for 2xx.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// True for 5xx or 429 — retryable per `PAT-RETRY-EXPBACK-001`.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        self.status == 429 || (self.status >= 500 && self.status < 600)
    }

    /// True for 401/403 — auth hard-fail.
    #[must_use]
    pub const fn is_auth_failure(&self) -> bool {
        self.status == 401 || self.status == 403
    }
}

/// Transport-layer error from the HTTP shim (network failure, TLS
/// negotiation error, body decode error). Distinct from a non-2xx
/// HubSpot response (the latter is surfaced via [`HubSpotResponse`]).
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum HubSpotHttpError {
    /// Connection / TLS / DNS / read-timeout failure.
    #[error("hubspot http transport failure: {0}")]
    Transport(String),
}

/// Sync HTTP transport surface the [`HubSpotCrmClient`] depends on.
/// The token bearer header is the implementation's responsibility:
/// `Authorization`, `Content-Type: application/json`, and the
/// `Accept` header SHOULD be set by the transport (the trait surface
/// keeps the client free of any HTTP framing decisions).
pub trait HubSpotHttp: core::fmt::Debug + Send + Sync {
    /// Execute the HTTP request. Implementations MUST NOT retry —
    /// retry policy lives in [`HubSpotCrmClient`].
    ///
    /// # Errors
    ///
    /// Returns [`HubSpotHttpError::Transport`] for network / TLS /
    /// timeout failures. Non-2xx HubSpot responses are NOT errors at
    /// this layer.
    fn execute(&self, request: &HubSpotRequest) -> Result<HubSpotResponse, HubSpotHttpError>;
}

// ----------------------------------------------------------------------
// RecordingHubSpotHttp test mock
// ----------------------------------------------------------------------

/// Scripted HubSpot HTTP mock. Each call pops the next scripted
/// response off the queue; the recorded request is pushed onto the
/// log for property inspection. Used by the in-crate tests of
/// [`HubSpotCrmClient`] and available to downstream callers that need
/// a deterministic fixture.
#[derive(Clone, Debug, Default)]
pub struct RecordingHubSpotHttp {
    state: Arc<Mutex<RecordingState>>,
}

#[derive(Debug, Default)]
struct RecordingState {
    /// Scripted responses (FIFO). When empty, the mock returns a
    /// transport failure so tests fail loudly on missing fixtures.
    responses: Vec<Result<HubSpotResponse, HubSpotHttpError>>,
    /// Recorded requests (in call order).
    requests: Vec<HubSpotRequest>,
}

impl RecordingHubSpotHttp {
    /// Construct an empty mock.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a scripted success response onto the end of the queue.
    pub fn push_response(&self, status: u16, body: impl Into<String>) {
        self.push_response_with_retry_after(status, body, None);
    }

    /// Push a scripted response with an optional `Retry-After` header.
    pub fn push_response_with_retry_after(
        &self,
        status: u16,
        body: impl Into<String>,
        retry_after_s: Option<u64>,
    ) {
        let resp = HubSpotResponse {
            status,
            body: body.into(),
            retry_after_s,
        };
        if let Ok(mut g) = self.state.lock() {
            g.responses.push(Ok(resp));
        }
    }

    /// Push a scripted transport failure onto the end of the queue.
    pub fn push_transport_failure(&self, msg: impl Into<String>) {
        if let Ok(mut g) = self.state.lock() {
            g.responses
                .push(Err(HubSpotHttpError::Transport(msg.into())));
        }
    }

    /// Snapshot the recorded requests (in call order).
    #[must_use]
    pub fn requests(&self) -> Vec<HubSpotRequest> {
        match self.state.lock() {
            Ok(g) => g.requests.clone(),
            Err(p) => p.into_inner().requests.clone(),
        }
    }

    /// Count of recorded requests.
    #[must_use]
    pub fn request_count(&self) -> usize {
        match self.state.lock() {
            Ok(g) => g.requests.len(),
            Err(p) => p.into_inner().requests.len(),
        }
    }
}

impl HubSpotHttp for RecordingHubSpotHttp {
    fn execute(&self, request: &HubSpotRequest) -> Result<HubSpotResponse, HubSpotHttpError> {
        let mut g = self.state.lock().map_err(|e| {
            HubSpotHttpError::Transport(format!("recording mock mutex poisoned: {e}"))
        })?;
        g.requests.push(request.clone());
        if g.responses.is_empty() {
            return Err(HubSpotHttpError::Transport(
                "recording mock: no scripted response for request".to_string(),
            ));
        }
        g.responses.remove(0)
    }
}

// ----------------------------------------------------------------------
// HubSpotCrmClient
// ----------------------------------------------------------------------

/// Initial deal stage for an enterprise inquiry. The HubSpot pipeline
/// is configured per portal; the canonical wire string is
/// `enterprise_inquiry` (matches the HubSpot dealstage internal name).
pub const DEAL_STAGE_ENTERPRISE_INQUIRY: &str = "enterprise_inquiry";

/// Max retries per `PAT-RETRY-EXPBACK-001`.
pub const MAX_RETRIES: u32 = 5;

/// Base backoff (ms) — exponential: 250, 500, 1000, 2000, 4000.
pub const BASE_BACKOFF_MS: u64 = 250;

/// Cap on the `Retry-After` server hint (s) so the saga stays bounded.
pub const RETRY_AFTER_CAP_S: u64 = 30;

/// Retry policy decision surface. Pure function for property tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryDecision {
    /// Retry after `delay_ms`.
    Retry {
        /// Delay before the next attempt (ms).
        delay_ms: u64,
    },
    /// Give up — retry budget exhausted.
    GiveUp,
    /// Auth hard-fail — surface immediately.
    AuthHardFail,
    /// Non-retryable 4xx — surface immediately.
    NonRetryable,
    /// Success — no retry needed.
    Success,
}

/// Compute the next retry decision given a response, attempt number,
/// and the (optional) `Retry-After` hint. Pure function exposed for
/// property tests.
#[must_use]
pub fn classify_retry(resp: &HubSpotResponse, attempt: u32) -> RetryDecision {
    if resp.is_success() {
        return RetryDecision::Success;
    }
    if resp.is_auth_failure() {
        return RetryDecision::AuthHardFail;
    }
    if !resp.is_retryable() {
        return RetryDecision::NonRetryable;
    }
    if attempt >= MAX_RETRIES {
        return RetryDecision::GiveUp;
    }
    // Retry-After (capped) overrides exponential backoff.
    let delay_ms = match resp.retry_after_s {
        Some(s) => s.min(RETRY_AFTER_CAP_S).saturating_mul(1_000),
        None => BASE_BACKOFF_MS.saturating_mul(1u64 << attempt.min(10)),
    };
    RetryDecision::Retry { delay_ms }
}

/// Sleep hook — production wires `std::thread::sleep`; tests wire a
/// no-op recorder so retries advance without real wall-clock waits.
pub trait HubSpotSleeper: core::fmt::Debug + Send + Sync {
    /// Sleep for `delay_ms` milliseconds (or record the call).
    fn sleep(&self, delay_ms: u64);
}

/// No-op sleeper used by tests + the default sync transport binding
/// (real sleeps live in the production HTTP shim wrapper, NOT the
/// trait surface, so the crate stays runtime-agnostic).
#[derive(Clone, Debug, Default)]
pub struct NoopSleeper {
    state: Arc<Mutex<Vec<u64>>>,
}

impl NoopSleeper {
    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot recorded sleep durations (ms).
    #[must_use]
    pub fn sleeps(&self) -> Vec<u64> {
        match self.state.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }
}

impl HubSpotSleeper for NoopSleeper {
    fn sleep(&self, delay_ms: u64) {
        if let Ok(mut g) = self.state.lock() {
            g.push(delay_ms);
        }
    }
}

/// Real HubSpot CRM client.
#[derive(Clone, Debug)]
pub struct HubSpotCrmClient<T, S>
where
    T: HubSpotHttp + Clone,
    S: HubSpotSleeper + Clone,
{
    http: T,
    sleeper: S,
    token: HubSpotToken,
    region: HubSpotRegion,
}

impl<T, S> HubSpotCrmClient<T, S>
where
    T: HubSpotHttp + Clone,
    S: HubSpotSleeper + Clone,
{
    /// Construct the client. Validates the token shape + region pair
    /// upfront so a misconfigured deployment fails at boot.
    ///
    /// # Errors
    ///
    /// Returns [`HubSpotConfigError`] when the token shape is invalid.
    pub fn new(
        http: T,
        sleeper: S,
        token: HubSpotToken,
        region: HubSpotRegion,
    ) -> Result<Self, HubSpotConfigError> {
        // Token was already validated by `HubSpotToken::new`; the
        // explicit parameter type forces callers down that path.
        Ok(Self {
            http,
            sleeper,
            token,
            region,
        })
    }

    /// Construct the client from environment variables (production
    /// boot path). Reads `HUBSPOT_PRIVATE_APP_TOKEN` + `HUBSPOT_REGION`.
    ///
    /// # Errors
    ///
    /// Returns [`HubSpotConfigError`] when either env var is missing
    /// or malformed.
    pub fn from_env<F>(http: T, sleeper: S, env: F) -> Result<Self, HubSpotConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let raw_token = env("HUBSPOT_PRIVATE_APP_TOKEN").ok_or_else(|| {
            HubSpotConfigError::EnvMissing("HUBSPOT_PRIVATE_APP_TOKEN".to_string())
        })?;
        let raw_region = env("HUBSPOT_REGION")
            .ok_or_else(|| HubSpotConfigError::EnvMissing("HUBSPOT_REGION".to_string()))?;
        let token = HubSpotToken::new(raw_token)?;
        let region = HubSpotRegion::parse(&raw_region)?;
        Self::new(http, sleeper, token, region)
    }

    /// Region.
    #[must_use]
    pub const fn region(&self) -> HubSpotRegion {
        self.region
    }

    /// Execute a request with retry policy.
    fn execute_with_retry(&self, request: &HubSpotRequest) -> Result<HubSpotResponse, CrmError> {
        let mut attempt: u32 = 0;
        loop {
            let resp = match self.http.execute(request) {
                Ok(r) => r,
                Err(HubSpotHttpError::Transport(msg)) => {
                    // Transport failure — treat as 5xx for retry purposes
                    // (synthetic retryable response).
                    if attempt >= MAX_RETRIES {
                        return Err(CrmError::Transport(format!(
                            "hubspot transport failure after {} attempts: {}",
                            attempt + 1,
                            msg
                        )));
                    }
                    let delay = BASE_BACKOFF_MS.saturating_mul(1u64 << attempt.min(10));
                    self.sleeper.sleep(delay);
                    attempt = attempt.saturating_add(1);
                    continue;
                }
            };

            match classify_retry(&resp, attempt) {
                RetryDecision::Success => return Ok(resp),
                RetryDecision::AuthHardFail => {
                    return Err(CrmError::Rejected(format!(
                        "hubspot auth hard-fail (status {}; token=<redacted>; rotate the Private App token)",
                        resp.status
                    )));
                }
                RetryDecision::NonRetryable => {
                    return Err(CrmError::Rejected(format!(
                        "hubspot rejected payload (status {}; body={})",
                        resp.status,
                        truncate(&resp.body, 256)
                    )));
                }
                RetryDecision::GiveUp => {
                    return Err(CrmError::Transport(format!(
                        "hubspot retry budget exhausted (status {}; attempts={})",
                        resp.status,
                        attempt + 1
                    )));
                }
                RetryDecision::Retry { delay_ms } => {
                    self.sleeper.sleep(delay_ms);
                    attempt = attempt.saturating_add(1);
                }
            }
        }
    }

    fn json_post(&self, path: &str, body: String) -> Result<HubSpotResponse, CrmError> {
        let request = HubSpotRequest {
            method: HubSpotMethod::Post,
            url: format!("{}{path}", self.region.base_url()),
            body,
            authorization: self.token.bearer(),
        };
        self.execute_with_retry(&request)
    }

    /// Create a HubSpot contact. Returns the HubSpot contact id (as a
    /// raw string from the response `id` field — production wiring
    /// parses JSON; here we use a minimal string scan to keep the
    /// crate's dependency surface minimal).
    ///
    /// # Errors
    ///
    /// Returns [`CrmError`] per [`HubSpotCrmClient::execute_with_retry`].
    pub fn create_contact(
        &self,
        email: &str,
        first_name: &str,
        last_name: &str,
        company: &str,
        inquiry_id: &InquiryId,
    ) -> Result<String, CrmError> {
        let body = build_contact_body(email, first_name, last_name, company, inquiry_id);
        let resp = self.json_post("/crm/v3/objects/contacts", body)?;
        parse_object_id(&resp.body)
    }

    /// Search-then-create a HubSpot company by legal name + tax id
    /// (idempotent via `hs_unique_creation_key`).
    ///
    /// # Errors
    ///
    /// Returns [`CrmError`] per [`HubSpotCrmClient::execute_with_retry`].
    pub fn find_or_create_company(
        &self,
        legal_name: &str,
        tax_id: Option<&str>,
        inquiry_id: &InquiryId,
    ) -> Result<String, CrmError> {
        let search_body = build_company_search_body(legal_name);
        let search_resp = self.json_post("/crm/v3/objects/companies/search", search_body)?;
        if let Some(existing) = parse_first_search_hit(&search_resp.body) {
            return Ok(existing);
        }
        let create_body = build_company_create_body(legal_name, tax_id, inquiry_id);
        let resp = self.json_post("/crm/v3/objects/companies", create_body)?;
        parse_object_id(&resp.body)
    }

    /// Create a HubSpot deal linked to the contact + company.
    ///
    /// # Errors
    ///
    /// Returns [`CrmError`] per [`HubSpotCrmClient::execute_with_retry`].
    pub fn create_deal(
        &self,
        contact_id: &str,
        company_id: &str,
        stage: &str,
        amount_usd: u64,
        inquiry_id: &InquiryId,
    ) -> Result<String, CrmError> {
        let body = build_deal_body(contact_id, company_id, stage, amount_usd, inquiry_id);
        let resp = self.json_post("/crm/v3/objects/deals", body)?;
        parse_object_id(&resp.body)
    }

    /// Create a HubSpot support ticket.
    ///
    /// # Errors
    ///
    /// Returns [`CrmError`] per [`HubSpotCrmClient::execute_with_retry`].
    pub fn create_ticket(
        &self,
        subject: &str,
        content: &str,
        owner_id: Option<&str>,
        inquiry_id: &InquiryId,
    ) -> Result<String, CrmError> {
        let body = build_ticket_body(subject, content, owner_id, inquiry_id);
        let resp = self.json_post("/crm/v3/objects/tickets", body)?;
        parse_object_id(&resp.body)
    }
}

impl<T, S> CrmClient for HubSpotCrmClient<T, S>
where
    T: HubSpotHttp + Clone,
    S: HubSpotSleeper + Clone,
{
    /// Decryption boundary (R2-11 / S-19 P1-NEW-3): unseals the
    /// [`SealedInquiry`] via the supplied
    /// [`InquiryPayloadEncryptor`] right before issuing the HubSpot
    /// REST requests. The unsealed PII is held in a local
    /// [`UnsealedInquiryPii`] for the duration of the three HTTP calls
    /// (contact + company + deal) and dropped on return. Plaintext PII
    /// is NEVER logged here (CTRL-PRIV-001).
    fn create_entry(
        &self,
        inquiry_id: &InquiryId,
        sealed: &SealedInquiry,
        tenant_id: &str,
        encryptor: &dyn InquiryPayloadEncryptor,
        _lead_score: u32,
    ) -> Result<CrmEntryId, CrmError> {
        // Residency hard-check FIRST — before any wire request. Uses
        // the sanitised surrogate (no decryption needed for this gate).
        if !is_residency_routable(sealed.sanitized.residency_requirements, self.region) {
            return Err(CrmError::Rejected(format!(
                "residency-mismatch: inquiry residency `{}` rejected on region `{}` per CTRL-PRIV-RES-001",
                sealed.sanitized.residency_requirements.as_str(),
                self.region.as_str()
            )));
        }
        // Tenant binding check — fail-CLOSED on AAD swap BEFORE
        // materialising plaintext.
        if sealed.payload.aad.tenant_id != tenant_id {
            return Err(CrmError::Encryption(
                "tenant_id disagrees with payload AAD (cross-tenant push rejected)".to_string(),
            ));
        }
        // ──── Decryption boundary START — DO NOT log fields of `unsealed` ────
        let unsealed: UnsealedInquiryPii = encryptor
            .unseal(&sealed.payload, &sealed.payload.aad)
            .map_err(CrmError::from)?;
        // Best-effort name split (HubSpot expects first/last; the
        // canonical form has no name field so we use the company as
        // the first-name fallback per the CRM ops runbook).
        let (first, last) = split_name(&unsealed.company);
        let contact_id = self.create_contact(
            &unsealed.email,
            &first,
            &last,
            &unsealed.company,
            inquiry_id,
        )?;
        let company_id = self.find_or_create_company(&unsealed.company, None, inquiry_id)?;
        let deal_id = self.create_deal(
            &contact_id,
            &company_id,
            DEAL_STAGE_ENTERPRISE_INQUIRY,
            0,
            inquiry_id,
        )?;
        // `unsealed` is dropped here — out of scope. The heap
        // allocations are released by stdlib; the keyed memory zeroize
        // path lives in `corelink-byok` for the wrapped DEK.
        drop(unsealed);
        Ok(CrmEntryId::new(deal_id))
        // ──── Decryption boundary END ────
    }

    fn compensate(&self, inquiry_id: &InquiryId, entry_id: &CrmEntryId) -> Result<(), CrmError> {
        // Compensating action: PATCH the deal to `closed_lost` +
        // tag the rollback reason via `hs_deal_stage_probability=0`
        // (HubSpot deal closed-lost convention).
        let body = format!(
            "{{\"properties\":{{\"dealstage\":\"closedlost\",\"hs_unique_creation_key\":\"{}\",\"saga_rollback\":\"true\"}}}}",
            escape_json(inquiry_id.as_str()),
        );
        let path = format!("/crm/v3/objects/deals/{}", escape_url(entry_id.as_str()));
        let request = HubSpotRequest {
            method: HubSpotMethod::Patch,
            url: format!("{}{path}", self.region.base_url()),
            body,
            authorization: self.token.bearer(),
        };
        let resp = self.execute_with_retry(&request)?;
        // Suppress unused warning on `resp` while keeping the type
        // pinned for future extension (e.g. parse the patched object).
        let _ = resp;
        Ok(())
    }
}

mod helpers;
use helpers::*;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests;
