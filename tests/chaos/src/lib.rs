//! `chaos-campaign` — Wave-22 chaos engineering campaign harness.
//!
//! Eight scenarios are pinned, each asserting the canonical
//! **fail-CLOSED observable triplet**:
//!
//! 1. The operation under chaos fails CLOSED — no silent success path,
//!    no partial commit visible to a caller.
//! 2. An audit event is emitted at the documented taxonomy node
//!    (`corelink.<surface>.<verb>.failure_injected`).
//! 3. The documented alert (SEV-1 / SEV-2) or SLO counter fires.
//!
//! # Why a harness crate exists at all
//!
//! Production crates (`corelink-failover-router`, `corelink-audit-chain`,
//! `corelink-byok`, `corelink-billing-stripe`, …) each ship in-process
//! failure-injection knobs (`inject_failure`, `FailingHealthProbe`,
//! `inject_provider_503`, …). Per-crate tests exercise those knobs
//! *inside* the crate. The campaign harness rolls them up into an
//! **opt-in (`--features chaos`) cross-crate matrix** that mirrors what
//! a real on-call would chain together: a network partition + an audit
//! sink failure at the same time, for example, must still leave the
//! system at a known fail-CLOSED state.
//!
//! All eight scenarios run with **in-process simulations only** —
//! no testcontainers, no network IO, no external infra — so the suite
//! is reproducible on a laptop and on CI.
//!
//! # Scenario index
//!
//! | # | Scenario                                        | Failure injected                              | Fail-CLOSED assertion                                                       | Audit event                                  | Alert                                |
//! | - | ----------------------------------------------- | --------------------------------------------- | --------------------------------------------------------------------------- | -------------------------------------------- | ------------------------------------ |
//! | 1 | Network partition between regions               | 5-minute cross-region link drop               | Read-only mode in degraded region; writes return 503                        | `corelink.failover.region.degraded`          | SEV-1 `region_isolated`              |
//! | 2 | D1 connection pool exhausted                    | All N pool slots taken; new acquire times out | Request returns 503 with `Retry-After` header; no partial transaction       | `corelink.d1.pool.exhausted`                 | SEV-2 `d1_pool_saturated`            |
//! | 3 | Neon shadow audit sink silent failure           | Sink returns Ok but persists nothing          | Sink-side checksum reconcile flags drift; R2 archive still writes succeed   | `corelink.audit.shadow_sink.silent_failure` | SEV-2 `audit_shadow_sink_drift`      |
//! | 4 | RLS GUC dropped mid-transaction                 | `app.current_tenant` GUC unset                | `WITH CHECK` policy rejects INSERT; no row visible from any tenant scope    | `corelink.rls.guc.dropped`                   | SEV-1 `rls_policy_violation_attempt` |
//! | 5 | BYOK provider 503                               | AWS/GCP/Azure/Vault all return 503            | Route layer returns 503; encrypt/decrypt fail-CLOSED; no plaintext leaked   | `corelink.byok.provider.unavailable`         | SEV-1 `byok_provider_unavailable`    |
//! | 6 | Stripe webhook timestamp drift                  | Sender clock skewed > 300s                    | Webhook handler rejects; signature considered invalid; no state mutation    | `corelink.billing.webhook.replay_window`     | SEV-2 `stripe_webhook_clock_skew`    |
//! | 7 | Clerk JWT issuer rotation mid-request           | JWKS endpoint returns new kid mid-flight      | Handler re-fetches JWKS; request succeeds after rotation; no cached drift   | `corelink.clerk.jwks.rotated`                | INFO `clerk_jwks_rotation`           |
//! | 8 | CAS blob mid-upload abort                       | Client aborts multipart upload mid-stream     | Server aborts upload; no `complete` issued; staged parts garbage-collected  | `corelink.cas.multipart.aborted`             | INFO `cas_multipart_aborted`         |
//!
//! Each scenario is encoded as a `#[test]` (gated by `#[cfg(feature = "chaos")]`).
//! The library exports the mini-models the scenarios drive. The models
//! are **deliberately self-contained** — they mirror the canonical
//! shapes the production crates expose without binding the harness to
//! their full surface. That keeps the campaign stable across refactors
//! and lets the harness assert invariants at the *contract* level.
//!
//! # Charter
//!
//! - `#![forbid(unsafe_code)]`
//! - no `unwrap` / `expect` / `panic` in library code (tests are
//!   allowed via crate-level lint scope override)
//! - all simulations are pure synchronous state machines

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

