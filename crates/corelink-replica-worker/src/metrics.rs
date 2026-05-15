//! Replication-lag SLI emit trait + in-memory fixture
//! (closes DEBT-011 P0-001 + part of P0-003).
//!
//! # What this module ships
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! the production wiring for Prometheus emit lives in the CF Worker glue (real
//! `corelink_replication_lag_seconds` histogram + `corelink_replication_batch_total`
//! counter + region/domain labels), but the **trait method boundary** at which
//! the worker calls `emit_lag`/`emit_batch_outcome` is fixed here so that:
//!
//! 1. The verifier (`scripts/verify-replication-lag.py`) can rely on the metric
//!    name + label schema being load-bearing (acceptance criterion §1 of
//!    `R-PREP-REPL-P0-001`).
//! 2. Integration tests can exercise the emit deterministically via
//!    [`InMemoryReplicationLagSli`] without standing up a Prometheus endpoint.
//! 3. Audit ordering is preserved: SLI emit happens **AFTER** the
//!    `replication.completed` audit (the audit is the canonical commit; the
//!    Sli is an observability side-channel — never the source of truth).
//!
//! # Cardinality discipline (INV-OBS-CARDINALITY-BUDGET S-09)
//!
//! Labels: `{domain, primary_region, replica_region}` — no `tenant_id` /
//! `blob_hash`. With 4 domains × 4 primary × 4 replica = 64 séries maximum;
//! actual sibling-only pairs reduce this to `4 domains × 4 pairs = 16 séries`.
//! Budget-safe.
//!
//! # Audit fail-CLOSED ordering (S-06 P0-2 lesson, reaffirmed)
//!
//! The Sli emit is **NEVER** allowed to short-circuit state mutation: a failed
//! `emit_lag` is logged via the audit sink (`detail = "sli_emit_failed: ..."`)
//! but does NOT block the replication completion. The audit chain is the
//! authoritative log; the SLI is "best-effort observability".

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::region::Region;

/// Canonical Prometheus metric name for replication-lag histogram.
///
/// Production wiring (CF Worker glue) MUST emit observations under this exact
/// name with the labels `{domain, primary_region, replica_region}` to satisfy
/// `SLO-REPLICATION-LAG-R2` (`slo_catalog.md §4.23`).
pub const METRIC_REPLICATION_LAG_SECONDS: &str = "corelink_replication_lag_seconds";

/// Canonical Prometheus metric name for per-batch outcome counter.
///
/// Distinguishes `ok` / `partial` / `failed` batches (silent-skip hazard
/// flagged in `2026-05-15-replication-audit.md §5.2`).
pub const METRIC_REPLICATION_BATCH_TOTAL: &str = "corelink_replication_batch_total";

/// Canonical Prometheus histogram bucket boundaries (seconds).
///
/// Anchored on the 60 s p99 SLO ceiling (`REPLICATION_LAG_P99_SLO_SECS`):
/// 5 s, 10 s, 30 s, 60 s (SLO), 120 s, 300 s, +Inf.
///
/// Implementations that produce real Prometheus histograms MUST use these
/// boundaries verbatim so that the verifier's `histogram_quantile(0.99, ...)`
/// query has a meaningful SLO-aligned bucket.
pub const REPLICATION_LAG_BUCKETS_SECONDS: [f64; 6] =
    [5.0, 10.0, 30.0, 60.0, 120.0, 300.0];

/// Canonical replication-domain label for the lag SLI.
///
/// Maps 1:1 to `scripts/verify-replication-lag.py::RPO_BUDGET_SECS` keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum ReplicationDomain {
    /// R2 hot blobs replicated by this crate's cron worker.
    R2Hot,
    /// R2 platform CRR (cold + AC + audit buckets) — measured indirectly.
    R2Crr,
    /// D1 read-replica lag (probed in `corelink-region`).
    D1,
    /// KV global eventual propagation.
    Kv,
    /// DO state sync-to-D1 age.
    Do,
}

impl ReplicationDomain {
    /// Canonical lowercase label value for Prometheus.
    pub fn as_str(self) -> &'static str {
        match self {
            ReplicationDomain::R2Hot => "r2_hot",
            ReplicationDomain::R2Crr => "r2_crr",
            ReplicationDomain::D1 => "d1",
            ReplicationDomain::Kv => "kv",
            ReplicationDomain::Do => "do",
        }
    }
}

impl std::fmt::Display for ReplicationDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-batch outcome (silent-skip hazard discriminator).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum BatchOutcome {
    /// All blobs in the batch were replicated and hash-verified.
    Ok,
    /// At least one blob succeeded; at least one failed (non-residency error).
    Partial,
    /// Zero blobs replicated (residency violation or all retries exhausted).
    Failed,
}

impl BatchOutcome {
    /// Canonical lowercase label value for Prometheus.
    pub fn as_str(self) -> &'static str {
        match self {
            BatchOutcome::Ok => "ok",
            BatchOutcome::Partial => "partial",
            BatchOutcome::Failed => "failed",
        }
    }
}

