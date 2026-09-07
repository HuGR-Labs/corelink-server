// ─── Seams (sync trait surfaces over the async/blocking backends) ────────────

/// Sync row-source seam over D1. The production impl
/// ([`D1HttpCustomerDb`]) bridges to the async [`D1HttpClient`]; tests
/// supply a hermetic mock (mirrors `billing_d1_http`'s test layout).
pub trait CustomerD1: Send + Sync + core::fmt::Debug {
    /// Run one parameterised statement; return the result rows (empty
    /// for non-`RETURNING` writes). `Err(String)` is a transport /
    /// decode / D1-level failure (fail-CLOSED at the caller).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any D1 transport, HTTP, or decode
    /// failure.
    fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String>;
}

/// Production [`CustomerD1`] over the CF D1 REST API. Single documented
/// sync↔async bridge point (same pattern as
/// `billing_d1_http::D1HttpBillingWriter::run`).
#[non_exhaustive]
pub struct D1HttpCustomerDb {
    /// Shared D1-over-HTTP client (owns + redacts the CF API token).
    d1: Arc<D1HttpClient>,
}

impl D1HttpCustomerDb {
    /// Wire the row source over a shared [`D1HttpClient`].
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    /// Build the row source over a fresh D1-over-HTTP client from `StorageEnv`.
    /// `None` when the storage env is unset/invalid (dev/CI) — mirrors
    /// `D1CustomerHandler::from_env` and `routes::dsr::build_d1_worker`. Used by
    /// `routes::customer::account_deletion_from_env` to give the self-serve
    /// account-delete requester its own D1 query seam.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let storage_env = crate::storage::StorageEnv::from_env()?;
        let d1 = D1HttpClient::new(&storage_env).ok()?;
        Some(Self::new(Arc::new(d1)))
    }
}

impl core::fmt::Debug for D1HttpCustomerDb {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The client's own Debug is never surfaced here so a leaked
        // Debug can never expose the CF API token.
        f.debug_struct("D1HttpCustomerDb")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl CustomerD1 for D1HttpCustomerDb {
    fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
        let d1 = Arc::clone(&self.d1);
        // The native server is `#[tokio::main]` (multi-thread); we are
        // inside an async task (the axum handler), so `block_in_place`
        // hands the worker thread back to the scheduler while
        // `Handle::current().block_on` drives the D1 round-trip.
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move { d1.query(sql, &binds).await })
        })
    }
}

/// Stripe billing-portal seam. The production impl drives the blocking
/// [`corelink_stripe_real::StripeRealClient`]; tests supply a mock.
pub trait PortalSessions: Send + Sync + core::fmt::Debug {
    /// Create a short-lived billing-portal session for
    /// `stripe_customer_id`; return the hosted portal URL.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any Stripe transport / API failure.
    fn create(
        &self,
        stripe_customer_id: &str,
        return_url: &str,
        idempotency_key: &str,
    ) -> Result<String, String>;
}

/// Production [`PortalSessions`] over the real Stripe HTTPS client
/// (`reqwest::blocking`, driven under `block_in_place` — same bridge
/// rationale as [`D1HttpCustomerDb`]).
#[non_exhaustive]
pub struct StripePortalSessions {
    /// Real Stripe client (owns + redacts the bearer key).
    stripe: Arc<corelink_stripe_real::StripeRealClient>,
}

impl StripePortalSessions {
    /// Wire the portal creator over a shared Stripe client.
    #[must_use]
    pub fn new(stripe: Arc<corelink_stripe_real::StripeRealClient>) -> Self {
        Self { stripe }
    }
}

impl core::fmt::Debug for StripePortalSessions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StripePortalSessions")
            .field("stripe", &"[StripeRealClient]")
            .finish()
    }
}

impl PortalSessions for StripePortalSessions {
    fn create(
        &self,
        stripe_customer_id: &str,
        return_url: &str,
        idempotency_key: &str,
    ) -> Result<String, String> {
        let stripe = Arc::clone(&self.stripe);
        tokio::task::block_in_place(move || {
            stripe.create_billing_portal_session(stripe_customer_id, return_url, idempotency_key)
        })
        .map(|s| s.url)
        .map_err(|e| format!("stripe billing-portal session failed: {e}"))
    }
}

// ─── Production audit / SLI collaborators ────────────────────────────────────

/// Production [`AuditSink`]: structured `tracing` emit ingested by the
/// CF Logs pipeline (same posture as `routes/internal_pat.rs`'s
/// `PatMinted` audit emit — full D1 audit emit is a follow-up). The
/// tracing emit is infallible, so the fail-CLOSED `AuditFailed` arm is
/// exercised only by injected-failure tests; unlike `InMemoryAuditSink`
/// this sink is **bounded** (no per-request Vec growth in production).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct TracingCustomerAuditSink;

impl TracingCustomerAuditSink {
    /// Construct the canonical tracing-backed sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl AuditSink for TracingCustomerAuditSink {
    fn emit(&self, event: AuditEvent) -> Result<(), String> {
        tracing::info!(
            kind = event.kind.slug(),
            tenant = %event.tenant,
            principal = %event.principal,
            resource = %event.resource,
            at_unix_ms = event.at_unix_ms,
            "customer audit event"
        );
        Ok(())
    }
}

/// Production [`SliObserver`]: structured `tracing` emit (control-plane
/// availability observations; the prometheus wiring is a follow-up).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct TracingCustomerSliObserver;

impl TracingCustomerSliObserver {
    /// Construct the canonical tracing-backed observer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl SliObserver for TracingCustomerSliObserver {
    fn observe(&self, obs: SliObservation) {
        tracing::debug!(
            sli = ?obs.sli,
            is_error = obs.is_error,
            latency_us = obs.latency_us,
            "customer SLI observation"
        );
    }
}
