//! HTTPS Stripe API client implementing
//! [`corelink_tier_selection::stripe::StripeClient`].
//!
//! # Wave-31 dual-mode auth (2026-05-21)
//!
//! Outbound Stripe API calls can route through either of two auth modes,
//! selected at process start via the `STRIPE_AUTH_MODE` env var:
//!
//! - `direct` (DEFAULT — current production): direct call to
//!   `api.stripe.com/v1/...` with `Authorization: Bearer sk_live_...`.
//!   CoreLink holds the upstream Stripe secret in
//!   `STRIPE_SECRET_KEY`. This is the path that runs while the HuGR
//!   Wallet broker is temporarily unavailable.
//! - `wallet-broker` (the wave-31 stream-1 path, kept whole — see
//!   `specs/_audits/sealed/2026-05-16-wallet-broker-stripe.md`): call routes
//!   through `{HUGR_WALLET_BASE}/_wallet/proxy/{HUGR_STRIPE_REF}/<path>`
//!   with `Authorization: Bearer hugrw_<token>`; CoreLink never holds
//!   the real upstream `sk_live_...`. Re-enable by flipping
//!   `STRIPE_AUTH_MODE=wallet-broker` once the wallet is healthy.
//!
//! Both modes fail CLOSED: missing required env vars OR upstream 5xx
//! after retries → typed error to caller. There is NEVER a silent
//! fallback to the other mode (that would expose the upstream key
//! through the broker, or vice versa).
//!
//! Webhook signature verification stays direct (see `webhook.rs`) — it
//! is inbound (Stripe → CoreLink) and uses a local
//! `STRIPE_WEBHOOK_SECRET` for HMAC verify; same in both modes.
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

/// Default direct-to-Stripe API base URL.
pub const DEFAULT_STRIPE_API_BASE: &str = "https://api.stripe.com";

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

/// Selectable auth mode for the Stripe HTTPS client.
///
/// Switched per process via the `STRIPE_AUTH_MODE` env var (see
/// [`StripeClientConfig::from_env`]). Both variants are
/// `#[non_exhaustive]` so future modes can be added without breaking
/// callers; every credential field is wrapped in
/// [`secrecy::SecretString`] so it cannot leak via `Debug` / `Display`
/// / panic output.
#[derive(Clone)]
#[non_exhaustive]
pub enum StripeAuthMode {
    /// Direct to `api.stripe.com` with
    /// `Authorization: Bearer sk_live_…` (or `sk_test_…`).
    ///
    /// This is the DEFAULT mode until the HuGR Wallet broker is
    /// operational again.
    Direct {
        /// Stripe API base URL (default
        /// [`DEFAULT_STRIPE_API_BASE`]).
        api_base: String,
        /// Upstream Stripe secret key (`sk_live_…` or `sk_test_…`).
        /// The value's prefix tells us live vs test; we do not branch
        /// on it here. Held as [`SecretString`] — never printed.
        api_key: SecretString,
    },
    /// Through the HuGR Wallet broker at
    /// `{wallet_base}/_wallet/proxy/{stripe_ref}/…` with
    /// `Authorization: Bearer hugrw_…`.
    WalletBroker {
        /// Base URL of the HuGR Wallet broker (e.g.
        /// `https://api.humangr.com` in prod, or a `wiremock` URI in
        /// tests).
        wallet_base: String,
        /// CoreLink-side `hugrw_` token authorising the proxy call.
        /// The wallet validates the token + its `proxy` scope on
        /// `stripe_ref`. Held as [`SecretString`] — never printed.
        wallet_token: SecretString,
        /// Wallet ref name for the Stripe upstream (canonical:
        /// `stripe-prod`).
        stripe_ref: String,
    },
}

impl core::fmt::Debug for StripeAuthMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Direct { api_base, .. } => f
                .debug_struct("StripeAuthMode::Direct")
                .field("api_base", api_base)
                .field("api_key", &"<redacted>")
                .finish(),
            Self::WalletBroker {
                wallet_base,
                stripe_ref,
                ..
            } => f
                .debug_struct("StripeAuthMode::WalletBroker")
                .field("wallet_base", wallet_base)
                .field("wallet_token", &"<redacted>")
                .field("stripe_ref", stripe_ref)
                .finish(),
        }
    }
}

