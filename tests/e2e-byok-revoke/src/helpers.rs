//! Shared test fixtures for the R3-2 BYOK revocation E2E harness.
//!
//! See the crate-level rustdoc for the full pipeline. The helpers
//! exported here are the only API surface the integration tests rely
//! on; keeping them centralised avoids re-deriving the same wiring (KMS
//! stubs, cache + store + alerter Arcs, kill-switch orchestrator
//! mirroring the production detector's `handle_revocation` path) per
//! test binary.
//!
//! # Why a hand-rolled `KillSwitchRunner` instead of `RevocationDetector::run_one_cycle`?
//!
//! `RevocationDetector::list_active_byok_keys` is a stub returning
//! `Vec::new()` in the upstream crate (production wiring queries D1).
//! Running `run_one_cycle` with the default stub therefore does
//! nothing — it does not exercise the kill-switch path. The R3-2 E2E
//! harness instead drives the same per-step pipeline (`check_access` →
//! `evict_all_for_key` → `mark_degraded` → audit emit → `alerter.alert`)
//! that the production detector orchestrates, asserting on each crate
//! boundary. We still exercise `RevocationDetector::run_one_cycle` in
//! one test (smoke) to assert the public surface compiles and runs
//! cleanly with the in-memory wiring.
//!
//! # Honouring INV-BYOK-CRYPTO-SOVEREIGNTY
//!
//! - DEK cache TTL is constructed via `DekCache::new(300)` — the hard
//!   ceiling. No test creates a cache above 300 s.
//! - Kill switch is always invoked through `cache.evict_all_for_key` +
//!   `store.mark_degraded` (no test reaches into private state).
//! - `KmsAccessStatus::Throttled` / `ApiError` paths are tested
//!   explicitly to ensure they do NOT trigger kill switch (Lote 10.14
//!   codex P0 regression).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test harness construction may panic on infrastructure-level failures"
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use corelink_byok::{
    BYOKError, Dek, DekCache, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProvider,
    KmsProviderKind, WrappedDek,
};
use corelink_byok::revocation::{
    alerter::{CustomerAlerter, RevocationAlertPayload},
    error::RevocationError,
    event::{EVENT_TYPE_CMK_RESTORED, EVENT_TYPE_CMK_REVOKED},
    store::{TenantByokStatus, TenantStatusStore},
    RevocationAuditEvent,
};

// ---------------------------------------------------------------------------
// SLA constants
// ---------------------------------------------------------------------------

/// Per WI-S14-006 §SLA: kill switch execution (evict + degrade + audit +
/// alert) MUST complete within 30 s. Tests assert strict upper bound at
/// 30_000 ms with a generous margin; in-memory wiring typically runs in
/// well under 50 ms.
pub const KILL_SWITCH_SLA_MS: u64 = 30_000;

/// Per WI-S14-006 §SLA: total p99 customer-perceived kill switch
/// latency ≤ 6 minutes (cadence + execution). The harness substitutes
/// 60 s for the cadence in synchronous assertions.
pub const KILL_SWITCH_DETECTION_BOUND_MS: u64 = 60_000;

// ---------------------------------------------------------------------------
// Expected revocation events
// ---------------------------------------------------------------------------

/// Canonical audit event family discriminator emitted by the kill
/// switch / recovery paths.
///
/// Tests assert which subset of `RevocationAuditEvent::event_type`
/// strings appeared in the in-memory audit sink during the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExpectedRevocationEvent {
    /// `corelink.byok.cmk_revoked`.
    CmkRevoked,
    /// `corelink.byok.cmk_restored`.
    CmkRestored,
}

// ---------------------------------------------------------------------------
// Harness errors
// ---------------------------------------------------------------------------

