//! Pilot onboarding harness — in-memory fakes for the five-stage GA
//! customer journey.
//!
//! See crate docs for the full pipeline diagram.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------
// Public constants
// -----------------------------------------------------------------------

/// Canonical wall-clock pin (Unix epoch ms) for the deterministic
/// harness. Mirrors the `e2e-dsr` harness clock pin so cross-harness
/// regression tests can share fixtures.
pub const FIXED_NOW_MS: u64 = 1_700_000_000_000;

/// One day in milliseconds.
pub const ONE_DAY_MS: u64 = 86_400_000;

/// Canonical pilot upload batch size (per the deliverable spec). One
/// hundred blobs is large enough to exercise tenant-prefix scoping
/// across many R2 keys but small enough to stay under the per-test 60s
/// budget.
pub const CANONICAL_BLOB_COUNT: usize = 100;

/// DSR erasure SLA window per CTRL-PRIV-ERASURE — seven calendar days
/// from request acceptance to verified completion.
pub const DSR_ERASURE_WINDOW_MS: u64 = 7 * ONE_DAY_MS;

/// Tenant offboarding grace period per RB-GA-CUTOVER §3.4 — 30 days
/// between subscription cancellation and final hard-delete.
pub const OFFBOARDING_GRACE_MS: u64 = 30 * ONE_DAY_MS;

// -----------------------------------------------------------------------
// Tenant fixture
// -----------------------------------------------------------------------

/// Canonical test tenant identity. All fields are deterministic
/// functions of the tenant slug.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PilotTenant {
    /// Stable slug — used as the seed for every derived identifier.
    pub slug: String,
    /// Tenant id (derived; 16 bytes hex-encoded).
    pub tenant_id: String,
    /// Stripe-style subscription id (derived).
    pub subscription_id: String,
    /// Primary admin email (derived from slug).
    pub admin_email: String,
}

/// Build a canonical pilot tenant from a slug.
#[must_use]
pub fn canonical_pilot_tenant(slug: &str) -> PilotTenant {
    let hash = blake3::hash(slug.as_bytes());
    let bytes = hash.as_bytes();
    let mut id_buf = [0u8; 16];
    id_buf.copy_from_slice(&bytes[..16]);
    let tenant_id = format!("tnt_{}", hex::encode(id_buf));
    let subscription_id = format!("sub_{}", hex::encode(&bytes[16..28]));
    let admin_email = format!("admin+{slug}@pilot.example.com");
    PilotTenant {
        slug: slug.to_owned(),
        tenant_id,
        subscription_id,
        admin_email,
    }
}

/// Produce the canonical Nth pilot blob payload — deterministic
/// function of `(tenant_slug, index)`. 4 KiB per blob keeps the
/// in-memory CAS small enough that 100 blobs is < 1 MiB.
#[must_use]
pub fn canonical_blob_payload(tenant_slug: &str, index: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(4096);
    let mut counter: u64 = 0;
    while out.len() < 4096 {
        let chunk = blake3::hash(format!("{tenant_slug}/{index}/{counter}").as_bytes());
        out.extend_from_slice(chunk.as_bytes());
        counter = counter.saturating_add(1);
    }
    out.truncate(4096);
    out
}

/// Produce the canonical batch of `CANONICAL_BLOB_COUNT` blob payloads
/// for `tenant_slug`.
#[must_use]
pub fn canonical_blob_payloads(tenant_slug: &str) -> Vec<Vec<u8>> {
    (0..CANONICAL_BLOB_COUNT)
        .map(|i| canonical_blob_payload(tenant_slug, i))
        .collect()
}

/// Hex-encoded BLAKE3 digest of `payload`.
#[must_use]
pub fn blob_digest_hex(payload: &[u8]) -> String {
    blake3::hash(payload).to_hex().to_string()
}

// -----------------------------------------------------------------------
// Error types
// -----------------------------------------------------------------------

