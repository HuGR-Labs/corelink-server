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

// ----------------------------------------------------------------------
// JSON body builders (intentionally tiny — no serde dependency in this
// crate; the production binary can swap to a typed builder if it adds
// serde-json itself)
// ----------------------------------------------------------------------

fn build_contact_body(
    email: &str,
    first_name: &str,
    last_name: &str,
    company: &str,
    inquiry_id: &InquiryId,
) -> String {
    format!(
        "{{\"properties\":{{\"email\":\"{}\",\"firstname\":\"{}\",\"lastname\":\"{}\",\"company\":\"{}\",\"hs_unique_creation_key\":\"contact:{}\"}}}}",
        escape_json(email),
        escape_json(first_name),
        escape_json(last_name),
        escape_json(company),
        escape_json(inquiry_id.as_str()),
    )
}

fn build_company_search_body(legal_name: &str) -> String {
    format!(
        "{{\"filterGroups\":[{{\"filters\":[{{\"propertyName\":\"name\",\"operator\":\"EQ\",\"value\":\"{}\"}}]}}],\"limit\":1}}",
        escape_json(legal_name),
    )
}

fn build_company_create_body(
    legal_name: &str,
    tax_id: Option<&str>,
    inquiry_id: &InquiryId,
) -> String {
    let tax_id_kv = match tax_id {
        Some(t) => format!(",\"tax_id\":\"{}\"", escape_json(t)),
        None => String::new(),
    };
    format!(
        "{{\"properties\":{{\"name\":\"{}\",\"hs_unique_creation_key\":\"company:{}\"{}}}}}",
        escape_json(legal_name),
        escape_json(inquiry_id.as_str()),
        tax_id_kv,
    )
}

fn build_deal_body(
    contact_id: &str,
    company_id: &str,
    stage: &str,
    amount_usd: u64,
    inquiry_id: &InquiryId,
) -> String {
    format!(
        "{{\"properties\":{{\"dealname\":\"Enterprise inquiry {iid}\",\"dealstage\":\"{stage}\",\"amount\":\"{amt}\",\"hs_unique_creation_key\":\"deal:{iid}\"}},\
         \"associations\":[\
           {{\"to\":{{\"id\":\"{cid}\"}},\"types\":[{{\"associationCategory\":\"HUBSPOT_DEFINED\",\"associationTypeId\":3}}]}},\
           {{\"to\":{{\"id\":\"{coid}\"}},\"types\":[{{\"associationCategory\":\"HUBSPOT_DEFINED\",\"associationTypeId\":5}}]}}\
         ]}}",
        iid = escape_json(inquiry_id.as_str()),
        stage = escape_json(stage),
        amt = amount_usd,
        cid = escape_json(contact_id),
        coid = escape_json(company_id),
    )
}

fn build_ticket_body(
    subject: &str,
    content: &str,
    owner_id: Option<&str>,
    inquiry_id: &InquiryId,
) -> String {
    let owner_kv = match owner_id {
        Some(o) => format!(",\"hubspot_owner_id\":\"{}\"", escape_json(o)),
        None => String::new(),
    };
    format!(
        "{{\"properties\":{{\"subject\":\"{}\",\"content\":\"{}\",\"hs_pipeline_stage\":\"1\",\"hs_unique_creation_key\":\"ticket:{}\"{}}}}}",
        escape_json(subject),
        escape_json(content),
        escape_json(inquiry_id.as_str()),
        owner_kv,
    )
}

/// JSON string escape — covers the four characters HubSpot will reject
/// (`"`, `\`, control chars).
fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// URL path-segment escape — covers `/` and non-ASCII.
fn escape_url(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Truncate a string to `max` chars, appending `…` if truncated. Used
/// when surfacing HubSpot error bodies into [`CrmError::Rejected`] so
/// the audit trail keeps a useful tail without leaking unbounded
/// HubSpot payload into our logs.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}…")
    }
}

/// Tiny "find `\"id\":\"...\"` field" parser — avoids a serde_json
/// dependency in this crate. Production wiring SHOULD replace this
/// with a real JSON parse once the crate adopts serde.
fn parse_object_id(body: &str) -> Result<String, CrmError> {
    parse_string_field(body, "\"id\"").ok_or_else(|| {
        CrmError::Rejected(format!(
            "hubspot response missing `id`: {}",
            truncate(body, 256)
        ))
    })
}