/// Auth-mode-agnostic configuration for the Stripe HTTPS client.
///
/// Wraps a [`StripeAuthMode`] which carries the credentials + URL
/// shape for the selected mode. Both this struct and the enum are
/// `#[non_exhaustive]`; the `Debug` impl redacts every credential.
/// Construct via [`StripeClientConfig::direct`] /
/// [`StripeClientConfig::wallet_broker`] /
/// [`StripeClientConfig::from_env`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct StripeClientConfig {
    /// Auth mode the client is configured for.
    pub mode: StripeAuthMode,
}

impl StripeClientConfig {
    /// Construct a Direct-mode config explicitly.
    #[must_use]
    pub fn direct(api_base: impl Into<String>, api_key: SecretString) -> Self {
        Self {
            mode: StripeAuthMode::Direct {
                api_base: api_base.into(),
                api_key,
            },
        }
    }

    /// Construct a Wallet-broker-mode config explicitly.
    #[must_use]
    pub fn wallet_broker(
        wallet_base: impl Into<String>,
        wallet_token: SecretString,
        stripe_ref: impl Into<String>,
    ) -> Self {
        Self {
            mode: StripeAuthMode::WalletBroker {
                wallet_base: wallet_base.into(),
                wallet_token,
                stripe_ref: stripe_ref.into(),
            },
        }
    }

    /// Resolve a [`StripeClientConfig`] from env vars.
    ///
    /// `STRIPE_AUTH_MODE` (default `direct` if unset) selects the
    /// mode. Accepted values:
    /// - `direct` → reads `STRIPE_API_BASE` (default
    ///   [`DEFAULT_STRIPE_API_BASE`]) + `STRIPE_SECRET_KEY`
    ///   (REQUIRED).
    /// - `wallet-broker` / `wallet_broker` → reads
    ///   `HUGR_WALLET_BASE` (default [`DEFAULT_HUGR_WALLET_BASE`]) +
    ///   `HUGR_WALLET_TOKEN` (REQUIRED) + `HUGR_STRIPE_REF` (default
    ///   [`DEFAULT_HUGR_STRIPE_REF`]).
    ///
    /// Both kebab-case (`wallet-broker`) and snake-case
    /// (`wallet_broker`) spellings of the mode are accepted to be
    /// friendly to deploy templating systems with different
    /// case-folding conventions.
    ///
    /// # Errors
    ///
    /// Returns [`StripeError::Authentication`] with a mode-specific
    /// message if:
    /// - `STRIPE_AUTH_MODE` is set to an unknown value, OR
    /// - the required credential for the selected mode is
    ///   missing/empty.
    pub fn from_env() -> Result<Self, StripeError> {
        let mode_raw = env::var("STRIPE_AUTH_MODE")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "direct".to_string());
        match mode_raw.as_str() {
            "direct" => Self::from_env_direct(),
            "wallet-broker" | "wallet_broker" => Self::from_env_wallet_broker(),
            other => Err(StripeError::Authentication(format!(
                "STRIPE_AUTH_MODE={other} is not recognised; expected 'direct' or 'wallet-broker'"
            ))),
        }
    }

    fn from_env_direct() -> Result<Self, StripeError> {
        let api_base = env::var("STRIPE_API_BASE")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_STRIPE_API_BASE.to_string());
        let api_key = env::var("STRIPE_SECRET_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                StripeError::Authentication(
                    "STRIPE_AUTH_MODE=direct requires STRIPE_SECRET_KEY (set $STRIPE_SECRET_KEY \
                     to your sk_live_… or sk_test_… key)"
                        .to_string(),
                )
            })?;
        Ok(Self::direct(api_base, SecretString::from(api_key)))
    }

    fn from_env_wallet_broker() -> Result<Self, StripeError> {
        let wallet_base = env::var("HUGR_WALLET_BASE")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_HUGR_WALLET_BASE.to_string());
        let stripe_ref = env::var("HUGR_STRIPE_REF")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_HUGR_STRIPE_REF.to_string());
        let wallet_token = env::var("HUGR_WALLET_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                StripeError::Authentication(
                    "STRIPE_AUTH_MODE=wallet-broker requires HUGR_WALLET_TOKEN (set \
                     $HUGR_WALLET_TOKEN to the hugrw_ proxy token)"
                        .to_string(),
                )
            })?;
        Ok(Self::wallet_broker(
            wallet_base,
            SecretString::from(wallet_token),
            stripe_ref,
        ))
    }

    /// Compute the effective base URL the next request will be POSTed
    /// under. Stripe paths (`/v1/...`) are appended by the HTTP layer.
    ///
    /// - `Direct` → `{api_base}` (e.g. `https://api.stripe.com`).
    /// - `WalletBroker` →
    ///   `{wallet_base}/_wallet/proxy/{stripe_ref}`.
    ///
    /// Trailing slashes on the base inputs are normalised.
    #[must_use]
    pub fn effective_base_url(&self) -> String {
        match &self.mode {
            StripeAuthMode::Direct { api_base, .. } => api_base.trim_end_matches('/').to_string(),
            StripeAuthMode::WalletBroker {
                wallet_base,
                stripe_ref,
                ..
            } => {
                let base = wallet_base.trim_end_matches('/');
                format!("{base}/_wallet/proxy/{stripe_ref}")
            }
        }
    }

    /// Return the bearer token the client uses for the
    /// `Authorization` header. Crate-internal — never leaks across
    /// the public API.
    fn bearer_token(&self) -> &SecretString {
        match &self.mode {
            StripeAuthMode::Direct { api_key, .. } => api_key,
            StripeAuthMode::WalletBroker { wallet_token, .. } => wallet_token,
        }
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
    ///   env resolution fails for the selected `STRIPE_AUTH_MODE`.
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
        let base_url = config.effective_base_url();
        Ok(StripeRealClient {
            config,
            base_url,
            retry_policy: self.retry_policy,
            http,
            clock: self.clock,
        })
    }
}