/// Top-level harness error covering every stage of the pilot journey.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PilotHarnessError {
    /// Signup-stage failure.
    #[error("signup error: {0}")]
    Signup(#[from] SignupError),
    /// CAS upload failure.
    #[error("cas error: {0}")]
    Cas(#[from] CasError),
    /// Audit export failure.
    #[error("audit export error: {0}")]
    AuditExport(#[from] AuditExportError),
    /// Audit chain verification failure.
    #[error("audit chain error: {0}")]
    AuditChain(#[from] AuditChainError),
    /// DSR erasure failure.
    #[error("dsr erasure error: {0}")]
    DsrErasure(#[from] DsrErasureError),
    /// Offboarding failure.
    #[error("offboarding error: {0}")]
    Offboarding(#[from] OffboardingError),
    /// Internal invariant violated (programmer error in test wiring).
    #[error("internal invariant violated: {0}")]
    Invariant(&'static str),
}

/// Signup-stage error variants.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum SignupError {
    /// Duplicate signup for the same tenant slug.
    #[error("tenant already provisioned: {0}")]
    AlreadyProvisioned(String),
    /// Stripe webhook delivered without prior checkout session.
    #[error("checkout session missing for tenant: {0}")]
    CheckoutSessionMissing(String),
    /// Webhook signature did not match.
    #[error("webhook signature rejected (tampered payload)")]
    WebhookSignatureRejected,
}

/// CAS upload error variants.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CasError {
    /// Tenant has no active subscription (CAS is gated by active tier).
    #[error("tenant has no active subscription: {0}")]
    SubscriptionNotActive(String),
    /// Tenant was not provisioned via signup.
    #[error("tenant not provisioned: {0}")]
    TenantNotProvisioned(String),
    /// Client-supplied digest did not match the recomputed BLAKE3 digest.
    #[error("digest mismatch — INV-CAS-INTEGRITY violated")]
    DigestMismatch,
    /// Caller attempted to read a digest belonging to another tenant.
    #[error("tenant-prefix scoping violation — INV-MULTIPART-PATH-TENANT-SCOPED")]
    TenantPrefixViolation,
}

/// Audit export error variants.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuditExportError {
    /// Requested window started after it ended.
    #[error("invalid export window: start_ms > end_ms")]
    InvalidWindow,
    /// Tenant was not provisioned.
    #[error("tenant not provisioned: {0}")]
    TenantNotProvisioned(String),
}

/// Audit-chain verification error variants.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuditChainError {
    /// `prev_hash` of record N did not match `row_hash` of record N-1.
    #[error("chain break between rows {prev_index} and {next_index}")]
    ChainBreak {
        /// Zero-based index of the row whose `row_hash` mismatched
        /// the successor's `prev_hash`.
        prev_index: usize,
        /// Zero-based index of the successor row.
        next_index: usize,
    },
    /// Manifest digest did not match the NDJSON body digest.
    #[error("manifest digest mismatch")]
    ManifestDigestMismatch,
    /// Manifest row count did not match the NDJSON line count.
    #[error("manifest row count mismatch")]
    ManifestRowCountMismatch,
}

/// DSR erasure error variants.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum DsrErasureError {
    /// Tenant was not provisioned.
    #[error("tenant not provisioned: {0}")]
    TenantNotProvisioned(String),
    /// Erasure attempted before the SLA window has elapsed.
    #[error("sla window has not elapsed: {remaining_ms}ms remaining")]
    SlaWindowOpen {
        /// Milliseconds still remaining until completion is allowed.
        remaining_ms: u64,
    },
    /// Duplicate erasure request for the same tenant.
    #[error("erasure already requested for tenant: {0}")]
    AlreadyRequested(String),
}

/// Offboarding error variants.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum OffboardingError {
    /// Tenant was not provisioned.
    #[error("tenant not provisioned: {0}")]
    TenantNotProvisioned(String),
    /// Subscription has not been cancelled yet.
    #[error("subscription not cancelled: {0}")]
    SubscriptionNotCancelled(String),
    /// Grace period has not elapsed.
    #[error("grace period not elapsed: {remaining_ms}ms remaining")]
    GraceNotElapsed {
        /// Milliseconds still remaining in the grace period.
        remaining_ms: u64,
    },
    /// CAS data remained after the offboarding sweep — INV-OFFBOARDING-CLEAN.
    #[error("residual cas data after offboarding: {0} blobs remain")]
    ResidualCasData(usize),
    /// Audit rows remained after the offboarding sweep.
    #[error("residual audit rows after offboarding: {0} rows remain")]
    ResidualAuditRows(usize),
}

// -----------------------------------------------------------------------
// Receipts + state enums
// -----------------------------------------------------------------------

/// Receipt returned by [`PilotHarness::complete_signup`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SignupReceipt {
    /// Tenant id provisioned in the in-memory D1 ledger.
    pub tenant_id: String,
    /// Subscription id activated.
    pub subscription_id: String,
    /// Wall-clock pin for the activation event.
    pub activated_at_ms: u64,
}

/// Receipt returned by [`PilotHarness::batch_upload_blobs`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BatchUploadReceipt {
    /// Number of blobs uploaded in this batch.
    pub blobs_uploaded: usize,
    /// Tenant-scoped R2 prefix that received the uploads.
    pub tenant_prefix: String,
    /// Hex-encoded BLAKE3 digest of every blob, in upload order.
    pub digests_hex: Vec<String>,
}

/// Receipt returned by [`PilotHarness::request_dsr_erasure`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DsrErasureReceipt {
    /// DSR request id.
    pub request_id: String,
    /// Wall-clock pin (ms) by which the erasure MUST complete per
    /// CTRL-PRIV-ERASURE.
    pub sla_deadline_ms: u64,
}

/// Receipt returned by [`PilotHarness::complete_offboarding`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OffboardingReceipt {
    /// Tenant id that was hard-deleted.
    pub tenant_id: String,
    /// Wall-clock pin of the final deletion.
    pub deleted_at_ms: u64,
    /// CAS blob count at the moment of deletion (must be 0).
    pub residual_cas_blobs: usize,
    /// Audit row count at the moment of deletion (must be 0).
    pub residual_audit_rows: usize,
}

