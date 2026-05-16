//! HTTPS Stripe API client implementing
//! [`corelink_tier_selection::stripe::StripeClient`].
//!
//! Uses `reqwest::blocking` for synchronous trait compatibility. The
//! consumer in `apps/server` wraps `create_checkout_session` calls in
//! `tokio::task::spawn_blocking` to keep the async runtime unblocked.

use std::env;
use std::sync::Arc;
use std::time::Duration;

use corelink_tier_selection::error::TierError;
use corelink_tier_selection::stripe::{
    CheckoutSessionRequest, CheckoutSessionResponse, StripeClient,
};
use corelink_tier_selection::tenant::StripeCustomerId;
use serde::Deserialize;

use crate::clock::{default_clock, Clock};
use crate::error::StripeError;
use crate::retry::RetryPolicy;

/// Canonical Stripe API base URL.
pub const STRIPE_API_BASE: &str = "https://api.stripe.com";

/// Builder for [`StripeRealClient`].
#[derive(Debug)]
#[non_exhaustive]
pub struct StripeRealClientBuilder {
    api_key: Option<String>,
    api_base: String,
    retry_policy: RetryPolicy,
    timeout: Duration,
    clock: Arc<dyn Clock + Send + Sync>,
}

impl Default for StripeRealClientBuilder {
    fn default() -> Self {
        Self {
            api_key: None,
            api_base: STRIPE_API_BASE.to_string(),
            retry_policy: RetryPolicy::default(),
            timeout: Duration::from_secs(30),
            // Wave-20: default = `SystemClock` on native, `WasmWorkerClock`
            // on wasm32 (this module is native-only so always `SystemClock`,
            // but `default_clock()` keeps the call site target-agnostic for
            // when the HTTPS client surface is ported to wasm32 in a
            // future wave).
            clock: default_clock(),
        }
    }
}

impl StripeRealClientBuilder {
    /// Construct an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the API key explicitly (overrides env resolution).
    #[must_use]
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Override the API base URL (used by tests + Stripe sandbox).
    #[must_use]
    pub fn api_base(mut self, base: impl Into<String>) -> Self {
        self.api_base = base.into();
        self
    }

    /// Override the retry policy.
    #[must_use]
    pub const fn retry_policy(mut self, p: RetryPolicy) -> Self {
        self.retry_policy = p;
        self
    }

    /// Override the per-request timeout.
    #[must_use]
    pub const fn timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    /// Inject a [`Clock`] implementation. Default = `SystemClock`
    /// (native) / `WasmWorkerClock` (wasm32). Tests typically pass
    /// [`crate::clock::InMemoryFakeClock`] for deterministic
    /// timestamps in idempotency keys / SLI markers.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock + Send + Sync>) -> Self {
        self.clock = clock;
        self
    }

    /// Build the client.
    ///
    /// # Errors
    ///
    /// - [`StripeError::Authentication`] if no API key was provided
    ///   and the env vars are unset.
    /// - [`StripeError::ApiConnection`] if the underlying HTTP client
    ///   fails to initialize.
    pub fn build(self) -> Result<StripeRealClient, StripeError> {
        let api_key = match self.api_key {
            Some(k) => k,
            None => resolve_api_key_from_env()?,
        };
        let http = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| StripeError::ApiConnection(e.to_string()))?;
        Ok(StripeRealClient {
            api_key,
            api_base: self.api_base,
            retry_policy: self.retry_policy,
            http,
            clock: self.clock,
        })
    }
}

/// Resolve the Stripe API key from env. Prefers `STRIPE_SECRET_KEY`
/// (live) over `STRIPE_SECRET_KEY_TEST`.
fn resolve_api_key_from_env() -> Result<String, StripeError> {
    if let Ok(k) = env::var("STRIPE_SECRET_KEY") {
        if !k.is_empty() {
            return Ok(k);
        }
    }
    if let Ok(k) = env::var("STRIPE_SECRET_KEY_TEST") {
        if !k.is_empty() {
            return Ok(k);
        }
    }
    Err(StripeError::Authentication(
        "neither STRIPE_SECRET_KEY nor STRIPE_SECRET_KEY_TEST is set".to_string(),
    ))
}

