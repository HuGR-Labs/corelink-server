//! Canary probe orchestrator: composes assertion ladder + digest
//! match + observability health + dispatch lag detection behind the
//! canonical audit-emit-BEFORE-mutation fail-CLOSED envelope.
//!
//! ## Audit-fail-CLOSED envelope
//!
//! Per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (HIGH; lift from S-07
//! P1-1 fix plus Lote 10.6bis pattern): every canary loop emit + per-
//! region ledger advance is preceded by the corresponding audit emit;
//! audit failure aborts the canary path + returns
//! [`CanaryError::Audit`]. The envelope ordering is:
//!
//! 1. `LoopExecuted` audit emit (informational; surfaces region +
//!    decision + latencies + dispatch lag).
//! 2. Branch on `CanaryDecision::Degraded` → emit `AssertionFailed`
//!    audit per the first breaching assertion arm (Lote 10.7bis-
//!    discipline: every breach attributed to a canonical assertion).
//! 3. Branch on `CanaryDecision::FailedRegion`:
//!    - digest mismatch → emit `DigestMismatch` audit + return
//!      `CanaryError::DigestMismatch`.
//!    - observability unhealthy → emit `ObservabilityUnhealthy` audit
//!      + return `CanaryError::ObservabilityUnhealthy`.
//!    - dispatch lag > 90s → emit `DispatchLagExceeded` audit + return
//!      `CanaryError::DispatchLagExceeded`.
//! 4. Per-region ledger advance AFTER all emits committed (sustained-
//!    72h count gated on `CanaryDecision::counts_toward_ship_gate()`).
//!
//! ## Per-region ledger
//!
//! Per `INV-TENANT-ISOLATION` (synthetic-tenant variant) + WI-S09-007
//! §1 invariant 1: the orchestrator carries a per-region ledger so
//! concurrent canary execution on region A never perturbs region B's
//! sustained-loop count. The ledger lives under a per-instance
//! `Arc<Mutex<>>` (F-001 closure; NO process-global `static
//! LazyLock`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::canary::assertion::{AssertionCeilings, CanaryLatenciesMs};
use crate::canary::audit::{CanaryAuditRecord, CanaryAuditSink};
use crate::canary::config::{DISPATCH_LAG_SEV3_MS, SYNTHETIC_CANARY_TENANT_ID};
use crate::canary::error::CanaryError;
use crate::canary::region::CanaryRegion;
use crate::canary::result::{CanaryDecision, CanaryLoopResult, ObservabilityHealthReport};

/// Per-region ledger snapshot exposed by [`InMemoryCanaryProbe::stats`].
///
/// Production wiring sources the same shape from the durable D1 mirror
/// (per-region cumulative counters + last-decision timestamp).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegionProbeStats {
    /// Cumulative count of `Pass` decisions in this region.
    pub pass_loops: u64,
    /// Cumulative count of `Degraded` decisions in this region.
    pub degraded_loops: u64,
    /// Cumulative count of `FailedRegion` decisions in this region.
    pub failed_region_loops: u64,
    /// Number of consecutive non-`Pass` decisions (resets on every
    /// `Pass`). Used by the SEV-2 alert source `3+ consecutive`
    /// threshold per WI-S09-007 §1 invariant 8.
    pub consecutive_non_pass: u64,
}

impl RegionProbeStats {
    /// Total loops executed in this region.
    #[must_use]
    pub const fn total_loops(&self) -> u64 {
        self.pass_loops
            .saturating_add(self.degraded_loops)
            .saturating_add(self.failed_region_loops)
    }
}

#[derive(Default, Debug)]
struct LedgerEntry {
    stats: RegionProbeStats,
}