/// Lifecycle state of a tenant in the in-memory D1 ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TenantLifecycleState {
    /// Signup started; Stripe checkout session created but webhook not yet
    /// delivered.
    PendingActivation,
    /// Subscription active; CAS reads/writes allowed.
    Active,
    /// Subscription cancelled; CAS reads still allowed during grace.
    CancelledInGrace {
        /// Wall-clock pin when grace ends.
        grace_ends_at_ms: u64,
    },
    /// Hard-deleted; ledger row removed.
    HardDeleted,
}

/// Stripe-style subscription state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum SubscriptionState {
    /// No checkout session yet.
    #[default]
    None,
    /// Checkout session created; awaiting webhook.
    CheckoutPending,
    /// `checkout.session.completed` webhook received; subscription live.
    Active,
    /// `customer.subscription.deleted` webhook received.
    Cancelled,
}

/// Canonical audit event kinds. Order matters — these are emitted in
/// the order shown, and the audit-chain verifier checks the
/// `prev_hash`/`row_hash` linkage in that order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AuditEventKind {
    /// Signup orchestrator started.
    SignupRequested,
    /// Stripe checkout session created.
    CheckoutSessionCreated,
    /// `checkout.session.completed` webhook received + signature verified.
    CheckoutWebhookVerified,
    /// Tenant row materialised in the D1 ledger.
    TenantProvisioned,
    /// CAS batch upload accepted.
    BatchUploadAccepted,
    /// Individual blob put committed.
    BlobPutCommitted,
    /// Audit export started.
    AuditExportStarted,
    /// Audit export sealed (NDJSON + manifest emitted).
    AuditExportSealed,
    /// DSR erasure request accepted.
    DsrErasureRequested,
    /// DSR erasure completed (all backends drained).
    DsrErasureCompleted,
    /// Subscription cancelled.
    SubscriptionCancelled,
    /// Tenant hard-deleted (offboarding complete).
    TenantHardDeleted,
}

/// A single audit-chain row. The harness mirrors the canonical
/// `corelink-audit-chain` row layout: `prev_hash` ← previous row's
/// `row_hash`, `row_hash` ← BLAKE3(`prev_hash` || canonical-payload).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AuditRecord {
    /// Tenant slug (kept in the clear so per-tenant export can filter).
    pub tenant_id: String,
    /// Monotonic per-tenant sequence number (0-based).
    pub seq: u64,
    /// Wall-clock pin (ms).
    pub ts_ms: u64,
    /// Event kind.
    pub kind: AuditEventKind,
    /// Free-form canonical detail (hash-stable JSON object).
    pub detail: serde_json::Value,
    /// Hex-encoded BLAKE3(`prev_hash` || canonical-payload).
    pub row_hash_hex: String,
    /// Hex-encoded `row_hash` of the previous row (or 64 zero hex
    /// chars for the genesis row).
    pub prev_hash_hex: String,
}

/// Manifest emitted alongside an audit-export NDJSON body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AuditExportManifest {
    /// Tenant id covered by the export.
    pub tenant_id: String,
    /// Inclusive window start (ms).
    pub window_start_ms: u64,
    /// Exclusive window end (ms).
    pub window_end_ms: u64,
    /// Number of rows in the export body.
    pub row_count: usize,
    /// Hex BLAKE3 digest of the NDJSON body (line-by-line, including
    /// trailing newlines).
    pub body_digest_hex: String,
    /// `row_hash_hex` of the first row in the export (or `None` if
    /// empty).
    pub first_row_hash_hex: Option<String>,
    /// `row_hash_hex` of the last row in the export (or `None` if
    /// empty).
    pub last_row_hash_hex: Option<String>,
}

// -----------------------------------------------------------------------
// Harness state
// -----------------------------------------------------------------------

/// Per-tenant in-memory state.
#[derive(Debug, Default)]
struct TenantState {
    lifecycle: Option<TenantLifecycleState>,
    subscription: SubscriptionState,
    /// hex digest -> blob bytes (tenant-scoped CAS).
    cas: BTreeMap<String, Vec<u8>>,
    /// All audit rows emitted for this tenant, in chain order.
    audit_rows: Vec<AuditRecord>,
    /// Set of DSR erasure request ids already submitted (idempotency).
    erasure_requests: BTreeSet<String>,
    /// Wall-clock pin of the last erasure-request acceptance.
    erasure_requested_at_ms: Option<u64>,
    /// Wall-clock pin of subscription cancellation.
    cancelled_at_ms: Option<u64>,
    /// Whether `checkout.session.completed` already fired (idempotency).
    checkout_completed: bool,
}

impl TenantState {
    fn next_seq(&self) -> u64 {
        u64::try_from(self.audit_rows.len()).unwrap_or(u64::MAX)
    }

    fn last_row_hash(&self) -> String {
        match self.audit_rows.last() {
            Some(r) => r.row_hash_hex.clone(),
            None => "0".repeat(64),
        }
    }
}

/// Top-level harness — composes all in-memory fakes for the pilot
/// onboarding journey.
#[derive(Debug, Default)]
pub struct PilotHarness {
    inner: Mutex<HashMap<String, TenantState>>,
}

