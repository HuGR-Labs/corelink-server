//! HTTPS Stripe API client implementing
//! [`corelink_tier_selection::stripe::StripeClient`].
//!
//! # Wave-31 wallet-broker refactor (stream-1)
//!
//! ALL outbound Stripe API calls route through the HuGR Wallet remote
//! credential broker at:
//!
//! ```text
//! {HUGR_WALLET_BASE}/_wallet/proxy/{HUGR_STRIPE_REF}/<upstream_path>
//!     Authorization: Bearer hugrw_<token>
//! ```
//!
//! The wallet validates the `hugrw_` token, strips the Authorization
//! header, decrypts the real Stripe key from KV (AES-256-GCM), injects
//! it as `Authorization: Bearer sk_live_...` on the upstream call,
//! and proxies the response back. CoreLink NEVER holds a real
//! upstream Stripe API secret. If the `hugrw_` token leaks, the wallet
//! owner rotates it without touching CoreLink and the underlying Stripe
//! key never leaks.
//!
//! Webhook signature verification stays direct (see `webhook.rs`) — it
//! is inbound (Stripe → CoreLink) and uses a local
//! `STRIPE_WEBHOOK_SECRET` for HMAC verify; no upstream key involved.
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
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use crate::clock::{default_clock, Clock};
use crate::error::StripeError;
use crate::retry::RetryPolicy;

/// Default HuGR Wallet broker base URL (production).
///
/// Tests inject a `wiremock::MockServer` URI here so they never hit the
/// real wallet. Production is set via the `HUGR_WALLET_BASE` env var
/// (see [`StripeClientConfig::from_env`]).
pub const DEFAULT_HUGR_WALLET_BASE: &str = "https://api.humangr.com";

/// Default HuGR Wallet ref name for the Stripe upstream.
///
/// Per wave-31 wallet-broker series stream-1 the canonical ref is
/// `stripe-prod`. Tests can override via `HUGR_STRIPE_REF`.
pub const DEFAULT_HUGR_STRIPE_REF: &str = "stripe-prod";

/// Wallet-broker configuration for the Stripe HTTPS client.
///
/// Holds everything required to build the upstream proxy URL +
/// `hugrw_` bearer credential. The token is wrapped in a
/// [`SecretString`] so it NEVER leaks via `Debug` / `Display` / panic
/// output. Construct via [`StripeClientConfig::new`] or
/// [`StripeClientConfig::from_env`].
#[derive(Clone)]
#[non_exhaustive]
pub struct StripeClientConfig {
    /// Base URL of the HuGR Wallet broker (e.g.
    /// `https://api.humangr.com` in prod, or a `wiremock` URI in
    /// tests). The proxy path `/_wallet/proxy/<ref>/<...>` is appended
    /// automatically.
    pub wallet_base: String,
    /// CoreLink-side `hugrw_` token authorising the proxy call. The
    /// wallet validates the token + its `proxy` scope on `stripe_ref`.
    /// Held as [`SecretString`] — never printed.
    pub wallet_token: SecretString,
    /// Wallet ref name for the Stripe upstream (canonical:
    /// `stripe-prod`).
    pub stripe_ref: String,
}

impl core::fmt::Debug for StripeClientConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StripeClientConfig")
            .field("wallet_base", &self.wallet_base)
            .field("wallet_token", &"<redacted>")
            .field("stripe_ref", &self.stripe_ref)
            .finish()
    }
}

impl StripeClientConfig {
    /// Construct a config explicitly (used by tests + by the bin
    /// crate at `apps/server` after it reads env).
    #[must_use]
    pub fn new(
        wallet_base: impl Into<String>,
        wallet_token: SecretString,
        stripe_ref: impl Into<String>,
    ) -> Self {
        Self {
            wallet_base: wallet_base.into(),
            wallet_token,
            stripe_ref: stripe_ref.into(),
        }
    }

    /// Resolve a [`StripeClientConfig`] from env vars.
    ///
    /// Reads:
    /// - `HUGR_WALLET_BASE`  (optional; defaults to
    ///   [`DEFAULT_HUGR_WALLET_BASE`]),
    /// - `HUGR_WALLET_TOKEN` (REQUIRED — the `hugrw_` token),
    /// - `HUGR_STRIPE_REF`   (optional; defaults to
    ///   [`DEFAULT_HUGR_STRIPE_REF`]).
    ///
    /// # Errors
    ///
    /// Returns [`StripeError::Authentication`] if `HUGR_WALLET_TOKEN`
    /// is missing or empty.
    pub fn from_env() -> Result<Self, StripeError> {
        let wallet_base = env::var("HUGR_WALLET_BASE")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_HUGR_WALLET_BASE.to_string());
        let stripe_ref = env::var("HUGR_STRIPE_REF")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_HUGR_STRIPE_REF.to_string());
        let raw_token = env::var("HUGR_WALLET_TOKEN").ok().filter(|s| !s.is_empty());
        let raw_token = raw_token.ok_or_else(|| {
            StripeError::Authentication(
                "HUGR_WALLET_TOKEN is not set (wave-31 wallet-broker series)".to_string(),
            )
        })?;
        Ok(Self::new(wallet_base, SecretString::from(raw_token), stripe_ref))
    }

    /// Construct the upstream proxy base URL —
    /// `{wallet_base}/_wallet/proxy/{stripe_ref}`. Stripe paths
    /// (`/v1/...`) are appended by the HTTP layer.
    #[must_use]
    pub fn proxy_base_url(&self) -> String {
        let base = self.wallet_base.trim_end_matches('/');
        format!("{base}/_wallet/proxy/{}", self.stripe_ref)
    }
}

