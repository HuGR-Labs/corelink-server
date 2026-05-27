//! `corelink-abuse` — Heuristic abuse-detection scoring (WI-S08-004).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **canonical
//! pure-logic skeleton** of the S-08 CAP-ABUSE-001 + CAP-ABUSE-002
//! heuristic abuse-detection plane (camada 4 of the 4-layer rate-limit
//! bulkhead PAT-RATE-LIMIT-001):
//!
//! 1. The canonical SQL artifact `migrations/d1/0013_abuse_scores.sql`
//!    embedded via [`MIGRATION_0013_ABUSE_SCORES`] so production code
//!    can pass the DDL to `wrangler d1 migrations apply` without
//!    re-reading from disk. Schema mirrors the per-tenant durable score
//!    history with 4-feature breakdown + rendered decision tier.
//! 2. The [`features`] module ships [`AbuseFeatures`] — the FROZEN
//!    4-feature canonical observation shape (`cpu_wallclock_ratio` +
//!    `egress_bytes_per_min` + `action_digest_entropy_bits` +
//!    `concurrent_exec_count` per sprint contract §5 R-S08-7 + WI
//!    §6.1.3) with NaN-safe constructor.
//! 3. The [`score`] module ships [`AbuseScore`] (clamped `[0.0, 1.0]`
//!    newtype), [`AbuseDecision`] `#[non_exhaustive]` (Benign /
//!    Suspicious / Malicious 3-arm gradient), [`compute_score`] (the
//!    canonical 4-feature weighted-sum formula with tier-aware
//!    baselines), and [`decide`] (threshold map: ≥ MALICIOUS_THRESHOLD
//!    → Malicious; ≥ SUSPICIOUS_THRESHOLD → Suspicious; else Benign).
//! 4. The [`config`] module ships [`AbuseConfig`] (per-instance F-001
//!    closure; thresholds + weights), [`AbuseFeatureWeights`] (canonical
//!    0.30/0.25/0.25/0.20 per WI §6.1.3 sum-to-one defensive validator),
//!    and [`egress_baseline_for_tier`] / [`exec_baseline_for_tier`]
//!    (5-tier canonical baselines per Lote 10.7bis P0-7).
//! 5. The [`audit`] module ships [`AbuseEventType`]
//!    (`#[non_exhaustive]` 7-event taxonomy:
//!    `corelink.abuse.{score_computed, decision_benign,
//!    decision_suspicious, decision_malicious, downgrade_applied,
//!    admin_review_triggered, suspend_applied}`) +
//!    [`AbuseAuditSink`] + [`InMemoryAbuseAuditSink`] + fail-closed
//!    envelope per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.
//! 6. The [`metrics`] module ships [`AbuseMetricsObserver`] +
//!    [`InMemoryAbuseMetrics`] + canonical 5-metric ladder
//!    (`corelink.abuse.{score, tier_count_total,
//!    auto_suspend_attempts_total, downgrade_applied_total,
//!    cross_tenant_feature_leak_total}`).
//! 7. The [`error`] module ships [`AbuseError`] `#[non_exhaustive]`.
//!    `AutoSuspendForbidden` is the LGPD Art. 20 + GDPR Art. 22 humane
//!    response canary — programmatic auto-suspend ALWAYS hits this arm
//!    regardless of score.
//! 8. The [`scorer`] module ships [`AbuseScorer`] trait +
//!    [`InMemoryAbuseScorer`] orchestrator wired to the audit + metrics
//!    + ratelimit dependencies. Per-instance Arc-Mutex-HashMap mirrors
//!    the production CF DO actor model byte-for-byte; F-001 closure
//!    preserved (see source for the canonical type).
//!
//! # Why heurística NOT ML
//!
//! Sprint contract §10 anti-scope ML; LGPD Art. 20 transparency
//! (customer can inspect features + weights + contributions). 4
//! features specifically chosen to cover compute farming + scraping +
//! spam + flood without ML opacity. ML deferred S-14+ if heurística
//! insuficiente.
//!
//! # 4-tier humane response gradient (LGPD Art. 20 + GDPR Art. 22)
//!
//! Per sprint contract §7.10.s08.3 humane response (Lote 9.4 Opus H-06
//! alignment), the response is gradient:
//!
//! - **Benign** (score < SUSPICIOUS_THRESHOLD = 0.5): no action.
//! - **Suspicious** (score in [0.5, 0.8)): silent downgrade — refill
//!   rate halved via `tenant_rate_override` mechanism (cross-WI
//!   integration with `corelink-ratelimit::RateLimiter::update_plan`).
//!   Auto-recoverable when score drops next cycle.
//! - **Malicious** (score in [0.8, 0.95)): admin review trigger SEV-2
//!   PagerDuty notification.
//! - **Suspend candidate** (score ≥ 0.95): SEV-1 alert; admin manual
//!   review required ≤ 24h SLA. **NEVER auto-applied per LGPD Art. 20
//!   + GDPR Art. 22**; programmatic [`AbuseScorer::auto_suspend`]
//!   attempts ALWAYS hit [`AbuseError::AutoSuspendForbidden`].
//!
//! The orchestrator-level [`AbuseDecision`] enum collapses the upper
//! two tiers into `Malicious` (the suspend-candidate sub-tier is
//! distinguished by score magnitude carried alongside; production
//! wiring at WI-S08-006 expands the `#[non_exhaustive]` enum
//! additively).
//!
//! # Composition with WI-S08-001 + WI-S08-002 + WI-S08-003 (camadas 1-4)
//!
//! The 4-layer bulkhead PAT-RATE-LIMIT-001 composes:
//!
//! 1. **Camada 2 — `corelink-edge::EdgePolicy`** (WI-S08-002): pre-auth
//!    IP / CIDR blocklist; consulted FIRST; zero-cost adversarial drop.
//! 2. **Camada 1 — `corelink-ratelimit::RateLimiter`** (WI-S08-001):
//!    per-tenant + per-IP token bucket; consulted SECOND; aggregate
//!    enforcement.
//! 3. **Camada 3 — `corelink-quota-cas::AtomicQuotaChecker`**
//!    (WI-S08-003): per-tenant storage hard-block via atomic CAS at
//!    100% boundary; consulted THIRD; canonical days-until-month-reset
//!    Retry-After per ADR-0020 FROZEN.
//! 4. **Camada 4 — THIS crate `corelink-abuse::AbuseScorer`**
//!    (WI-S08-004): heuristic 4-feature weighted-sum scoring with 3-arm
//!    gradient response; consulted FOURTH (proactive layer detecting
//!    abusive patterns BEFORE SLO breach). NEVER auto-suspends per
//!    LGPD Art. 20.
//!
//! Production wiring at WI-S08-006 composes the four in this canonical
//! order. The cross-WI integration mechanism for the SilentDowngrade
//! tier is the new shared D1 table `tenant_rate_override` per WI §6.1.4
//! CI-2; this crate's in-memory orchestrator drives the observable
//! behaviour via a direct
//! [`corelink_ratelimit::RateLimiter::update_plan`] call which the
//! production wiring replaces with the override-row read-and-multiply
//! path.
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-TENANT-ISOLATION** (CRITICAL, TLA+): per-tenant scoring;
//!   NO cross-tenant feature comparison; pinned by
//!   `prop_tenant_isolation` + `prop_per_tenant_isolation_concurrent`.
//! - **INV-AVAIL-ISOLATION** (HIGH; spec_contract §8 +
//!   invariant_registry §3.8): abuse detection complements bulkhead
//!   camadas 1-3.
//! - **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+): every decision audit
//!   emitted BEFORE state mutation; pinned by
//!   `prop_audit_emit_per_decision_arm` +
//!   `audit_failure_aborts_decision_and_no_downgrade_applied`.
//! - **AbuseScore non-negativity + clamp**: score ∈ `[0.0, 1.0]`
//!   structurally; pinned by `prop_score_in_range_0_1`.
//! - **Score monotonicity in each feature**: each `*_norm` is
//!   non-decreasing in its input; each weight is non-negative; pinned
//!   by `prop_score_monotonic_in_features`.
//! - **Decision threshold semantics**: ≥ on each threshold; pinned by
//!   `prop_decision_threshold_boundary`.
//! - **AutoSuspendForbidden enforcement**: programmatic auto-suspend
//!   ALWAYS returns the typed error; pinned by
//!   `prop_auto_suspend_forbidden`.
//! - **Calibration target** (sprint contract §6 DoD; Lote 10.8bis
//!   P0-E corrected): n=50 benign + n=50 abusive synthetic workloads;
//!   95% CI FP rate ≤ 5% upper-bound + 95% CI TP rate ≥ 80%
//!   lower-bound. Pinned by the `calibration_abuse` test fixture.
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - **No `tokio`** in `src/` (wasm32-clean lib code; tokio only in
//!   tests if needed).
//! - **No process-global `static LazyLock<Mutex<>>`** — F-001 closure
//!   preserved via per-instance `Arc<Mutex<HashMap<…>>>`.
//! - **No ML / multi-tenant comparative scoring** (sprint contract §10
//!   anti-scope; LGPD Art. 20 transparency; LINDDUN linkability
//!   concerns).
//!
//! # Trait-abstraction-defer
//!
//! Real CF DO singleton `AbuseScoreCron-<region>`, real D1 atomic
//! batch, Tower-layer production wiring (gated
//! `corelink-worker/tower-middleware`), 100k nightly proptest sustained
//! 7d, chaos 11, customer self-service endpoints (S-13 dependency),
//! admin endpoints, appeal queue, and 30d shadow re-calibration
//! workflow — all consolidated alongside WI-S08-006 PRR ship gate.

#![allow(
    clippy::doc_lazy_continuation,
    clippy::doc_overindented_list_items,
    reason = "module-level docs use deep nested numbered/bulleted lists; \
              clippy's auto-detection is over-aggressive on the canonical \
              4-paragraph + 4-tier gradient prose"
)]