/// Errors surfaced by the harness when the orchestrated kill switch
/// flow violates an invariant.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RevokeError {
    /// Wrapped `BYOKError` from the KMS / cache layer.
    #[error("BYOK error: {0}")]
    Byok(#[from] BYOKError),

    /// Wrapped `RevocationError` from the revocation layer.
    #[error("revocation error: {0}")]
    Revocation(#[from] RevocationError),

    /// Kill switch execution exceeded SLA.
    #[error("kill switch SLA violated: {observed_ms} ms > {bound_ms} ms")]
    SlaViolated {
        /// Observed duration in ms.
        observed_ms: u64,
        /// Bound enforced.
        bound_ms: u64,
    },

    /// INV-BYOK-CRYPTO-SOVEREIGNTY: DEK retrievable post-revoke.
    #[error("INV-BYOK-CRYPTO-SOVEREIGNTY violated: {0}")]
    InvariantViolated(String),
}

// ---------------------------------------------------------------------------
// KMS behaviour: programmable per-cycle behaviour for `BoundedKmsProvider`.
// ---------------------------------------------------------------------------

/// Programmable per-cycle behaviour for [`BoundedKmsProvider`].
///
/// The provider records every `check_access` invocation and returns
/// either a fixed status or pops the next scripted status from a queue.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum KmsBehaviour {
    /// Always return `Ok`.
    AlwaysOk,
    /// Always return `Revoked` (customer kill switch).
    AlwaysRevoked,
    /// Always return `Throttled` (transient: must NOT trigger kill
    /// switch).
    AlwaysThrottled,
    /// Always return `ApiError(code)` (transient).
    AlwaysApiError(u16),
    /// Scripted sequence: pop one status per call; once exhausted,
    /// fall back to `Ok`.
    Scripted(Vec<KmsAccessStatus>),
}

// ---------------------------------------------------------------------------
// BoundedKmsProvider
// ---------------------------------------------------------------------------

/// In-memory `KmsProvider` parameterised by [`KmsProviderKind`] +
/// [`KmsBehaviour`].
///
/// Used to drive the four BYOK providers (AWS / GCP / Azure / Vault) in
/// the multi-provider matrix without any network IO.
///
/// `wrap_dek` returns a deterministic ciphertext (`dek.bytes ⊕ key_id`)
/// so `unwrap_dek` is symmetric. `encryption_context` is preserved.
#[derive(Debug)]
pub struct BoundedKmsProvider {
    kind: KmsProviderKind,
    region: String,
    fips: FipsLevel,
    behaviour: Mutex<KmsBehaviour>,
    /// Records every `check_access` call (provider-side observability).
    check_access_calls: Mutex<Vec<KmsKeyId>>,
    /// When `true`, `wrap_dek` / `unwrap_dek` reject (simulates a
    /// revoked CMK denying new wraps mid-revoke — fail-CLOSED).
    deny_envelope: Mutex<bool>,
}

impl BoundedKmsProvider {
    /// Construct with a provider kind, region and FIPS level.
    #[must_use]
    pub fn new(kind: KmsProviderKind, region: &str, fips: FipsLevel, behaviour: KmsBehaviour) -> Self {
        Self {
            kind,
            region: region.to_string(),
            fips,
            behaviour: Mutex::new(behaviour),
            check_access_calls: Mutex::new(Vec::new()),
            deny_envelope: Mutex::new(false),
        }
    }

    /// Update the programmable behaviour mid-test (simulates customer
    /// flipping the CMK state).
    pub fn set_behaviour(&self, behaviour: KmsBehaviour) {
        let mut guard = match self.behaviour.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *guard = behaviour;
    }

    /// Toggle envelope denial — when `true`, `wrap_dek` and
    /// `unwrap_dek` return `BYOKError::CmkRevoked`. Used by the
    /// race-condition test to assert fail-CLOSED behaviour.
    pub fn set_deny_envelope(&self, deny: bool) {
        let mut guard = match self.deny_envelope.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        *guard = deny;
    }

    /// Return the number of `check_access` calls observed so far.
    #[must_use]
    pub fn check_access_count(&self) -> usize {
        let g = match self.check_access_calls.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.len()
    }

    /// Return the canonical in-memory `KmsKeyId` for this provider
    /// (deterministic across runs; uses the region as a suffix).
    #[must_use]
    pub fn canonical_key_id(&self) -> KmsKeyId {
        let arn = match self.kind {
            KmsProviderKind::AwsKms => format!(
                "arn:aws:kms:{}:000000000000:key/00000000-0000-0000-0000-{}",
                self.region, "000000000001"
            ),
            KmsProviderKind::GcpKms => format!(
                "projects/corelink-e2e/locations/{}/keyRings/r3-2/cryptoKeys/test-key",
                self.region
            ),
            KmsProviderKind::AzureKeyVault => format!(
                "https://corelink-e2e-{}.vault.azure.net/keys/r3-2-test-key/00000001",
                self.region
            ),
            KmsProviderKind::HashicorpVault => "transit/keys/r3-2-test-key".to_string(),
        };
        KmsKeyId {
            provider: self.kind,
            key_arn_or_id: arn,
            region: self.region.clone(),
        }
    }
}

