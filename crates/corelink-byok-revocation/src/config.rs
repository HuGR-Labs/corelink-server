//! [`RevocationConfig`] — tunable parameters for the revocation detector.

use std::time::Duration;

/// Hard minimum check interval: 30 seconds.
///
/// Prevents misconfiguration where check_interval < 30 s would create
/// excessive KMS API call pressure.
pub const MIN_CHECK_INTERVAL_SECS: u64 = 30;

/// Canonical check interval per spec: 60 seconds.
pub const DEFAULT_CHECK_INTERVAL_SECS: u64 = 60;

/// Number of consecutive API errors / throttle responses before the
/// detector conservatively degrades the tenant to read-only.
///
/// 3 cycles × 60 s = 3 minutes before false-positive degrade.
pub const DEFAULT_SUSTAINED_FAILURE_THRESHOLD: u32 = 3;

/// Configuration for [`RevocationDetector`].
///
/// [`RevocationDetector`]: crate::detector::RevocationDetector
///
/// # Example
///
/// ```rust
/// use corelink_byok_revocation::RevocationConfig;
///
/// let config = RevocationConfig::default();
/// assert_eq!(config.check_interval_secs, 60);
/// assert_eq!(config.sustained_failure_threshold, 3);
/// ```
#[derive(Debug, Clone)]
pub struct RevocationConfig {
    /// KMS access check interval per active tenant.
    ///
    /// Canonical: 60 seconds. Hard minimum: 30 seconds.
    /// Set via `Default::default()`.
    pub check_interval_secs: u64,

    /// Number of consecutive transient failures (Throttled / ApiError)
    /// before a tenant is conservatively degraded to read-only.
    pub sustained_failure_threshold: u32,

    /// Whether to emit detailed tracing spans for each check cycle.
    ///
    /// Disable in high-cardinality environments; default `true`.
    pub emit_trace_spans: bool,
}

impl Default for RevocationConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: DEFAULT_CHECK_INTERVAL_SECS,
            sustained_failure_threshold: DEFAULT_SUSTAINED_FAILURE_THRESHOLD,
            emit_trace_spans: true,
        }
    }
}

impl RevocationConfig {
    /// Return the check interval as a [`Duration`].
    #[must_use]
    pub fn check_interval(&self) -> Duration {
        Duration::from_secs(self.check_interval_secs.max(MIN_CHECK_INTERVAL_SECS))
    }
}
