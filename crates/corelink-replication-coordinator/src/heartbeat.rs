//! Per-region heartbeat registry.
//!
//! In production each `corelink-replica-worker` DO emits a heartbeat
//! every 30 s; the coordinator (running as a singleton DO) records the
//! latest heartbeat per region. Staleness exceeding
//! [`HEARTBEAT_STALE_SECONDS`] is one of the inputs to the
//! [`crate::ReplicationCoordinator::evaluate`] decision tree.
//!
//! This module ships the **pure-logic** trait + an `Arc<Mutex<>>`-based
//! `InMemory` implementation (F-001 closure). Real DO state binding is
//! `trait-abstraction-defer` per charter.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::error::CoordinatorError;
use crate::lag::LagBundle;
use crate::Region;

/// Heartbeat staleness threshold (seconds). Crossing this threshold
/// flags the heartbeat as stale; combined with lag-SLO breach it
/// triggers the promotion decision tree.
///
/// Matches the replica-worker tick cadence (30 s) ×2 to absorb a single
/// missed tick without false-positive promotion.
pub const HEARTBEAT_STALE_SECONDS: u64 = 60;

/// A single heartbeat record from a replica-worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heartbeat {
    /// The region the heartbeat originates from.
    pub region: Region,
    /// Wall-clock timestamp of the heartbeat, milliseconds since epoch.
    pub timestamp_ms: u64,
    /// The lag bundle observed by this region at the heartbeat instant.
    pub lag: LagBundle,
}

impl Heartbeat {
    /// Convenience constructor.
    #[must_use]
    pub const fn new(region: Region, timestamp_ms: u64, lag: LagBundle) -> Self {
        Heartbeat {
            region,
            timestamp_ms,
            lag,
        }
    }

    /// Whether this heartbeat is fresh given `now_ms`.
    ///
    /// Returns `false` if `now_ms < timestamp_ms` (clock skew) —
    /// fail-CLOSED conservative: treat the heartbeat as stale.
    #[must_use]
    pub fn is_fresh(self, now_ms: u64) -> bool {
        let age_ms = now_ms.saturating_sub(self.timestamp_ms);
        // Strict clock-skew guard: if the heartbeat is in the future,
        // treat as stale.
        if now_ms < self.timestamp_ms {
            return false;
        }
        age_ms / 1_000 < HEARTBEAT_STALE_SECONDS
    }
}

/// Heartbeat registry trait.
///
/// Implementations MUST be `Send + Sync` so the coordinator can hold an
/// `Arc<dyn HeartbeatRegistry>`. Operations linearize concurrent writers
/// — in production via the DO singleton, in tests via per-instance
/// `Arc<Mutex<>>`.
pub trait HeartbeatRegistry: std::fmt::Debug + Send + Sync {
    /// Record a heartbeat. Last-write-wins per region.
    ///
    /// # Errors
    ///
    /// Returns [`CoordinatorError::Internal`] if the underlying lock is
    /// poisoned (production wiring satisfies via DO singleton; this
    /// surface lets tests assert the forensic emit).
    fn record(&self, hb: Heartbeat) -> Result<(), CoordinatorError>;

    /// Read the latest heartbeat for `region`, if any.
    ///
    /// # Errors
    ///
    /// Returns [`CoordinatorError::Internal`] on lock poisoning.
    fn latest(&self, region: Region) -> Result<Option<Heartbeat>, CoordinatorError>;

    /// Read the latest heartbeat for every region with a record.
    ///
    /// # Errors
    ///
    /// Returns [`CoordinatorError::Internal`] on lock poisoning.
    fn all(&self) -> Result<Vec<Heartbeat>, CoordinatorError>;
}

/// In-memory `Arc<Mutex<>>`-backed heartbeat registry (per-instance
/// F-001 closure).
#[derive(Debug, Default)]
pub struct InMemoryHeartbeatRegistry {
    inner: Arc<Mutex<HashMap<&'static str, Heartbeat>>>,
}

impl InMemoryHeartbeatRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl HeartbeatRegistry for InMemoryHeartbeatRegistry {
    fn record(&self, hb: Heartbeat) -> Result<(), CoordinatorError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| CoordinatorError::Internal(format!("heartbeat lock poisoned: {e}")))?;
        guard.insert(hb.region.as_str(), hb);
        Ok(())
    }

    fn latest(&self, region: Region) -> Result<Option<Heartbeat>, CoordinatorError> {
        let guard = self
            .inner
            .lock()
            .map_err(|e| CoordinatorError::Internal(format!("heartbeat lock poisoned: {e}")))?;
        Ok(guard.get(region.as_str()).copied())
    }

    fn all(&self) -> Result<Vec<Heartbeat>, CoordinatorError> {
        let guard = self
            .inner
            .lock()
            .map_err(|e| CoordinatorError::Internal(format!("heartbeat lock poisoned: {e}")))?;
        Ok(guard.values().copied().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_under_threshold() {
        let hb = Heartbeat::new(Region::Wnam, 1_000_000, LagBundle::zero());
        // 1 s after = fresh.
        assert!(hb.is_fresh(1_001_000));
        // 59 s after = fresh (under 60 s threshold).
        assert!(hb.is_fresh(1_000_000 + 59 * 1_000));
    }

    #[test]
    fn stale_at_threshold() {
        let hb = Heartbeat::new(Region::Wnam, 1_000_000, LagBundle::zero());
        // 60 s after = stale (>= threshold).
        assert!(!hb.is_fresh(1_000_000 + 60 * 1_000));
    }

    #[test]
    fn future_timestamp_treated_as_stale() {
        let hb = Heartbeat::new(Region::Wnam, 1_000_000, LagBundle::zero());
        // Clock skew: heartbeat from the "future" → conservative stale.
        assert!(!hb.is_fresh(500_000));
    }

    #[test]
    fn record_and_retrieve() -> Result<(), CoordinatorError> {
        let reg = InMemoryHeartbeatRegistry::new();
        let hb = Heartbeat::new(Region::Enam, 2_000_000, LagBundle::zero());
        reg.record(hb)?;
        assert_eq!(reg.latest(Region::Enam)?, Some(hb));
        assert_eq!(reg.latest(Region::Wnam)?, None);
        Ok(())
    }

    #[test]
    fn last_write_wins() -> Result<(), CoordinatorError> {
        let reg = InMemoryHeartbeatRegistry::new();
        reg.record(Heartbeat::new(Region::Enam, 1_000, LagBundle::zero()))?;
        reg.record(Heartbeat::new(Region::Enam, 2_000, LagBundle::zero()))?;
        let latest = reg.latest(Region::Enam)?;
        assert_eq!(latest.map(|h| h.timestamp_ms), Some(2_000));
        Ok(())
    }

    #[test]
    fn all_returns_every_region_with_a_record() -> Result<(), CoordinatorError> {
        let reg = InMemoryHeartbeatRegistry::new();
        reg.record(Heartbeat::new(Region::Wnam, 1_000, LagBundle::zero()))?;
        reg.record(Heartbeat::new(Region::Enam, 1_000, LagBundle::zero()))?;
        let all = reg.all()?;
        assert_eq!(all.len(), 2);
        Ok(())
    }
}
