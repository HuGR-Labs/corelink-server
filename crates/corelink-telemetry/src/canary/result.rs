//! Canary loop result + decision + observability health report types.
//!
//! ## Decision arms
//!
//! Per WI-S09-007 §1 invariant 2 + §1 invariant 3:
//!
//! - `Pass` — every assertion ladder bound met + every observability
//!   stack component healthy.
//! - `Degraded` — one or more latency ceiling missed (informational
//!   degradation; SEV-2 alert source after 3 consecutive in same
//!   region) but BLAKE3 digest match holds AND observability components
//!   remain healthy.
//! - `FailedRegion` — BLAKE3 digest mismatch (data integrity issue;
//!   SEV-1 alert) OR observability stack component unhealthy (SEV-3
//!   alert) OR canary cron drift > 90s sustained (SEV-3 alert).

use crate::canary::assertion::{AssertionCeilings, CanaryLatenciesMs};
use crate::canary::region::CanaryRegion;

/// Canonical canary loop decision taxonomy. The `#[non_exhaustive]`
/// marker reserves additive growth for follow-on sprints (e.g. S-14
/// enterprise tier custom decision arms / S-13 admin manual override).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CanaryDecision {
    /// Every assertion ladder bound met; every observability stack
    /// component healthy. Canary loop counted toward 72h ship-gate
    /// target.
    Pass,
    /// One or more latency ceiling missed; BLAKE3 digest match holds;
    /// observability stack components remain healthy. SEV-2 alert
    /// source after 3 consecutive `Degraded` decisions in the same
    /// region.
    Degraded,
    /// BLAKE3 digest mismatch (data integrity issue; SEV-1 alert) OR
    /// observability stack component unhealthy (SEV-3 alert) OR
    /// canary cron drift > 90s (SEV-3 alert). Canary loop NOT counted
    /// toward 72h ship-gate target.
    FailedRegion,
}

impl CanaryDecision {
    /// Canonical slug used in audit records + alert annotations.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Degraded => "degraded",
            Self::FailedRegion => "failed_region",
        }
    }

    /// Whether this decision counts toward the 72h sustained loop
    /// ship-gate target per WI-S09-007 §6.1.5 + sprint contract §6
    /// DoD.
    #[must_use]
    pub const fn counts_toward_ship_gate(self) -> bool {
        matches!(self, Self::Pass)
    }
}

impl core::fmt::Display for CanaryDecision {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.slug())
    }
}

/// Canonical 3-element decision list — pinned at the type system
/// layer for surface-stability regression tests.
#[must_use]
pub const fn canonical_canary_decisions() -> &'static [CanaryDecision; 3] {
    &[
        CanaryDecision::Pass,
        CanaryDecision::Degraded,
        CanaryDecision::FailedRegion,
    ]
}

/// Observability stack component canonical taxonomy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum HealthComponent {
    /// Mimir tenant ingest path (Logpush → Mimir SLA p99 ≤ 30s).
    Mimir,
    /// Loki tenant query path (LogQL p99 ≤ 5s hot tier).
    Loki,
    /// Tempo tenant trace visibility (BatchSpanProcessor flush p99 ≤
    /// 30s).
    Tempo,
    /// Grafana dashboard refresh latency (p99 ≤ 3s).
    Dashboard,
}

impl HealthComponent {
    /// Canonical slug.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Mimir => "mimir",
            Self::Loki => "loki",
            Self::Tempo => "tempo",
            Self::Dashboard => "dashboard",
        }
    }
}

impl core::fmt::Display for HealthComponent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.slug())
    }
}

/// Canonical 4-element health-component list.
#[must_use]
pub const fn canonical_health_components() -> &'static [HealthComponent; 4] {
    &[
        HealthComponent::Mimir,
        HealthComponent::Loki,
        HealthComponent::Tempo,
        HealthComponent::Dashboard,
    ]
}

/// Observability stack health snapshot per WI-S09-007 §6.1.4 +
/// `observability_model.md §12`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservabilityHealthReport {
    /// Mimir tenant ingest lag p99 in ms (canonical SLO ceiling
    /// 30_000).
    pub mimir_ingest_lag_ms: u32,
    /// Loki query p99 in ms (canonical SLO ceiling 5_000 hot tier).
    pub loki_query_p99_ms: u32,
    /// Tempo trace visibility lag p99 in ms (canonical SLO ceiling
    /// 30_000 BatchSpanProcessor flush).
    pub tempo_trace_lag_ms: u32,
    /// Grafana dashboard refresh p99 in ms (canonical SLO ceiling
    /// 3_000).
    pub dashboard_refresh_p99_ms: u32,
}