/// Production Stripe HTTPS client (dual-mode: direct or wallet-broker).
///
/// The `Debug` impl REDACTS every credential — secrets MUST never
/// appear in logs.
pub struct StripeRealClient {
    config: StripeClientConfig,
    /// Cached effective base URL — either `{api_base}` (Direct) or
    /// `{wallet_base}/_wallet/proxy/{stripe_ref}` (WalletBroker).
    /// Avoids recomputing per-request.
    base_url: String,
    retry_policy: RetryPolicy,
    http: reqwest::blocking::Client,
    // Wave-20: held for future timestamp-bearing operations (idempotency
    // key TTL eviction, retry-budget windows). Currently the production
    // HTTPS layer reads no wall-clock — Stripe owns idempotency-key
    // retention. Kept here as the canonical injection point so test
    // harnesses can pin time via `.with_clock(...)` once a clock-dependent
    // operation lands.
    #[allow(
        dead_code,
        reason = "wave-20 injection scaffold; consumed by future timestamp ops"
    )]
    clock: Arc<dyn Clock + Send + Sync>,
}

impl core::fmt::Debug for StripeRealClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StripeRealClient")
            .field("config", &self.config)
            .field("base_url", &self.base_url)
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

    /// Borrow the effective base URL — the value the next request
    /// will be POSTed under. Used by tests to pin the per-mode URL
    /// contract.
    #[must_use]
    pub fn effective_base_url(&self) -> &str {
        &self.base_url
    }

    /// Borrow the [`StripeClientConfig`] used to construct this client.
    #[must_use]
    pub fn config(&self) -> &StripeClientConfig {
        &self.config
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
        let url = format!("{}{}", self.base_url, path);
        let mut attempt: u32 = 0;
        loop {
            let req = self
                .http
                .post(&url)
                .bearer_auth(self.config.bearer_token().expose_secret())
                .header("Idempotency-Key", idempotency_key)
                .header("Stripe-Version", "2024-06-20")
                .form(form);
            let resp = match req.send() {
                Ok(r) => r,
                Err(e) => {
                    // Transport-level failure — fail-CLOSED retry up to
                    // budget. No silent fallback to the OTHER mode
                    // (would expose the upstream key through the
                    // broker, or vice versa).
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
        let url = format!("{}{}", self.base_url, path);
        let mut attempt: u32 = 0;
        loop {
            let req = self
                .http
                .get(&url)
                .bearer_auth(self.config.bearer_token().expose_secret())
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

    /// Build the form for GDPR/LGPD erasure pseudonymization of a Stripe
    /// customer. Extracted so the pseudonymize-only invariant is unit-testable
    /// WITHOUT a live Stripe round-trip. By construction this form NEVER
    /// carries a delete primitive — it only overwrites the PII fields
    /// (`email`/`name`) and clears the optional PII fields (`phone`/`address`),
    /// then stamps the `pii_redacted` + `erasure_dsr_id` metadata markers.
    fn pseudonymize_customer_form(
        pseudo_email: &str,
        pseudo_name: &str,
        erasure_dsr_id: &str,
    ) -> Vec<(&'static str, String)> {
        vec![
            ("email", pseudo_email.to_string()),
            ("name", pseudo_name.to_string()),
            // Clear the optional PII fields (Stripe interprets an empty value
            // as an unset). Belt-and-suspenders even if they were never set.
            ("phone", String::new()),
            ("address", String::new()),
            ("metadata[pii_redacted]", "true".to_string()),
            ("metadata[erasure_dsr_id]", erasure_dsr_id.to_string()),
        ]
    }

    /// `POST /v1/customers/:id` — pseudonymize a customer's PII for a DSR
    /// erasure (WI-S11-008 Stripe backend; `backends/stripe.rs`).
    ///
    /// Overwrites `email`/`name`, clears `phone`/`address`, and stamps the
    /// `pii_redacted` + `erasure_dsr_id` metadata. **NEVER calls
    /// `Customer.delete`** — deleting the customer object would break invoice
    /// integrity (GAAP ASC 606 + LGPD Art. 16 fiscal 5y retention). This crate
    /// deliberately exposes NO customer-delete primitive (WI AC-004 / §28
    /// R-004; ADR-S11-013).
    ///
    /// `idempotency_key` MUST be deterministic per `(dsr_id, customer)` so a
    /// retried erasure is a safe replay.
    ///
    /// # Errors
    /// See [`StripeError`] taxonomy.
    pub fn pseudonymize_customer(
        &self,
        id: &str,
        pseudo_email: &str,
        pseudo_name: &str,
        erasure_dsr_id: &str,
        idempotency_key: &str,
    ) -> Result<CustomerObject, StripeError> {
        let form = Self::pseudonymize_customer_form(pseudo_email, pseudo_name, erasure_dsr_id);
        self.post_form::<CustomerObject>(&format!("/v1/customers/{id}"), &form, idempotency_key)
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

    /// `POST /v1/billing_portal/sessions` — create a Customer Portal
    /// session.
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
        let idem = format!("checkout:{}:{}", req.tenant_id.as_str(), req.tier.as_str());
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
    let ty = parsed
        .as_ref()
        .and_then(|p| p.ty.clone())
        .unwrap_or_default();
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

    /// Process-wide env-var lock to serialise tests that mutate
    /// `STRIPE_AUTH_MODE` / `STRIPE_SECRET_KEY` / `HUGR_WALLET_*` /
    /// `STRIPE_API_BASE` / `HUGR_STRIPE_REF`. Cargo runs `#[test]`s
    /// concurrently per binary, and these process-global env vars
    /// race otherwise.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Capture + clear every env var these tests touch; restore on
    /// drop. Used as a scope guard inside each env-touching test.
    struct EnvGuard {
        saved: Vec<(&'static str, Option<String>)>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new(vars: &[&'static str]) -> Self {
            // .unwrap_or_else on poison so a previous test panic
            // doesn't take down the whole suite — we still want the
            // exclusion semantics. Tests are allowed to panic per
            // module-level allow.
            let lock = ENV_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let saved: Vec<_> = vars.iter().map(|k| (*k, env::var(*k).ok())).collect();
            for k in vars {
                env::remove_var(k);
            }
            Self { saved, _lock: lock }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in self.saved.drain(..) {
                match v {
                    Some(val) => env::set_var(k, val),
                    None => env::remove_var(k),
                }
            }
        }
    }

    const ENV_KEYS: &[&str] = &[
        "STRIPE_AUTH_MODE",
        "STRIPE_API_BASE",
        "STRIPE_SECRET_KEY",
        "HUGR_WALLET_BASE",
        "HUGR_WALLET_TOKEN",
        "HUGR_STRIPE_REF",
    ];

    fn direct_test_config() -> StripeClientConfig {
        StripeClientConfig::direct(
            "https://api.stripe.test",
            SecretString::from("sk_test_DUMMY".to_string()),
        )
    }

    fn wallet_test_config() -> StripeClientConfig {
        StripeClientConfig::wallet_broker(
            "https://wallet.test",
            SecretString::from("hugrw_test_token".to_string()),
            "stripe-prod",
        )
    }

    #[test]
    fn debug_redacts_in_both_modes() {
        // Direct.
        let cfg_direct = StripeClientConfig::direct(
            "https://api.stripe.test",
            SecretString::from("sk_test_DO_NOT_LOG_ME".to_string()),
        );
        let dbg_direct = format!("{cfg_direct:?}");
        assert!(
            dbg_direct.contains("<redacted>"),
            "Direct config Debug must redact: {dbg_direct}"
        );
        assert!(
            !dbg_direct.contains("sk_"),
            "Direct config Debug must NOT contain sk_ prefix: {dbg_direct}"
        );
        assert!(
            !dbg_direct.contains("DO_NOT_LOG_ME"),
            "Direct config Debug must NOT contain raw key: {dbg_direct}"
        );

        // WalletBroker.
        let cfg_wallet = StripeClientConfig::wallet_broker(
            "https://wallet.test",
            SecretString::from("hugrw_DO_NOT_LOG_ME".to_string()),
            "stripe-prod",
        );
        let dbg_wallet = format!("{cfg_wallet:?}");
        assert!(
            dbg_wallet.contains("<redacted>"),
            "Wallet config Debug must redact: {dbg_wallet}"
        );
        assert!(
            !dbg_wallet.contains("hugrw_"),
            "Wallet config Debug must NOT contain hugrw_ prefix: {dbg_wallet}"
        );
        assert!(
            !dbg_wallet.contains("DO_NOT_LOG_ME"),
            "Wallet config Debug must NOT contain raw token: {dbg_wallet}"
        );
    }

    #[test]
    fn debug_redacts_on_built_client_both_modes() {
        // Direct client.
        let c_direct = StripeRealClient::builder()
            .config(StripeClientConfig::direct(
                "https://api.stripe.test",
                SecretString::from("sk_test_SECRET".to_string()),
            ))
            .build()
            .unwrap();
        let dbg = format!("{c_direct:?}");
        assert!(dbg.contains("<redacted>"));
        assert!(
            !dbg.contains("sk_"),
            "Direct client Debug leaked sk_: {dbg}"
        );
        assert!(!dbg.contains("SECRET"));

        // Wallet client.
        let c_wallet = StripeRealClient::builder()
            .config(StripeClientConfig::wallet_broker(
                "https://wallet.test",
                SecretString::from("hugrw_SECRET".to_string()),
                "stripe-prod",
            ))
            .build()
            .unwrap();
        let dbg = format!("{c_wallet:?}");
        assert!(dbg.contains("<redacted>"));
        assert!(
            !dbg.contains("hugrw_"),
            "Wallet client Debug leaked hugrw_: {dbg}"
        );
        assert!(!dbg.contains("SECRET"));
    }

    #[test]
    fn builder_direct_yields_api_stripe_base() {
        let c = StripeRealClient::builder()
            .config(direct_test_config())
            .build()
            .unwrap();
        assert_eq!(c.effective_base_url(), "https://api.stripe.test");
    }

    #[test]
    fn builder_wallet_yields_wallet_proxy_base() {
        let c = StripeRealClient::builder()
            .config(wallet_test_config())
            .build()
            .unwrap();
        assert_eq!(
            c.effective_base_url(),
            "https://wallet.test/_wallet/proxy/stripe-prod"
        );
    }

    #[test]
    fn effective_base_url_trims_trailing_slash_direct() {
        let cfg = StripeClientConfig::direct(
            "https://api.stripe.test/",
            SecretString::from("sk_test_x".to_string()),
        );
        assert_eq!(cfg.effective_base_url(), "https://api.stripe.test");
    }

    #[test]
    fn effective_base_url_trims_trailing_slash_wallet() {
        let cfg = StripeClientConfig::wallet_broker(
            "https://wallet.test/",
            SecretString::from("hugrw_x".to_string()),
            "stripe-prod",
        );
        assert_eq!(
            cfg.effective_base_url(),
            "https://wallet.test/_wallet/proxy/stripe-prod"
        );
    }

    #[test]
    fn from_env_direct_default_when_unset() {
        let _g = EnvGuard::new(ENV_KEYS);
        // STRIPE_AUTH_MODE unset → defaults to "direct".
        env::set_var("STRIPE_SECRET_KEY", "sk_test_envdefault");
        let cfg = StripeClientConfig::from_env().unwrap();
        match cfg.mode {
            StripeAuthMode::Direct { ref api_base, .. } => {
                assert_eq!(api_base, DEFAULT_STRIPE_API_BASE);
            }
            other => panic!("expected Direct mode by default, got {other:?}"),
        }
    }

    #[test]
    fn from_env_direct_explicit() {
        let _g = EnvGuard::new(ENV_KEYS);
        env::set_var("STRIPE_AUTH_MODE", "direct");
        env::set_var("STRIPE_SECRET_KEY", "sk_test_explicit");
        env::set_var("STRIPE_API_BASE", "https://api.stripe.local");
        let cfg = StripeClientConfig::from_env().unwrap();
        match cfg.mode {
            StripeAuthMode::Direct {
                ref api_base,
                ref api_key,
            } => {
                assert_eq!(api_base, "https://api.stripe.local");
                assert_eq!(api_key.expose_secret(), "sk_test_explicit");
            }
            other => panic!("expected Direct mode, got {other:?}"),
        }
    }

    #[test]
    fn from_env_wallet_broker() {
        let _g = EnvGuard::new(ENV_KEYS);
        env::set_var("STRIPE_AUTH_MODE", "wallet-broker");
        env::set_var("HUGR_WALLET_TOKEN", "hugrw_envtest");
        env::set_var("HUGR_WALLET_BASE", "https://wallet.local");
        env::set_var("HUGR_STRIPE_REF", "stripe-staging");
        let cfg = StripeClientConfig::from_env().unwrap();
        match cfg.mode {
            StripeAuthMode::WalletBroker {
                ref wallet_base,
                ref wallet_token,
                ref stripe_ref,
            } => {
                assert_eq!(wallet_base, "https://wallet.local");
                assert_eq!(wallet_token.expose_secret(), "hugrw_envtest");
                assert_eq!(stripe_ref, "stripe-staging");
            }
            other => panic!("expected WalletBroker mode, got {other:?}"),
        }
    }

    #[test]
    fn from_env_wallet_broker_snake_case() {
        let _g = EnvGuard::new(ENV_KEYS);
        env::set_var("STRIPE_AUTH_MODE", "wallet_broker");
        env::set_var("HUGR_WALLET_TOKEN", "hugrw_snake");
        let cfg = StripeClientConfig::from_env().unwrap();
        match cfg.mode {
            StripeAuthMode::WalletBroker {
                ref wallet_base,
                ref stripe_ref,
                ..
            } => {
                assert_eq!(wallet_base, DEFAULT_HUGR_WALLET_BASE);
                assert_eq!(stripe_ref, DEFAULT_HUGR_STRIPE_REF);
            }
            other => panic!("expected WalletBroker mode (snake_case), got {other:?}"),
        }
    }

    #[test]
    fn pseudonymize_customer_form_redacts_pii_and_never_deletes() {
        let form = StripeRealClient::pseudonymize_customer_form(
            "erased+cus_x@redacted.invalid",
            "erased_ab12cd34",
            "0190a1b2-c3d4-7890-abcd-ef0123456789",
        );
        let get = |k: &str| form.iter().find(|(key, _)| *key == k).map(|(_, v)| v.clone());

        // PII fields overwritten with the pseudonyms.
        assert_eq!(get("email").as_deref(), Some("erased+cus_x@redacted.invalid"));
        assert_eq!(get("name").as_deref(), Some("erased_ab12cd34"));
        // Optional PII fields cleared (Stripe unsets on empty value).
        assert_eq!(get("phone").as_deref(), Some(""));
        assert_eq!(get("address").as_deref(), Some(""));
        // Redaction markers stamped.
        assert_eq!(get("metadata[pii_redacted]").as_deref(), Some("true"));
        assert_eq!(
            get("metadata[erasure_dsr_id]").as_deref(),
            Some("0190a1b2-c3d4-7890-abcd-ef0123456789")
        );
        // INVARIANT: pseudonymize-only — the form must carry NO delete
        // primitive (WI AC-004 / ADR-S11-013). A `Customer.delete` is a POST
        // to /v1/customers/:id/delete or a DELETE verb, never a form field —
        // but assert no key hints at deletion as a defensive tripwire.
        assert!(
            form.iter().all(|(k, _)| !k.contains("delete") && !k.contains("deleted")),
            "erasure form must never carry a delete primitive: {form:?}"
        );
    }

    #[test]
    fn from_env_unknown_mode_fails() {
        let _g = EnvGuard::new(ENV_KEYS);
        env::set_var("STRIPE_AUTH_MODE", "lol");
        let err = StripeClientConfig::from_env().unwrap_err();
        match err {
            StripeError::Authentication(msg) => {
                assert!(
                    msg.contains("STRIPE_AUTH_MODE")
                        && msg.contains("lol")
                        && msg.contains("direct")
                        && msg.contains("wallet-broker"),
                    "unknown-mode error must name the var + reject + valid modes: {msg}"
                );
            }
            other => panic!("expected Authentication for unknown mode, got {other:?}"),
        }
    }

    #[test]
    fn from_env_direct_missing_key_fails() {
        let _g = EnvGuard::new(ENV_KEYS);
        env::set_var("STRIPE_AUTH_MODE", "direct");
        // STRIPE_SECRET_KEY intentionally not set.
        let err = StripeClientConfig::from_env().unwrap_err();
        match err {
            StripeError::Authentication(msg) => {
                assert!(
                    msg.contains("STRIPE_SECRET_KEY"),
                    "direct-mode missing-key error must name STRIPE_SECRET_KEY: {msg}"
                );
                assert!(
                    msg.contains("direct"),
                    "direct-mode missing-key error must be mode-specific: {msg}"
                );
            }
            other => panic!("expected Authentication, got {other:?}"),
        }

        // Empty also fails (filter-empty semantics).
        env::set_var("STRIPE_SECRET_KEY", "");
        let err = StripeClientConfig::from_env().unwrap_err();
        assert!(matches!(err, StripeError::Authentication(_)));
    }

    #[test]
    fn from_env_wallet_broker_missing_token_fails() {
        let _g = EnvGuard::new(ENV_KEYS);
        env::set_var("STRIPE_AUTH_MODE", "wallet-broker");
        // HUGR_WALLET_TOKEN intentionally not set.
        let err = StripeClientConfig::from_env().unwrap_err();
        match err {
            StripeError::Authentication(msg) => {
                assert!(
                    msg.contains("HUGR_WALLET_TOKEN"),
                    "wallet-broker missing-token error must name HUGR_WALLET_TOKEN: {msg}"
                );
                assert!(
                    msg.contains("wallet-broker"),
                    "wallet-broker missing-token error must be mode-specific: {msg}"
                );
            }
            other => panic!("expected Authentication, got {other:?}"),
        }

        // Empty also fails.
        env::set_var("HUGR_WALLET_TOKEN", "");
        let err = StripeClientConfig::from_env().unwrap_err();
        assert!(matches!(err, StripeError::Authentication(_)));
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