// ============================================================================
// Scenario 1: Network partition / region failover
// ============================================================================

/// Region identifiers used by the harness. Mirrors the canonical
/// `corelink-failover-router::Region` surface without binding to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CampaignRegion {
    /// US-East primary.
    UsEast,
    /// EU-West replica.
    EuWest,
}

/// Health observed by the failover router for a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionHealth {
    /// All probes green.
    Healthy,
    /// Multi-signal degraded (5xx rate, latency, consecutive failures).
    Degraded,
    /// Probe consistently unreachable — region partitioned.
    Down,
}

/// Outcome of a request routed through the failover model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteOutcome {
    /// Served from the named region.
    Served(CampaignRegion),
    /// Failed CLOSED — write attempted against a degraded/down region
    /// while replica catchup was incomplete.
    FailedClosed503,
}

/// Minimal failover router used by scenarios 1 & 2. Read-only mode kicks
/// in as soon as the primary degrades; writes return 503 until heal.
#[derive(Debug, Default)]
pub struct CampaignFailoverModel {
    primary_health: BTreeMap<CampaignRegion, RegionHealth>,
    audit_events: Vec<String>,
    sev1_alerts: Vec<String>,
}

impl CampaignFailoverModel {
    /// Construct a fresh model with both regions healthy.
    #[must_use]
    pub fn new() -> Self {
        let mut primary_health = BTreeMap::new();
        primary_health.insert(CampaignRegion::UsEast, RegionHealth::Healthy);
        primary_health.insert(CampaignRegion::EuWest, RegionHealth::Healthy);
        Self {
            primary_health,
            audit_events: Vec::new(),
            sev1_alerts: Vec::new(),
        }
    }

    /// Inject a partition affecting one region (used by Scenario 1).
    pub fn inject_partition(&mut self, r: CampaignRegion) {
        self.primary_health.insert(r, RegionHealth::Down);
        self.audit_events
            .push("corelink.failover.region.degraded".into());
        self.sev1_alerts.push("region_isolated".into());
    }

    /// Heal a region.
    pub fn heal(&mut self, r: CampaignRegion) {
        self.primary_health.insert(r, RegionHealth::Healthy);
        self.audit_events
            .push("corelink.failover.region.recovered".into());
    }

    /// Route a write request. Returns `FailedClosed503` if the target
    /// region is degraded/down.
    #[must_use]
    pub fn route_write(&self, region: CampaignRegion) -> RouteOutcome {
        match self.primary_health.get(&region).copied() {
            Some(RegionHealth::Healthy) => RouteOutcome::Served(region),
            _ => RouteOutcome::FailedClosed503,
        }
    }

    /// Route a read request. Reads fail over to the partner region when
    /// the primary is unhealthy.
    #[must_use]
    pub fn route_read(&self, primary: CampaignRegion) -> RouteOutcome {
        match self.primary_health.get(&primary).copied() {
            Some(RegionHealth::Healthy) => RouteOutcome::Served(primary),
            _ => {
                let partner = match primary {
                    CampaignRegion::UsEast => CampaignRegion::EuWest,
                    CampaignRegion::EuWest => CampaignRegion::UsEast,
                };
                match self.primary_health.get(&partner).copied() {
                    Some(RegionHealth::Healthy) => RouteOutcome::Served(partner),
                    _ => RouteOutcome::FailedClosed503,
                }
            }
        }
    }

    /// Audit trail emitted by the model.
    #[must_use]
    pub fn audit_events(&self) -> &[String] {
        &self.audit_events
    }

    /// SEV-1 alerts emitted by the model.
    #[must_use]
    pub fn sev1_alerts(&self) -> &[String] {
        &self.sev1_alerts
    }
}

// ============================================================================
// Scenario 2: D1 connection pool exhaustion
// ============================================================================

/// Outcome of a D1 acquire under chaos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum D1AcquireOutcome {
    /// Acquired a connection from the pool.
    Acquired,
    /// Pool exhausted — caller must retry. Includes the `Retry-After`
    /// (seconds) hint surfaced as a response header.
    Pool503 {
        /// Seconds the client should wait before retrying.
        retry_after_secs: u32,
    },
}

