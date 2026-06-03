//! Monthly rollback budget cap enforcer (WI-S13-005 §6.1.4).
//!
//! Implements measured-burn enforcement (Lote 10.13 codex P1 fix):
//! per-rollback error budget consumption is computed from real error
//! counts, not a fixed 5% estimate. Budget cap is 30% monthly (rolling
//! 30d window).
//!
//! ## Budget cap semantics
//!
//! - `consumed_ratio_mtd` = `SUM(budget_consumed_bps) / 3000` where
//!   3000 bps = 30% expressed in basis points.
//! - When `consumed_ratio_mtd > 1.0`, further rollouts are frozen +
//!   SEV-2 alert.
//! - Window: rolling 30d (not calendar month, to avoid "reset gaming"
//!   on the 1st day of each month).
//! - Manual override: Architect + Security lead approval + ADR waiver.
//!
//! Per F-001: per-instance `Arc<Mutex<>>`.

use std::sync::{Arc, Mutex};

use super::error::RolloutError;
use uuid::Uuid;

/// Record of a single auto-rollback's budget consumption.
#[derive(Debug, Clone)]
pub struct BudgetRecord {
    /// Rollout handle that auto-rolled back.
    pub handle_id: Uuid,
    /// Unix timestamp (ms) when rollback started.
    pub rollback_started_ms: u64,
    /// Unix timestamp (ms) when rollback completed.
    pub rollback_completed_ms: u64,
    /// Number of 5xx / SLO-violation errors consumed during rollback
    /// window (real count from `corelink_http_errors_5xx_total` +
    /// `corelink_slo_burn_rate`).
    pub error_count_consumed: u64,
    /// Monthly error budget target (total errors allowed per month).
    pub monthly_error_budget_target: u64,
    /// Computed basis points consumed: `error_count_consumed /
    /// monthly_error_budget_target × 10000`. Clamped to [0, 10000].
    pub budget_consumed_bps: u32,
}

impl BudgetRecord {
    /// Compute basis-points consumption from measured error counts.
    ///
    /// Returns `budget_consumed_bps` in range [0, 10000].
    #[must_use]
    pub fn compute_bps(error_count_consumed: u64, monthly_error_budget_target: u64) -> u32 {
        if monthly_error_budget_target == 0 {
            return 0;
        }
        let bps = (error_count_consumed as u128)
            .saturating_mul(10_000)
            .saturating_div(monthly_error_budget_target as u128);
        u32::try_from(bps.min(10_000)).unwrap_or(10_000)
    }
}

/// Monthly budget tracker trait. Production binding: D1
/// `rollout_budget_consumption` table; rolling 30d window query.
pub trait BudgetTracker: Send + Sync {
    /// Record a new rollback's budget consumption (measured burn).
    ///
    /// # Errors
    ///
    /// Returns [`RolloutError::Storage`] on D1 write failure.
    fn record_rollback(&self, record: BudgetRecord) -> Result<(), RolloutError>;

    /// Compute the current consumed ratio for the rolling 30d window.
    /// Returns 0.0 if no rollbacks recorded.
    ///
    /// Formula: `SUM(budget_consumed_bps) / 3000` (3000 bps = 30%).
    ///
    /// # Errors
    ///
    /// Returns [`RolloutError::Storage`] on D1 read failure.
    fn consumed_ratio(&self) -> Result<f64, RolloutError>;

    /// Returns `true` if the budget cap is exceeded (consumed > 30%).
    ///
    /// # Errors
    ///
    /// Propagates storage errors from [`Self::consumed_ratio`].
    fn is_exceeded(&self) -> Result<bool, RolloutError> {
        self.consumed_ratio().map(|r| r > 1.0)
    }
}

/// In-memory budget tracker for tests.
///
/// Per F-001: per-instance `Arc<Mutex<>>` (no global state).
#[derive(Debug, Clone)]
pub struct InMemoryBudgetTracker {
    records: Arc<Mutex<Vec<BudgetRecord>>>,
    /// Rolling window duration in milliseconds (default: 30d = 30 × 86400 × 1000).
    window_ms: u64,
    /// "Current time" for window calculation (updated by tests).
    now_ms: Arc<Mutex<u64>>,
}