/// Canary probe trait. Production wiring composes:
///
/// - `WorkerCanaryProbe` — CF Workers cron-trigger wired probe via
///   `worker::Fetch` HTTP client + real R2 PUT/GET + AC lookup +
///   `blake3::hash` digest verify + Mimir/Loki/Tempo/Grafana health
///   probe HTTPS endpoints (deferred to S-20 GA gate per
///   `trait-abstraction-defer` charter pattern).
pub trait CanaryProbe: Send + Sync + core::fmt::Debug {
    /// Execute one canary loop for `region` per the canonical CAS PUT
    /// + GET + AC lookup + BLAKE3 verify flow + observability stack
    /// health probe + dispatch lag detection.
    ///
    /// # Errors
    ///
    /// - [`CanaryError::Audit`] on any audit sink failure (fail-
    ///   CLOSED).
    /// - [`CanaryError::DigestMismatch`] on BLAKE3 digest mismatch
    ///   (data integrity issue; SEV-1).
    /// - [`CanaryError::ObservabilityUnhealthy`] on any observability
    ///   stack component breach (SEV-3).
    /// - [`CanaryError::DispatchLagExceeded`] on canary cron drift >
    ///   90s (SEV-3).
    /// - [`CanaryError::AssertionFailed`] on any latency ceiling
    ///   breach (SEV-2 after 3 consecutive in same region).
    /// - [`CanaryError::Internal`] on mutex poisoning of the per-
    ///   region ledger.
    fn execute_canary_loop(
        &self,
        region: CanaryRegion,
        latencies: CanaryLatenciesMs,
        digest_match: bool,
        health: ObservabilityHealthReport,
        dispatch_lag_ms: u64,
        now_ms: u64,
    ) -> Result<CanaryLoopResult, CanaryError>;
}

/// In-memory canary probe orchestrator. Composes
/// [`AssertionCeilings`] (canonical SLO ladder) with a
/// [`CanaryAuditSink`] (audit-emit-BEFORE-mutation fail-CLOSED
/// envelope) and a per-region ledger under per-instance
/// `Arc<Mutex<>>` F-001 closure.
#[derive(Clone, Debug)]
pub struct InMemoryCanaryProbe<A>
where
    A: CanaryAuditSink,
{
    audit_sink: Arc<A>,
    ceilings: AssertionCeilings,
    ledger: Arc<Mutex<HashMap<CanaryRegion, LedgerEntry>>>,
}