/// Production Stripe HTTPS client.
///
/// The `Debug` impl REDACTS the API key — secrets MUST never appear
/// in logs.
pub struct StripeRealClient {
    api_key: String,
    api_base: String,
    retry_policy: RetryPolicy,
    http: reqwest::blocking::Client,
    // Wave-20: held for future timestamp-bearing operations (idempotency
    // key TTL eviction, retry-budget windows). Currently the production
    // HTTPS layer reads no wall-clock — Stripe owns idempotency-key
    // retention. Kept here as the canonical injection point so test
    // harnesses can pin time via `.with_clock(...)` once a clock-dependent
    // operation lands.
    #[allow(dead_code, reason = "wave-20 injection scaffold; consumed by future timestamp ops")]
    clock: Arc<dyn Clock + Send + Sync>,
}

impl core::fmt::Debug for StripeRealClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StripeRealClient")
            .field("api_key", &"<redacted>")
            .field("api_base", &self.api_base)
            .field("retry_policy", &self.retry_policy)
            .field("clock", &self.clock)
            .finish()
    }
}

impl StripeRealClient {
    /// Construct from env vars (`STRIPE_SECRET_KEY` or `STRIPE_SECRET_KEY_TEST`).
    ///
    /// # Errors
    /// See [`StripeRealClientBuilder::build`].
    pub fn from_env() -> Result<Self, StripeError> {
        StripeRealClientBuilder::new().build()
    }

    /// Returns a new [`StripeRealClientBuilder`].
    #[must_use]
    pub fn builder() -> StripeRealClientBuilder {
        StripeRealClientBuilder::new()
    }

    /// Borrow the configured API base (e.g. for assertion in tests).
    #[must_use]
    pub fn api_base(&self) -> &str {
        &self.api_base
    }