/// Bounded D1 pool simulator. Exposes `inject_saturation` so a scenario
/// can starve the pool deterministically.
#[derive(Debug)]
pub struct CampaignD1Pool {
    capacity: u32,
    in_use: u32,
    audit_events: Vec<String>,
    sev2_alerts: Vec<String>,
}

impl CampaignD1Pool {
    /// Build a pool with the given capacity.
    #[must_use]
    pub fn new(capacity: u32) -> Self {
        Self {
            capacity,
            in_use: 0,
            audit_events: Vec::new(),
            sev2_alerts: Vec::new(),
        }
    }

    /// Saturate the pool (used by Scenario 2 — pretend every slot is
    /// occupied by a slow neighbouring tenant).
    pub fn inject_saturation(&mut self) {
        self.in_use = self.capacity;
    }

    /// Try to acquire a connection.
    pub fn acquire(&mut self) -> D1AcquireOutcome {
        if self.in_use >= self.capacity {
            self.audit_events.push("corelink.d1.pool.exhausted".into());
            self.sev2_alerts.push("d1_pool_saturated".into());
            return D1AcquireOutcome::Pool503 {
                retry_after_secs: 2,
            };
        }
        self.in_use += 1;
        D1AcquireOutcome::Acquired
    }

    /// Audit trail.
    #[must_use]
    pub fn audit_events(&self) -> &[String] {
        &self.audit_events
    }

    /// SEV-2 alerts.
    #[must_use]
    pub fn sev2_alerts(&self) -> &[String] {
        &self.sev2_alerts
    }
}

// ============================================================================
// Scenario 3: Neon shadow sink silent failure
// ============================================================================

/// Persistence outcome for an audit row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkPersistResult {
    /// Row persisted and visible to a subsequent reconcile pass.
    Persisted,
    /// Sink returned Ok but the row never materialized (silent failure).
    SilentDrop,
}

/// Audit dual-write model: every row must hit both R2 archive and the
/// Neon shadow sink. Scenario 3 toggles the shadow sink into silent
/// failure mode and verifies the reconcile pass detects drift.
#[derive(Debug)]
pub struct CampaignAuditDualWrite {
    r2_archive: Vec<String>,
    neon_shadow: Vec<String>,
    shadow_silent: bool,
    audit_events: Vec<String>,
    sev2_alerts: Vec<String>,
}

impl Default for CampaignAuditDualWrite {
    fn default() -> Self {
        Self::new()
    }
}

impl CampaignAuditDualWrite {
    /// Fresh model — both sinks healthy.
    #[must_use]
    pub fn new() -> Self {
        Self {
            r2_archive: Vec::new(),
            neon_shadow: Vec::new(),
            shadow_silent: false,
            audit_events: Vec::new(),
            sev2_alerts: Vec::new(),
        }
    }

    /// Toggle the neon shadow sink into silent-failure mode.
    pub fn inject_shadow_silent_failure(&mut self) {
        self.shadow_silent = true;
    }

    /// Emit an audit row. R2 archive always writes (fail-CLOSED on the
    /// canonical primary); the shadow sink follows the chaos flag.
    pub fn emit(&mut self, row: &str) -> SinkPersistResult {
        self.r2_archive.push(row.to_string());
        if self.shadow_silent {
            // Silent drop: sink reports success but nothing persists.
            return SinkPersistResult::SilentDrop;
        }
        self.neon_shadow.push(row.to_string());
        SinkPersistResult::Persisted
    }

    /// Reconcile R2 archive against the shadow sink. Drift fires the
    /// SEV-2 alert.
    pub fn reconcile(&mut self) -> bool {
        if self.r2_archive.len() != self.neon_shadow.len() {
            self.audit_events
                .push("corelink.audit.shadow_sink.silent_failure".into());
            self.sev2_alerts.push("audit_shadow_sink_drift".into());
            return false;
        }
        true
    }

    /// Rows landed in R2.
    #[must_use]
    pub fn r2_archive(&self) -> &[String] {
        &self.r2_archive
    }

    /// Rows landed in the shadow sink.
    #[must_use]
    pub fn neon_shadow(&self) -> &[String] {
        &self.neon_shadow
    }

    /// Audit events.
    #[must_use]
    pub fn audit_events(&self) -> &[String] {
        &self.audit_events
    }

    /// SEV-2 alerts.
    #[must_use]
    pub fn sev2_alerts(&self) -> &[String] {
        &self.sev2_alerts
    }
}