impl<A> InMemoryCanaryProbe<A>
where
    A: CanaryAuditSink,
{
    /// Construct a fresh orchestrator bound to `audit_sink` with the
    /// canonical SLO ceilings ladder per WI-S09-007 §1 invariant 2.
    #[must_use]
    pub fn new(audit_sink: Arc<A>) -> Self {
        Self::with_ceilings(audit_sink, AssertionCeilings::canonical())
    }

    /// Construct a fresh orchestrator bound to `audit_sink` with
    /// custom assertion ceilings (used by S-14 enterprise tier
    /// custom-SLO fixture forward).
    #[must_use]
    pub fn with_ceilings(audit_sink: Arc<A>, ceilings: AssertionCeilings) -> Self {
        Self {
            audit_sink,
            ceilings,
            ledger: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Snapshot the per-region ledger entry. Returns the default
    /// `RegionProbeStats` if no loop has executed in `region` yet.
    ///
    /// # Errors
    ///
    /// Returns [`CanaryError::Internal`] on mutex poisoning.
    pub fn stats(&self, region: CanaryRegion) -> Result<RegionProbeStats, CanaryError> {
        let g = self
            .ledger
            .lock()
            .map_err(|_| CanaryError::Internal("per-region ledger mutex poisoned".to_string()))?;
        Ok(g.get(&region).map_or_else(RegionProbeStats::default, |e| e.stats))
    }

    /// Sum every region's `pass_loops` count to expose the cumulative
    /// 72h sustained-loop ship-gate progress per sprint contract §6
    /// DoD (target 12_960; Lote 10.9bis P0-A corrected from 38_880).
    ///
    /// # Errors
    ///
    /// Returns [`CanaryError::Internal`] on mutex poisoning.
    pub fn total_pass_loops(&self) -> Result<u64, CanaryError> {
        let g = self
            .ledger
            .lock()
            .map_err(|_| CanaryError::Internal("per-region ledger mutex poisoned".to_string()))?;
        Ok(g.values()
            .map(|e| e.stats.pass_loops)
            .fold(0u64, u64::saturating_add))
    }

    fn emit_audit(&self, record: CanaryAuditRecord) -> Result<(), CanaryError> {
        self.audit_sink.emit(record).map_err(CanaryError::from)
    }
}

impl<A> CanaryProbe for InMemoryCanaryProbe<A>
where
    A: CanaryAuditSink,
{
    fn execute_canary_loop(
        &self,
        region: CanaryRegion,
        latencies: CanaryLatenciesMs,
        digest_match: bool,
        health: ObservabilityHealthReport,
        dispatch_lag_ms: u64,
        now_ms: u64,
    ) -> Result<CanaryLoopResult, CanaryError> {
        let decision = CanaryLoopResult::compute_decision(
            latencies,
            self.ceilings,
            digest_match,
            health,
            dispatch_lag_ms,
        );
        let tenant_id = SYNTHETIC_CANARY_TENANT_ID.to_string();

        // 1. LoopExecuted audit emit (informational; before any error
        //    return so the verifier can reconstruct every loop.).
        self.emit_audit(CanaryAuditRecord::loop_executed(
            region,
            decision,
            tenant_id.clone(),
            dispatch_lag_ms,
            now_ms,
        ))?;

        // 2. Branch on decision arms; emit + (optionally) return.
        match decision {
            CanaryDecision::Pass => {}
            CanaryDecision::Degraded => {
                let breach = latencies.first_breach(self.ceilings).unwrap_or_default_breach();
                self.emit_audit(CanaryAuditRecord::assertion_failed(
                    region,
                    decision,
                    breach,
                    tenant_id.clone(),
                    dispatch_lag_ms,
                    now_ms,
                ))?;
            }
            CanaryDecision::FailedRegion => {
                if !digest_match {
                    self.emit_audit(CanaryAuditRecord::digest_mismatch(
                        region,
                        tenant_id.clone(),
                        dispatch_lag_ms,
                        now_ms,
                    ))?;
                    self.advance_ledger(region, decision)?;
                    return Err(CanaryError::DigestMismatch {
                        region,
                        written: "expected_canary_blob".to_string(),
                        read: "observed_blob_corruption".to_string(),
                    });
                }
                if let Some(component) = health.first_unhealthy() {
                    self.emit_audit(CanaryAuditRecord::observability_unhealthy(
                        region,
                        component,
                        tenant_id.clone(),
                        dispatch_lag_ms,
                        now_ms,
                    ))?;
                    self.advance_ledger(region, decision)?;
                    return Err(CanaryError::ObservabilityUnhealthy { region, component });
                }
                if dispatch_lag_ms > DISPATCH_LAG_SEV3_MS {
                    self.emit_audit(CanaryAuditRecord::dispatch_lag_exceeded(
                        region,
                        tenant_id.clone(),
                        dispatch_lag_ms,
                        now_ms,
                    ))?;
                    self.advance_ledger(region, decision)?;
                    return Err(CanaryError::DispatchLagExceeded {
                        region,
                        observed_ms: dispatch_lag_ms,
                        threshold_ms: DISPATCH_LAG_SEV3_MS,
                    });
                }
            }
        }

        // 3. Per-region ledger advance AFTER all emits committed.
        self.advance_ledger(region, decision)?;

        Ok(CanaryLoopResult {
            region,
            now_ms,
            dispatch_lag_ms,
            latencies,
            digest_match,
            observability_health: health,
            decision,
        })
    }
}

impl<A> InMemoryCanaryProbe<A>
where
    A: CanaryAuditSink,
{
    fn advance_ledger(
        &self,
        region: CanaryRegion,
        decision: CanaryDecision,
    ) -> Result<(), CanaryError> {
        let mut g = self
            .ledger
            .lock()
            .map_err(|_| CanaryError::Internal("per-region ledger mutex poisoned".to_string()))?;
        let entry = g.entry(region).or_default();
        match decision {
            CanaryDecision::Pass => {
                entry.stats.pass_loops = entry.stats.pass_loops.saturating_add(1);
                entry.stats.consecutive_non_pass = 0;
            }
            CanaryDecision::Degraded => {
                entry.stats.degraded_loops = entry.stats.degraded_loops.saturating_add(1);
                entry.stats.consecutive_non_pass =
                    entry.stats.consecutive_non_pass.saturating_add(1);
            }
            CanaryDecision::FailedRegion => {
                entry.stats.failed_region_loops =
                    entry.stats.failed_region_loops.saturating_add(1);
                entry.stats.consecutive_non_pass =
                    entry.stats.consecutive_non_pass.saturating_add(1);
            }
        }
        Ok(())
    }
}

trait FirstBreachOrDefault {
    fn unwrap_or_default_breach(self) -> crate::canary::assertion::CanaryAssertion;
}

impl FirstBreachOrDefault for Option<crate::canary::assertion::CanaryAssertion> {
    fn unwrap_or_default_breach(self) -> crate::canary::assertion::CanaryAssertion {
        // `Degraded` arm is reached only when at least one latency
        // ceiling is breached, so `first_breach` is always `Some(_)`.
        // The fallback arm is a defensive default that cannot occur
        // under correct call-site composition; use `CasPutP99` (the
        // most expensive ceiling) as the canonical "unknown breach"
        // fallback so the audit row remains structurally valid.
        self.unwrap_or(crate::canary::assertion::CanaryAssertion::CasPutP99)
    }
}

/// Always-failing canary probe for adversarial tests.
#[derive(Debug, Default)]
pub struct FailingCanaryProbe;

impl FailingCanaryProbe {
    /// Construct a fresh always-failing probe.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl CanaryProbe for FailingCanaryProbe {
    fn execute_canary_loop(
        &self,
        _region: CanaryRegion,
        _latencies: CanaryLatenciesMs,
        _digest_match: bool,
        _health: ObservabilityHealthReport,
        _dispatch_lag_ms: u64,
        _now_ms: u64,
    ) -> Result<CanaryLoopResult, CanaryError> {
        Err(CanaryError::Internal(
            "induced canary probe failure (test fixture)".to_string(),
        ))
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
    use crate::canary::audit::{CanaryAuditEventType, FailingCanaryAuditSink, InMemoryCanaryAuditSink};
    use crate::canary::result::HealthComponent;

    fn fresh_probe() -> InMemoryCanaryProbe<InMemoryCanaryAuditSink> {
        InMemoryCanaryProbe::new(Arc::new(InMemoryCanaryAuditSink::new()))
    }

    #[test]
    fn pass_decision_counts_toward_ship_gate() {
        let p = fresh_probe();
        let r = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(50, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                1,
            )
            .unwrap();
        assert_eq!(r.decision, CanaryDecision::Pass);
        let stats = p.stats(CanaryRegion::Enam).unwrap();
        assert_eq!(stats.pass_loops, 1);
        assert_eq!(stats.consecutive_non_pass, 0);
        assert_eq!(stats.total_loops(), 1);
    }

    #[test]
    fn degraded_decision_emits_assertion_failed_audit() {
        let sink = Arc::new(InMemoryCanaryAuditSink::new());
        let p = InMemoryCanaryProbe::new(Arc::clone(&sink));
        let _ = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(101, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                1,
            )
            .unwrap();
        let snap = sink.snapshot_of(CanaryAuditEventType::AssertionFailed);
        assert_eq!(snap.len(), 1);
        assert_eq!(snap.first().map(|r| r.assertion_slug), Some("cas_put_p99_ms"));
        let stats = p.stats(CanaryRegion::Enam).unwrap();
        assert_eq!(stats.degraded_loops, 1);
        assert_eq!(stats.consecutive_non_pass, 1);
    }

    #[test]
    fn digest_mismatch_returns_failed_region_error() {
        let sink = Arc::new(InMemoryCanaryAuditSink::new());
        let p = InMemoryCanaryProbe::new(Arc::clone(&sink));
        let err = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(50, 25, 15),
                false,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                1,
            )
            .unwrap_err();
        assert!(matches!(err, CanaryError::DigestMismatch { .. }));
        let snap = sink.snapshot_of(CanaryAuditEventType::DigestMismatch);
        assert_eq!(snap.len(), 1);
        let stats = p.stats(CanaryRegion::Enam).unwrap();
        assert_eq!(stats.failed_region_loops, 1);
    }

    #[test]
    fn observability_unhealthy_returns_failed_region_error() {
        let sink = Arc::new(InMemoryCanaryAuditSink::new());
        let p = InMemoryCanaryProbe::new(Arc::clone(&sink));
        let mut bad_health = ObservabilityHealthReport::healthy_fixture();
        bad_health.mimir_ingest_lag_ms = 30_001;
        let err = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(50, 25, 15),
                true,
                bad_health,
                10,
                1,
            )
            .unwrap_err();
        match err {
            CanaryError::ObservabilityUnhealthy { component, .. } => {
                assert_eq!(component, HealthComponent::Mimir);
            }
            _ => panic!("expected ObservabilityUnhealthy"),
        }
        let snap = sink.snapshot_of(CanaryAuditEventType::ObservabilityUnhealthy);
        assert_eq!(snap.len(), 1);
    }

    #[test]
    fn dispatch_lag_exceeded_returns_failed_region_error() {
        let sink = Arc::new(InMemoryCanaryAuditSink::new());
        let p = InMemoryCanaryProbe::new(Arc::clone(&sink));
        let err = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(50, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                90_001,
                1,
            )
            .unwrap_err();
        assert!(matches!(err, CanaryError::DispatchLagExceeded { .. }));
        let snap = sink.snapshot_of(CanaryAuditEventType::DispatchLagExceeded);
        assert_eq!(snap.len(), 1);
    }

    #[test]
    fn audit_failure_aborts_canary_path_fail_closed() {
        let sink = Arc::new(FailingCanaryAuditSink::new());
        let p = InMemoryCanaryProbe::new(sink);
        let err = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(50, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                1,
            )
            .unwrap_err();
        assert!(matches!(err, CanaryError::Audit(_)));
        // No ledger advance on audit failure.
        let stats = p.stats(CanaryRegion::Enam).unwrap();
        assert_eq!(stats.total_loops(), 0);
    }

    #[test]
    fn per_region_isolation_no_cross_contamination() {
        let p = fresh_probe();
        let _ = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(50, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                1,
            )
            .unwrap();
        let _ = p
            .execute_canary_loop(
                CanaryRegion::Weur,
                CanaryLatenciesMs::new(50, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                2,
            )
            .unwrap();
        assert_eq!(p.stats(CanaryRegion::Enam).unwrap().pass_loops, 1);
        assert_eq!(p.stats(CanaryRegion::Weur).unwrap().pass_loops, 1);
        assert_eq!(p.stats(CanaryRegion::Apac).unwrap().pass_loops, 0);
        assert_eq!(p.total_pass_loops().unwrap(), 2);
    }

    #[test]
    fn consecutive_non_pass_resets_on_pass() {
        let p = fresh_probe();
        // First: degraded.
        let _ = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(101, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                1,
            )
            .unwrap();
        // Second: degraded.
        let _ = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(102, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                2,
            )
            .unwrap();
        assert_eq!(
            p.stats(CanaryRegion::Enam).unwrap().consecutive_non_pass,
            2
        );
        // Third: pass — reset.
        let _ = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(50, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                3,
            )
            .unwrap();
        assert_eq!(
            p.stats(CanaryRegion::Enam).unwrap().consecutive_non_pass,
            0
        );
    }

    #[test]
    fn failing_canary_probe_returns_internal_error() {
        let p = FailingCanaryProbe::new();
        let err = p
            .execute_canary_loop(
                CanaryRegion::Enam,
                CanaryLatenciesMs::new(50, 25, 15),
                true,
                ObservabilityHealthReport::healthy_fixture(),
                10,
                1,
            )
            .unwrap_err();
        assert!(matches!(err, CanaryError::Internal(_)));
    }
}