/// Parse the first hit from a `/search` response (`{"results":[{"id":"123",...}],...}`).
/// Returns `None` when results is empty or absent.
#[must_use]
fn parse_first_search_hit(body: &str) -> Option<String> {
    // Find `"results":[` then the first `"id":"..."` after it.
    let results_idx = body.find("\"results\":[")?;
    let tail = body.get(results_idx..)?;
    if tail.starts_with("\"results\":[]")
        || tail.starts_with("\"results\":[ ]")
        || tail.starts_with("\"results\": []")
    {
        return None;
    }
    parse_string_field(tail, "\"id\"")
}

fn parse_string_field(body: &str, key: &str) -> Option<String> {
    let key_idx = body.find(key)?;
    let after = body.get(key_idx.checked_add(key.len())?..)?;
    let colon_idx = after.find(':')?;
    let after_colon = after.get(colon_idx.checked_add(1)?..)?;
    let trimmed = after_colon.trim_start();
    if !trimmed.starts_with('"') {
        return None;
    }
    let rest = trimmed.get(1..)?;
    let end = rest.find('"')?;
    rest.get(..end).map(str::to_string)
}

fn split_name(full: &str) -> (String, String) {
    let trimmed = full.trim();
    if let Some((first, last)) = trimmed.split_once(' ') {
        (first.to_string(), last.to_string())
    } else {
        (trimmed.to_string(), "—".to_string())
    }
}

