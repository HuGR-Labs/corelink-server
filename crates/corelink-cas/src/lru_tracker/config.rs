//! LRU tracker config knobs surfaced by the production binding.
//!
//! Defaults pin canonical thresholds from WI-S07-004 §6.1.3 +
//! §6.1.4 + §6.1.5 + §6.1.7:
//!
//! - **`queue_size_max`** — bounded queue cap per DO instance (overflow
//!   drops OLDEST FIFO; canonical 10_000).
//! - **`batch_size`** — D1 batch cap per flush (canonical 250 per Lote
//!   10.5bis lesson; 1ms/row × 250 = 250ms p99 budget).
//! - **`flush_interval_ms`** — DO alarm cadence (canonical 30_000ms
//!   coalescing window per WI §6.1.6).
//! - **`drift_violation_threshold_ms`** — INV-LRU-CONSISTENCY guard
//!   threshold (canonical 60_000ms; flush-failure detection).
//! - **`refresh_threshold_ms`** — intra-DO dedup window (canonical
//!   60_000ms; if existing entry's `accessed_at_ms >= now -
//!   refresh_threshold_ms`, the new record is coalesced without
//!   bumping the timestamp).

/// Canonical bounded queue size cap (entries).
pub const DEFAULT_QUEUE_SIZE_MAX: usize = 10_000;

/// Canonical D1 flush batch cap (rows). Mirrors
/// `corelink-eviction::MAX_LRU_BATCH_SIZE` (Lote 10.5bis lesson).
pub const DEFAULT_BATCH_SIZE: usize = 250;

/// Canonical DO alarm flush cadence (ms; 30s coalescing window per WI
/// §6.1.6).
pub const DEFAULT_FLUSH_INTERVAL_MS: u64 = 30_000;

/// Canonical drift-violation threshold (ms; 60s per spec contract §6
/// DoD INV-LRU-CONSISTENCY guard).
pub const DEFAULT_DRIFT_VIOLATION_THRESHOLD_MS: u64 = 60_000;

/// Canonical refresh-threshold window (ms; 60s intra-DO dedup window
/// per WI §6.1.5 — skip update if `now - last_accessed_at < 60s`).
pub const DEFAULT_REFRESH_THRESHOLD_MS: u64 = 60_000;

/// Knobs driving the LRU tracker decision engine.
///
/// All fields are private; access via inherent methods so the
/// production wiring cannot accidentally drift from canonical defaults
/// without going through [`LruConfig::with_overrides`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LruConfig {
    /// Bounded queue size cap (entries; overflow drops OLDEST FIFO).
    queue_size_max: usize,
    /// D1 batch cap per flush (rows).
    batch_size: usize,
    /// DO alarm flush cadence (ms).
    flush_interval_ms: u64,
    /// INV-LRU-CONSISTENCY violation threshold (ms).
    drift_violation_threshold_ms: u64,
    /// Intra-DO dedup window (ms; refresh-threshold short-circuit).
    refresh_threshold_ms: u64,
}

impl LruConfig {
    /// Construct with the canonical defaults pinned to the spec
    /// contract.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            queue_size_max: DEFAULT_QUEUE_SIZE_MAX,
            batch_size: DEFAULT_BATCH_SIZE,
            flush_interval_ms: DEFAULT_FLUSH_INTERVAL_MS,
            drift_violation_threshold_ms: DEFAULT_DRIFT_VIOLATION_THRESHOLD_MS,
            refresh_threshold_ms: DEFAULT_REFRESH_THRESHOLD_MS,
        }
    }

    /// Construct with explicit knob overrides (test wiring +
    /// admin-tier customisation in S-13 forward).
    ///
    /// Returns `None` when:
    /// - `queue_size_max == 0` (queue would always overflow).
    /// - `batch_size == 0` OR `batch_size > queue_size_max` (batch
    ///   exceeds queue capacity).
    /// - `flush_interval_ms == 0` (flush would saturate the runtime).
    /// - `drift_violation_threshold_ms == 0` (every flush would fire
    ///   a consistency violation).
    /// - `refresh_threshold_ms == 0` (no intra-DO dedup; every record
    ///   would write).
    #[must_use]
    pub const fn with_overrides(
        queue_size_max: usize,
        batch_size: usize,
        flush_interval_ms: u64,
        drift_violation_threshold_ms: u64,
        refresh_threshold_ms: u64,
    ) -> Option<Self> {
        if queue_size_max == 0 {
            return None;
        }
        if batch_size == 0 || batch_size > queue_size_max {
            return None;
        }
        if flush_interval_ms == 0 {
            return None;
        }
        if drift_violation_threshold_ms == 0 {
            return None;
        }
        if refresh_threshold_ms == 0 {
            return None;
        }
        Some(Self {
            queue_size_max,
            batch_size,
            flush_interval_ms,
            drift_violation_threshold_ms,
            refresh_threshold_ms,
        })
    }

    /// Bounded queue cap.
    #[must_use]
    pub const fn queue_size_max(&self) -> usize {
        self.queue_size_max
    }

    /// D1 flush batch cap.
    #[must_use]
    pub const fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// DO alarm flush cadence (ms).
    #[must_use]
    pub const fn flush_interval_ms(&self) -> u64 {
        self.flush_interval_ms
    }

    /// INV-LRU-CONSISTENCY violation threshold (ms).
    #[must_use]
    pub const fn drift_violation_threshold_ms(&self) -> u64 {
        self.drift_violation_threshold_ms
    }

    /// Intra-DO dedup window (ms).
    #[must_use]
    pub const fn refresh_threshold_ms(&self) -> u64 {
        self.refresh_threshold_ms
    }
}

