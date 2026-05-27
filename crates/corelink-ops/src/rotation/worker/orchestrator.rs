//! [`RotationOrchestrator`] — top-level rotation coordinator.
//!
//! Composes adapter + state machine + metrics; exposes
//! `run_rotation(now_ms)` which drives the full canonical lifecycle:
//! generate → promote → rekey_downstream → retire → destroy (driven
//! by the caller providing the appropriate timestamps).
//!
//! In production: the Cloudflare Workers Cron trigger calls
//! `run_rotation` with `Date.now()` milliseconds. The orchestrator is
//! stateless; the adapter + state machine hold per-asset state.
//!
//! # Per-asset cron schedule (WI-S13-003 §6.1.1)
//!
//! - TDK: weekly cron (7d cadence + 7d overlap).
//! - PAT signing: daily cron 02:00 UTC.
//! - Audit chain: daily cron 02:30 UTC (per-region staggered).
//! - Admin signing: daily cron (same as PAT signing).
//! - BYOK: customer-triggered (no cron; API endpoint).

use std::sync::Arc;

use corelink_rotation_adapters::{AssetClass, KeyHandle, RotationAdapter, RotationError};

use super::metrics::RotationMetricsSink;
use super::state_machine::{RotationPhase, RotationStateMachine};

/// Cron schedule offsets per asset class (for documentation; production
/// cron binding is in `wrangler.toml`).
pub mod cron {
    /// TDK weekly cron (7d cadence; Monday 00:00 UTC).
    pub const TDK_WEEKLY: &str = "0 0 * * MON";
    /// PAT signing daily cron (02:00 UTC).
    pub const PAT_SIGNING_DAILY: &str = "0 2 * * *";
    /// Audit chain daily cron (02:30 UTC; staggered from PAT signing).
    pub const AUDIT_CHAIN_DAILY: &str = "30 2 * * *";
    /// Admin signing daily cron (02:00 UTC; same cadence as PAT signing).
    pub const ADMIN_SIGNING_DAILY: &str = "0 2 * * *";
    /// BYOK: no cron; customer-triggered via POST /v1/customer/byok/rotate.
    pub const BYOK_CUSTOMER_TRIGGERED: &str = "(customer-triggered)";
}

/// Outcome of a full rotation lifecycle step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RotationOutcome {
    /// Rotation phase completed successfully.
    Ok {
        /// Phase completed.
        phase: super::state_machine::RotationPhase,
        /// The key handle after the transition.
        handle: KeyHandle,
    },
    /// Rotation was rolled back (PAT-ROLL-FORWARD-001).
    RolledBack {
        /// The key that was rolled back.
        rolled_back: KeyHandle,
        /// The key re-promoted to Active.
        re_promoted: KeyHandle,
    },
}

/// Top-level rotation orchestrator.
///
/// Generic over adapter (`A`), state machine (`SM`), and metrics (`M`)
/// to allow composition of test fakes without production dependencies.
#[derive(Debug)]
pub struct RotationOrchestrator<A, SM, M>
where
    A: RotationAdapter,
    SM: RotationStateMachine,
    M: RotationMetricsSink,
{
    adapter: Arc<A>,
    state_machine: Arc<SM>,
    metrics: Arc<M>,
    region: String,
}