/// Builder for [`StripeRealClient`].
#[derive(Debug)]
#[non_exhaustive]
pub struct StripeRealClientBuilder {
    config: Option<StripeClientConfig>,
    retry_policy: RetryPolicy,
    timeout: Duration,
    clock: Arc<dyn Clock + Send + Sync>,
}

impl Default for StripeRealClientBuilder {
    fn default() -> Self {
        Self {
            config: None,
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

    /// Inject a [`StripeClientConfig`] explicitly (overrides env
    /// resolution at [`Self::build`]).
    #[must_use]
    pub fn config(mut self, cfg: StripeClientConfig) -> Self {
        self.config = Some(cfg);
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
    /// - [`StripeError::Authentication`] if no config was injected and
    ///   `HUGR_WALLET_TOKEN` is unset.
    /// - [`StripeError::ApiConnection`] if the underlying HTTP client
    ///   fails to initialize.
    pub fn build(self) -> Result<StripeRealClient, StripeError> {
        let config = match self.config {
            Some(c) => c,
            None => StripeClientConfig::from_env()?,
        };
        let http = reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| StripeError::ApiConnection(e.to_string()))?;
        let proxy_base = config.proxy_base_url();
        Ok(StripeRealClient {
            config,
            proxy_base,
            retry_policy: self.retry_policy,
            http,
            clock: self.clock,
        })
    }
}

/// Production Stripe HTTPS client routing through the HuGR Wallet
/// broker.
///
/// The `Debug` impl REDACTS the `hugrw_` token — secrets MUST never
/// appear in logs.
pub struct StripeRealClient {
    config: StripeClientConfig,
    /// Cached proxy base — `{wallet_base}/_wallet/proxy/{stripe_ref}`.
    /// Avoids recomputing per-request; tested in
    /// `client_uses_wallet_proxy_url`.
    proxy_base: String,
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
            .field("config", &self.config)
            .field("proxy_base", &self.proxy_base)
            .field("retry_policy", &self.retry_policy)
            .field("clock", &self.clock)
            .finish()
    }
}

impl StripeRealClient {
    /// Construct from env vars via [`StripeClientConfig::from_env`].
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

    /// Borrow the proxy base URL — i.e. the value the next request
    /// will be POSTed under (`{wallet_base}/_wallet/proxy/{stripe_ref}`).
    /// Used by tests to pin the wallet-broker URL contract.
    #[must_use]
    pub fn proxy_base(&self) -> &str {
        &self.proxy_base
    }

    /// Borrow the [`StripeClientConfig`] used to construct this client.
    #[must_use]
    pub fn config(&self) -> &StripeClientConfig {
        &self.config
    }

