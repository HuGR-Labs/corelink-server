//! Hot-blob replication-coverage SLI
//! (closes DEBT-011 R-PREP-REPL-P2-002).
//!
//! # What this module ships
//!
//! The replica-worker's aggregation window (`AGGREGATION_WINDOW_DAYS` = 30)
//! creates a blind spot for newly-hot blobs: a blob that becomes hot mid-window
//! may not be replicated until the *next* offline aggregation tick. This SLI
//! instruments "fraction of newly-hot blobs replicated within 24 h of becoming
//! hot" so we can inform ADR proposals about whether to shorten the
//! aggregation window post-GA (audit §3.1 edge case (c)).
//!
//! Per the corelink autonomous execution charter (`trait-abstraction-defer`),
//! the production wiring lives in the CF Worker cron (real D1 `hot_blobs`
//! window query + `corelink_hot_blob_replication_coverage_ratio` gauge emit
//! per aggregation tick). Here we ship:
//!
//! 1. [`HotBlobCoverageSli`] trait — fixed boundary the aggregation tick
//!    calls **AFTER** the `aggregation.completed` audit emit (audit canonical;
//!    SLI is best-effort observability — same ordering as the lag SLI in
//!    `metrics.rs`).
//! 2. [`InMemoryHotBlobCoverageSli`] — deterministic fixture (per-instance
//!    `Mutex<>` F-001 closure).
//! 3. Canonical metric name + 0.90 target ratio constants matching
//!    `slo_catalog.md §4.28`.
//! 4. [`compute_coverage_ratio`] — pure helper that turns
//!    `(replicated_within_24h, classified_hot)` into a deterministic ratio
//!    with explicit semantics for the empty-window edge case.
//!
//! # Cardinality discipline (INV-OBS-CARDINALITY-BUDGET S-09)
//!
//! Labels: `{region}` — no `tenant_id` / `blob_hash`. With 4 regions = 4
//! séries maximum. Budget-safe.
//!
//! # Audit ordering
//!
//! The coverage gauge is emitted **AFTER** `aggregation.completed` audit. If
//! the SLI emit fails, the aggregation outcome is unchanged (audit fail-CLOSED
//! preserved); a `coverage_emit_failed` detail line is appended via the audit
//! sink so the failure is forensically observable but never blocks state.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::region::Region;

/// Canonical Prometheus metric name for hot-blob replication-coverage ratio.
///
/// LOAD-BEARING: alerting rules + dashboard panels in `slo_catalog.md §4.28`
/// rely on this exact name.
pub const METRIC_HOT_BLOB_REPLICATION_COVERAGE_RATIO: &str =
    "corelink_hot_blob_replication_coverage_ratio";

/// Target coverage ratio (`replicated_within_24h / classified_hot`) per ticket
/// P2-002 acceptance criterion §2. At least 90% of newly-hot blobs must
/// replicate within 24 h of qualification.
pub const HOT_BLOB_COVERAGE_TARGET_RATIO: f64 = 0.90;

/// "24 h after qualification" window in seconds — the replication deadline for
/// counting toward the coverage ratio numerator.
pub const HOT_BLOB_COVERAGE_DEADLINE_SECONDS: u64 = 24 * 3600;

/// Aggregation-window observation tick — one sample per (window, region) pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageObservation {
    /// Region the aggregation window covered.
    pub region: Region,
    /// Number of blobs classified hot during the window.
    pub classified_hot: u64,
    /// Number of `classified_hot` blobs replicated within 24 h of qualification.
    pub replicated_within_24h: u64,
    /// Computed ratio (`replicated_within_24h / classified_hot`); 1.0 if
    /// `classified_hot == 0` (empty-window vacuous truth — see
    /// [`compute_coverage_ratio`]).
    pub ratio: f64,
    /// Timestamp (ms since epoch) when the observation was recorded.
    pub timestamp_ms: u64,
}

impl CoverageObservation {
    /// `true` iff this observation meets the 0.90 target ratio.
    #[must_use]
    pub fn meets_target(&self) -> bool {
        self.ratio >= HOT_BLOB_COVERAGE_TARGET_RATIO
    }
}