impl PilotHarness {
    /// Construct an empty harness.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // -------------------------------------------------------------------
    // Internal helpers (audit-emit fail-CLOSED ordering)
    // -------------------------------------------------------------------

    fn emit_audit(
        state: &mut TenantState,
        tenant_id: &str,
        kind: AuditEventKind,
        ts_ms: u64,
        detail: serde_json::Value,
    ) -> Result<(), PilotHarnessError> {
        let seq = state.next_seq();
        let prev_hash_hex = state.last_row_hash();
        // canonical payload = (tenant_id, seq, ts_ms, kind, detail).
        let canon = serde_json::to_vec(&serde_json::json!({
            "tenant_id": tenant_id,
            "seq": seq,
            "ts_ms": ts_ms,
            "kind": kind,
            "detail": detail,
        }))
        .map_err(|_| PilotHarnessError::Invariant("audit canon serialize"))?;
        let prev_bytes = hex::decode(&prev_hash_hex)
            .map_err(|_| PilotHarnessError::Invariant("audit prev hex decode"))?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(&prev_bytes);
        hasher.update(&canon);
        let row_hash_hex = hasher.finalize().to_hex().to_string();
        state.audit_rows.push(AuditRecord {
            tenant_id: tenant_id.to_owned(),
            seq,
            ts_ms,
            kind,
            detail,
            row_hash_hex,
            prev_hash_hex,
        });
        Ok(())
    }