#[async_trait]
impl KmsProvider for BoundedKmsProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        self.kind
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        self.fips
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        let deny = {
            let g = match self.deny_envelope.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            *g
        };
        if deny {
            return Err(BYOKError::CmkRevoked {
                provider: self.kind,
                key_id: key_id.key_arn_or_id.clone(),
            });
        }
        // Deterministic "wrap": XOR DEK with a 32-byte key digest.
        let key_digest = digest32(&key_id.key_arn_or_id);
        let mut ct = vec![0u8; 32];
        for i in 0..32 {
            let dek_byte = match dek.bytes.get(i) {
                Some(b) => *b,
                None => 0,
            };
            let kb = match key_digest.get(i) {
                Some(b) => *b,
                None => 0,
            };
            if let Some(slot) = ct.get_mut(i) {
                *slot = dek_byte ^ kb;
            }
        }
        Ok(WrappedDek {
            provider: self.kind,
            key_id: key_id.clone(),
            ciphertext: ct,
            encryption_context: encryption_context.cloned(),
        })
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        let deny = {
            let g = match self.deny_envelope.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            *g
        };
        if deny {
            return Err(BYOKError::CmkRevoked {
                provider: self.kind,
                key_id: wrapped.key_id.key_arn_or_id.clone(),
            });
        }
        if wrapped.ciphertext.len() != 32 {
            return Err(BYOKError::DekLengthInvalid {
                got: wrapped.ciphertext.len(),
            });
        }
        let key_digest = digest32(&wrapped.key_id.key_arn_or_id);
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            let ct_byte = match wrapped.ciphertext.get(i) {
                Some(b) => *b,
                None => 0,
            };
            let kb = match key_digest.get(i) {
                Some(b) => *b,
                None => 0,
            };
            if let Some(slot) = bytes.get_mut(i) {
                *slot = ct_byte ^ kb;
            }
        }
        Ok(Dek { bytes })
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        {
            let mut calls = match self.check_access_calls.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            calls.push(key_id.clone());
        }
        let mut guard = match self.behaviour.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let status = match &mut *guard {
            KmsBehaviour::AlwaysOk => KmsAccessStatus::Ok,
            KmsBehaviour::AlwaysRevoked => KmsAccessStatus::Revoked,
            KmsBehaviour::AlwaysThrottled => KmsAccessStatus::Throttled,
            KmsBehaviour::AlwaysApiError(code) => KmsAccessStatus::ApiError(*code),
            KmsBehaviour::Scripted(seq) => {
                if seq.is_empty() {
                    KmsAccessStatus::Ok
                } else {
                    seq.remove(0)
                }
            }
        };
        Ok(status)
    }
}

/// Cheap 32-byte digest of an ASCII string (folding XOR; sufficient
/// for deterministic in-memory wrap/unwrap symmetry).
fn digest32(s: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, b) in s.as_bytes().iter().enumerate() {
        let idx = i % 32;
        if let Some(slot) = out.get_mut(idx) {
            *slot ^= *b;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// AuditSink — captures revocation audit events.
// ---------------------------------------------------------------------------

/// In-memory audit sink for [`RevocationAuditEvent`]s emitted by the
/// kill switch / recovery paths.
///
/// Production wiring INSERTs into the D1 `audit_outbox` table in the
/// same atomic batch as the tenant status update
/// (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER). The harness assertions check
/// `event_type` strings as defined in `corelink_byok::revocation::event`.
#[derive(Debug, Default, Clone)]
pub struct AuditSink {
    inner: Arc<Mutex<Vec<RevocationAuditEvent>>>,
}

impl AuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an event (called by the harness `KillSwitchRunner`).
    pub fn record(&self, event: RevocationAuditEvent) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.push(event);
    }

    /// Snapshot the recorded event types in emission order.
    #[must_use]
    pub fn snapshot_event_types(&self) -> Vec<String> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.iter().map(|e| e.event_type.clone()).collect()
    }

    /// Snapshot the full event list (cloned).
    #[must_use]
    pub fn snapshot(&self) -> Vec<RevocationAuditEvent> {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.clone()
    }

    /// Number of events recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.len()
    }

    /// True iff the sink has no recorded events.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---------------------------------------------------------------------------