    /// POST to a Stripe form-encoded endpoint with idempotency + retry.
    ///
    /// `idempotency_key` MUST be deterministic per logical request so
    /// retries are safe per Stripe spec.
    fn post_form<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        form: &[(&str, String)],
        idempotency_key: &str,
    ) -> Result<T, StripeError> {
        let url = format!("{}{}", self.api_base, path);
        let mut attempt: u32 = 0;
        loop {
            let req = self
                .http
                .post(&url)
                .basic_auth(&self.api_key, Some(""))
                .header("Idempotency-Key", idempotency_key)
                .header("Stripe-Version", "2024-06-20")
                .form(form);
            let resp = match req.send() {
                Ok(r) => r,
                Err(e) => {
                    // Transport-level failure — retryable up to budget.
                    if let Some(ms) = self.retry_policy.next_sleep_ms(attempt, None) {
                        std::thread::sleep(Duration::from_millis(ms));
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                    return Err(StripeError::ApiConnection(e.to_string()));
                }
            };
            let status = resp.status().as_u16();
            let retry_after = parse_retry_after(&resp);
            if (200..300).contains(&status) {
                return resp
                    .json::<T>()
                    .map_err(|e| StripeError::ApiConnection(format!("json decode: {e}")));
            }
            if RetryPolicy::is_retryable_status(status) {
                if let Some(ms) = self.retry_policy.next_sleep_ms(attempt, retry_after) {
                    std::thread::sleep(Duration::from_millis(ms));
                    attempt = attempt.saturating_add(1);
                    continue;
                }
            }
            // Non-retryable OR retries exhausted → map to typed error.
            let body = resp.text().unwrap_or_default();
            return Err(map_api_error(status, &body, retry_after));
        }
    }

    /// GET a Stripe endpoint with retry on 5xx/429.
    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, StripeError> {
        let url = format!("{}{}", self.api_base, path);
        let mut attempt: u32 = 0;
        loop {
            let req = self
                .http
                .get(&url)
                .basic_auth(&self.api_key, Some(""))
                .header("Stripe-Version", "2024-06-20");
            let resp = match req.send() {
                Ok(r) => r,
                Err(e) => {
                    if let Some(ms) = self.retry_policy.next_sleep_ms(attempt, None) {
                        std::thread::sleep(Duration::from_millis(ms));
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                    return Err(StripeError::ApiConnection(e.to_string()));
                }
            };
            let status = resp.status().as_u16();
            let retry_after = parse_retry_after(&resp);
            if (200..300).contains(&status) {
                return resp
                    .json::<T>()
                    .map_err(|e| StripeError::ApiConnection(format!("json decode: {e}")));
            }
            if RetryPolicy::is_retryable_status(status) {
                if let Some(ms) = self.retry_policy.next_sleep_ms(attempt, retry_after) {
                    std::thread::sleep(Duration::from_millis(ms));
                    attempt = attempt.saturating_add(1);
                    continue;
                }
            }
            let body = resp.text().unwrap_or_default();
            return Err(map_api_error(status, &body, retry_after));
        }
    }

    /// `POST /v1/customers` — create a customer.
    ///
    /// # Errors
    /// See [`StripeError`] taxonomy.
    pub fn create_customer(
        &self,
        email: &str,
        tenant_id: &str,
        idempotency_key: &str,
    ) -> Result<CustomerObject, StripeError> {
        let form = vec![
            ("email", email.to_string()),
            ("metadata[tenant_id]", tenant_id.to_string()),
        ];
        self.post_form::<CustomerObject>("/v1/customers", &form, idempotency_key)
    }

    /// `GET /v1/customers/:id` — fetch a customer.
    ///
    /// # Errors
    /// See [`StripeError`] taxonomy.
    pub fn get_customer(&self, id: &str) -> Result<CustomerObject, StripeError> {
        self.get::<CustomerObject>(&format!("/v1/customers/{id}"))
    }

    /// `POST /v1/subscriptions` — create a subscription.
    ///
    /// # Errors
    /// See [`StripeError`] taxonomy.
    pub fn create_subscription(
        &self,
        customer_id: &str,
        price_id: &str,
        idempotency_key: &str,
    ) -> Result<SubscriptionObject, StripeError> {
        let form = vec![
            ("customer", customer_id.to_string()),
            ("items[0][price]", price_id.to_string()),
        ];
        self.post_form::<SubscriptionObject>("/v1/subscriptions", &form, idempotency_key)
    }

    /// `GET /v1/subscriptions/:id` — fetch a subscription.
    ///
    /// # Errors
    /// See [`StripeError`] taxonomy.
    pub fn get_subscription(&self, id: &str) -> Result<SubscriptionObject, StripeError> {
        self.get::<SubscriptionObject>(&format!("/v1/subscriptions/{id}"))
    }

    /// `POST /v1/billing_portal/sessions` — create a Customer Portal session.
    ///
    /// # Errors
    /// See [`StripeError`] taxonomy.
    pub fn create_billing_portal_session(
        &self,
        customer_id: &str,
        return_url: &str,
        idempotency_key: &str,
    ) -> Result<BillingPortalSession, StripeError> {
        let form = vec![
            ("customer", customer_id.to_string()),
            ("return_url", return_url.to_string()),
        ];
        self.post_form::<BillingPortalSession>(
            "/v1/billing_portal/sessions",
            &form,
            idempotency_key,
        )
    }

    /// `POST /v1/checkout/sessions` — typed Stripe Checkout creation
    /// (returns the raw Stripe object).
    fn create_checkout_session_raw(
        &self,
        req: &CheckoutSessionRequest,
        idempotency_key: &str,
        price_id: &str,
    ) -> Result<CheckoutSessionObject, StripeError> {
        let form = vec![
            ("mode", "subscription".to_string()),
            ("customer_email", req.customer_email.clone()),
            ("success_url", req.success_url.clone()),
            ("cancel_url", req.cancel_url.clone()),
            ("line_items[0][price]", price_id.to_string()),
            ("line_items[0][quantity]", "1".to_string()),
            ("metadata[tenant_id]", req.tenant_id.as_str().to_string()),
            ("metadata[tier]", req.tier.as_str().to_string()),
        ];
        self.post_form::<CheckoutSessionObject>("/v1/checkout/sessions", &form, idempotency_key)
    }
}

impl StripeClient for StripeRealClient {
    fn create_checkout_session(
        &self,
        req: &CheckoutSessionRequest,
    ) -> Result<CheckoutSessionResponse, TierError> {
        // Idempotency key: deterministic per (tenant, tier).
        // 24h Stripe window matches the WI §6.4 60s lock — same caller
        // retrying within the lock window gets the identical session.
        let idem = format!(
            "checkout:{}:{}",
            req.tenant_id.as_str(),
            req.tier.as_str()
        );
        // Production wiring resolves price_id per tier via env or a
        // config map; we pull from env for now so callers can override
        // without touching this crate.
        let price_env = format!("STRIPE_PRICE_ID_{}", req.tier.as_str().to_uppercase());
        let price_id = env::var(&price_env)
            .map_err(|_| TierError::Stripe(format!("env var {price_env} not set")))?;

        let raw = self
            .create_checkout_session_raw(req, &idem, &price_id)
            .map_err(|e| TierError::Stripe(e.to_string()))?;

        let customer = raw
            .customer
            .ok_or_else(|| TierError::Stripe("missing customer on checkout session".to_string()))?;

        let url = raw
            .url
            .ok_or_else(|| TierError::Stripe("missing url on checkout session".to_string()))?;
        Ok(CheckoutSessionResponse::new(
            raw.id,
            StripeCustomerId::new(customer),
            url,
        ))
    }
}

/// Parse `Retry-After` header (seconds).
fn parse_retry_after(resp: &reqwest::blocking::Response) -> Option<u64> {
    resp.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

/// Map a non-2xx Stripe response to the typed [`StripeError`].
fn map_api_error(http_status: u16, body: &str, retry_after_seconds: Option<u64>) -> StripeError {
    #[derive(Deserialize)]
    struct Envelope {
        error: Option<ApiErr>,
    }
    #[derive(Deserialize)]
    struct ApiErr {
        #[serde(rename = "type")]
        ty: Option<String>,
        code: Option<String>,
        message: Option<String>,
    }

    let parsed: Option<ApiErr> = serde_json::from_str::<Envelope>(body)
        .ok()
        .and_then(|e| e.error);
    let ty = parsed.as_ref().and_then(|p| p.ty.clone()).unwrap_or_default();
    let code = parsed
        .as_ref()
        .and_then(|p| p.code.clone())
        .unwrap_or_default();
    let message = parsed
        .as_ref()
        .and_then(|p| p.message.clone())
        .unwrap_or_else(|| body.to_string());

    match http_status {
        401 => StripeError::Authentication(message),
        402 => StripeError::CardDeclined {
            code: if code.is_empty() {
                "card_declined".to_string()
            } else {
                code
            },
            message,
        },
        429 => StripeError::RateLimited {
            message,
            retry_after_seconds,
        },
        400 if ty == "idempotency_error" => StripeError::Idempotency(message),
        400 => StripeError::InvalidRequest(message),
        _ => StripeError::Generic {
            http_status,
            code,
            message,
        },
    }
}

/// Stripe `customer` object (subset of fields we read).
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct CustomerObject {
    /// Stripe-assigned customer id (`cus_...`).
    pub id: String,
    /// Customer email (echoed).
    pub email: Option<String>,
}

/// Stripe `subscription` object (subset).
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct SubscriptionObject {
    /// Stripe-assigned subscription id (`sub_...`).
    pub id: String,
    /// Subscription status (`active`, `past_due`, `canceled`, ...).
    pub status: String,
    /// Customer id.
    pub customer: String,
}

/// Stripe `billing_portal.session` object (subset).
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct BillingPortalSession {
    /// Session id (`bps_...`).
    pub id: String,
    /// Hosted portal URL.
    pub url: String,
    /// Customer id.
    pub customer: String,
}

/// Stripe `checkout.session` object (subset).
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct CheckoutSessionObject {
    /// Session id (`cs_...`).
    pub id: String,
    /// Hosted checkout URL (None for `mode=setup` etc.).
    pub url: Option<String>,
    /// Customer id (None until session completes for some flows; for
    /// `mode=subscription` Stripe sets it pre-completion).
    pub customer: Option<String>,
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

    #[test]
    fn debug_redacts_api_key() {
        // Construct via builder to avoid env reliance.
        let c = StripeRealClient::builder()
            .api_key("sk_test_DO_NOT_LOG_ME")
            .build()
            .unwrap();
        let dbg = format!("{c:?}");
        assert!(dbg.contains("<redacted>"), "Debug must redact key: {dbg}");
        assert!(
            !dbg.contains("DO_NOT_LOG_ME"),
            "Debug must NOT contain raw key: {dbg}"
        );
    }

    #[test]
    fn builder_defaults() {
        let c = StripeRealClient::builder()
            .api_key("sk_test_x")
            .build()
            .unwrap();
        assert_eq!(c.api_base(), STRIPE_API_BASE);
    }

    #[test]
    fn from_env_fails_without_keys() {
        // Snapshot + clear; restore at end. SAFETY: tests are single-threaded
        // per `cargo test`'s default for env-touching tests when run
        // serially. We don't use `--test-threads` overrides.
        let live = env::var("STRIPE_SECRET_KEY").ok();
        let test = env::var("STRIPE_SECRET_KEY_TEST").ok();
        env::remove_var("STRIPE_SECRET_KEY");
        env::remove_var("STRIPE_SECRET_KEY_TEST");
        let err = StripeRealClient::from_env().unwrap_err();
        assert!(matches!(err, StripeError::Authentication(_)));
        // Restore for other tests.
        if let Some(v) = live {
            env::set_var("STRIPE_SECRET_KEY", v);
        }
        if let Some(v) = test {
            env::set_var("STRIPE_SECRET_KEY_TEST", v);
        }
    }

    #[test]
    fn map_api_error_authentication() {
        let body = r#"{"error":{"type":"invalid_request_error","message":"Invalid API Key"}}"#;
        let e = map_api_error(401, body, None);
        assert!(matches!(e, StripeError::Authentication(_)));
    }

    #[test]
    fn map_api_error_card_declined() {
        let body = r#"{"error":{"type":"card_error","code":"insufficient_funds","message":"Your card has insufficient funds."}}"#;
        let e = map_api_error(402, body, None);
        match e {
            StripeError::CardDeclined { code, .. } => assert_eq!(code, "insufficient_funds"),
            other => panic!("expected CardDeclined, got {other:?}"),
        }
    }

    #[test]
    fn map_api_error_rate_limited_with_retry_after() {
        let body = r#"{"error":{"type":"rate_limit_error","message":"Too many requests"}}"#;
        let e = map_api_error(429, body, Some(7));
        match e {
            StripeError::RateLimited {
                retry_after_seconds,
                ..
            } => assert_eq!(retry_after_seconds, Some(7)),
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[test]
    fn map_api_error_idempotency() {
        let body = r#"{"error":{"type":"idempotency_error","message":"Keys collide"}}"#;
        let e = map_api_error(400, body, None);
        assert!(matches!(e, StripeError::Idempotency(_)));
    }

    #[test]
    fn map_api_error_invalid_request() {
        let body = r#"{"error":{"type":"invalid_request_error","message":"Bad param"}}"#;
        let e = map_api_error(400, body, None);
        assert!(matches!(e, StripeError::InvalidRequest(_)));
    }

    #[test]
    fn map_api_error_generic_5xx() {
        let body = r#"{"error":{"type":"api_error","message":"Internal"}}"#;
        let e = map_api_error(500, body, None);
        match e {
            StripeError::Generic { http_status, .. } => assert_eq!(http_status, 500),
            other => panic!("expected Generic, got {other:?}"),
        }
    }

    #[test]
    fn map_api_error_unparseable_body() {
        let e = map_api_error(503, "<html>nginx</html>", None);
        match e {
            StripeError::Generic {
                http_status,
                message,
                ..
            } => {
                assert_eq!(http_status, 503);
                assert!(message.contains("nginx"));
            }
            other => panic!("expected Generic, got {other:?}"),
        }
    }
}