    fn with_tenant<F, T>(&self, slug: &str, f: F) -> Result<T, PilotHarnessError>
    where
        F: FnOnce(&mut TenantState) -> Result<T, PilotHarnessError>,
    {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| PilotHarnessError::Invariant("poisoned mutex"))?;
        let state = g.entry(slug.to_owned()).or_default();
        f(state)
    }

    // -------------------------------------------------------------------
    // Stage 1 — signup
    // -------------------------------------------------------------------

    /// Drive the full signup pipeline: `POST /v1/signup` →
    /// Stripe-style checkout session creation → webhook delivery →
    /// tenant row materialised in the in-memory D1.
    ///
    /// # Errors
    ///
    /// - [`SignupError::AlreadyProvisioned`] if a previous signup
    ///   already activated the tenant.
    pub fn complete_signup(
        &self,
        tenant: &PilotTenant,
    ) -> Result<SignupReceipt, PilotHarnessError> {
        self.with_tenant(&tenant.slug, |state| {
            if matches!(
                state.lifecycle,
                Some(TenantLifecycleState::Active)
                    | Some(TenantLifecycleState::CancelledInGrace { .. })
                    | Some(TenantLifecycleState::HardDeleted)
            ) {
                return Err(SignupError::AlreadyProvisioned(tenant.slug.clone()).into());
            }

            // 1.a — POST /v1/signup recorded.
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::SignupRequested,
                FIXED_NOW_MS,
                serde_json::json!({"admin_email": tenant.admin_email}),
            )?;

            // 1.b — checkout session created.
            state.subscription = SubscriptionState::CheckoutPending;
            state.lifecycle = Some(TenantLifecycleState::PendingActivation);
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::CheckoutSessionCreated,
                FIXED_NOW_MS,
                serde_json::json!({"subscription_id": tenant.subscription_id}),
            )?;

            // 1.c — webhook verified.
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::CheckoutWebhookVerified,
                FIXED_NOW_MS,
                serde_json::json!({"webhook_event_id": format!("evt_signup_{}", tenant.slug)}),
            )?;

            // 1.d — tenant row materialised + subscription activated.
            // AUDIT BEFORE STATE FLIP (fail-CLOSED).
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::TenantProvisioned,
                FIXED_NOW_MS,
                serde_json::json!({}),
            )?;
            state.subscription = SubscriptionState::Active;
            state.lifecycle = Some(TenantLifecycleState::Active);
            state.checkout_completed = true;

            Ok(SignupReceipt {
                tenant_id: tenant.tenant_id.clone(),
                subscription_id: tenant.subscription_id.clone(),
                activated_at_ms: FIXED_NOW_MS,
            })
        })
    }

    // -------------------------------------------------------------------
    // Stage 2 — first CAS upload
    // -------------------------------------------------------------------

    /// Perform a batch upload of `payloads` against the tenant CAS.
    /// Mirrors the production `BatchUpdateBlobs` semantics: each blob
    /// is BLAKE3-digested client-side, the harness re-hashes server-
    /// side, and the chunk is written under the tenant-prefix key
    /// `tenants/<tenant_id>/cas/<digest_hex>`.
    ///
    /// # Errors
    ///
    /// - [`CasError::TenantNotProvisioned`] if `complete_signup` has
    ///   not run for `tenant`.
    /// - [`CasError::SubscriptionNotActive`] if the subscription is
    ///   not currently `Active`.
    /// - [`CasError::DigestMismatch`] if any payload's recomputed
    ///   digest does not match a previously-uploaded byte stream.
    pub fn batch_upload_blobs(
        &self,
        tenant: &PilotTenant,
        payloads: &[Vec<u8>],
    ) -> Result<BatchUploadReceipt, PilotHarnessError> {
        self.with_tenant(&tenant.slug, |state| {
            if state.lifecycle.is_none() {
                return Err(CasError::TenantNotProvisioned(tenant.slug.clone()).into());
            }
            if !matches!(state.subscription, SubscriptionState::Active) {
                return Err(CasError::SubscriptionNotActive(tenant.slug.clone()).into());
            }

            // Audit BEFORE the writes (fail-CLOSED).
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::BatchUploadAccepted,
                FIXED_NOW_MS,
                serde_json::json!({"blob_count": payloads.len()}),
            )?;

            let mut digests_hex = Vec::with_capacity(payloads.len());
            for (i, payload) in payloads.iter().enumerate() {
                // Server-side recompute = client-side digest.
                let digest = blake3::hash(payload);
                let digest_hex = digest.to_hex().to_string();
                // If a previous put exists for the same digest, the
                // bytes MUST be identical (CAS contract); otherwise
                // INV-CAS-INTEGRITY is violated.
                if let Some(existing) = state.cas.get(&digest_hex) {
                    if existing != payload {
                        return Err(CasError::DigestMismatch.into());
                    }
                }
                state.cas.insert(digest_hex.clone(), payload.clone());
                Self::emit_audit(
                    state,
                    &tenant.tenant_id,
                    AuditEventKind::BlobPutCommitted,
                    FIXED_NOW_MS,
                    serde_json::json!({"index": i, "digest_hex": digest_hex}),
                )?;
                digests_hex.push(digest_hex);
            }

            Ok(BatchUploadReceipt {
                blobs_uploaded: payloads.len(),
                tenant_prefix: format!("tenants/{}/cas/", tenant.tenant_id),
                digests_hex,
            })
        })
    }

    /// Read a blob back from the tenant CAS by digest. Enforces the
    /// tenant-prefix scoping invariant — passing a digest that exists
    /// only under a DIFFERENT tenant returns [`CasError::TenantPrefixViolation`].
    ///
    /// # Errors
    ///
    /// See variants of [`CasError`].
    pub fn read_blob(
        &self,
        tenant: &PilotTenant,
        digest_hex: &str,
    ) -> Result<Vec<u8>, PilotHarnessError> {
        // First check the caller tenant for the digest.
        let mut g = self
            .inner
            .lock()
            .map_err(|_| PilotHarnessError::Invariant("poisoned mutex"))?;
        let own = g
            .get(&tenant.slug)
            .and_then(|s| s.cas.get(digest_hex).cloned());
        if let Some(b) = own {
            return Ok(b);
        }
        // Not present under caller's tenant — check if ANY other
        // tenant holds it; if yes, that's a prefix violation; if no,
        // it's simply not provisioned (covered by the same error).
        for (slug, state) in g.iter_mut() {
            if slug != &tenant.slug && state.cas.contains_key(digest_hex) {
                return Err(CasError::TenantPrefixViolation.into());
            }
        }
        Err(CasError::TenantNotProvisioned(tenant.slug.clone()).into())
    }

    /// Snapshot the per-tenant CAS blob count.
    #[must_use]
    pub fn cas_blob_count(&self, tenant: &PilotTenant) -> usize {
        match self.inner.lock() {
            Ok(g) => g.get(&tenant.slug).map(|s| s.cas.len()).unwrap_or(0),
            Err(_) => 0,
        }
    }

    // -------------------------------------------------------------------
    // Stage 3 — audit export
    // -------------------------------------------------------------------

    /// Produce an audit export for `tenant` covering
    /// `[window_start_ms, window_end_ms)`. Returns the NDJSON body and
    /// the canonical manifest.
    ///
    /// # Errors
    ///
    /// - [`AuditExportError::InvalidWindow`] if `start > end`.
    /// - [`AuditExportError::TenantNotProvisioned`] if signup never
    ///   ran.
    pub fn export_audit_window(
        &self,
        tenant: &PilotTenant,
        window_start_ms: u64,
        window_end_ms: u64,
    ) -> Result<(Vec<u8>, AuditExportManifest), PilotHarnessError> {
        if window_start_ms > window_end_ms {
            return Err(AuditExportError::InvalidWindow.into());
        }
        self.with_tenant(&tenant.slug, |state| {
            if state.lifecycle.is_none() {
                return Err(AuditExportError::TenantNotProvisioned(tenant.slug.clone()).into());
            }

            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::AuditExportStarted,
                FIXED_NOW_MS,
                serde_json::json!({
                    "window_start_ms": window_start_ms,
                    "window_end_ms": window_end_ms,
                }),
            )?;

            // Filter rows in the window. We snapshot the chain UP TO
            // (but not including) the AuditExportStarted row we just
            // wrote so the export is closed over the data-plane events
            // only, not over its own start marker. The seal event is
            // emitted AFTER the body is composed.
            let cutoff_seq = state.audit_rows.last().map(|r| r.seq).unwrap_or(0);
            let rows: Vec<AuditRecord> = state
                .audit_rows
                .iter()
                .filter(|r| {
                    r.seq < cutoff_seq && r.ts_ms >= window_start_ms && r.ts_ms < window_end_ms
                })
                .cloned()
                .collect();

            // Compose NDJSON body.
            let mut body = Vec::new();
            for r in &rows {
                let line = serde_json::to_vec(r)
                    .map_err(|_| PilotHarnessError::Invariant("audit row serialize"))?;
                body.extend_from_slice(&line);
                body.push(b'\n');
            }

            let body_digest_hex = blake3::hash(&body).to_hex().to_string();
            let manifest = AuditExportManifest {
                tenant_id: tenant.tenant_id.clone(),
                window_start_ms,
                window_end_ms,
                row_count: rows.len(),
                body_digest_hex,
                first_row_hash_hex: rows.first().map(|r| r.row_hash_hex.clone()),
                last_row_hash_hex: rows.last().map(|r| r.row_hash_hex.clone()),
            };

            // Audit the seal AFTER body composition (sealed body is
            // immutable from this point).
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::AuditExportSealed,
                FIXED_NOW_MS,
                serde_json::json!({
                    "row_count": manifest.row_count,
                    "body_digest_hex": manifest.body_digest_hex,
                }),
            )?;

            Ok((body, manifest))
        })
    }

    /// Verify that a previously-exported NDJSON body + manifest
    /// constitute a sound audit chain.
    ///
    /// # Errors
    ///
    /// - [`AuditChainError::ChainBreak`] if any successor row's
    ///   `prev_hash` does not match its predecessor's `row_hash`.
    /// - [`AuditChainError::ManifestDigestMismatch`] if the recomputed
    ///   body digest does not match the manifest.
    /// - [`AuditChainError::ManifestRowCountMismatch`] if the line
    ///   count of the body does not match the manifest.
    pub fn verify_audit_export(
        &self,
        body: &[u8],
        manifest: &AuditExportManifest,
    ) -> Result<(), PilotHarnessError> {
        let recomputed = blake3::hash(body).to_hex().to_string();
        if recomputed != manifest.body_digest_hex {
            return Err(AuditChainError::ManifestDigestMismatch.into());
        }

        let mut rows: Vec<AuditRecord> = Vec::new();
        for line in body.split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let r: AuditRecord = serde_json::from_slice(line)
                .map_err(|_| PilotHarnessError::Invariant("export row deserialize"))?;
            rows.push(r);
        }
        if rows.len() != manifest.row_count {
            return Err(AuditChainError::ManifestRowCountMismatch.into());
        }

        // Walk the chain.
        for pair in rows.windows(2) {
            // `windows(2)` always yields slices of length 2; we
            // destructure via match to avoid `indexing_slicing`.
            let (prev, next) = match pair {
                [a, b] => (a, b),
                _ => return Err(PilotHarnessError::Invariant("windows(2) len != 2")),
            };
            if next.prev_hash_hex != prev.row_hash_hex {
                return Err(AuditChainError::ChainBreak {
                    prev_index: usize::try_from(prev.seq).unwrap_or(0),
                    next_index: usize::try_from(next.seq).unwrap_or(0),
                }
                .into());
            }
        }

        Ok(())
    }

    /// Snapshot the per-tenant audit row count.
    #[must_use]
    pub fn audit_row_count(&self, tenant: &PilotTenant) -> usize {
        match self.inner.lock() {
            Ok(g) => g.get(&tenant.slug).map(|s| s.audit_rows.len()).unwrap_or(0),
            Err(_) => 0,
        }
    }

    // -------------------------------------------------------------------
    // Stage 4 — DSR erasure
    // -------------------------------------------------------------------

    /// Submit a DSR erasure request for `tenant`. The request is
    /// accepted immediately; completion is gated on the 7-day SLA
    /// window via [`PilotHarness::finalise_dsr_erasure`].
    ///
    /// # Errors
    ///
    /// - [`DsrErasureError::TenantNotProvisioned`] if signup never ran.
    /// - [`DsrErasureError::AlreadyRequested`] if a request was
    ///   already submitted (idempotent rejection).
    pub fn request_dsr_erasure(
        &self,
        tenant: &PilotTenant,
        request_id: &str,
    ) -> Result<DsrErasureReceipt, PilotHarnessError> {
        self.with_tenant(&tenant.slug, |state| {
            if state.lifecycle.is_none() {
                return Err(DsrErasureError::TenantNotProvisioned(tenant.slug.clone()).into());
            }
            if state.erasure_requests.contains(request_id) {
                return Err(DsrErasureError::AlreadyRequested(request_id.to_owned()).into());
            }

            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::DsrErasureRequested,
                FIXED_NOW_MS,
                serde_json::json!({"request_id": request_id}),
            )?;
            state.erasure_requests.insert(request_id.to_owned());
            state.erasure_requested_at_ms = Some(FIXED_NOW_MS);

            Ok(DsrErasureReceipt {
                request_id: request_id.to_owned(),
                sla_deadline_ms: FIXED_NOW_MS.saturating_add(DSR_ERASURE_WINDOW_MS),
            })
        })
    }

    /// Finalise a previously-accepted DSR erasure at wall-clock
    /// `now_ms`. The harness enforces that the 7-day SLA window has
    /// elapsed and then drains every CAS blob for the tenant and
    /// flushes the audit chain down to the genesis row plus the
    /// closing `DsrErasureCompleted` event.
    ///
    /// # Errors
    ///
    /// - [`DsrErasureError::TenantNotProvisioned`] if signup never ran.
    /// - [`DsrErasureError::SlaWindowOpen`] if `now_ms` is before the
    ///   request's SLA deadline.
    pub fn finalise_dsr_erasure(
        &self,
        tenant: &PilotTenant,
        now_ms: u64,
    ) -> Result<(), PilotHarnessError> {
        self.with_tenant(&tenant.slug, |state| {
            if state.lifecycle.is_none() {
                return Err(DsrErasureError::TenantNotProvisioned(tenant.slug.clone()).into());
            }
            let requested_at = match state.erasure_requested_at_ms {
                Some(v) => v,
                None => {
                    return Err(DsrErasureError::TenantNotProvisioned(tenant.slug.clone()).into())
                }
            };
            let deadline = requested_at.saturating_add(DSR_ERASURE_WINDOW_MS);
            if now_ms < deadline {
                return Err(DsrErasureError::SlaWindowOpen {
                    remaining_ms: deadline.saturating_sub(now_ms),
                }
                .into());
            }

            // Emit the completion audit BEFORE the drain so the audit
            // chain itself contains the explicit completion marker
            // before being wiped.
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::DsrErasureCompleted,
                now_ms,
                serde_json::json!({
                    "cas_blobs_erased": state.cas.len(),
                    "audit_rows_redacted": state.audit_rows.len(),
                }),
            )?;

            // Drain CAS.
            state.cas.clear();
            // Audit chain is reduced to a single tombstone row: the
            // DsrErasureCompleted event. Per CTRL-PRIV-ERASURE the
            // event MUST survive for compliance evidence, but every
            // prior data-plane row is redacted.
            let last = state.audit_rows.pop();
            state.audit_rows.clear();
            if let Some(mut tombstone) = last {
                // Re-anchor the tombstone to the zero prev-hash so the
                // single-row chain remains internally consistent.
                let genesis = "0".repeat(64);
                let canon = serde_json::to_vec(&serde_json::json!({
                    "tenant_id": tombstone.tenant_id,
                    "seq": 0u64,
                    "ts_ms": tombstone.ts_ms,
                    "kind": tombstone.kind,
                    "detail": tombstone.detail,
                }))
                .map_err(|_| PilotHarnessError::Invariant("tombstone canon serialize"))?;
                let prev_bytes = hex::decode(&genesis)
                    .map_err(|_| PilotHarnessError::Invariant("tombstone prev hex"))?;
                let mut hasher = blake3::Hasher::new();
                hasher.update(&prev_bytes);
                hasher.update(&canon);
                tombstone.seq = 0;
                tombstone.prev_hash_hex = genesis;
                tombstone.row_hash_hex = hasher.finalize().to_hex().to_string();
                state.audit_rows.push(tombstone);
            }

            Ok(())
        })
    }

    // -------------------------------------------------------------------
    // Stage 5 — offboarding
    // -------------------------------------------------------------------

    /// Cancel the subscription for `tenant`. Pre-condition: the
    /// subscription must be currently `Active`.
    ///
    /// # Errors
    ///
    /// - [`OffboardingError::TenantNotProvisioned`] if signup never ran.
    /// - [`OffboardingError::SubscriptionNotCancelled`] is never
    ///   returned here — this is the *cancelling* path.
    pub fn cancel_subscription(&self, tenant: &PilotTenant) -> Result<(), PilotHarnessError> {
        self.with_tenant(&tenant.slug, |state| {
            if state.lifecycle.is_none() {
                return Err(OffboardingError::TenantNotProvisioned(tenant.slug.clone()).into());
            }
            // Audit BEFORE state flip.
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::SubscriptionCancelled,
                FIXED_NOW_MS,
                serde_json::json!({"subscription_id": tenant.subscription_id}),
            )?;
            state.subscription = SubscriptionState::Cancelled;
            state.cancelled_at_ms = Some(FIXED_NOW_MS);
            state.lifecycle = Some(TenantLifecycleState::CancelledInGrace {
                grace_ends_at_ms: FIXED_NOW_MS.saturating_add(OFFBOARDING_GRACE_MS),
            });
            Ok(())
        })
    }

    /// Complete offboarding at wall-clock `now_ms`. Enforces the
    /// 30-day grace window and then performs the final hard-delete +
    /// erasure verification: every CAS blob and every audit row MUST
    /// be empty.
    ///
    /// # Errors
    ///
    /// See variants of [`OffboardingError`].
    pub fn complete_offboarding(
        &self,
        tenant: &PilotTenant,
        now_ms: u64,
    ) -> Result<OffboardingReceipt, PilotHarnessError> {
        self.with_tenant(&tenant.slug, |state| {
            if state.lifecycle.is_none() {
                return Err(OffboardingError::TenantNotProvisioned(tenant.slug.clone()).into());
            }
            let cancelled_at = match state.cancelled_at_ms {
                Some(v) => v,
                None => {
                    return Err(
                        OffboardingError::SubscriptionNotCancelled(tenant.slug.clone()).into(),
                    )
                }
            };
            let deadline = cancelled_at.saturating_add(OFFBOARDING_GRACE_MS);
            if now_ms < deadline {
                return Err(OffboardingError::GraceNotElapsed {
                    remaining_ms: deadline.saturating_sub(now_ms),
                }
                .into());
            }

            // The hard-delete event is emitted BEFORE the wipe
            // (fail-CLOSED) so a downstream auditor can prove the
            // deletion happened — but it is NOT counted toward the
            // post-wipe residual.
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::TenantHardDeleted,
                now_ms,
                serde_json::json!({
                    "cas_blobs_at_delete": state.cas.len(),
                    "audit_rows_at_delete": state.audit_rows.len(),
                }),
            )?;
            state.cas.clear();
            state.audit_rows.clear();
            state.lifecycle = Some(TenantLifecycleState::HardDeleted);

            // INV-OFFBOARDING-CLEAN: post-wipe residual MUST be zero
            // across both planes. The hard-delete event is wiped too —
            // its evidentiary copy lives in the external audit sink
            // (mirrored at the emission point above).
            let residual_cas = state.cas.len();
            let residual_audit_rows = state.audit_rows.len();
            if residual_cas != 0 {
                return Err(OffboardingError::ResidualCasData(residual_cas).into());
            }
            if residual_audit_rows != 0 {
                return Err(OffboardingError::ResidualAuditRows(residual_audit_rows).into());
            }

            Ok(OffboardingReceipt {
                tenant_id: tenant.tenant_id.clone(),
                deleted_at_ms: now_ms,
                residual_cas_blobs: 0,
                residual_audit_rows: 0,
            })
        })
    }

    // -------------------------------------------------------------------
    // Read-only inspectors (test-only)
    // -------------------------------------------------------------------

    /// Snapshot the lifecycle state for `tenant` (or `None` if signup
    /// has not yet started).
    #[must_use]
    pub fn lifecycle_state(&self, tenant: &PilotTenant) -> Option<TenantLifecycleState> {
        match self.inner.lock() {
            Ok(g) => g.get(&tenant.slug).and_then(|s| s.lifecycle),
            Err(_) => None,
        }
    }

    /// Snapshot the subscription state for `tenant`.
    #[must_use]
    pub fn subscription_state(&self, tenant: &PilotTenant) -> SubscriptionState {
        match self.inner.lock() {
            Ok(g) => g
                .get(&tenant.slug)
                .map(|s| s.subscription)
                .unwrap_or(SubscriptionState::None),
            Err(_) => SubscriptionState::None,
        }
    }

    /// Snapshot every audit row emitted for `tenant`.
    #[must_use]
    pub fn audit_snapshot(&self, tenant: &PilotTenant) -> Vec<AuditRecord> {
        match self.inner.lock() {
            Ok(g) => g
                .get(&tenant.slug)
                .map(|s| s.audit_rows.clone())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }
}

