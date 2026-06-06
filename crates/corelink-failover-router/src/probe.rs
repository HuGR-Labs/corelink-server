//! Health probe trait + in-memory implementation + adversarial fixture.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::health::{RegionHealthSnapshot, CONSECUTIVE_FAILURES_THRESHOLD};
use crate::Region;

/// Health probe cadence (5 minutes, in seconds).
///
/// Synthetic GET probe per region on this cadence.
pub const HEALTH_PROBE_CADENCE_SECS: u64 = 300;

/// Health probe trait for per-region synthetic GET request.
///
/// # Examples
///
/// ```
/// use corelink_failover_router::{HealthProbe, InMemoryHealthProbe, Region};
///
/// let probe = InMemoryHealthProbe::new();
/// let snap = probe.probe(Region::Wnam, 0).unwrap();
/// assert!(snap.health.is_healthy()); // default healthy
/// ```
pub trait HealthProbe: std::fmt::Debug + Send + Sync {
    /// Execute a synthetic GET probe for the given region.
    ///
    /// Returns a health snapshot based on current observed signals.
    fn probe(&self, region: Region, timestamp_ms: u64) -> Result<RegionHealthSnapshot, String>;
}

/// In-memory health probe for tests and local orchestration.
///
/// Defaults to all regions healthy. Call `inject_degraded` to simulate outages.
///
/// Thread-safe via `Arc<Mutex<>>` per-instance (F-001 closure).
#[derive(Debug)]
pub struct InMemoryHealthProbe {
    states: Arc<Mutex<HashMap<String, ProbeState>>>,
}

#[derive(Debug, Clone)]
struct ProbeState {
    rate_5xx_pct: f64,
    latency_p99_ms: u64,
    consecutive_failures: u32,
}

impl Default for ProbeState {
    fn default() -> Self {
        ProbeState {
            rate_5xx_pct: 0.0,
            latency_p99_ms: 50, // nominal healthy latency
            consecutive_failures: 0,
        }
    }
}

impl InMemoryHealthProbe {
    /// Create a new in-memory probe with all regions healthy.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_failover_router::{HealthProbe, InMemoryHealthProbe, Region};
    ///
    /// let p = InMemoryHealthProbe::new();
    /// let snap = p.probe(Region::Enam, 0).unwrap();
    /// assert!(snap.health.is_healthy());
    /// ```
    pub fn new() -> Self {
        InMemoryHealthProbe {
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Inject degraded signals for a region (simulates outage).
    ///
    /// Sets rate_5xx_pct, latency_p99_ms, and consecutive_failures to
    /// values that trigger all 3 failover signals.
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_failover_router::{HealthProbe, InMemoryHealthProbe, Region, RegionHealth};
    ///
    /// let p = InMemoryHealthProbe::new();
    /// p.inject_degraded(Region::Weur);
    /// let snap = p.probe(Region::Weur, 0).unwrap();
    /// assert_eq!(snap.health, RegionHealth::Degraded);
    /// ```
    pub fn inject_degraded(&self, region: Region) {
        if let Ok(mut states) = self.states.lock() {
            states.insert(
                region.as_str().to_owned(),
                ProbeState {
                    rate_5xx_pct: 5.0,   // > 1% threshold
                    latency_p99_ms: 500, // > 300ms SLO
                    consecutive_failures: CONSECUTIVE_FAILURES_THRESHOLD + 1,
                },
            );
        }
    }

    /// Inject healthy signals for a region (simulate recovery).
    ///
    /// # Examples
    ///
    /// ```
    /// use corelink_failover_router::{HealthProbe, InMemoryHealthProbe, Region, RegionHealth};
    ///
    /// let p = InMemoryHealthProbe::new();
    /// p.inject_degraded(Region::Sam);
    /// p.inject_healthy(Region::Sam);
    /// let snap = p.probe(Region::Sam, 0).unwrap();
    /// assert_eq!(snap.health, RegionHealth::Healthy);
    /// ```
    pub fn inject_healthy(&self, region: Region) {
        if let Ok(mut states) = self.states.lock() {
            states.insert(region.as_str().to_owned(), ProbeState::default());
        }
    }

    /// Set custom probe signals for fine-grained testing.
    pub fn set_state(
        &self,
        region: Region,
        rate_5xx_pct: f64,
        latency_p99_ms: u64,
        consecutive_failures: u32,
    ) {
        if let Ok(mut states) = self.states.lock() {
            states.insert(
                region.as_str().to_owned(),
                ProbeState {
                    rate_5xx_pct,
                    latency_p99_ms,
                    consecutive_failures,
                },
            );
        }
    }
}

impl Default for InMemoryHealthProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthProbe for InMemoryHealthProbe {
    fn probe(&self, region: Region, timestamp_ms: u64) -> Result<RegionHealthSnapshot, String> {
        let state = self
            .states
            .lock()
            .map_err(|e| e.to_string())?
            .get(region.as_str())
            .cloned()
            .unwrap_or_default();

        Ok(RegionHealthSnapshot::evaluate(
            region,
            state.rate_5xx_pct,
            state.latency_p99_ms,
            state.consecutive_failures,
            timestamp_ms,
        ))
    }
}

/// Adversarial health probe — always returns an error.
///
/// Used to verify fail-CLOSED semantics when probe fails.
///
/// # Examples
///
/// ```
/// use corelink_failover_router::{FailingHealthProbe, HealthProbe, Region};
///
/// let p = FailingHealthProbe::new("network partition");
/// assert!(p.probe(Region::Wnam, 0).is_err());
/// ```
#[derive(Debug)]
pub struct FailingHealthProbe {
    message: String,
}

impl FailingHealthProbe {
    /// Create a failing probe with the given error message.
    pub fn new(message: impl Into<String>) -> Self {
        FailingHealthProbe {
            message: message.into(),
        }
    }
}

impl HealthProbe for FailingHealthProbe {
    fn probe(&self, _region: Region, _timestamp_ms: u64) -> Result<RegionHealthSnapshot, String> {
        Err(self.message.clone())
    }
}