impl ObservabilityHealthReport {
    /// Whether every component meets its canonical SLO ceiling.
    #[must_use]
    pub const fn all_components_healthy(&self) -> bool {
        self.mimir_ingest_lag_ms <= 30_000
            && self.loki_query_p99_ms <= 5_000
            && self.tempo_trace_lag_ms <= 30_000
            && self.dashboard_refresh_p99_ms <= 3_000
    }

    /// Identify the first component breaching its canonical SLO
    /// ceiling. Returns `None` if every component is healthy.
    #[must_use]
    pub const fn first_unhealthy(&self) -> Option<HealthComponent> {
        if self.mimir_ingest_lag_ms > 30_000 {
            return Some(HealthComponent::Mimir);
        }
        if self.loki_query_p99_ms > 5_000 {
            return Some(HealthComponent::Loki);
        }
        if self.tempo_trace_lag_ms > 30_000 {
            return Some(HealthComponent::Tempo);
        }
        if self.dashboard_refresh_p99_ms > 3_000 {
            return Some(HealthComponent::Dashboard);
        }
        None
    }

    /// Healthy snapshot fixture for tests.
    #[must_use]
    pub const fn healthy_fixture() -> Self {
        Self {
            mimir_ingest_lag_ms: 1_000,
            loki_query_p99_ms: 100,
            tempo_trace_lag_ms: 1_000,
            dashboard_refresh_p99_ms: 100,
        }
    }
}

/// Canonical canary loop result snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanaryLoopResult {
    /// Region where the canary loop executed.
    pub region: CanaryRegion,
    /// Producer-side wall-clock instant (Unix epoch ms) at loop end.
    pub now_ms: u64,
    /// Observed dispatch lag relative to the canonical 60s cron
    /// interval, in ms (informational; > 90_000 = SEV-3 alert source).
    pub dispatch_lag_ms: u64,
    /// Observed canary loop latencies (CAS PUT / CAS GET / AC
    /// LOOKUP).
    pub latencies: CanaryLatenciesMs,
    /// Whether the BLAKE3 digest match held for this loop.
    pub digest_match: bool,
    /// Observed observability stack health snapshot.
    pub observability_health: ObservabilityHealthReport,
    /// Canonical decision computed from the assertion ladder + digest
    /// match + observability health + dispatch lag threshold.
    pub decision: CanaryDecision,
}

impl CanaryLoopResult {
    /// Compute the canonical [`CanaryDecision`] from a
    /// `(region, latencies, digest_match, health, dispatch_lag_ms)`
    /// tuple per the WI-S09-007 §1 invariant ladder.
    ///
    /// The ordering matters: `FailedRegion` arms (digest mismatch /
    /// observability unhealthy / dispatch lag > 90s) take precedence
    /// over latency ceilings; the latter trigger `Degraded` only.
    #[must_use]
    pub fn compute_decision(
        latencies: CanaryLatenciesMs,
        ceilings: AssertionCeilings,
        digest_match: bool,
        health: ObservabilityHealthReport,
        dispatch_lag_ms: u64,
    ) -> CanaryDecision {
        if !digest_match {
            return CanaryDecision::FailedRegion;
        }
        if !health.all_components_healthy() {
            return CanaryDecision::FailedRegion;
        }
        if dispatch_lag_ms > crate::canary::config::DISPATCH_LAG_SEV3_MS {
            return CanaryDecision::FailedRegion;
        }
        if latencies.within_ceilings(ceilings) {
            CanaryDecision::Pass
        } else {
            CanaryDecision::Degraded
        }
    }
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
    fn decision_slugs_pinned() {
        assert_eq!(CanaryDecision::Pass.slug(), "pass");
        assert_eq!(CanaryDecision::Degraded.slug(), "degraded");
        assert_eq!(CanaryDecision::FailedRegion.slug(), "failed_region");
    }