impl<A, SM, M> RotationOrchestrator<A, SM, M>
where
    A: RotationAdapter,
    SM: RotationStateMachine,
    M: RotationMetricsSink,
{
    /// Construct a new orchestrator.
    #[must_use]
    pub fn new(adapter: Arc<A>, state_machine: Arc<SM>, metrics: Arc<M>, region: String) -> Self {
        Self {
            adapter,
            state_machine,
            metrics,
            region,
        }
    }

    /// Generate a new key (Pending state). Step 1 of the lifecycle.
    ///
    /// # Errors
    ///
    /// [`RotationError::RotationInFlight`] if already in-flight.
    /// [`RotationError::Kms`] on key generation failure.
    pub fn generate(&self, now_ms: u64) -> Result<RotationOutcome, RotationError> {
        let handle = self.adapter.generate(now_ms)?;
        self.state_machine.record_transition(
            &handle,
            corelink_rotation_adapters::KeyState::Pending,
            RotationPhase::Generate,
            &self.region,
            now_ms,
        )?;
        self.metrics
            .set_in_progress(self.adapter.asset_class(), "pending", 1);
        Ok(RotationOutcome::Ok {
            phase: RotationPhase::Generate,
            handle,
        })
    }

    /// Promote `new` key to Active; previous Active → Overlap. Step 2.
    ///
    /// Emits overlap window metric observation per asset class.
    ///
    /// # Errors
    ///
    /// [`RotationError::InvalidTransition`] if `new.state != Pending`.
    /// [`RotationError::OverlapExceedsHardUpper`] if overlap > 30d.
    /// [`RotationError::Audit`] on audit emit failure.
    pub fn promote(&self, new: &KeyHandle, now_ms: u64) -> Result<RotationOutcome, RotationError> {
        let promoted = self.adapter.promote(new, now_ms)?;
        self.state_machine.record_transition(
            &promoted,
            corelink_rotation_adapters::KeyState::Active,
            RotationPhase::Promote,
            &self.region,
            now_ms,
        )?;
        // Emit overlap window metric (canonical table 3.2.1 baseline).
        let overlap_secs = self.adapter.asset_class().overlap_seconds();
        self.metrics
            .observe_overlap_seconds(self.adapter.asset_class(), overlap_secs);
        self.metrics
            .set_in_progress(self.adapter.asset_class(), "active", 1);
        Ok(RotationOutcome::Ok {
            phase: RotationPhase::Promote,
            handle: promoted,
        })
    }

    /// Re-key downstream resources. Step 3 (TDK only; no-op for others).
    ///
    /// Progress reported via `corelink_admin_rotation_rekey_progress_ratio`.
    ///
    /// # Errors
    ///
    /// [`RotationError::Storage`] on downstream re-key failure.
    pub fn rekey_downstream(
        &self,
        new: &KeyHandle,
        now_ms: u64,
    ) -> Result<RotationOutcome, RotationError> {
        let metrics_ref = Arc::clone(&self.metrics);
        let asset_class = self.adapter.asset_class();
        self.adapter.rekey_downstream(new, &|progress| {
            metrics_ref.record_rekey_progress(asset_class, progress);
        })?;
        self.state_machine.record_transition(
            new,
            new.state, // state unchanged; this is a progress event
            RotationPhase::RekeyDownstream,
            &self.region,
            now_ms,
        )?;
        Ok(RotationOutcome::Ok {
            phase: RotationPhase::RekeyDownstream,
            handle: new.clone(),
        })
    }

    /// Retire the old overlap key. Step 4.
    ///
    /// # Errors
    ///
    /// [`RotationError::InvalidTransition`] if `old.state != Overlap`.
    /// [`RotationError::Audit`] on audit emit failure.
    pub fn retire(
        &self,
        old: &KeyHandle,
        now_ms: u64,
    ) -> Result<RotationOutcome, RotationError> {
        let retired = self.adapter.retire(old, now_ms)?;
        self.state_machine.record_transition(
            &retired,
            corelink_rotation_adapters::KeyState::Retired,
            RotationPhase::Retire,
            &self.region,
            now_ms,
        )?;
        self.metrics
            .set_in_progress(self.adapter.asset_class(), "retired", 1);
        Ok(RotationOutcome::Ok {
            phase: RotationPhase::Retire,
            handle: retired,
        })
    }

    /// Destroy the retired key. Step 5.
    ///
    /// # Errors
    ///
    /// [`RotationError::InvalidTransition`] if `retired.state != Retired`.
    /// [`RotationError::Audit`] on audit emit failure.
    pub fn destroy(
        &self,
        retired: &KeyHandle,
        now_ms: u64,
    ) -> Result<RotationOutcome, RotationError> {
        let destroyed = self.adapter.destroy(retired, now_ms)?;
        self.state_machine.record_transition(
            &destroyed,
            corelink_rotation_adapters::KeyState::Destroyed,
            RotationPhase::Destroy,
            &self.region,
            now_ms,
        )?;
        self.metrics
            .increment_rotation_total(self.adapter.asset_class(), "ok");
        self.metrics
            .set_in_progress(self.adapter.asset_class(), "destroyed", 1);
        Ok(RotationOutcome::Ok {
            phase: RotationPhase::Destroy,
            handle: destroyed,
        })
    }

    /// Rollback (PAT-ROLL-FORWARD-001). Reverts promotion.
    ///
    /// # Errors
    ///
    /// [`RotationError::InvalidTransition`] on invalid state.
    /// [`RotationError::Audit`] on audit emit failure.
    pub fn rollback(
        &self,
        new: &KeyHandle,
        previous: &KeyHandle,
        now_ms: u64,
    ) -> Result<RotationOutcome, RotationError> {
        let (rolled_back, re_promoted) = self.adapter.rollback(new, previous, now_ms)?;
        self.state_machine.record_transition(
            &rolled_back,
            corelink_rotation_adapters::KeyState::RolledBack,
            RotationPhase::Rollback,
            &self.region,
            now_ms,
        )?;
        self.metrics
            .increment_rotation_total(self.adapter.asset_class(), "rolled_back");
        Ok(RotationOutcome::RolledBack {
            rolled_back,
            re_promoted,
        })
    }

    /// Return the asset class of the configured adapter.
    #[must_use]
    pub fn asset_class(&self) -> AssetClass {
        self.adapter.asset_class()
    }

    /// Return the region string.
    #[must_use]
    pub fn region(&self) -> &str {
        &self.region
    }
}