/// Embedded canonical migration SQL (D1 Cloudflare SQLite) for
/// `abuse_score_history` (WI-S08-004; per-tenant durable score time
/// series + 4-feature breakdown + rendered decision tier).
///
/// The exact bytes ship to production via `scripts/migrate_d1.sh` /
/// `wrangler d1 migrations apply`. The simulator does not parse this
/// string; the algorithmic invariants are re-implemented directly so
/// test failures are easy to triage.
pub const MIGRATION_0013_ABUSE_SCORES: &str =
    include_str!("../../../migrations/d1/0013_abuse_scores.sql");

pub mod audit;
pub mod config;
pub mod error;
pub mod features;
pub mod metrics;
pub mod score;
pub mod scorer;

pub use audit::{
    canonical_audit_event_strings, AbuseAuditRecord, AbuseAuditSink,
    AbuseAuditSinkError, AbuseEventType, FailingAbuseAuditSink,
    InMemoryAbuseAuditSink,
};
pub use config::{
    egress_baseline_for_tier, exec_baseline_for_tier, AbuseConfig,
    AbuseFeatureWeights, DEFAULT_CPU_WEIGHT, DEFAULT_EGRESS_WEIGHT,
    DEFAULT_ENTROPY_WEIGHT, DEFAULT_EXEC_WEIGHT, MALICIOUS_THRESHOLD,
    SUSPICIOUS_THRESHOLD,
};
pub use error::AbuseError;
pub use features::AbuseFeatures;
pub use metrics::{
    canonical_metric_names, AbuseMetricKind, AbuseMetricsObserver,
    AbuseMetricsObserverError, AbuseTierLabel, FailingAbuseMetrics,
    InMemoryAbuseMetrics,
};
pub use score::{compute_score, decide, AbuseDecision, AbuseScore};
pub use scorer::{
    AbuseRollingWindow, AbuseScoreOutcome, AbuseScorer,
    InMemoryAbuseScorer,
};

/// Returns the canonical schema version recorded by the latest
/// migration in the D1 `abuse` domain.
///
/// Version is sequential within the D1 domain (`blob_meta` = 1,
/// `ac_meta` = 2, …, `quota_reservations` = 9, `ratelimit_buckets` = 10,
/// `edge_blocklist` = 11, `quota_cas_attempts` = 12,
/// **`abuse_score_history` = 13**).
#[must_use]
pub const fn abuse_schema_version() -> u32 {
    13
}