    /// POST to a Stripe form-encoded endpoint via the wallet proxy
    /// with idempotency + retry.
    ///
    /// `idempotency_key` MUST be deterministic per logical request so
    /// retries are safe per Stripe spec.
    fn post_form<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        form: &[(&str, String)],
        idempotency_key: &str,
    ) -> Result<T, StripeError> {
        let url = format!("{}{}", self.proxy_base, path);
        let mut attempt: u32 = 0;
        loop {
            let req = self
                .http
                .post(&url)
                .bearer_auth(self.config.wallet_token.expose_secret())
                .header("Idempotency-Key", idempotency_key)
                .header("Stripe-Version", "2024-06-20")
                .form(form);
            let resp = match req.send() {
                Ok(r) => r,
                Err(e) => {
                    // Transport-level failure — fail-CLOSED retry up to
                    // budget (wave-31: NO fallback to direct Stripe;
                    // CoreLink no longer holds the upstream key).
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

    /// GET a Stripe endpoint via the wallet proxy with retry on
    /// 5xx/429.
    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, StripeError> {
        let url = format!("{}{}", self.proxy_base, path);
        let mut attempt: u32 = 0;
        loop {
            let req = self
                .http
                .get(&url)
                .bearer_auth(self.config.wallet_token.expose_secret())
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

    /// `POST /v1/customers` — create a customer (via wallet proxy).
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

    /// `GET /v1/customers/:id` — fetch a customer (via wallet proxy).
    ///
    /// # Errors
    /// See [`StripeError`] taxonomy.
    pub fn get_customer(&self, id: &str) -> Result<CustomerObject, StripeError> {
        self.get::<CustomerObject>(&format!("/v1/customers/{id}"))
    }

    /// `POST /v1/subscriptions` — create a subscription (via wallet
    /// proxy).
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

    /// `GET /v1/subscriptions/:id` — fetch a subscription (via wallet
    /// proxy).
    ///
    /// # Errors
    /// See [`StripeError`] taxonomy.
    pub fn get_subscription(&self, id: &str) -> Result<SubscriptionObject, StripeError> {
        self.get::<SubscriptionObject>(&format!("/v1/subscriptions/{id}"))
    }

    /// `POST /v1/billing_portal/sessions` — create a Customer Portal
    /// session (via wallet proxy).
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
    /// via wallet proxy (returns the raw Stripe object).
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

    fn test_config() -> StripeClientConfig {
        StripeClientConfig::new(
            "https://wallet.test",
            SecretString::from("hugrw_test_token".to_string()),
            "stripe-prod",
        )
    }

    #[test]
    fn debug_redacts_wallet_token() {
        let c = StripeRealClient::builder()
            .config(StripeClientConfig::new(
                "https://wallet.test",
                SecretString::from("hugrw_DO_NOT_LOG_ME".to_string()),
                "stripe-prod",
            ))
            .build()
            .unwrap();
        let dbg = format!("{c:?}");
        assert!(dbg.contains("<redacted>"), "Debug must redact token: {dbg}");
        assert!(
            !dbg.contains("DO_NOT_LOG_ME"),
            "Debug must NOT contain raw token: {dbg}"
        );
    }

    #[test]
    fn config_debug_redacts_token() {
        let cfg = StripeClientConfig::new(
            "https://wallet.test",
            SecretString::from("hugrw_SECRET_VALUE".to_string()),
            "stripe-prod",
        );
        let dbg = format!("{cfg:?}");
        assert!(dbg.contains("<redacted>"));
        assert!(!dbg.contains("SECRET_VALUE"));
    }

    #[test]
    fn builder_defaults_consume_injected_config() {
        let c = StripeRealClient::builder().config(test_config()).build().unwrap();
        // Wave-31: proxy base = `{wallet_base}/_wallet/proxy/{stripe_ref}`.
        assert_eq!(
            c.proxy_base(),
            "https://wallet.test/_wallet/proxy/stripe-prod"
        );
    }

    #[test]
    fn proxy_base_trims_trailing_slash() {
        let cfg = StripeClientConfig::new(
            "https://wallet.test/",
            SecretString::from("hugrw_x".to_string()),
            "stripe-prod",
        );
        assert_eq!(
            cfg.proxy_base_url(),
            "https://wallet.test/_wallet/proxy/stripe-prod"
        );
    }

    /// Single env-touching test. The process-global env is shared
    /// across the test runner threads, so we serialise both checks
    /// (missing-token → Authentication error AND defaults applied
    /// when only the token is set) into one test that owns the
    /// transitions deterministically and restores callers' state on
    /// exit. Two separate `#[test]`s would race the cargo test
    /// scheduler.
    #[test]
    fn from_env_contract() {
        let base = env::var("HUGR_WALLET_BASE").ok();
        let tok = env::var("HUGR_WALLET_TOKEN").ok();
        let r = env::var("HUGR_STRIPE_REF").ok();

        // 1. Missing HUGR_WALLET_TOKEN → Authentication.
        env::remove_var("HUGR_WALLET_BASE");
        env::remove_var("HUGR_WALLET_TOKEN");
        env::remove_var("HUGR_STRIPE_REF");
        let err = StripeClientConfig::from_env().unwrap_err();
        assert!(matches!(err, StripeError::Authentication(_)));

        // 2. Empty HUGR_WALLET_TOKEN → Authentication (filter-empty
        //    semantics; matches deploy-config "unset === empty" rule).
        env::set_var("HUGR_WALLET_TOKEN", "");
        let err = StripeClientConfig::from_env().unwrap_err();
        assert!(matches!(err, StripeError::Authentication(_)));

        // 3. Token set + base/ref unset → defaults applied.
        env::set_var("HUGR_WALLET_TOKEN", "hugrw_envtest");
        let cfg = StripeClientConfig::from_env().unwrap();
        assert_eq!(cfg.wallet_base, DEFAULT_HUGR_WALLET_BASE);
        assert_eq!(cfg.stripe_ref, DEFAULT_HUGR_STRIPE_REF);
        assert_eq!(cfg.wallet_token.expose_secret(), "hugrw_envtest");

        // 4. Explicit base + ref → values flow through.
        env::set_var("HUGR_WALLET_BASE", "https://wallet.local");
        env::set_var("HUGR_STRIPE_REF", "stripe-staging");
        let cfg = StripeClientConfig::from_env().unwrap();
        assert_eq!(cfg.wallet_base, "https://wallet.local");
        assert_eq!(cfg.stripe_ref, "stripe-staging");

        // Restore.
        match base {
            Some(v) => env::set_var("HUGR_WALLET_BASE", v),
            None => env::remove_var("HUGR_WALLET_BASE"),
        }
        match tok {
            Some(v) => env::set_var("HUGR_WALLET_TOKEN", v),
            None => env::remove_var("HUGR_WALLET_TOKEN"),
        }
        match r {
            Some(v) => env::set_var("HUGR_STRIPE_REF", v),
            None => env::remove_var("HUGR_STRIPE_REF"),
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