impl Default for LruConfig {
    fn default() -> Self {
        Self::canonical()
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
    fn canonical_constants_pinned() {
        assert_eq!(DEFAULT_QUEUE_SIZE_MAX, 10_000);
        assert_eq!(DEFAULT_BATCH_SIZE, 250);
        assert_eq!(DEFAULT_FLUSH_INTERVAL_MS, 30_000);
        assert_eq!(DEFAULT_DRIFT_VIOLATION_THRESHOLD_MS, 60_000);
        assert_eq!(DEFAULT_REFRESH_THRESHOLD_MS, 60_000);
    }

    #[test]
    fn canonical_config_uses_canonical_defaults() {
        let c = LruConfig::canonical();
        assert_eq!(c.queue_size_max(), 10_000);
        assert_eq!(c.batch_size(), 250);
        assert_eq!(c.flush_interval_ms(), 30_000);
        assert_eq!(c.drift_violation_threshold_ms(), 60_000);
        assert_eq!(c.refresh_threshold_ms(), 60_000);
    }

    #[test]
    fn default_matches_canonical() {
        assert_eq!(LruConfig::default(), LruConfig::canonical());
    }

    #[test]
    fn with_overrides_accepts_valid() {
        let c = LruConfig::with_overrides(100, 25, 5_000, 30_000, 30_000).unwrap();
        assert_eq!(c.queue_size_max(), 100);
        assert_eq!(c.batch_size(), 25);
        assert_eq!(c.flush_interval_ms(), 5_000);
        assert_eq!(c.drift_violation_threshold_ms(), 30_000);
        assert_eq!(c.refresh_threshold_ms(), 30_000);
    }

    #[test]
    fn with_overrides_rejects_zero_queue_size() {
        assert!(LruConfig::with_overrides(0, 25, 5_000, 30_000, 30_000).is_none());
    }

    #[test]
    fn with_overrides_rejects_zero_batch_size() {
        assert!(LruConfig::with_overrides(100, 0, 5_000, 30_000, 30_000).is_none());
    }

    #[test]
    fn with_overrides_rejects_batch_exceeding_queue() {
        assert!(LruConfig::with_overrides(100, 101, 5_000, 30_000, 30_000).is_none());
    }

    #[test]
    fn with_overrides_rejects_zero_flush_interval() {
        assert!(LruConfig::with_overrides(100, 25, 0, 30_000, 30_000).is_none());
    }

    #[test]
    fn with_overrides_rejects_zero_drift_threshold() {
        assert!(LruConfig::with_overrides(100, 25, 5_000, 0, 30_000).is_none());
    }

    #[test]
    fn with_overrides_rejects_zero_refresh_threshold() {
        assert!(LruConfig::with_overrides(100, 25, 5_000, 30_000, 0).is_none());
    }

    #[test]
    fn with_overrides_accepts_batch_eq_queue() {
        // Boundary: batch_size == queue_size_max (allowed; whole queue
        // fits in one batch).
        let c = LruConfig::with_overrides(100, 100, 5_000, 30_000, 30_000);
        assert!(c.is_some());
    }
}