/// Pure helper: compute `replicated_within_24h / classified_hot`.
///
/// Empty-window semantics: if `classified_hot == 0`, return `1.0` (vacuous
/// truth — no blobs to replicate means the coverage target is trivially met).
/// This matches the operational intuition that an empty aggregation window
/// should never page on coverage.
///
/// Overflow / inversion: if `replicated_within_24h > classified_hot` (which
/// should never happen — replicated set is a subset of classified set), the
/// caller's accounting is wrong; we clamp to `1.0` defensively rather than
/// emit a ratio > 1.0 (Prometheus alerting rules anchor on `< 0.90`).
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::coverage::compute_coverage_ratio;
///
/// assert_eq!(compute_coverage_ratio(0, 0), 1.0);
/// assert_eq!(compute_coverage_ratio(9, 10), 0.9);
/// assert_eq!(compute_coverage_ratio(10, 10), 1.0);
/// // Inversion clamps to 1.0 defensively.
/// assert_eq!(compute_coverage_ratio(11, 10), 1.0);
/// ```
#[must_use]
pub fn compute_coverage_ratio(replicated_within_24h: u64, classified_hot: u64) -> f64 {
    if classified_hot == 0 {
        return 1.0;
    }
    if replicated_within_24h >= classified_hot {
        return 1.0;
    }
    (replicated_within_24h as f64) / (classified_hot as f64)
}

/// Hot-blob replication-coverage SLI emit trait.
///
/// Production impl wraps a `prometheus::GaugeVec` keyed on `{region}`; the
/// [`InMemoryHotBlobCoverageSli`] stores observations in a `Vec` for tests.
///
/// # Examples
///
/// ```
/// use corelink_replica_worker::coverage::{
///     HotBlobCoverageSli, InMemoryHotBlobCoverageSli,
/// };
/// use corelink_replica_worker::Region;
///
/// let sli = InMemoryHotBlobCoverageSli::new();
/// sli.emit_coverage(Region::Wnam, 95, 100, 0).expect("emit");
/// let obs = sli.observations();
/// assert_eq!(obs.len(), 1);
/// assert!((obs[0].ratio - 0.95).abs() < f64::EPSILON);
/// assert!(obs[0].meets_target());
/// ```
pub trait HotBlobCoverageSli: std::fmt::Debug + Send + Sync {
    /// Emit a coverage observation for one aggregation window in `region`.
    ///
    /// # Errors
    /// Returns `Err(String)` if the underlying metric backend fails. Callers
    /// MUST NOT propagate this as a state mutation failure (best-effort
    /// observability per audit ordering in module docs).
    fn emit_coverage(
        &self,
        region: Region,
        replicated_within_24h: u64,
        classified_hot: u64,
        timestamp_ms: u64,
    ) -> Result<(), String>;

    /// Canonical Prometheus metric name (load-bearing for dashboards).
    fn metric_name(&self) -> &'static str {
        METRIC_HOT_BLOB_REPLICATION_COVERAGE_RATIO
    }
}

/// In-memory implementation of [`HotBlobCoverageSli`].
///
/// Per-instance `Mutex<>` (F-001 closure). Stores all observations for
/// deterministic assertion in unit + integration tests.
#[derive(Debug, Default)]
pub struct InMemoryHotBlobCoverageSli {
    obs: Mutex<Vec<CoverageObservation>>,
}

