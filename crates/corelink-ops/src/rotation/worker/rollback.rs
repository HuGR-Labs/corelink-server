//! PAT-ROLL-FORWARD-001 auto-rollback driver.
//!
//! Probes `adapter.downstream_error_rate()` at a configurable interval.
//! If the rate exceeds `threshold` (default 1.0%) sustained for
//! `sustain_window_probes` consecutive probes (default 5 probes × 60s
//! interval = 5 min sustained), the rollback is triggered via the adapter.
//!
//! # Design
//!
//! The driver is synchronous (no `tokio`; wasm32-clean). In production
//! the Cloudflare Workers Cron binding calls `probe()` every 60 seconds;
//! the driver accumulates consecutive above-threshold probes and triggers
//! rollback when the sustain window is reached.
//!
//! # INV-KEY-NO-SKIP preservation on rollback
//!
//! On rollback: `new` → `RolledBack`; `previous_active` re-promoted to
//! `Active`. At all times exactly one key is in `Active` state; writes
//! are only accepted from the `Active` key.

use corelink_rotation_adapters::{KeyHandle, RotationAdapter, RotationError};

use super::metrics::RotationMetricsSink;
use super::state_machine::{RotationPhase, RotationStateMachine};

/// Context passed to [`RollbackDriver::probe`] bundling all mutable
/// references required for a single probe cycle.
pub struct ProbeContext<'a, A, SM, M>
where
    A: RotationAdapter,
    SM: RotationStateMachine,
    M: RotationMetricsSink,
{
    /// The rotation adapter being monitored.
    pub adapter: &'a A,
    /// The newly promoted key (rollback reverts this).
    pub new_key: &'a KeyHandle,
    /// The previous active key demoted to Overlap (rollback re-promotes).
    pub previous_key: &'a KeyHandle,
    /// State machine for audit record emission.
    pub state_machine: &'a SM,
    /// Metrics sink for counter/gauge updates.
    pub metrics: &'a M,
    /// Region string for audit records.
    pub region: &'a str,
    /// Current timestamp milliseconds.
    pub now_ms: u64,
}

impl<'a, A, SM, M> core::fmt::Debug for ProbeContext<'a, A, SM, M>
where
    A: RotationAdapter,
    SM: RotationStateMachine,
    M: RotationMetricsSink,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ProbeContext")
            .field("region", &self.region)
            .field("now_ms", &self.now_ms)
            .field("new_key_id", &self.new_key.key_id)
            .field("previous_key_id", &self.previous_key.key_id)
            .finish_non_exhaustive()
    }
}

/// Default downstream error threshold triggering auto-rollback (1%).
pub const DEFAULT_ERROR_THRESHOLD: f64 = 0.01;
/// Default number of consecutive above-threshold probes before rollback
/// (5 probes × 60s interval = 5 min sustained window).
pub const DEFAULT_SUSTAIN_WINDOW_PROBES: u32 = 5;

/// Auto-rollback configuration (PAT-ROLL-FORWARD-001).
#[derive(Debug, Clone, Copy)]
pub struct RollbackConfig {
    /// Error rate threshold (0.0..=1.0) above which a probe counts.
    /// Default: 0.01 (1%).
    pub error_threshold: f64,
    /// Number of consecutive above-threshold probes before rollback.
    /// Default: 5 (5 min sustained at 60s probe interval).
    pub sustain_window_probes: u32,
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            error_threshold: DEFAULT_ERROR_THRESHOLD,
            sustain_window_probes: DEFAULT_SUSTAIN_WINDOW_PROBES,
        }
    }
}

/// Outcome of a single PAT-ROLL-FORWARD-001 probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackOutcome {
    /// Error rate is below threshold; rotation continues.
    Nominal,
    /// Error rate is above threshold but sustain window not yet reached.
    Accumulating {
        /// Number of consecutive above-threshold probes so far.
        consecutive: u32,
    },
    /// Sustain window reached; rollback triggered.
    RollbackTriggered,
}

/// PAT-ROLL-FORWARD-001 auto-rollback driver.
#[derive(Debug)]
pub struct RollbackDriver {
    config: RollbackConfig,
    consecutive_above_threshold: u32,
}

impl RollbackDriver {
    /// Construct a new rollback driver with the given config.
    #[must_use]
    pub fn new(config: RollbackConfig) -> Self {
        Self {
            config,
            consecutive_above_threshold: 0,
        }
    }

    /// Construct with default config (1% threshold; 5 min sustain).
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(RollbackConfig::default())
    }

    /// Probe the downstream error rate using the supplied [`ProbeContext`].
    /// If the sustain window is reached, trigger rollback via the adapter.
    ///
    /// # Errors
    ///
    /// [`RotationError::DownstreamErrorThreshold`] when rollback is
    /// triggered (carries the sampled rate).
    /// [`RotationError::Storage`] on adapter error-rate probe failure.
    pub fn probe<A, SM, M>(
        &mut self,
        ctx: ProbeContext<'_, A, SM, M>,
    ) -> Result<RollbackOutcome, RotationError>
    where
        A: RotationAdapter,
        SM: RotationStateMachine,
        M: RotationMetricsSink,
    {
        let rate = ctx.adapter.downstream_error_rate()?;
        ctx.metrics
            .record_downstream_error_rate(ctx.adapter.asset_class(), rate);

        if rate > self.config.error_threshold {
            self.consecutive_above_threshold += 1;

            if self.consecutive_above_threshold >= self.config.sustain_window_probes {
                // Sustained threshold breach — trigger rollback.
                let (rolled_back, _re_promoted) =
                    ctx.adapter
                        .rollback(ctx.new_key, ctx.previous_key, ctx.now_ms)?;

                ctx.state_machine.record_transition(
                    &rolled_back,
                    corelink_rotation_adapters::KeyState::RolledBack,
                    RotationPhase::Rollback,
                    ctx.region,
                    ctx.now_ms,
                )?;

                ctx.metrics
                    .increment_rotation_total(ctx.adapter.asset_class(), "rolled_back");
                self.consecutive_above_threshold = 0;

                return Err(RotationError::DownstreamErrorThreshold(rate));
            }

            return Ok(RollbackOutcome::Accumulating {
                consecutive: self.consecutive_above_threshold,
            });
        }

        // Rate below threshold — reset consecutive counter.
        self.consecutive_above_threshold = 0;
        Ok(RollbackOutcome::Nominal)
    }

    /// Return the current count of consecutive above-threshold probes.
    #[must_use]
    pub const fn consecutive_above_threshold(&self) -> u32 {
        self.consecutive_above_threshold
    }

    /// Reset the consecutive probe counter (after rotation completes or
    /// a rollback is resolved).
    pub fn reset(&mut self) {
        self.consecutive_above_threshold = 0;
    }
}