/// Single lag observation emitted at `replication.completed` audit boundary.
///
/// `lag_seconds` = `replication_completed_ts − blob.last_access_ms / 1000`
/// per the acceptance criterion in `replication-followup-tickets.md
/// R-PREP-REPL-P0-001 §1`.
#[derive(Debug, Clone, PartialEq)]
pub struct LagObservation {
    /// Canonical replication domain.
    pub domain: ReplicationDomain,
    /// Source region of the replication operation.
    pub primary_region: Region,
    /// Destination region of the replication operation.
    pub replica_region: Region,
    /// Observed lag in seconds (`replication_completed_ts − blob.last_access`).
    pub lag_seconds: f64,
    /// Timestamp (ms since epoch) at which the observation was emitted.
    pub timestamp_ms: u64,
}

/// Replication-lag SLI emit trait.
///
/// Production impl wraps a `prometheus::HistogramVec` keyed on
/// `{domain, primary_region, replica_region}`; the [`InMemoryReplicationLagSli`]
/// stores observations in a `Vec` for deterministic tests.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::metrics::{
///     InMemoryReplicationLagSli, ReplicationDomain, ReplicationLagSli,
/// };
/// use corelink_replica_worker::Region;
///
/// let sli = InMemoryReplicationLagSli::new();
/// sli.emit_lag(ReplicationDomain::R2Hot, Region::Wnam, Region::Enam, 12.5, 0)
///     .expect("emit");
/// assert_eq!(sli.observations().len(), 1);
/// assert_eq!(sli.observations()[0].lag_seconds, 12.5);
/// ```
pub trait ReplicationLagSli: std::fmt::Debug + Send + Sync {
    /// Emit a single lag observation (one blob replicated).
    ///
    /// # Errors
    /// Returns `Err(String)` if the underlying metric backend fails. Callers
    /// MUST NOT propagate this error as a state mutation failure (best-effort
    /// observability); they should log via the audit sink and continue.
    fn emit_lag(
        &self,
        domain: ReplicationDomain,
        primary_region: Region,
        replica_region: Region,
        lag_seconds: f64,
        timestamp_ms: u64,
    ) -> Result<(), String>;

    /// Emit a per-batch outcome counter increment.
    ///
    /// `ok` / `partial` / `failed` discriminates the silent-skip hazard called
    /// out in `2026-05-15-replication-audit.md §5.2`.
    ///
    /// # Errors
    /// Returns `Err(String)` if the underlying counter backend fails.
    fn emit_batch_outcome(
        &self,
        domain: ReplicationDomain,
        outcome: BatchOutcome,
        timestamp_ms: u64,
    ) -> Result<(), String>;
}

/// Per-batch outcome observation (counter row).
#[derive(Debug, Clone, PartialEq)]
pub struct BatchOutcomeObservation {
    /// Canonical replication domain.
    pub domain: ReplicationDomain,
    /// Discriminator (ok / partial / failed).
    pub outcome: BatchOutcome,
    /// Timestamp (ms since epoch) at which the observation was emitted.
    pub timestamp_ms: u64,
}

/// In-memory implementation of [`ReplicationLagSli`].
///
/// Per-instance `Arc<Mutex<>>` (F-001 closure). Stores all observations for
/// deterministic assertion in unit + integration tests.
#[derive(Debug, Default)]
pub struct InMemoryReplicationLagSli {
    lag: Mutex<Vec<LagObservation>>,
    batches: Mutex<Vec<BatchOutcomeObservation>>,
}