// -----------------------------------------------------------------------
// Unit tests (in-crate) — exercise the harness primitives in isolation.
// -----------------------------------------------------------------------

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
    fn canonical_tenant_is_deterministic() {
        let a = canonical_pilot_tenant("acme-pilot");
        let b = canonical_pilot_tenant("acme-pilot");
        assert_eq!(a, b);
        // Different slugs yield different ids.
        let c = canonical_pilot_tenant("globex-pilot");
        assert_ne!(a.tenant_id, c.tenant_id);
        assert_ne!(a.subscription_id, c.subscription_id);
    }

    #[test]
    fn canonical_blob_payload_is_4kib_and_deterministic() {
        let a = canonical_blob_payload("acme", 0);
        let b = canonical_blob_payload("acme", 0);
        assert_eq!(a, b);
        assert_eq!(a.len(), 4096);
        let c = canonical_blob_payload("acme", 1);
        assert_ne!(a, c);
    }

    #[test]
    fn signup_emits_four_audit_rows_in_order() {
        let h = PilotHarness::new();
        let t = canonical_pilot_tenant("acme");
        h.complete_signup(&t).unwrap();
        let rows = h.audit_snapshot(&t);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].kind, AuditEventKind::SignupRequested);
        assert_eq!(rows[1].kind, AuditEventKind::CheckoutSessionCreated);
        assert_eq!(rows[2].kind, AuditEventKind::CheckoutWebhookVerified);
        assert_eq!(rows[3].kind, AuditEventKind::TenantProvisioned);
        // Chain integrity: each row's prev_hash matches the previous
        // row's row_hash.
        for w in rows.windows(2) {
            assert_eq!(w[1].prev_hash_hex, w[0].row_hash_hex);
        }
    }
}