// ============================================================================
// Scenario 4: RLS GUC dropout
// ============================================================================

/// Outcome of an RLS-protected INSERT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RlsInsertOutcome {
    /// INSERT accepted; row visible to the owning tenant.
    Inserted,
    /// `WITH CHECK` policy rejected — fail-CLOSED.
    PolicyRejected,
}

/// Mini-model of a single RLS-protected table. `app.current_tenant` is
/// the session GUC; if absent the WITH CHECK policy rejects.
#[derive(Debug, Default)]
pub struct CampaignRlsTable {
    current_tenant: Option<String>,
    rows: BTreeMap<String, BTreeSet<String>>, // tenant -> row ids
    audit_events: Vec<String>,
    sev1_alerts: Vec<String>,
}

impl CampaignRlsTable {
    /// Empty table; no session GUC set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the session GUC.
    pub fn set_session_tenant(&mut self, tenant: impl Into<String>) {
        self.current_tenant = Some(tenant.into());
    }

    /// Drop the session GUC mid-tx (chaos injection).
    pub fn drop_session_tenant(&mut self) {
        self.current_tenant = None;
    }

    /// Attempt an INSERT.
    pub fn insert(&mut self, claimed_tenant: &str, row_id: &str) -> RlsInsertOutcome {
        // WITH CHECK: session GUC must be set and must match the claimed
        // tenant. Either drift fail-CLOSEs.
        let allowed = self
            .current_tenant
            .as_deref()
            .map(|t| t == claimed_tenant)
            .unwrap_or(false);
        if !allowed {
            self.audit_events.push("corelink.rls.guc.dropped".into());
            self.sev1_alerts.push("rls_policy_violation_attempt".into());
            return RlsInsertOutcome::PolicyRejected;
        }
        self.rows
            .entry(claimed_tenant.to_string())
            .or_default()
            .insert(row_id.to_string());
        RlsInsertOutcome::Inserted
    }

    /// Rows visible to a tenant.
    #[must_use]
    pub fn rows_for(&self, tenant: &str) -> Vec<&str> {
        self.rows
            .get(tenant)
            .map(|s| s.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Audit events.
    #[must_use]
    pub fn audit_events(&self) -> &[String] {
        &self.audit_events
    }

    /// SEV-1 alerts.
    #[must_use]
    pub fn sev1_alerts(&self) -> &[String] {
        &self.sev1_alerts
    }
}

// ============================================================================
// Scenario 5: BYOK provider 503
// ============================================================================

/// BYOK provider identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ByokProvider {
    /// AWS KMS.
    AwsKms,
    /// GCP KMS.
    GcpKms,
    /// Azure Key Vault.
    AzureKv,
    /// HashiCorp Vault.
    Vault,
}

/// BYOK envelope outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByokOutcome {
    /// Provider returned the data key — proceed.
    KeyAcquired(ByokProvider),
    /// Provider unavailable — fail CLOSED, return 503.
    ProviderUnavailable503,
}

/// Multi-provider BYOK model. Scenario 5 marks every provider as 503
/// and asserts the route layer fails CLOSED — encrypt/decrypt return
/// 503 rather than falling back to plaintext or a default key.
#[derive(Debug)]
pub struct CampaignByokModel {
    provider_state: BTreeMap<ByokProvider, bool>, // true = available
    audit_events: Vec<String>,
    sev1_alerts: Vec<String>,
}

impl Default for CampaignByokModel {
    fn default() -> Self {
        Self::new()
    }
}

impl CampaignByokModel {
    /// All providers available.
    #[must_use]
    pub fn new() -> Self {
        let mut s = BTreeMap::new();
        for p in [
            ByokProvider::AwsKms,
            ByokProvider::GcpKms,
            ByokProvider::AzureKv,
            ByokProvider::Vault,
        ] {
            s.insert(p, true);
        }
        Self {
            provider_state: s,
            audit_events: Vec::new(),
            sev1_alerts: Vec::new(),
        }
    }

    /// Mark a provider unavailable (503).
    pub fn inject_provider_503(&mut self, p: ByokProvider) {
        self.provider_state.insert(p, false);
    }