impl InMemoryBudgetTracker {
    /// Create a new tracker with default 30d rolling window.
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
            window_ms: 30 * 24 * 3600 * 1000,
            now_ms: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a tracker with custom window duration (for testing).
    #[must_use]
    pub fn with_window_ms(window_ms: u64) -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
            window_ms,
            now_ms: Arc::new(Mutex::new(0)),
        }
    }

    /// Advance the tracker's notion of "now" (for window testing).
    ///
    /// # Errors
    ///
    /// Returns [`RolloutError::Internal`] if the mutex is poisoned.
    pub fn set_now_ms(&self, now_ms: u64) -> Result<(), RolloutError> {
        *self
            .now_ms
            .lock()
            .map_err(|e| RolloutError::Internal(format!("budget now_ms mutex poisoned: {e}")))? =
            now_ms;
        Ok(())
    }

    fn current_now_ms(&self) -> Result<u64, RolloutError> {
        self.now_ms
            .lock()
            .map_err(|e| RolloutError::Internal(format!("budget now_ms mutex poisoned: {e}")))
            .map(|g| *g)
    }
}

impl Default for InMemoryBudgetTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl BudgetTracker for InMemoryBudgetTracker {
    fn record_rollback(&self, record: BudgetRecord) -> Result<(), RolloutError> {
        self.records
            .lock()
            .map_err(|e| RolloutError::Internal(format!("budget records mutex poisoned: {e}")))?
            .push(record);
        Ok(())
    }

    fn consumed_ratio(&self) -> Result<f64, RolloutError> {
        let now_ms = self.current_now_ms()?;
        let records = self
            .records
            .lock()
            .map_err(|e| RolloutError::Internal(format!("budget records mutex poisoned: {e}")))?;
        let window_start = now_ms.saturating_sub(self.window_ms);
        let total_bps: u64 = records
            .iter()
            .filter(|r| r.rollback_started_ms >= window_start)
            .map(|r| u64::from(r.budget_consumed_bps))
            .fold(0u64, |acc, bps| acc.saturating_add(bps));
        // 3000 bps = 30% of monthly error budget
        Ok(total_bps as f64 / 3000.0)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests use panics as assertions"
)]
mod tests {
    use super::*;

    fn record(started_ms: u64, bps: u32) -> BudgetRecord {
        BudgetRecord {
            handle_id: Uuid::now_v7(),
            rollback_started_ms: started_ms,
            rollback_completed_ms: started_ms + 60_000,
            error_count_consumed: 100,
            monthly_error_budget_target: 10_000,
            budget_consumed_bps: bps,
        }
    }

    #[test]
    fn compute_bps_correct() {
        // 1000 errors out of 10000 target = 10% = 1000 bps
        assert_eq!(BudgetRecord::compute_bps(1000, 10_000), 1000);
        // 3000 / 10000 = 30% = 3000 bps
        assert_eq!(BudgetRecord::compute_bps(3000, 10_000), 3000);
        // overflow protection: error > target → clamp to 10000
        assert_eq!(BudgetRecord::compute_bps(20_000, 10_000), 10_000);
        // zero budget target → 0
        assert_eq!(BudgetRecord::compute_bps(1_000, 0), 0);
    }

    #[test]
    fn empty_tracker_returns_zero() {
        let t = InMemoryBudgetTracker::new();
        assert_eq!(t.consumed_ratio().unwrap(), 0.0);
        assert!(!t.is_exceeded().unwrap());
    }

    #[test]
    fn single_rollback_under_cap() {
        let t = InMemoryBudgetTracker::new();
        t.set_now_ms(1_000_000).unwrap();
        // 500 bps = 5% — under 30% cap (3000 bps)
        t.record_rollback(record(999_000, 500)).unwrap();
        let ratio = t.consumed_ratio().unwrap();
        assert!((ratio - 500.0 / 3000.0).abs() < 1e-9);
        assert!(!t.is_exceeded().unwrap());
    }

    #[test]
    fn over_cap_triggers_exceeded() {
        let t = InMemoryBudgetTracker::new();
        t.set_now_ms(1_000_000).unwrap();
        // 3 × 1100 bps = 3300 bps > 3000 bps cap
        for _ in 0..3 {
            t.record_rollback(record(999_000, 1100)).unwrap();
        }
        assert!(t.is_exceeded().unwrap());
    }

    #[test]
    fn rolling_window_excludes_old_records() {
        let t = InMemoryBudgetTracker::with_window_ms(60_000); // 1 min window
        t.set_now_ms(1_000_000).unwrap();
        // Record outside window (started 2 min ago)
        t.record_rollback(record(999_000 - 120_000, 3000)).unwrap();
        // Record inside window
        t.record_rollback(record(999_900, 500)).unwrap();
        // Only the in-window record counts
        let ratio = t.consumed_ratio().unwrap();
        assert!((ratio - 500.0 / 3000.0).abs() < 1e-9);
    }
}