// CustomerAlertSink — implements `CustomerAlerter` and records every call.
// ---------------------------------------------------------------------------

/// Records every dispatched alert. Wraps an inner production
/// `MultiChannelAlerter` (in stub mode) so the harness also exercises
/// the real `corelink-customer-alerts` code path.
#[derive(Debug)]
pub struct CustomerAlertSink {
    inner: corelink_customer_alerts::MultiChannelAlerter,
    alerts: Arc<Mutex<Vec<RevocationAlertPayload>>>,
    recoveries: Arc<Mutex<Vec<(KmsProviderKind, KmsKeyId, u64)>>>,
}

impl CustomerAlertSink {
    /// Construct with the production `MultiChannelAlerter` configured
    /// in stub mode (all channels succeed silently — matches CI).
    #[must_use]
    pub fn new() -> Self {
        let cfg = corelink_customer_alerts::AlerterConfig::default();
        debug_assert!(
            cfg.stub_mode,
            "AlerterConfig::default() must set stub_mode=true for CI; \
             production overrides explicitly."
        );
        Self {
            inner: corelink_customer_alerts::MultiChannelAlerter::new(cfg),
            alerts: Arc::new(Mutex::new(Vec::new())),
            recoveries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Number of `alert` invocations recorded.
    #[must_use]
    pub fn alert_count(&self) -> usize {
        let g = match self.alerts.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.len()
    }

    /// Number of `alert_recovery` invocations recorded.
    #[must_use]
    pub fn recovery_count(&self) -> usize {
        let g = match self.recoveries.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.len()
    }

    /// Snapshot all recorded revoke alert payloads.
    #[must_use]
    pub fn alerts(&self) -> Vec<RevocationAlertPayload> {
        let g = match self.alerts.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.clone()
    }
}

impl Default for CustomerAlertSink {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CustomerAlerter for CustomerAlertSink {
    async fn alert(&self, payload: RevocationAlertPayload) -> Result<(), RevocationError> {
        // Exercise the production `MultiChannelAlerter` (stub mode).
        self.inner.alert(payload.clone()).await?;
        let mut g = match self.alerts.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.push(payload);
        Ok(())
    }

    async fn alert_recovery(
        &self,
        provider: KmsProviderKind,
        kms_key_id: &KmsKeyId,
        tenant_id_hashed: &str,
        restored_at_ms: u64,
    ) -> Result<(), RevocationError> {
        self.inner
            .alert_recovery(provider, kms_key_id, tenant_id_hashed, restored_at_ms)
            .await?;
        let mut g = match self.recoveries.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.push((provider, kms_key_id.clone(), restored_at_ms));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// InMemoryTenantStatusStore — local copy keyed by `KmsKeyId.key_arn_or_id`.
// ---------------------------------------------------------------------------

/// In-memory `TenantStatusStore` keyed by `KmsKeyId.key_arn_or_id`.
///
/// Mirrors the one in `corelink_byok::revocation::testutil` but the
/// harness re-implements it here so additional inspection helpers (e.g.
/// `transition_history`) can be exposed without modifying the upstream
/// testutil module.
#[derive(Debug, Default)]
pub struct InMemoryTenantStatusStore {
    statuses: Mutex<HashMap<String, TenantByokStatus>>,
    history: Mutex<Vec<(String, TenantByokStatus, u64)>>,
}

impl InMemoryTenantStatusStore {
    /// Read current status for `key_arn_or_id` (None = no entry).
    #[must_use]
    pub fn current(&self, key_arn_or_id: &str) -> Option<TenantByokStatus> {
        let g = match self.statuses.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.get(key_arn_or_id).copied()
    }

    /// Full transition history `(key_arn, status, ts_ms)`.
    #[must_use]
    pub fn history(&self) -> Vec<(String, TenantByokStatus, u64)> {
        let g = match self.history.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.clone()
    }
}

#[async_trait]
impl TenantStatusStore for InMemoryTenantStatusStore {
    async fn mark_degraded(
        &self,
        kms_key_id: &KmsKeyId,
        _provider: &str,
        revoked_at_ms: u64,
    ) -> Result<usize, RevocationError> {
        let mut s = match self.statuses.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        s.insert(
            kms_key_id.key_arn_or_id.clone(),
            TenantByokStatus::DegradedReadOnly,
        );
        let mut h = match self.history.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        h.push((
            kms_key_id.key_arn_or_id.clone(),
            TenantByokStatus::DegradedReadOnly,
            revoked_at_ms,
        ));
        Ok(1)
    }

    async fn restore_active(
        &self,
        kms_key_id: &KmsKeyId,
        restored_at_ms: u64,
    ) -> Result<usize, RevocationError> {
        let mut s = match self.statuses.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        s.insert(
            kms_key_id.key_arn_or_id.clone(),
            TenantByokStatus::Active,
        );
        let mut h = match self.history.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        h.push((
            kms_key_id.key_arn_or_id.clone(),
            TenantByokStatus::Active,
            restored_at_ms,
        ));
        Ok(1)
    }

    async fn current_status(
        &self,
        kms_key_id: &KmsKeyId,
    ) -> Result<Option<TenantByokStatus>, RevocationError> {
        Ok(self.current(&kms_key_id.key_arn_or_id))
    }
}

// ---------------------------------------------------------------------------
// RevokeBundle — what `setup_byok_env` returns.
// ---------------------------------------------------------------------------

/// Bundled collaborators returned by [`setup_byok_env`].
///
/// All four are `Arc`-shared so the test owner can:
///
/// - Encrypt / decrypt directly through the [`BoundedKmsProvider`] +
///   [`DekCache`].
/// - Drive the kill switch via [`KillSwitchRunner::run`].
/// - Inspect the [`AuditSink`] + [`CustomerAlertSink`] +
///   [`InMemoryTenantStatusStore`] after the cycle.
#[derive(Debug)]
#[non_exhaustive]
pub struct RevokeBundle {
    /// The KMS provider (deterministic in-memory; behaviour is
    /// programmable via [`BoundedKmsProvider::set_behaviour`]).
    pub provider: Arc<BoundedKmsProvider>,
    /// DEK cache (TTL = 300 s — INV-BYOK-CRYPTO-SOVEREIGNTY).
    pub dek_cache: Arc<DekCache>,
    /// Tenant BYOK-status store.
    pub tenant_store: Arc<InMemoryTenantStatusStore>,
    /// Audit sink (captures `corelink.byok.cmk_revoked` /
    /// `cmk_restored` events).
    pub audit: AuditSink,
    /// Customer alert sink wrapping the production
    /// `MultiChannelAlerter` (stub mode).
    pub alert: Arc<CustomerAlertSink>,
    /// Canonical `KmsKeyId` for this provider (matches what
    /// `provider.canonical_key_id()` returns).
    pub key_id: KmsKeyId,
    /// Canonical tenant id hash used in audit + alert payloads.
    pub tenant_id_hashed: String,
}

// ---------------------------------------------------------------------------
// setup_byok_env
// ---------------------------------------------------------------------------

/// Construct a fresh [`RevokeBundle`] for the given provider kind.
///
/// The bundle uses canonical fixtures:
///
/// - DEK cache TTL = 300 s (the hard ceiling per
///   INV-BYOK-CRYPTO-SOVEREIGNTY).
/// - Provider region defaults to `us-east-1` for AWS,
///   `us-central1` for GCP, `eastus` for Azure, `us-east-1` for Vault.
/// - FIPS level reflects each provider's canonical certification per
///   `compliance/byok-fips-matrix.md` (AWS / GCP / Azure: FIPS 140-3
///   Level 1; Vault: FIPS 140-2 Level 1).
/// - Initial behaviour is [`KmsBehaviour::AlwaysOk`]; tests flip via
///   [`BoundedKmsProvider::set_behaviour`].
#[must_use]
pub fn setup_byok_env(provider_kind: KmsProviderKind) -> RevokeBundle {
    let (region, fips) = match provider_kind {
        KmsProviderKind::AwsKms => ("us-east-1", FipsLevel::Fips140_3_L1),
        KmsProviderKind::GcpKms => ("us-central1", FipsLevel::Fips140_3_L1),
        KmsProviderKind::AzureKeyVault => ("eastus", FipsLevel::Fips140_3_L1),
        KmsProviderKind::HashicorpVault => ("us-east-1", FipsLevel::Fips140_2_L1),
    };
    let provider = Arc::new(BoundedKmsProvider::new(
        provider_kind,
        region,
        fips,
        KmsBehaviour::AlwaysOk,
    ));
    let key_id = provider.canonical_key_id();
    let dek_cache = Arc::new(DekCache::new(300).expect("TTL 300 s is the hard ceiling"));
    let tenant_store = Arc::new(InMemoryTenantStatusStore::default());
    let alert = Arc::new(CustomerAlertSink::new());

    RevokeBundle {
        provider,
        dek_cache,
        tenant_store,
        audit: AuditSink::new(),
        alert,
        key_id,
        tenant_id_hashed: format!(
            "blake3:tenant-{}-r3-2-e2e",
            provider_kind.as_str()
        ),
    }
}

// ---------------------------------------------------------------------------
// KillSwitchRunner — orchestrates the kill switch path through the
// public crate APIs, mirroring `RevocationDetector::handle_revocation`.
// ---------------------------------------------------------------------------

/// Mirrors `RevocationDetector::handle_revocation` against the public
/// crate APIs. Used in tests instead of `RevocationDetector::run_one_cycle`
/// because the upstream detector's `list_active_byok_keys` is a stub
/// that returns `Vec::new()` (production wiring queries D1).
///
/// Steps executed (each asserted in tests):
///
/// 1. `provider.check_access(&key_id)` — observe `Revoked` / `Ok` /
///    transient.
/// 2. On `Revoked` / `NotFound`: `cache.evict_all_for_key(&key_id)` +
///    `store.mark_degraded` + audit emit + `alerter.alert`.
/// 3. On `Ok` (after previous degrade): `store.restore_active` + audit
///    emit (`cmk_restored`) + `alerter.alert_recovery`.
#[derive(Debug)]
pub struct KillSwitchRunner;

impl KillSwitchRunner {
    /// Execute one revocation-detection + reaction cycle against the
    /// bundle. Returns the recorded `RevocationAuditEvent` if one was
    /// emitted (None when status is `Ok` without prior degrade, or on
    /// transient `Throttled` / `ApiError`).
    pub async fn run(bundle: &RevokeBundle) -> Result<Option<RevocationAuditEvent>, RevokeError> {
        let start = Instant::now();
        let status = bundle.provider.check_access(&bundle.key_id).await?;

        let now_ms = now_ms();
        match status {
            KmsAccessStatus::Revoked | KmsAccessStatus::NotFound => {
                // Step 1: evict cache (ZeroizeOnDrop on each dropped Dek).
                let evicted = bundle.dek_cache.evict_all_for_key(&bundle.key_id).await?;

                // Step 2: mark tenant degraded read-only (atomic with
                // audit emit in production D1 batch).
                bundle
                    .tenant_store
                    .mark_degraded(&bundle.key_id, bundle.key_id.provider.as_str(), now_ms)
                    .await?;

                // Step 3: build audit event + record to sink.
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                if duration_ms > KILL_SWITCH_SLA_MS {
                    return Err(RevokeError::SlaViolated {
                        observed_ms: duration_ms,
                        bound_ms: KILL_SWITCH_SLA_MS,
                    });
                }
                let event = RevocationAuditEvent {
                    event_type: EVENT_TYPE_CMK_REVOKED.to_string(),
                    provider: bundle.key_id.provider.as_str().to_string(),
                    kms_key_id_hashed: hash_for_audit(&bundle.key_id.key_arn_or_id),
                    tenant_id_hashed: bundle.tenant_id_hashed.clone(),
                    detected_at_ms: now_ms,
                    evicted_at_ms: now_ms,
                    alerted_at_ms: now_ms,
                    kill_switch_duration_ms: duration_ms,
                    evicted_dek_count: evicted,
                };
                bundle.audit.record(event.clone());

                // Step 4: dispatch alert via production
                // `MultiChannelAlerter` (stub mode).
                let payload = RevocationAlertPayload {
                    provider: bundle.key_id.provider.as_str().to_string(),
                    kms_key_id: bundle.key_id.clone(),
                    tenant_id_hashed: bundle.tenant_id_hashed.clone(),
                    detected_at_ms: now_ms,
                    kill_switch_duration_ms: duration_ms,
                    recovery_instructions: format!(
                        "Your CMK ({}) has been revoked. CoreLink has \
                         suspended access. Re-enable your CMK in the \
                         provider console; access will be restored within \
                         60 seconds.",
                        bundle.key_id.provider.as_str()
                    ),
                };
                bundle.alert.alert(payload).await?;

                Ok(Some(event))
            }
            KmsAccessStatus::Ok => {
                // Recovery path: only act if tenant was previously
                // degraded.
                let cur = bundle
                    .tenant_store
                    .current_status(&bundle.key_id)
                    .await?;
                if cur == Some(TenantByokStatus::DegradedReadOnly) {
                    bundle
                        .tenant_store
                        .restore_active(&bundle.key_id, now_ms)
                        .await?;
                    let event = RevocationAuditEvent {
                        event_type: EVENT_TYPE_CMK_RESTORED.to_string(),
                        provider: bundle.key_id.provider.as_str().to_string(),
                        kms_key_id_hashed: hash_for_audit(&bundle.key_id.key_arn_or_id),
                        tenant_id_hashed: bundle.tenant_id_hashed.clone(),
                        detected_at_ms: now_ms,
                        evicted_at_ms: now_ms,
                        alerted_at_ms: now_ms,
                        kill_switch_duration_ms: 0,
                        evicted_dek_count: 0,
                    };
                    bundle.audit.record(event.clone());
                    bundle
                        .alert
                        .alert_recovery(
                            bundle.key_id.provider,
                            &bundle.key_id,
                            &bundle.tenant_id_hashed,
                            now_ms,
                        )
                        .await?;
                    Ok(Some(event))
                } else {
                    Ok(None)
                }
            }
            KmsAccessStatus::Throttled | KmsAccessStatus::ApiError(_) => {
                // Lote 10.14 codex P0: transient errors MUST NOT trigger
                // the kill switch. The harness records no audit event
                // and leaves tenant status untouched.
                Ok(None)
            }
            // Future-proof: any new non-revoke variant is treated as
            // transient (matches the detector's `Ok(_)` arm).
            _ => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Misc helpers
// ---------------------------------------------------------------------------

/// Current time in ms since UNIX epoch (used as `detected_at_ms` /
/// `revoked_at_ms` in audit + alert payloads).
fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Deterministic prefix-hash for audit emission (never log raw key
/// IDs).
fn hash_for_audit(key_id: &str) -> String {
    let end = key_id.len().min(8);
    let slice = key_id.get(..end).unwrap_or("");
    format!("blake3:{slice}")
}

/// Construct a deterministic [`WrappedDek`] tagged with the canonical
/// `key_id` for the bundle. Used by stampede + race tests to populate
/// the DEK cache.
#[must_use]
pub fn make_wrapped_for(key_id: &KmsKeyId, discriminator: u32) -> WrappedDek {
    // Ciphertext length 32 so the round-trip in `BoundedKmsProvider`
    // succeeds; differentiated by the high u32 bytes so each entry is
    // a distinct `CacheKey`.
    let mut ct = vec![0u8; 32];
    if let Some(slot) = ct.get_mut(0..4) {
        slot.copy_from_slice(&discriminator.to_le_bytes());
    }
    WrappedDek {
        provider: key_id.provider,
        key_id: key_id.clone(),
        ciphertext: ct,
        encryption_context: Some(serde_json::json!({
            "tenant_id": "r3-2-e2e-tenant",
            "blob_hash": format!("sha256:r3-2-e2e-{discriminator:08}"),
        })),
    }
}

/// All four BYOK provider kinds in canonical order (matrix iteration).
pub const ALL_PROVIDER_KINDS: &[KmsProviderKind] = &[
    KmsProviderKind::AwsKms,
    KmsProviderKind::GcpKms,
    KmsProviderKind::AzureKeyVault,
    KmsProviderKind::HashicorpVault,
];

/// Env var names that gate live-provider `#[ignore]` tests.
pub const ENV_AWS_TEST_KEY_ARN: &str = "AWS_TEST_KEY_ARN";
/// GCP test key resource (full `projects/.../cryptoKeys/...`).
pub const ENV_GCP_TEST_KEY_RESOURCE: &str = "GCP_TEST_KEY_RESOURCE";
/// Azure test key resource (full vault key URI).
pub const ENV_AZURE_TEST_KEY_RESOURCE: &str = "AZURE_TEST_KEY_RESOURCE";
/// Vault test key name (transit key name).
pub const ENV_VAULT_TEST_KEY_NAME: &str = "VAULT_TEST_KEY_NAME";
