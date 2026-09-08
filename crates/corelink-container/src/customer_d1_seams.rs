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

    /// Atomically persist a `pat.created` customer-audit row together with
    /// the PAT mutation. The default keeps read-only test seams from gaining
    /// an arbitrary SQL batch surface.
    fn create_pat_with_audit(
        &self,
        _op: CustomerPatCreateOperation,
    ) -> Result<(), CustomerAtomicError> {
        Err(CustomerAtomicError::Unsupported(
            "atomic PAT create is not supported by this D1 seam".to_owned(),
        ))
    }

    /// Atomically persist a `team.invited` customer-audit row together with
    /// the invited-seat mutation.
    fn invite_team_member_with_audit(
        &self,
        _op: CustomerTeamInviteOperation,
    ) -> Result<(), CustomerAtomicError> {
        Err(CustomerAtomicError::Unsupported(
            "atomic team invite is not supported by this D1 seam".to_owned(),
        ))
    }
}

/// Typed values for the fixed PAT-create transaction.
#[derive(Debug, Clone)]
pub struct CustomerPatCreateOperation {
    /// Owning tenant id.
    pub tenant_id: String,
    /// Public PAT identifier.
    pub pat_id: String,
    /// Hashed PAT secret.
    pub pat_hash: String,
    /// Persisted base scope.
    pub scope: String,
    /// Expiry in Unix milliseconds.
    pub expires_ms: i64,
    /// Opaque shown-once marker.
    pub shown_once_token: String,
    /// Creation instant in Unix milliseconds.
    pub created_ms: i64,
    /// Embedded PAT token identifier.
    pub token_id: String,
    /// Customer display name.
    pub name: String,
    /// Whether this PAT is find-only.
    pub find_only: bool,
    /// Audit actor.
    pub audit_actor: String,
    /// Audit timestamp in Unix milliseconds.
    pub audit_ts_ms: i64,
    /// PII-safe customer audit detail.
    pub audit_detail: String,
}

/// Typed values for the fixed team-invite transaction.
#[derive(Debug, Clone)]
pub struct CustomerTeamInviteOperation {
    /// Owning tenant id.
    pub tenant_id: String,
    /// Invitation id placeholder until acceptance.
    pub invitation_id: String,
    /// Pseudonymized invitee email hash.
    pub email_hash: String,
    /// One-time invitation-token digest.
    pub invitation_token_hash: String,
    /// Canonical persisted role.
    pub role: String,
    /// Issuing principal.
    pub invited_by: String,
    /// Invitation instant in Unix milliseconds.
    pub invited_at_ms: i64,
    /// Audit actor.
    pub audit_actor: String,
    /// Audit timestamp in Unix milliseconds.
    pub audit_ts_ms: i64,
    /// PII-safe customer audit detail.
    pub audit_detail: String,
}

/// Failure stage for a typed customer transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomerAtomicError {
    /// The audit statement failed.
    Audit(String),
    /// The handler mutation failed and D1 rolled the audit statement back.
    Mutation(String),
    /// REST transport or response failure before a statement was identified.
    Transport(String),
    /// A non-production seam did not implement the operation.
    Unsupported(String),
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

    fn create_pat_with_audit(
        &self,
        op: CustomerPatCreateOperation,
    ) -> Result<(), CustomerAtomicError> {
        self.atomic_batch(vec![
            D1BatchStatement::new(
                "INSERT INTO customer_audit_events \
                 (tenant_id, event_type, actor, target, ts_ms, detail) \
                 VALUES (?1, 'pat.created', ?2, ?3, ?4, ?5)",
                vec![
                    json!(op.tenant_id.clone()),
                    json!(op.audit_actor),
                    json!(op.pat_id.clone()),
                    json!(op.audit_ts_ms),
                    json!(op.audit_detail),
                ],
            ),
            D1BatchStatement::new(
                "INSERT INTO pat \
                 (pat_id, tenant_id, pat_hash, scope, expires_ms, \
                  shown_once_token, shown_once_consumed, created_ms, token_id, name, find_only) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10)",
                vec![
                    json!(op.pat_id),
                    json!(op.tenant_id),
                    json!(op.pat_hash),
                    json!(op.scope),
                    json!(op.expires_ms),
                    json!(op.shown_once_token),
                    json!(op.created_ms),
                    json!(op.token_id),
                    json!(op.name),
                    json!(i32::from(op.find_only)),
                ],
            ),
        ])
    }

    fn invite_team_member_with_audit(
        &self,
        op: CustomerTeamInviteOperation,
    ) -> Result<(), CustomerAtomicError> {
        self.atomic_batch(vec![
            D1BatchStatement::new(
                "INSERT INTO customer_audit_events \
                 (tenant_id, event_type, actor, target, ts_ms, detail) \
                 VALUES (?1, 'team.invited', ?2, ?3, ?4, ?5)",
                vec![
                    json!(op.tenant_id.clone()),
                    json!(op.audit_actor),
                    json!(op.invitation_id.clone()),
                    json!(op.audit_ts_ms),
                    json!(op.audit_detail),
                ],
            ),
            D1BatchStatement::new(
                "INSERT INTO team_member \
                 (tenant_id, user_id, email_hash, invitation_token_hash, role, status, invited_by, invited_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'invited', ?6, ?7)",
                vec![
                    json!(op.tenant_id),
                    json!(op.invitation_id),
                    json!(op.email_hash),
                    json!(op.invitation_token_hash),
                    json!(op.role),
                    json!(op.invited_by),
                    json!(op.invited_at_ms),
                ],
            ),
        ])
    }
}

impl D1HttpCustomerDb {
    /// Execute one of the fixed customer mutation batches. This helper is
    /// deliberately not part of `CustomerD1`, preventing arbitrary SQL batch
    /// exposure to domain callers.
    fn atomic_batch(
        &self,
        statements: Vec<D1BatchStatement>,
    ) -> Result<(), CustomerAtomicError> {
        let d1 = Arc::clone(&self.d1);
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move { d1.batch(statements).await })
        })
        .map(|_| ())
        .map_err(|e| match e.statement {
            Some(0) => CustomerAtomicError::Audit(e.message),
            Some(1) => CustomerAtomicError::Mutation(e.message),
            _ => CustomerAtomicError::Transport(e.message),
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