// ----------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------

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
    use crate::encryption::{seal_inquiry, InMemoryInquiryPayloadEncryptor};
    use crate::form::{BYOKRequirementsKind, EnterpriseInquiryForm, ResidencyKind, Role};

    fn token() -> HubSpotToken {
        HubSpotToken::new("pat-na1-AAAAAAAAAAAAAAAA-deadbeef").unwrap()
    }

    fn eu_form() -> EnterpriseInquiryForm {
        EnterpriseInquiryForm {
            company: "Acme GmbH".to_string(),
            role: Role::Ciso,
            email: "ciso@acme.example".to_string(),
            phone_optional: None,
            expected_gb_per_month: 5_000,
            byok_requirements: BYOKRequirementsKind::AwsKms,
            residency_requirements: ResidencyKind::Eu,
            additional_notes: None,
            locale: "en-DE".to_string(),
            use_case: "EU cache".to_string(),
        }
    }

    fn us_form() -> EnterpriseInquiryForm {
        let mut f = eu_form();
        f.residency_requirements = ResidencyKind::Us;
        f.company = "Acme Inc".to_string();
        f
    }

    fn client_us(
        http: RecordingHubSpotHttp,
    ) -> HubSpotCrmClient<RecordingHubSpotHttp, NoopSleeper> {
        HubSpotCrmClient::new(http, NoopSleeper::new(), token(), HubSpotRegion::Us1).unwrap()
    }

    fn client_eu(
        http: RecordingHubSpotHttp,
    ) -> HubSpotCrmClient<RecordingHubSpotHttp, NoopSleeper> {
        HubSpotCrmClient::new(http, NoopSleeper::new(), token(), HubSpotRegion::Eu1).unwrap()
    }

    // -------- T1 token validation --------
    #[test]
    fn token_shape_validation_rejects_malformed_token() {
        assert!(matches!(
            HubSpotToken::new("not-a-token"),
            Err(HubSpotConfigError::InvalidTokenShape)
        ));
        assert!(matches!(
            HubSpotToken::new("pat-na1-short"),
            Err(HubSpotConfigError::InvalidTokenShape)
        ));
        let t = HubSpotToken::new("pat-na1-AAAAAAAAAAAAAAAA-deadbeef").unwrap();
        assert_eq!(t.bearer(), "Bearer pat-na1-AAAAAAAAAAAAAAAA-deadbeef");
        // Debug must redact.
        assert_eq!(format!("{:?}", t), "HubSpotToken(<redacted>)");
    }

    // -------- T2 region routing --------
    #[test]
    fn region_base_url_routes_us_vs_eu() {
        assert_eq!(HubSpotRegion::Us1.base_url(), "https://api.hubapi.com");
        assert_eq!(HubSpotRegion::Eu1.base_url(), "https://api.hubapi.eu");
    }

    // -------- T3 residency mismatch rejection --------
    #[test]
    fn eu_inquiry_against_us_region_rejected_pre_flight() {
        let http = RecordingHubSpotHttp::new();
        let c = client_us(http.clone());
        let enc = InMemoryInquiryPayloadEncryptor::new();
        let sealed = seal_inquiry(&eu_form(), "tenant-1", b"k", &enc).unwrap();
        let err = c
            .create_entry(
                &InquiryId::new("inq-eu-on-us"),
                &sealed,
                "tenant-1",
                &enc,
                100,
            )
            .unwrap_err();
        assert!(matches!(err, CrmError::Rejected(ref m) if m.contains("residency-mismatch")));
        // No wire request was issued.
        assert_eq!(http.request_count(), 0);
    }

    // -------- T4 us inquiry against eu rejected --------
    #[test]
    fn us_inquiry_against_eu_region_rejected_pre_flight() {
        let http = RecordingHubSpotHttp::new();
        let c = client_eu(http.clone());
        let enc = InMemoryInquiryPayloadEncryptor::new();
        let sealed = seal_inquiry(&us_form(), "tenant-1", b"k", &enc).unwrap();
        let err = c
            .create_entry(
                &InquiryId::new("inq-us-on-eu"),
                &sealed,
                "tenant-1",
                &enc,
                50,
            )
            .unwrap_err();
        assert!(matches!(err, CrmError::Rejected(ref m) if m.contains("residency-mismatch")));
        assert_eq!(http.request_count(), 0);
    }

    // -------- T5 contact create happy path --------
    #[test]
    fn create_contact_happy_path() {
        let http = RecordingHubSpotHttp::new();
        http.push_response(
            201,
            "{\"id\":\"100001\",\"properties\":{\"email\":\"a@b.c\"}}",
        );
        let c = client_us(http.clone());
        let id = c
            .create_contact(
                "a@b.c",
                "Alice",
                "Acme",
                "Acme Inc",
                &InquiryId::new("inq-1"),
            )
            .unwrap();
        assert_eq!(id, "100001");
        let req = &http.requests()[0];
        assert_eq!(req.method, HubSpotMethod::Post);
        assert_eq!(req.url, "https://api.hubapi.com/crm/v3/objects/contacts");
        assert!(req.body.contains("\"email\":\"a@b.c\""));
        assert!(req
            .body
            .contains("\"hs_unique_creation_key\":\"contact:inq-1\""));
    }

    // -------- T6 company search-then-create idempotent (hit) --------
    #[test]
    fn find_or_create_company_returns_existing_when_search_hits() {
        let http = RecordingHubSpotHttp::new();
        http.push_response(
            200,
            "{\"total\":1,\"results\":[{\"id\":\"900\",\"properties\":{\"name\":\"Acme Inc\"}}]}",
        );
        let c = client_us(http.clone());
        let id = c
            .find_or_create_company("Acme Inc", Some("US-12-345"), &InquiryId::new("inq-2"))
            .unwrap();
        assert_eq!(id, "900");
        // ONLY the search request fired — no create.
        assert_eq!(http.request_count(), 1);
        assert!(http.requests()[0]
            .url
            .ends_with("/crm/v3/objects/companies/search"));
    }

    // -------- T7 company search-then-create idempotent (miss → create) --------
    #[test]
    fn find_or_create_company_creates_when_search_misses() {
        let http = RecordingHubSpotHttp::new();
        http.push_response(200, "{\"total\":0,\"results\":[]}");
        http.push_response(
            201,
            "{\"id\":\"901\",\"properties\":{\"name\":\"Acme Inc\"}}",
        );
        let c = client_us(http.clone());
        let id = c
            .find_or_create_company("Acme Inc", None, &InquiryId::new("inq-3"))
            .unwrap();
        assert_eq!(id, "901");
        let reqs = http.requests();
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0].url.ends_with("/crm/v3/objects/companies/search"));
        assert!(reqs[1].url.ends_with("/crm/v3/objects/companies"));
        assert!(reqs[1]
            .body
            .contains("\"hs_unique_creation_key\":\"company:inq-3\""));
    }

    // -------- T8 deal create linked to contact + company --------
    #[test]
    fn create_deal_carries_associations_and_stage() {
        let http = RecordingHubSpotHttp::new();
        http.push_response(201, "{\"id\":\"7777\"}");
        let c = client_us(http.clone());
        let id = c
            .create_deal(
                "C-1",
                "CO-2",
                DEAL_STAGE_ENTERPRISE_INQUIRY,
                250_000,
                &InquiryId::new("inq-4"),
            )
            .unwrap();
        assert_eq!(id, "7777");
        let body = &http.requests()[0].body;
        assert!(body.contains("\"dealstage\":\"enterprise_inquiry\""));
        assert!(body.contains("\"amount\":\"250000\""));
        assert!(body.contains("\"to\":{\"id\":\"C-1\"}"));
        assert!(body.contains("\"to\":{\"id\":\"CO-2\"}"));
        assert!(body.contains("\"hs_unique_creation_key\":\"deal:inq-4\""));
    }

    // -------- T9 create_entry end-to-end (3 calls) --------
    #[test]
    fn create_entry_orchestrates_contact_company_deal() {
        let http = RecordingHubSpotHttp::new();
        // contact
        http.push_response(201, "{\"id\":\"1001\"}");
        // company search miss
        http.push_response(200, "{\"total\":0,\"results\":[]}");
        // company create
        http.push_response(201, "{\"id\":\"2001\"}");
        // deal
        http.push_response(201, "{\"id\":\"3001\"}");
        let c = client_eu(http.clone());
        let enc = InMemoryInquiryPayloadEncryptor::new();
        let sealed = seal_inquiry(&eu_form(), "tenant-1", b"k", &enc).unwrap();
        let entry = c
            .create_entry(&InquiryId::new("inq-e2e"), &sealed, "tenant-1", &enc, 100)
            .unwrap();
        assert_eq!(entry.as_str(), "3001");
        assert_eq!(http.request_count(), 4);
        for r in http.requests() {
            assert!(r.url.starts_with("https://api.hubapi.eu"));
        }
        // Plaintext PII reached the HubSpot wire (this IS the
        // permitted decryption boundary).
        let bodies: Vec<String> = http.requests().iter().map(|r| r.body.clone()).collect();
        let joined = bodies.join("\n");
        assert!(joined.contains("ciso@acme.example"));
        assert!(joined.contains("Acme GmbH"));
    }

    // -------- T-r2-11 a: AAD tenant swap rejected at HubSpot boundary --------
    #[test]
    fn r2_11_hubspot_rejects_cross_tenant_push() {
        let http = RecordingHubSpotHttp::new();
        let c = client_eu(http.clone());
        let enc = InMemoryInquiryPayloadEncryptor::new();
        let sealed = seal_inquiry(&eu_form(), "tenant-A", b"k", &enc).unwrap();
        let err = c
            .create_entry(&InquiryId::new("inq-x"), &sealed, "tenant-B", &enc, 100)
            .unwrap_err();
        assert!(matches!(err, CrmError::Encryption(_)));
        // Zero wire requests issued — fail closed BEFORE wire egress.
        assert_eq!(http.request_count(), 0);
    }

    // -------- T10 retry on 5xx with backoff --------
    #[test]
    fn retry_on_5xx_succeeds_after_two_failures() {
        let http = RecordingHubSpotHttp::new();
        http.push_response(503, "{\"status\":\"error\"}");
        http.push_response(502, "{\"status\":\"error\"}");
        http.push_response(201, "{\"id\":\"42\"}");
        let sleeper = NoopSleeper::new();
        let c = HubSpotCrmClient::new(http.clone(), sleeper.clone(), token(), HubSpotRegion::Us1)
            .unwrap();
        let id = c
            .create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-retry"))
            .unwrap();
        assert_eq!(id, "42");
        assert_eq!(http.request_count(), 3);
        // Sleeps recorded: 250ms then 500ms (exponential).
        let sleeps = sleeper.sleeps();
        assert_eq!(sleeps, vec![250, 500]);
    }

    // -------- T11 retry budget exhaustion --------
    #[test]
    fn retry_budget_exhaustion_after_five_5xx() {
        let http = RecordingHubSpotHttp::new();
        for _ in 0..6 {
            http.push_response(503, "x");
        }
        let c = client_us(http.clone());
        let err = c
            .create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-x"))
            .unwrap_err();
        assert!(matches!(err, CrmError::Transport(ref m) if m.contains("retry budget exhausted")));
        // 5 retries + 1 initial = 6 attempts.
        assert_eq!(http.request_count(), 6);
    }

    // -------- T12 hard-fail on 401 (no retry) --------
    #[test]
    fn hard_fail_on_401_no_retry() {
        let http = RecordingHubSpotHttp::new();
        http.push_response(401, "{\"category\":\"INVALID_AUTHENTICATION\"}");
        let c = client_us(http.clone());
        let err = c
            .create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-401"))
            .unwrap_err();
        assert!(matches!(err, CrmError::Rejected(ref m) if m.contains("auth hard-fail")));
        // Token redaction MUST hold in the error.
        let CrmError::Rejected(m) = err else {
            panic!("expected Rejected variant");
        };
        assert!(m.contains("<redacted>"));
        // Single attempt.
        assert_eq!(http.request_count(), 1);
    }

    // -------- T13 retry-after honoured --------
    #[test]
    fn retry_after_header_honoured_and_capped() {
        let http = RecordingHubSpotHttp::new();
        http.push_response_with_retry_after(429, "{}", Some(2));
        http.push_response_with_retry_after(429, "{}", Some(60)); // capped to 30
        http.push_response(201, "{\"id\":\"99\"}");
        let sleeper = NoopSleeper::new();
        let c = HubSpotCrmClient::new(http.clone(), sleeper.clone(), token(), HubSpotRegion::Us1)
            .unwrap();
        c.create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-429"))
            .unwrap();
        // Sleeps: 2_000 (server hint), 30_000 (capped from 60s).
        assert_eq!(sleeper.sleeps(), vec![2_000, 30_000]);
    }

    // -------- T14 4xx non-retryable hard-fail --------
    #[test]
    fn validation_4xx_is_not_retried() {
        let http = RecordingHubSpotHttp::new();
        http.push_response(
            400,
            "{\"category\":\"VALIDATION_ERROR\",\"message\":\"bad email\"}",
        );
        let c = client_us(http.clone());
        let err = c
            .create_contact("not-an-email", "A", "B", "Co", &InquiryId::new("inq-400"))
            .unwrap_err();
        assert!(matches!(err, CrmError::Rejected(_)));
        assert_eq!(http.request_count(), 1);
    }

    // -------- T15 ticket create --------
    #[test]
    fn create_ticket_carries_subject_content_owner() {
        let http = RecordingHubSpotHttp::new();
        http.push_response(201, "{\"id\":\"T-1\"}");
        let c = client_us(http.clone());
        let id = c
            .create_ticket(
                "Enterprise inquiry",
                "Please follow up",
                Some("OWNER-1"),
                &InquiryId::new("inq-tkt"),
            )
            .unwrap();
        assert_eq!(id, "T-1");
        let body = &http.requests()[0].body;
        assert!(body.contains("\"subject\":\"Enterprise inquiry\""));
        assert!(body.contains("\"hubspot_owner_id\":\"OWNER-1\""));
    }

    // -------- T16 compensate PATCHes the deal --------
    #[test]
    fn compensate_patches_deal_to_closedlost() {
        let http = RecordingHubSpotHttp::new();
        http.push_response(
            200,
            "{\"id\":\"3001\",\"properties\":{\"dealstage\":\"closedlost\"}}",
        );
        let c = client_us(http.clone());
        c.compensate(&InquiryId::new("inq-cmp"), &CrmEntryId::new("3001"))
            .unwrap();
        let req = &http.requests()[0];
        assert_eq!(req.method, HubSpotMethod::Patch);
        assert!(req.url.ends_with("/crm/v3/objects/deals/3001"));
        assert!(req.body.contains("\"dealstage\":\"closedlost\""));
        assert!(req.body.contains("\"saga_rollback\":\"true\""));
    }

    // -------- T17 transport failure retried then succeeds --------
    #[test]
    fn transport_failure_retried_then_succeeds() {
        let http = RecordingHubSpotHttp::new();
        http.push_transport_failure("connection reset");
        http.push_response(201, "{\"id\":\"55\"}");
        let c = client_us(http.clone());
        let id = c
            .create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-net"))
            .unwrap();
        assert_eq!(id, "55");
    }

    // -------- T18 from_env happy path --------
    #[test]
    fn from_env_reads_token_and_region() {
        let env = |k: &str| match k {
            "HUBSPOT_PRIVATE_APP_TOKEN" => Some("pat-na1-AAAAAAAAAAAAAAAA-deadbeef".to_string()),
            "HUBSPOT_REGION" => Some("eu1".to_string()),
            _ => None,
        };
        let c = HubSpotCrmClient::from_env(RecordingHubSpotHttp::new(), NoopSleeper::new(), env)
            .unwrap();
        assert_eq!(c.region(), HubSpotRegion::Eu1);
    }

    // -------- T19 from_env missing var --------
    #[test]
    fn from_env_missing_var_errors() {
        let env = |_: &str| -> Option<String> { None };
        let err = HubSpotCrmClient::from_env(RecordingHubSpotHttp::new(), NoopSleeper::new(), env)
            .unwrap_err();
        assert!(matches!(err, HubSpotConfigError::EnvMissing(_)));
    }

    // -------- T20 classify_retry pure helper --------
    #[test]
    fn classify_retry_pure_helper() {
        let ok = HubSpotResponse {
            status: 200,
            body: String::new(),
            retry_after_s: None,
        };
        assert_eq!(classify_retry(&ok, 0), RetryDecision::Success);
        let auth = HubSpotResponse {
            status: 401,
            body: String::new(),
            retry_after_s: None,
        };
        assert_eq!(classify_retry(&auth, 0), RetryDecision::AuthHardFail);
        let nr = HubSpotResponse {
            status: 422,
            body: String::new(),
            retry_after_s: None,
        };
        assert_eq!(classify_retry(&nr, 0), RetryDecision::NonRetryable);
        let r5 = HubSpotResponse {
            status: 503,
            body: String::new(),
            retry_after_s: None,
        };
        assert_eq!(
            classify_retry(&r5, 0),
            RetryDecision::Retry { delay_ms: 250 }
        );
        assert_eq!(classify_retry(&r5, 5), RetryDecision::GiveUp);
    }

    // -------- T-extra: Authorization header injected --------
    #[test]
    fn authorization_header_injected_into_request() {
        let http = RecordingHubSpotHttp::new();
        http.push_response(201, "{\"id\":\"1\"}");
        let c = client_us(http.clone());
        c.create_contact("a@b.c", "A", "B", "Co", &InquiryId::new("inq-auth"))
            .unwrap();
        let req = &http.requests()[0];
        assert_eq!(
            req.authorization,
            "Bearer pat-na1-AAAAAAAAAAAAAAAA-deadbeef"
        );
        // Debug must redact the authorization field.
        let dbg = format!("{:?}", req);
        assert!(dbg.contains("authorization: \"<redacted>\""));
        assert!(!dbg.contains("pat-na1"));
    }

    // -------- T21 residency_routable matrix --------
    #[test]
    fn residency_routable_matrix() {
        assert!(is_residency_routable(ResidencyKind::Eu, HubSpotRegion::Eu1));
        assert!(!is_residency_routable(
            ResidencyKind::Eu,
            HubSpotRegion::Us1
        ));
        assert!(is_residency_routable(ResidencyKind::Us, HubSpotRegion::Us1));
        assert!(!is_residency_routable(
            ResidencyKind::Us,
            HubSpotRegion::Eu1
        ));
        // None / Sam / Apac / Specific currently permitted on either
        // (legal review out-of-band).
        assert!(is_residency_routable(
            ResidencyKind::None,
            HubSpotRegion::Us1
        ));
        assert!(is_residency_routable(
            ResidencyKind::Apac,
            HubSpotRegion::Eu1
        ));
    }
}