impl InMemoryReplicationLagSli {
    /// Create an empty in-memory SLI.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all lag observations recorded so far.
    pub fn observations(&self) -> Vec<LagObservation> {
        self.lag
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Snapshot of all batch-outcome observations recorded so far.
    pub fn batch_observations(&self) -> Vec<BatchOutcomeObservation> {
        self.batches
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Count observations matching a domain (helper for tests).
    pub fn lag_count_for(&self, domain: ReplicationDomain) -> usize {
        self.observations()
            .iter()
            .filter(|o| o.domain == domain)
            .count()
    }
}

impl ReplicationLagSli for InMemoryReplicationLagSli {
    fn emit_lag(
        &self,
        domain: ReplicationDomain,
        primary_region: Region,
        replica_region: Region,
        lag_seconds: f64,
        timestamp_ms: u64,
    ) -> Result<(), String> {
        // Defensive: NaN / negative lag = bug upstream; reject loudly (still
        // best-effort — callers must NOT fail the replication on this).
        if !lag_seconds.is_finite() || lag_seconds < 0.0 {
            return Err(format!(
                "invalid lag_seconds {lag_seconds:?} for domain={domain} \
                 primary={primary_region} replica={replica_region}"
            ));
        }
        self.lag
            .lock()
            .map_err(|e| e.to_string())?
            .push(LagObservation {
                domain,
                primary_region,
                replica_region,
                lag_seconds,
                timestamp_ms,
            });
        Ok(())
    }

    fn emit_batch_outcome(
        &self,
        domain: ReplicationDomain,
        outcome: BatchOutcome,
        timestamp_ms: u64,
    ) -> Result<(), String> {
        self.batches
            .lock()
            .map_err(|e| e.to_string())?
            .push(BatchOutcomeObservation {
                domain,
                outcome,
                timestamp_ms,
            });
        Ok(())
    }
}

/// Adversarial SLI that always fails — used to assert that replication
/// completion is **not** blocked by SLI emit failure.
#[derive(Debug)]
pub struct FailingReplicationLagSli {
    message: String,
}

impl FailingReplicationLagSli {
    /// Construct with a canned error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl ReplicationLagSli for FailingReplicationLagSli {
    fn emit_lag(
        &self,
        _domain: ReplicationDomain,
        _primary_region: Region,
        _replica_region: Region,
        _lag_seconds: f64,
        _timestamp_ms: u64,
    ) -> Result<(), String> {
        Err(self.message.clone())
    }

    fn emit_batch_outcome(
        &self,
        _domain: ReplicationDomain,
        _outcome: BatchOutcome,
        _timestamp_ms: u64,
    ) -> Result<(), String> {
        Err(self.message.clone())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn buckets_anchor_on_slo_ceiling() {
        // 60 s SLO must appear as a bucket boundary for the verifier
        // `histogram_quantile(0.99, ...)` query to produce a meaningful answer.
        assert!(REPLICATION_LAG_BUCKETS_SECONDS.contains(&60.0));
        // Boundaries must be strictly increasing.
        for w in REPLICATION_LAG_BUCKETS_SECONDS.windows(2) {
            assert!(w[0] < w[1], "buckets not strictly increasing: {:?}", w);
        }
    }

    #[test]
    fn domain_labels_match_verifier_keys() {
        // These string values are LOAD-BEARING for
        // scripts/verify-replication-lag.py::RPO_BUDGET_SECS.
        assert_eq!(ReplicationDomain::R2Hot.as_str(), "r2_hot");
        assert_eq!(ReplicationDomain::R2Crr.as_str(), "r2_crr");
        assert_eq!(ReplicationDomain::D1.as_str(), "d1");
        assert_eq!(ReplicationDomain::Kv.as_str(), "kv");
        assert_eq!(ReplicationDomain::Do.as_str(), "do");
    }

    #[test]
    fn inmemory_sli_records_lag_observation() {
        let sli = InMemoryReplicationLagSli::new();
        sli.emit_lag(
            ReplicationDomain::R2Hot,
            Region::Wnam,
            Region::Enam,
            42.0,
            1_700_000_000_000,
        )
        .expect("emit_lag should succeed");
        let obs = sli.observations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].domain, ReplicationDomain::R2Hot);
        assert_eq!(obs[0].primary_region, Region::Wnam);
        assert_eq!(obs[0].replica_region, Region::Enam);
        assert!((obs[0].lag_seconds - 42.0).abs() < f64::EPSILON);
        assert_eq!(obs[0].timestamp_ms, 1_700_000_000_000);
    }

    #[test]
    fn inmemory_sli_rejects_nan_lag() {
        let sli = InMemoryReplicationLagSli::new();
        let r = sli.emit_lag(
            ReplicationDomain::R2Hot,
            Region::Wnam,
            Region::Enam,
            f64::NAN,
            0,
        );
        assert!(r.is_err());
    }

    #[test]
    fn inmemory_sli_rejects_negative_lag() {
        let sli = InMemoryReplicationLagSli::new();
        let r = sli.emit_lag(
            ReplicationDomain::R2Hot,
            Region::Wnam,
            Region::Enam,
            -1.0,
            0,
        );
        assert!(r.is_err());
    }

    #[test]
    fn inmemory_sli_records_batch_outcomes() {
        let sli = InMemoryReplicationLagSli::new();
        sli.emit_batch_outcome(ReplicationDomain::R2Hot, BatchOutcome::Ok, 0)
            .expect("emit_batch_outcome ok");
        sli.emit_batch_outcome(ReplicationDomain::R2Hot, BatchOutcome::Partial, 0)
            .expect("emit_batch_outcome partial");
        sli.emit_batch_outcome(ReplicationDomain::R2Hot, BatchOutcome::Failed, 0)
            .expect("emit_batch_outcome failed");
        let batches = sli.batch_observations();
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].outcome, BatchOutcome::Ok);
        assert_eq!(batches[1].outcome, BatchOutcome::Partial);
        assert_eq!(batches[2].outcome, BatchOutcome::Failed);
    }

    #[test]
    fn failing_sli_returns_error() {
        let sli = FailingReplicationLagSli::new("forced");
        assert!(sli
            .emit_lag(
                ReplicationDomain::R2Hot,
                Region::Wnam,
                Region::Enam,
                10.0,
                0
            )
            .is_err());
        assert!(sli
            .emit_batch_outcome(ReplicationDomain::R2Hot, BatchOutcome::Ok, 0)
            .is_err());
    }

    #[test]
    fn batch_outcome_label_values_are_canonical() {
        // These string values are LOAD-BEARING for Prometheus alerting rules
        // and the verifier's batch-partial fraction calculation.
        assert_eq!(BatchOutcome::Ok.as_str(), "ok");
        assert_eq!(BatchOutcome::Partial.as_str(), "partial");
        assert_eq!(BatchOutcome::Failed.as_str(), "failed");
    }
}