    /// Acquire a data key, preferring the supplied provider then any
    /// healthy fallback. Fails CLOSED if no provider is available.
    pub fn acquire_data_key(&mut self, preferred: ByokProvider) -> ByokOutcome {
        if self
            .provider_state
            .get(&preferred)
            .copied()
            .unwrap_or(false)
        {
            return ByokOutcome::KeyAcquired(preferred);
        }
        for (p, ok) in &self.provider_state {
            if *ok {
                return ByokOutcome::KeyAcquired(*p);
            }
        }
        self.audit_events
            .push("corelink.byok.provider.unavailable".into());
        self.sev1_alerts.push("byok_provider_unavailable".into());
        ByokOutcome::ProviderUnavailable503
    }

    /// Audit events.
    #[must_use]
    pub fn audit_events(&self) -> &[String] {
        &self.audit_events
    }

    /// SEV-1 alerts.
    #[must_use]
    pub fn sev1_alerts(&self) -> &[String] {
        &self.sev1_alerts
    }
}

// ============================================================================
// Scenario 6: Stripe webhook timestamp drift
// ============================================================================

/// Stripe webhook verification outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookOutcome {
    /// Signature + timestamp valid — process.
    Accepted,
    /// Signature OK but timestamp outside replay window — reject.
    OutsideReplayWindow,
    /// Signature mismatch — reject.
    SignatureMismatch,
}

/// Stripe webhook verifier. Replay window is 300s by default per Stripe
/// guidance; Scenario 6 simulates a clock-skewed sender.
#[derive(Debug)]
pub struct CampaignWebhookVerifier {
    replay_window_secs: i64,
    audit_events: Vec<String>,
    sev2_alerts: Vec<String>,
}

impl Default for CampaignWebhookVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl CampaignWebhookVerifier {
    /// Default verifier (300s window).
    #[must_use]
    pub fn new() -> Self {
        Self {
            replay_window_secs: 300,
            audit_events: Vec::new(),
            sev2_alerts: Vec::new(),
        }
    }

    /// Verify a webhook. `signed_ok` mirrors the upstream signature
    /// check (HMAC-SHA256 over `t.body`). `sender_ts` and `verifier_now`
    /// are wall-clock seconds.
    pub fn verify(&mut self, signed_ok: bool, sender_ts: i64, verifier_now: i64) -> WebhookOutcome {
        if !signed_ok {
            self.audit_events
                .push("corelink.billing.webhook.signature_invalid".into());
            return WebhookOutcome::SignatureMismatch;
        }
        let drift = (verifier_now - sender_ts).abs();
        if drift > self.replay_window_secs {
            self.audit_events
                .push("corelink.billing.webhook.replay_window".into());
            self.sev2_alerts.push("stripe_webhook_clock_skew".into());
            return WebhookOutcome::OutsideReplayWindow;
        }
        WebhookOutcome::Accepted
    }

    /// Audit events.
    #[must_use]
    pub fn audit_events(&self) -> &[String] {
        &self.audit_events
    }

    /// SEV-2 alerts.
    #[must_use]
    pub fn sev2_alerts(&self) -> &[String] {
        &self.sev2_alerts
    }
}

// ============================================================================
// Scenario 7: Clerk JWKS rotation mid-request
// ============================================================================

/// Clerk verification outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JwksOutcome {
    /// Token verified against the cached JWKS.
    VerifiedCached(String),
    /// JWKS cache missed; re-fetch succeeded; token verified.
    VerifiedAfterRefetch(String),
    /// Token verification failed even after re-fetch — fail CLOSED.
    Unverified,
}

/// Clerk JWKS cache simulator. A rotation publishes a new `kid` while
/// the in-process cache still holds the old one; the verifier must
/// re-fetch transparently.
#[derive(Debug)]
pub struct CampaignClerkJwks {
    cached_kid: String,
    upstream_kid: String,
    audit_events: Vec<String>,
    info_events: Vec<String>,
}

impl CampaignClerkJwks {
    /// Build with a starting `kid`.
    #[must_use]
    pub fn new(kid: impl Into<String>) -> Self {
        let k = kid.into();
        Self {
            cached_kid: k.clone(),
            upstream_kid: k,
            audit_events: Vec::new(),
            info_events: Vec::new(),
        }
    }

    /// Inject a JWKS rotation upstream.
    pub fn inject_rotation(&mut self, new_kid: impl Into<String>) {
        self.upstream_kid = new_kid.into();
    }

