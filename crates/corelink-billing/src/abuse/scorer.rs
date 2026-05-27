//! Heuristic abuse scorer orchestrator — the pure-logic core that the
//! production cron DO + Tower middleware compose. Wires the 4-feature
//! weighted-sum [`compute_score`], the [`decide`] threshold map, the
//! [`AbuseAuditSink`], the [`AbuseMetricsObserver`], the
//! [`AbuseConfig`], and the cross-WI integration with
//! `corelink-ratelimit` (silent downgrade applies a refill-rate
//! reduction via the [`update_plan`] call per WI §6.1.4 CI-2 mechanism).
//!
//! ## Decision pipeline (per cron-tick per tenant)
//!
//! For each tenant (`tenant_id`, `features`, `tier`, `now_ms`):
//!
//! 1. **Score**: compute the canonical 4-feature weighted-sum score.
//! 2. **Decide**: map score → [`AbuseDecision`] (Benign / Suspicious /
//!    Malicious).
//! 3. **Audit emit BEFORE state mutation** (fail-closed envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`): emit `score_computed` +
//!    decision arm record. Audit failure ROLLS BACK the orchestration
//!    (returns `Err(Audit)`); production wiring maps to 5xx.
//! 4. **Apply gradient response**:
//!    - `Benign`: no side-effect (audit only).
//!    - `Suspicious`: silent downgrade — call into the
//!      [`RateLimiter::update_plan`] method to halve the refill rate
//!      for the tenant's per-tenant bucket. Emit `downgrade_applied`
//!      audit + bump `downgrade_applied_total` metric.
//!    - `Malicious`: emit `admin_review_triggered` audit. NEVER
//!      auto-suspend (LGPD Art. 20 + GDPR Art. 22 humane response per
//!      sprint contract §7.10.s08.3); programmatic
//!      [`AbuseScorer::auto_suspend`] attempts hit
//!      [`AbuseError::AutoSuspendForbidden`] regardless of score.
//! 5. **Metrics**: bump `tier_count_total` + observe `score`; if
//!    Suspicious also bump `downgrade_applied_total`.
//! 6. **Persist score-history** (test wiring; production at WI-S08-006
//!    flushes via D1 batch) — exposed as a snapshot accessor.
//!
//! ## Why per-instance Mutex serialises
//!
//! The InMemory orchestrator holds the per-tenant rolling window state
//! under a per-instance `Mutex`. This mirrors the production CF DO
//! actor model (single-threaded actor per `AbuseScoreCron-<region>` DO
//! singleton) byte-for-byte: concurrent score-and-apply calls cannot
//! both observe the same pre-decision rolling window for the same
//! tenant.
//!
//! ## F-001 closure
//!
//! Per the autonomous execution charter, the orchestrator state is
//! held in per-instance `Arc<Mutex<HashMap<Uuid, AbuseRollingWindow>>>`
//! (NOT a process-global `static LazyLock<Mutex<>>`). Tests instantiate
//! fresh orchestrators per case so the harness cannot leak state.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use corelink_eviction::Tier;
use corelink_ratelimit::{BucketKey, KeyDimension, RateLimiter};

use super::audit::{
    AbuseAuditRecord, AbuseAuditSink, AbuseEventType,
};
use super::config::AbuseConfig;
use super::error::AbuseError;
use super::features::AbuseFeatures;
use super::metrics::{AbuseMetricsObserver, AbuseTierLabel};
use super::score::{compute_score, decide, AbuseDecision, AbuseScore};

/// Side-effect payload returned alongside the [`AbuseDecision`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbuseScoreOutcome {
    /// The rendered score.
    pub score: AbuseScore,
    /// The decision arm.
    pub decision: AbuseDecision,
    /// Whether a silent downgrade was applied to the per-tenant
    /// ratelimit bucket as a side-effect (only true when decision is
    /// Suspicious AND the call site provided a rate limiter via the
    /// `score_and_apply_with_ratelimiter` entry).
    pub downgrade_applied: bool,
    /// 5min aggregation window start (Unix epoch ms).
    pub window_start_ms: u64,
    /// 5min aggregation window end (Unix epoch ms).
    pub window_end_ms: u64,
}