impl InMemoryHotBlobCoverageSli {
    /// Create an empty in-memory SLI.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of all observations recorded so far.
    pub fn observations(&self) -> Vec<CoverageObservation> {
        self.obs.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Count observations for a region (helper for tests).
    pub fn count_for(&self, region: Region) -> usize {
        self.observations()
            .iter()
            .filter(|o| o.region == region)
            .count()
    }
}

impl HotBlobCoverageSli for InMemoryHotBlobCoverageSli {
    fn emit_coverage(
        &self,
        region: Region,
        replicated_within_24h: u64,
        classified_hot: u64,
        timestamp_ms: u64,
    ) -> Result<(), String> {
        let ratio = compute_coverage_ratio(replicated_within_24h, classified_hot);
        self.obs
            .lock()
            .map_err(|e| e.to_string())?
            .push(CoverageObservation {
                region,
                classified_hot,
                replicated_within_24h,
                ratio,
                timestamp_ms,
            });
        Ok(())
    }
}

/// Adversarial SLI that always fails — asserts the aggregation outcome is
/// **not** blocked by coverage SLI emit failure.
#[derive(Debug)]
pub struct FailingHotBlobCoverageSli {
    message: String,
}

impl FailingHotBlobCoverageSli {
    /// Construct with a canned error message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl HotBlobCoverageSli for FailingHotBlobCoverageSli {
    fn emit_coverage(
        &self,
        _region: Region,
        _replicated_within_24h: u64,
        _classified_hot: u64,
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
    fn metric_name_is_canonical() {
        assert_eq!(
            METRIC_HOT_BLOB_REPLICATION_COVERAGE_RATIO,
            "corelink_hot_blob_replication_coverage_ratio"
        );
    }

    #[test]
    fn target_ratio_matches_acceptance_criterion() {
        // Ticket P2-002 §2: target ≥ 0.90.
        assert!((HOT_BLOB_COVERAGE_TARGET_RATIO - 0.90).abs() < f64::EPSILON);
    }

    #[test]
    fn deadline_is_24_hours() {
        assert_eq!(HOT_BLOB_COVERAGE_DEADLINE_SECONDS, 24 * 3600);
    }

    #[test]
    fn compute_ratio_empty_window_is_one() {
        // Empty-window vacuous truth: no blobs ≠ failure.
        assert_eq!(compute_coverage_ratio(0, 0), 1.0);
    }

    #[test]
    fn compute_ratio_inversion_clamps_to_one() {
        // Defensive: replicated > classified is impossible by construction,
        // but if upstream accounting glitches we clamp rather than emit > 1.0
        // which would silently disable the `< 0.90` alerting rule.
        assert_eq!(compute_coverage_ratio(11, 10), 1.0);
    }

    #[test]
    fn compute_ratio_boundary_target() {
        // Exactly 90% = meets target (`>=` semantics).
        let r = compute_coverage_ratio(9, 10);
        assert!((r - 0.9).abs() < f64::EPSILON);
        let obs = CoverageObservation {
            region: Region::Wnam,
            classified_hot: 10,
            replicated_within_24h: 9,
            ratio: r,
            timestamp_ms: 0,
        };
        assert!(obs.meets_target());
        // 89% does NOT meet target.
        let r2 = compute_coverage_ratio(89, 100);
        let obs2 = CoverageObservation {
            region: Region::Wnam,
            classified_hot: 100,
            replicated_within_24h: 89,
            ratio: r2,
            timestamp_ms: 0,
        };
        assert!(!obs2.meets_target());
    }

    #[test]
    fn inmemory_sli_records_observation() {
        let sli = InMemoryHotBlobCoverageSli::new();
        sli.emit_coverage(Region::Wnam, 95, 100, 1_700_000_000_000)
            .expect("emit");
        let obs = sli.observations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].region, Region::Wnam);
        assert_eq!(obs[0].classified_hot, 100);
        assert_eq!(obs[0].replicated_within_24h, 95);
        assert!((obs[0].ratio - 0.95).abs() < f64::EPSILON);
        assert!(obs[0].meets_target());
        assert_eq!(obs[0].timestamp_ms, 1_700_000_000_000);
    }

    #[test]
    fn inmemory_sli_records_below_target() {
        // 80% < 90% target — must record but `meets_target` false.
        let sli = InMemoryHotBlobCoverageSli::new();
        sli.emit_coverage(Region::Enam, 80, 100, 0).expect("emit");
        let obs = sli.observations();
        assert_eq!(obs.len(), 1);
        assert!((obs[0].ratio - 0.80).abs() < f64::EPSILON);
        assert!(!obs[0].meets_target());
    }

    #[test]
    fn inmemory_sli_empty_window_records_perfect_ratio() {
        let sli = InMemoryHotBlobCoverageSli::new();
        sli.emit_coverage(Region::Weur, 0, 0, 0).expect("emit");
        let obs = sli.observations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].ratio, 1.0);
        assert!(obs[0].meets_target());
    }

    #[test]
    fn inmemory_sli_count_per_region() {
        let sli = InMemoryHotBlobCoverageSli::new();
        sli.emit_coverage(Region::Wnam, 9, 10, 0).expect("emit");
        sli.emit_coverage(Region::Wnam, 95, 100, 0).expect("emit");
        sli.emit_coverage(Region::Enam, 5, 5, 0).expect("emit");
        assert_eq!(sli.count_for(Region::Wnam), 2);
        assert_eq!(sli.count_for(Region::Enam), 1);
        assert_eq!(sli.count_for(Region::Weur), 0);
    }

    #[test]
    fn failing_sli_returns_error() {
        let sli = FailingHotBlobCoverageSli::new("forced");
        assert!(sli.emit_coverage(Region::Wnam, 9, 10, 0).is_err());
    }

    #[test]
    fn coverage_sli_trait_object_safe() {
        let slis: Vec<Box<dyn HotBlobCoverageSli>> = vec![
            Box::new(InMemoryHotBlobCoverageSli::new()),
            Box::new(FailingHotBlobCoverageSli::new("x")),
        ];
        assert_eq!(slis.len(), 2);
    }
}