    /// Verify a token signed by `signing_kid`.
    pub fn verify(&mut self, signing_kid: &str) -> JwksOutcome {
        if signing_kid == self.cached_kid {
            return JwksOutcome::VerifiedCached(signing_kid.into());
        }
        // Cache miss → re-fetch from upstream.
        self.cached_kid = self.upstream_kid.clone();
        self.audit_events.push("corelink.clerk.jwks.rotated".into());
        self.info_events.push("clerk_jwks_rotation".into());
        if signing_kid == self.cached_kid {
            return JwksOutcome::VerifiedAfterRefetch(signing_kid.into());
        }
        JwksOutcome::Unverified
    }

    /// Audit events.
    #[must_use]
    pub fn audit_events(&self) -> &[String] {
        &self.audit_events
    }

    /// INFO events.
    #[must_use]
    pub fn info_events(&self) -> &[String] {
        &self.info_events
    }
}

// ============================================================================
// Scenario 8: CAS multipart abort
// ============================================================================

/// Multipart upload state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipartState {
    /// Upload created, accepting parts.
    Open,
    /// Client (or server) aborted the upload.
    Aborted,
    /// Client issued `complete` and the manifest sealed.
    Completed,
}

/// Multipart upload model. Scenario 8 starts an upload, streams a few
/// parts, then aborts mid-stream and asserts no `complete` is callable
/// and staged parts are GC-able.
#[derive(Debug)]
pub struct CampaignMultipart {
    state: MultipartState,
    staged_parts: VecDeque<u32>, // part numbers
    audit_events: Vec<String>,
    info_events: Vec<String>,
}

impl CampaignMultipart {
    /// Open a new upload.
    #[must_use]
    pub fn open() -> Self {
        Self {
            state: MultipartState::Open,
            staged_parts: VecDeque::new(),
            audit_events: Vec::new(),
            info_events: Vec::new(),
        }
    }

    /// Stage a part. No-op if not Open.
    pub fn stage_part(&mut self, n: u32) {
        if self.state == MultipartState::Open {
            self.staged_parts.push_back(n);
        }
    }

    /// Abort the upload (client or server). Idempotent.
    pub fn abort(&mut self) {
        if self.state == MultipartState::Open {
            self.state = MultipartState::Aborted;
            self.audit_events
                .push("corelink.cas.multipart.aborted".into());
            self.info_events.push("cas_multipart_aborted".into());
        }
    }

    /// Attempt to complete. Only succeeds if still Open.
    pub fn complete(&mut self) -> Result<(), &'static str> {
        if self.state != MultipartState::Open {
            return Err("multipart not open");
        }
        self.state = MultipartState::Completed;
        Ok(())
    }

    /// Garbage-collect staged parts (post-abort). Returns the GC'd count.
    pub fn gc(&mut self) -> usize {
        if self.state != MultipartState::Aborted {
            return 0;
        }
        let n = self.staged_parts.len();
        self.staged_parts.clear();
        n
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> &MultipartState {
        &self.state
    }

    /// Staged-parts queue.
    #[must_use]
    pub fn staged_parts(&self) -> &VecDeque<u32> {
        &self.staged_parts
    }

    /// Audit events.
    #[must_use]
    pub fn audit_events(&self) -> &[String] {
        &self.audit_events
    }

    /// INFO events.
    #[must_use]
    pub fn info_events(&self) -> &[String] {
        &self.info_events
    }
}

// ============================================================================
// Shared assertions
// ============================================================================

/// Assert that an audit-event slice contains exactly one occurrence of
/// the canonical taxonomy node. Returned `Err` carries a human-readable
/// message; tests can `.unwrap()` it under the per-file lint scope.
///
/// # Errors
///
/// Returns `Err` if the event does not appear exactly once.
pub fn assert_audit_emitted_once(events: &[String], canonical: &str) -> Result<(), String> {
    let count = events.iter().filter(|e| e.as_str() == canonical).count();
    if count == 1 {
        Ok(())
    } else {
        Err(format!(
            "expected audit event `{canonical}` once, found {count} in {events:?}"
        ))
    }
}

/// Assert that an alert slice contains the canonical alert name.
///
/// # Errors
///
/// Returns `Err` if the alert is not present.
pub fn assert_alert_fired(alerts: &[String], canonical: &str) -> Result<(), String> {
    if alerts.iter().any(|a| a.as_str() == canonical) {
        Ok(())
    } else {
        Err(format!(
            "expected alert `{canonical}`, none found in {alerts:?}"
        ))
    }
}