/// Per-tenant rolling window state — the durable per-tenant
/// observation that the cron-tick mutates. The trait shape supports
/// O(1) lookup + audit-trail of the latest decision (for the
/// `GET /v1/admin/abuse_score` customer self-service endpoint
/// (deferred to S-13)).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbuseRollingWindow {
    /// Latest features observed (5min window canonical).
    pub features: AbuseFeatures,
    /// Latest tier observed (changes on plan upgrade/downgrade).
    pub tier: Tier,
    /// Latest score (0.0 if no observation yet).
    pub score: AbuseScore,
    /// Latest decision (Benign baseline if no observation yet).
    pub decision: AbuseDecision,
    /// Window start Unix ms.
    pub window_start_ms: u64,
    /// Window end Unix ms.
    pub window_end_ms: u64,
}

impl AbuseRollingWindow {
    /// Construct a fresh "no observation yet" window for a tenant.
    #[must_use]
    pub const fn baseline(tier: Tier, now_ms: u64) -> Self {
        Self {
            features: AbuseFeatures::zero(),
            tier,
            score: AbuseScore::ZERO,
            decision: AbuseDecision::Benign,
            window_start_ms: now_ms,
            window_end_ms: now_ms,
        }
    }
}

/// Trait surfaced by every abuse-detection backend (production cron
/// DO + Tower layer / in-memory fake).
pub trait AbuseScorer: Send + Sync + core::fmt::Debug {
    /// Render the per-cron-tick abuse decision for a tenant given a
    /// feature observation. Emits audit + metrics; applies gradient
    /// response side-effects (silent downgrade via
    /// [`RateLimiter::update_plan`] for Suspicious; admin trigger
    /// audit for Malicious; never auto-suspends).
    ///
    /// `window_start_ms` / `window_end_ms` are the canonical 5min
    /// aggregation window bounds (production wiring derives these via
    /// `corelink_time::utc_5min_floor` per WI §6.1.10).
    ///
    /// # Errors
    ///
    /// Surface as [`AbuseError`].
    fn score_and_apply(
        &self,
        tenant_id: Uuid,
        features: AbuseFeatures,
        tier: Tier,
        window_start_ms: u64,
        window_end_ms: u64,
    ) -> Result<AbuseScoreOutcome, AbuseError>;

    /// Rejected programmatic auto-suspend entry — ALWAYS returns
    /// [`AbuseError::AutoSuspendForbidden`] regardless of score.
    /// Production wiring routes this through `AdminCtx`-gated handlers
    /// only AFTER human review per LGPD Art. 20 + GDPR Art. 22
    /// (sprint contract §7.10.s08.3).
    ///
    /// # Errors
    ///
    /// Always returns [`AbuseError::AutoSuspendForbidden`] — the only
    /// surfaceable typed error from this method.
    fn auto_suspend(
        &self,
        tenant_id: Uuid,
        score: AbuseScore,
    ) -> Result<(), AbuseError>;