    #[test]
    fn ship_gate_count_only_for_pass() {
        assert!(CanaryDecision::Pass.counts_toward_ship_gate());
        assert!(!CanaryDecision::Degraded.counts_toward_ship_gate());
        assert!(!CanaryDecision::FailedRegion.counts_toward_ship_gate());
    }

    #[test]
    fn canonical_decisions_three_unique_arms() {
        let v = canonical_canary_decisions();
        assert_eq!(v.len(), 3);
        let mut set = std::collections::HashSet::new();
        for d in v {
            assert!(set.insert(d.slug()));
        }
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn canonical_health_components_four_unique() {
        let v = canonical_health_components();
        assert_eq!(v.len(), 4);
        let mut set = std::collections::HashSet::new();
        for c in v {
            assert!(set.insert(c.slug()));
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn observability_health_canonical_ceilings() {
        let healthy = ObservabilityHealthReport::healthy_fixture();
        assert!(healthy.all_components_healthy());
        assert_eq!(healthy.first_unhealthy(), None);

        let mut bad = healthy;
        bad.mimir_ingest_lag_ms = 30_001;
        assert!(!bad.all_components_healthy());
        assert_eq!(bad.first_unhealthy(), Some(HealthComponent::Mimir));

        let mut bad = healthy;
        bad.loki_query_p99_ms = 5_001;
        assert_eq!(bad.first_unhealthy(), Some(HealthComponent::Loki));

        let mut bad = healthy;
        bad.tempo_trace_lag_ms = 30_001;
        assert_eq!(bad.first_unhealthy(), Some(HealthComponent::Tempo));

        let mut bad = healthy;
        bad.dashboard_refresh_p99_ms = 3_001;
        assert_eq!(bad.first_unhealthy(), Some(HealthComponent::Dashboard));
    }

    #[test]
    fn compute_decision_pass_arm() {
        let d = CanaryLoopResult::compute_decision(
            CanaryLatenciesMs::new(50, 25, 15),
            AssertionCeilings::canonical(),
            true,
            ObservabilityHealthReport::healthy_fixture(),
            10,
        );
        assert_eq!(d, CanaryDecision::Pass);
    }

    #[test]
    fn compute_decision_degraded_on_latency_breach() {
        let d = CanaryLoopResult::compute_decision(
            CanaryLatenciesMs::new(101, 25, 15),
            AssertionCeilings::canonical(),
            true,
            ObservabilityHealthReport::healthy_fixture(),
            10,
        );
        assert_eq!(d, CanaryDecision::Degraded);
    }

    #[test]
    fn compute_decision_failed_region_on_digest_mismatch() {
        let d = CanaryLoopResult::compute_decision(
            CanaryLatenciesMs::new(50, 25, 15),
            AssertionCeilings::canonical(),
            false,
            ObservabilityHealthReport::healthy_fixture(),
            10,
        );
        assert_eq!(d, CanaryDecision::FailedRegion);
    }

    #[test]
    fn compute_decision_failed_region_on_observability_unhealthy() {
        let mut bad = ObservabilityHealthReport::healthy_fixture();
        bad.mimir_ingest_lag_ms = 30_001;
        let d = CanaryLoopResult::compute_decision(
            CanaryLatenciesMs::new(50, 25, 15),
            AssertionCeilings::canonical(),
            true,
            bad,
            10,
        );
        assert_eq!(d, CanaryDecision::FailedRegion);
    }

    #[test]
    fn compute_decision_failed_region_on_dispatch_lag() {
        let d = CanaryLoopResult::compute_decision(
            CanaryLatenciesMs::new(50, 25, 15),
            AssertionCeilings::canonical(),
            true,
            ObservabilityHealthReport::healthy_fixture(),
            90_001,
        );
        assert_eq!(d, CanaryDecision::FailedRegion);
    }

    #[test]
    fn digest_mismatch_takes_precedence_over_latency() {
        let d = CanaryLoopResult::compute_decision(
            CanaryLatenciesMs::new(101, 51, 31),
            AssertionCeilings::canonical(),
            false,
            ObservabilityHealthReport::healthy_fixture(),
            10,
        );
        assert_eq!(d, CanaryDecision::FailedRegion);
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", CanaryDecision::Pass), "pass");
        assert_eq!(format!("{}", HealthComponent::Mimir), "mimir");
    }
}