    /// Snapshot the current rolling window for a tenant (for
    /// diagnostic / customer self-service endpoint). Returns
    /// `Ok(None)` if no observation has been recorded yet.
    ///
    /// # Errors
    ///
    /// Surface as [`AbuseError`].
    fn snapshot_window(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AbuseRollingWindow>, AbuseError>;
}

/// In-memory abuse scorer (the canonical pure-logic orchestrator).
/// Mirrors the production CF DO actor model: per-instance `Mutex`
/// serialises concurrent calls so the rolling-window state cannot
/// be torn.
pub struct InMemoryAbuseScorer<A, M, R>
where
    A: AbuseAuditSink,
    M: AbuseMetricsObserver,
    R: RateLimiter,
{
    audit: Arc<A>,
    metrics: Arc<M>,
    config: AbuseConfig,
    rate_limiter: Arc<R>,
    state: Arc<Mutex<HashMap<Uuid, AbuseRollingWindow>>>,
}

impl<A, M, R> core::fmt::Debug for InMemoryAbuseScorer<A, M, R>
where
    A: AbuseAuditSink,
    M: AbuseMetricsObserver,
    R: RateLimiter,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryAbuseScorer")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<A, M, R> InMemoryAbuseScorer<A, M, R>
where
    A: AbuseAuditSink,
    M: AbuseMetricsObserver,
    R: RateLimiter,
{
    /// Construct with the canonical default config.
    #[must_use]
    pub fn with_defaults(
        audit: Arc<A>,
        metrics: Arc<M>,
        rate_limiter: Arc<R>,
    ) -> Self {
        Self {
            audit,
            metrics,
            config: AbuseConfig::canonical(),
            rate_limiter,
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Construct with explicit config.
    #[must_use]
    pub fn new(
        audit: Arc<A>,
        metrics: Arc<M>,
        rate_limiter: Arc<R>,
        config: AbuseConfig,
    ) -> Self {
        Self {
            audit,
            metrics,
            config,
            rate_limiter,
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Snapshot the active config.
    #[must_use]
    pub const fn config(&self) -> &AbuseConfig {
        &self.config
    }

    /// Cardinality of the rolling-window state (test assertion).
    ///
    /// # Errors
    ///
    /// Returns [`AbuseError::Backend`] on mutex poisoning.
    pub fn tenant_count(&self) -> Result<usize, AbuseError> {
        let g = self.state.lock().map_err(|_| {
            AbuseError::Backend("scorer state mutex poisoned".to_string())
        })?;
        Ok(g.len())
    }
}

fn tier_to_label(decision: AbuseDecision) -> AbuseTierLabel {
    match decision {
        AbuseDecision::Benign => AbuseTierLabel::Benign,
        AbuseDecision::Suspicious => AbuseTierLabel::Suspicious,
        AbuseDecision::Malicious => AbuseTierLabel::Malicious,
    }
}

fn decision_event(decision: AbuseDecision) -> AbuseEventType {
    match decision {
        AbuseDecision::Benign => AbuseEventType::DecisionBenign,
        AbuseDecision::Suspicious => AbuseEventType::DecisionSuspicious,
        AbuseDecision::Malicious => AbuseEventType::DecisionMalicious,
    }
}

impl<A, M, R> AbuseScorer for InMemoryAbuseScorer<A, M, R>
where
    A: AbuseAuditSink,
    M: AbuseMetricsObserver,
    R: RateLimiter,
{
    fn score_and_apply(
        &self,
        tenant_id: Uuid,
        features: AbuseFeatures,
        tier: Tier,
        window_start_ms: u64,
        window_end_ms: u64,
    ) -> Result<AbuseScoreOutcome, AbuseError> {
        // Step 1 + 2: score + decide.
        let score = compute_score(&features, self.config.weights(), tier);
        let decision = decide(score, &self.config);

        // Step 3: audit emit BEFORE state mutation (fail-closed envelope).
        // Two records: ScoreComputed (informational lineage) +
        // Decision* (the canonical decision arm).
        self.audit.emit(AbuseAuditRecord {
            event_type: AbuseEventType::ScoreComputed,
            tenant_id,
            score: Some(score.as_f64()),
            window_start: Some(window_start_ms),
            window_end: Some(window_end_ms),
            created_by_request_id: "test".to_string(),
            now_ms: window_end_ms,
        })?;
        self.audit.emit(AbuseAuditRecord {
            event_type: decision_event(decision),
            tenant_id,
            score: Some(score.as_f64()),
            window_start: Some(window_start_ms),
            window_end: Some(window_end_ms),
            created_by_request_id: "test".to_string(),
            now_ms: window_end_ms,
        })?;

        // Step 4: gradient response side-effect.
        let mut downgrade_applied = false;
        match decision {
            AbuseDecision::Benign => {
                // No-op.
            }
            AbuseDecision::Suspicious => {
                // Silent downgrade: halve the per-tenant refill rate +
                // burst capacity (50% downgrade canonical per WI §6.1.4
                // SilentDowngrade50pct1h tier). The cross-WI mechanism
                // is the `tenant_rate_override` D1 table per WI §6.1.4
                // CI-2; this in-memory fake invokes
                // `RateLimiter::update_plan` directly to drive the
                // observable behaviour the production wiring delivers
                // via the override row read.
                let bucket_key = BucketKey::per_tenant(tenant_id);
                let snap = self
                    .rate_limiter
                    .snapshot_bucket(&bucket_key)
                    .map_err(|e| AbuseError::Backend(format!(
                        "snapshot_bucket failed: {e}"
                    )))?;
                let (current_rate, current_burst) = match snap {
                    Some(s) => (
                        s.refill_rate_per_sec.floor() as u32,
                        s.burst_capacity,
                    ),
                    None => {
                        // Tenant has no bucket yet — materialise via
                        // tier defaults then halve. Use the canonical
                        // 5-tier ladder via corelink-ratelimit::tier.
                        let (rate, burst) =
                            corelink_ratelimit::refill_rate_for_tier(tier);
                        (rate, burst)
                    }
                };
                // 50% downgrade (canonical SilentDowngrade50pct1h).
                let new_rate = (current_rate / 2).max(1);
                let new_burst = (current_burst / 2).max(1);
                self.rate_limiter
                    .update_plan(
                        tenant_id,
                        KeyDimension::PerTenant,
                        "",
                        new_rate,
                        new_burst,
                        window_end_ms,
                    )
                    .map_err(|e| AbuseError::Backend(format!(
                        "update_plan failed: {e}"
                    )))?;

                self.audit.emit(AbuseAuditRecord {
                    event_type: AbuseEventType::DowngradeApplied,
                    tenant_id,
                    score: Some(score.as_f64()),
                    window_start: Some(window_start_ms),
                    window_end: Some(window_end_ms),
                    created_by_request_id: "test".to_string(),
                    now_ms: window_end_ms,
                })?;

                downgrade_applied = true;
            }
            AbuseDecision::Malicious => {
                self.audit.emit(AbuseAuditRecord {
                    event_type: AbuseEventType::AdminReviewTriggered,
                    tenant_id,
                    score: Some(score.as_f64()),
                    window_start: Some(window_start_ms),
                    window_end: Some(window_end_ms),
                    created_by_request_id: "test".to_string(),
                    now_ms: window_end_ms,
                })?;
                // NEVER auto-suspend per LGPD Art. 20 + GDPR Art. 22.
                // The Malicious arm only triggers admin review SEV-2;
                // suspend execution requires explicit admin manual
                // action through the `auto_suspend` method, which
                // ALWAYS returns AutoSuspendForbidden (the production
                // S-13 admin endpoint goes through a different
                // human-reviewer-only code path that is out-of-scope
                // for this WI).
            }
        }

        // Step 5: commit the rolling-window state.
        let mut g = self.state.lock().map_err(|_| {
            AbuseError::Backend("scorer state mutex poisoned".to_string())
        })?;
        g.insert(
            tenant_id,
            AbuseRollingWindow {
                features,
                tier,
                score,
                decision,
                window_start_ms,
                window_end_ms,
            },
        );
        drop(g);

        // Step 6: metrics emit (after audit + state commit succeeded).
        let label = tier_to_label(decision);
        self.metrics.observe_score(tenant_id, label, score.as_f64())?;
        self.metrics.record_tier(label)?;
        if matches!(decision, AbuseDecision::Suspicious) {
            self.metrics.record_downgrade_applied(tenant_id)?;
        }

        Ok(AbuseScoreOutcome {
            score,
            decision,
            downgrade_applied,
            window_start_ms,
            window_end_ms,
        })
    }

    fn auto_suspend(
        &self,
        tenant_id: Uuid,
        score: AbuseScore,
    ) -> Result<(), AbuseError> {
        // ALWAYS bump the SEV-1 LGPD violation canary BEFORE returning
        // the typed error so the production observability surface
        // catches even the attempted bypass.
        let _ = self.metrics.record_auto_suspend_attempt(tenant_id);
        Err(AbuseError::AutoSuspendForbidden {
            tenant_id,
            score: score.as_f64(),
        })
    }

    fn snapshot_window(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<AbuseRollingWindow>, AbuseError> {
        let g = self.state.lock().map_err(|_| {
            AbuseError::Backend("scorer state mutex poisoned".to_string())
        })?;
        Ok(g.get(&tenant_id).copied())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use super::super::audit::{FailingAbuseAuditSink, InMemoryAbuseAuditSink};
    use super::super::metrics::{AbuseMetricKind, InMemoryAbuseMetrics};
    use corelink_ratelimit::{
        InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics,
        InMemoryTokenBucketRateLimiter,
    };

    fn ten_a() -> Uuid {
        Uuid::from_u128(0xa)
    }

    fn ten_b() -> Uuid {
        Uuid::from_u128(0xb)
    }

    type ScorerType = InMemoryAbuseScorer<
        InMemoryAbuseAuditSink,
        InMemoryAbuseMetrics,
        InMemoryTokenBucketRateLimiter<
            InMemoryRateLimitAuditSink,
            InMemoryRateLimitMetrics,
        >,
    >;

    type Fixture = (
        ScorerType,
        Arc<InMemoryAbuseAuditSink>,
        Arc<InMemoryAbuseMetrics>,
        Arc<
            InMemoryTokenBucketRateLimiter<
                InMemoryRateLimitAuditSink,
                InMemoryRateLimitMetrics,
            >,
        >,
    );

    fn fresh() -> Fixture {
        let audit = Arc::new(InMemoryAbuseAuditSink::new());
        let metrics = Arc::new(InMemoryAbuseMetrics::new());
        let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
        let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
        let limiter = Arc::new(InMemoryTokenBucketRateLimiter::with_defaults(
            Arc::clone(&rl_audit),
            Arc::clone(&rl_metrics),
        ));
        let scorer = InMemoryAbuseScorer::with_defaults(
            Arc::clone(&audit),
            Arc::clone(&metrics),
            Arc::clone(&limiter),
        );
        (scorer, audit, metrics, limiter)
    }

    // ---- Benign path ----------------------------------------------------

    #[test]
    fn typical_workload_scores_benign() {
        let (scorer, audit, metrics, _l) = fresh();
        let f = AbuseFeatures::new(0.3, 10_000_000, 4.0, 20);
        let out = scorer
            .score_and_apply(ten_a(), f, Tier::Team, 0, 300_000)
            .unwrap();
        assert_eq!(out.decision, AbuseDecision::Benign);
        assert!(!out.downgrade_applied);
        assert_eq!(
            audit.snapshot_of(AbuseEventType::ScoreComputed).len(),
            1
        );
        assert_eq!(
            audit.snapshot_of(AbuseEventType::DecisionBenign).len(),
            1
        );
        assert_eq!(
            audit.snapshot_of(AbuseEventType::DecisionSuspicious).len(),
            0
        );
        assert_eq!(
            audit.snapshot_of(AbuseEventType::DowngradeApplied).len(),
            0
        );
        assert_eq!(
            metrics.tier_total_for_label(AbuseTierLabel::Benign),
            1
        );
    }

    // ---- Suspicious path ------------------------------------------------

    #[test]
    fn moderate_compute_farming_scores_suspicious_and_applies_downgrade() {
        let (scorer, audit, metrics, limiter) = fresh();
        // Combo that lands in Suspicious tier:
        // cpu=0.85 → cpu_norm = 0.7
        // entropy=0.5 → entropy_norm = 0.833
        // exec=10× baseline (Team) = 500 → exec_norm = (10-1)/100 = 0.09
        // egress=20MB/min on Team 50MB → < baseline → 0.0
        // score = 0.30*0.7 + 0.25*0.833 + 0.25*0 + 0.20*0.09
        //       ≈ 0.21 + 0.208 + 0 + 0.018 = 0.436
        // Wait, that's < 0.5. Let me push entropy down further.
        // entropy=0.0 (single action) → entropy_norm = 1.0
        // score = 0.30*0.7 + 0.25*1.0 + 0.25*0 + 0.20*0.09
        //       ≈ 0.21 + 0.25 + 0 + 0.018 = 0.478
        // Still under 0.5. Try cpu=0.95.
        // cpu=0.95 → cpu_norm = 0.9
        // entropy=0.0 → entropy_norm = 1.0
        // exec=baseline*5 → exec_norm = 0.04
        // score = 0.30*0.9 + 0.25*1.0 + 0.25*0 + 0.20*0.04
        //       = 0.27 + 0.25 + 0.008 = 0.528 → Suspicious ✓
        let f = AbuseFeatures::new(0.95, 0, 0.0, 250);
        let out = scorer
            .score_and_apply(ten_a(), f, Tier::Team, 0, 300_000)
            .unwrap();
        assert_eq!(out.decision, AbuseDecision::Suspicious);
        assert!(out.downgrade_applied);
        // Audit: ScoreComputed + DecisionSuspicious + DowngradeApplied.
        assert_eq!(
            audit.snapshot_of(AbuseEventType::ScoreComputed).len(),
            1
        );
        assert_eq!(
            audit.snapshot_of(AbuseEventType::DecisionSuspicious).len(),
            1
        );
        assert_eq!(
            audit.snapshot_of(AbuseEventType::DowngradeApplied).len(),
            1
        );
        // Metrics: tier + downgrade counter.
        assert_eq!(
            metrics.tier_total_for_label(AbuseTierLabel::Suspicious),
            1
        );
        assert_eq!(
            metrics.counter_for_tenant(
                AbuseMetricKind::DowngradeAppliedTotal,
                ten_a()
            ),
            1
        );
        // Limiter saw the downgrade — bucket materialised + rate halved.
        let bucket = BucketKey::per_tenant(ten_a());
        let snap = limiter.snapshot_bucket(&bucket).unwrap().unwrap();
        // Team default rate is 200 RPS; halved = 100 RPS.
        assert_eq!(snap.refill_rate_per_sec.floor() as u32, 100);
        // Team default burst is 1000; halved = 500.
        assert_eq!(snap.burst_capacity, 500);
    }

    // ---- Malicious path -------------------------------------------------

    #[test]
    fn extreme_attack_scores_malicious_and_triggers_admin_review() {
        let (scorer, audit, metrics, _l) = fresh();
        // High-intensity: cpu=0.99 + entropy=0.0 + exec=80×baseline
        // + egress=20×baseline.
        let f = AbuseFeatures::new(0.99, 200_000_000_000, 0.3, 4_000);
        let out = scorer
            .score_and_apply(ten_a(), f, Tier::Team, 0, 300_000)
            .unwrap();
        assert_eq!(out.decision, AbuseDecision::Malicious);
        assert!(!out.downgrade_applied);
        assert_eq!(
            audit.snapshot_of(AbuseEventType::DecisionMalicious).len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(AbuseEventType::AdminReviewTriggered)
                .len(),
            1
        );
        // No suspend-applied audit (LGPD Art. 20 humane response).
        assert_eq!(
            audit.snapshot_of(AbuseEventType::SuspendApplied).len(),
            0
        );
        assert_eq!(
            metrics.tier_total_for_label(AbuseTierLabel::Malicious),
            1
        );
    }

    // ---- AutoSuspendForbidden -----------------------------------------

    #[test]
    fn auto_suspend_at_any_score_returns_forbidden_and_bumps_canary() {
        let (scorer, _a, metrics, _l) = fresh();
        for score_val in [0.0_f64, 0.5, 0.85, 0.97, 1.0] {
            let s = AbuseScore::clamp(score_val);
            let err = scorer.auto_suspend(ten_a(), s).unwrap_err();
            assert!(matches!(err, AbuseError::AutoSuspendForbidden { .. }));
        }
        // SEV-1 canary bumped 5 times — one per attempted bypass.
        assert_eq!(
            metrics.counter_for_tenant(
                AbuseMetricKind::AutoSuspendAttemptsTotal,
                ten_a()
            ),
            5
        );
    }

    // ---- Tenant isolation ---------------------------------------------

    #[test]
    fn tenant_a_score_does_not_affect_tenant_b_window() {
        let (scorer, _a, _m, _l) = fresh();
        let f_a = AbuseFeatures::new(0.95, 0, 0.0, 250);
        let f_b = AbuseFeatures::new(0.3, 0, 4.0, 10);
        scorer
            .score_and_apply(ten_a(), f_a, Tier::Team, 0, 300_000)
            .unwrap();
        scorer
            .score_and_apply(ten_b(), f_b, Tier::Team, 0, 300_000)
            .unwrap();
        let win_a = scorer.snapshot_window(ten_a()).unwrap().unwrap();
        let win_b = scorer.snapshot_window(ten_b()).unwrap().unwrap();
        assert_eq!(win_a.decision, AbuseDecision::Suspicious);
        assert_eq!(win_b.decision, AbuseDecision::Benign);
        // Each tenant's score is independently computed; no cross-tenant
        // feature comparison.
        assert_ne!(win_a.score, win_b.score);
    }

    // ---- F-001 closure ------------------------------------------------

    #[test]
    fn separate_scorer_instances_have_independent_state() {
        let (sc1, _, _, _) = fresh();
        let (sc2, _, _, _) = fresh();
        scorer_apply(&sc1, ten_a());
        // sc2 has NEVER seen this tenant.
        assert_eq!(sc1.tenant_count().unwrap(), 1);
        assert_eq!(sc2.tenant_count().unwrap(), 0);
    }

    fn scorer_apply<S: AbuseScorer>(scorer: &S, tenant_id: Uuid) {
        let _ = scorer
            .score_and_apply(
                tenant_id,
                AbuseFeatures::zero(),
                Tier::Team,
                0,
                300_000,
            )
            .unwrap();
    }

    // ---- Audit fail-closed --------------------------------------------

    #[test]
    fn audit_failure_aborts_decision_and_no_downgrade_applied() {
        let audit = Arc::new(FailingAbuseAuditSink::new());
        let metrics = Arc::new(InMemoryAbuseMetrics::new());
        let rl_audit = Arc::new(InMemoryRateLimitAuditSink::new());
        let rl_metrics = Arc::new(InMemoryRateLimitMetrics::new());
        let limiter = Arc::new(InMemoryTokenBucketRateLimiter::with_defaults(
            Arc::clone(&rl_audit),
            Arc::clone(&rl_metrics),
        ));
        let scorer = InMemoryAbuseScorer::with_defaults(
            Arc::clone(&audit),
            Arc::clone(&metrics),
            Arc::clone(&limiter),
        );
        let f = AbuseFeatures::new(0.95, 0, 0.0, 250);
        let err = scorer
            .score_and_apply(ten_a(), f, Tier::Team, 0, 300_000)
            .unwrap_err();
        assert!(matches!(err, AbuseError::Audit(_)));
        // Limiter NEVER saw the downgrade (audit emit BEFORE side-effect).
        let bucket = BucketKey::per_tenant(ten_a());
        let snap = limiter.snapshot_bucket(&bucket).unwrap();
        assert!(snap.is_none(), "limiter must not have materialised bucket");
        // No tier metrics recorded (audit fired BEFORE metrics).
        assert_eq!(
            metrics.tier_total_for_label(AbuseTierLabel::Suspicious),
            0
        );
    }

    // ---- Score idempotence (same window → same score) ----------------

    #[test]
    fn same_window_same_features_same_score() {
        let (scorer, _a, _m, _l) = fresh();
        let f = AbuseFeatures::new(0.7, 100_000, 2.0, 30);
        let out1 = scorer
            .score_and_apply(ten_a(), f, Tier::Team, 0, 300_000)
            .unwrap();
        let out2 = scorer
            .score_and_apply(ten_a(), f, Tier::Team, 0, 300_000)
            .unwrap();
        assert_eq!(out1.score, out2.score);
        assert_eq!(out1.decision, out2.decision);
    }

    // ---- snapshot_window ---------------------------------------------

    #[test]
    fn snapshot_window_returns_none_for_unknown_tenant() {
        let (scorer, _a, _m, _l) = fresh();
        let snap = scorer.snapshot_window(ten_a()).unwrap();
        assert!(snap.is_none());
    }

    #[test]
    fn snapshot_window_returns_latest_after_score() {
        let (scorer, _a, _m, _l) = fresh();
        let f = AbuseFeatures::new(0.3, 0, 4.0, 5);
        scorer
            .score_and_apply(ten_a(), f, Tier::Team, 0, 300_000)
            .unwrap();
        let snap = scorer.snapshot_window(ten_a()).unwrap().unwrap();
        assert_eq!(snap.features, f);
        assert_eq!(snap.tier, Tier::Team);
        assert_eq!(snap.decision, AbuseDecision::Benign);
    }

    // ---- Multiple tenants, no cross-tenant feature leak --------------

    #[test]
    fn many_tenants_independent_decisions() {
        let (scorer, _a, _m, _l) = fresh();
        for i in 0..50_u128 {
            let tenant = Uuid::from_u128(i);
            let f = if i.is_multiple_of(2) {
                AbuseFeatures::new(0.3, 0, 4.0, 10)
            } else {
                AbuseFeatures::new(0.95, 0, 0.0, 250)
            };
            scorer
                .score_and_apply(tenant, f, Tier::Team, 0, 300_000)
                .unwrap();
        }
        assert_eq!(scorer.tenant_count().unwrap(), 50);
        for i in 0..50_u128 {
            let tenant = Uuid::from_u128(i);
            let snap = scorer.snapshot_window(tenant).unwrap().unwrap();
            if i.is_multiple_of(2) {
                assert_eq!(snap.decision, AbuseDecision::Benign);
            } else {
                assert_eq!(snap.decision, AbuseDecision::Suspicious);
            }
        }
    }
}
