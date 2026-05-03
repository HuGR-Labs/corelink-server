//! Stripe-submission control surface trait + InMemory orchestrator-side
//! fake.
//!
//! Per WI-S10-004 §1 invariant 7 + §6.1.20 chaos #3 (Layer 3 drift =
//! customer-facing invoice may be wrong = legal exposure) the SEV-1 arm
//! of the orchestrator MUST halt further usage_record submissions to
//! Stripe until operator clearance. Production wiring at WI-S10-007
//! binds this to the canonical D1 `stripe_submission_state` flag table
//! that the WI-S10-003 adapter reads at every `record_usage` call;
//! flipping the flag pauses the upstream WI-S10-003 cron at its next
//! tick (close-of-month bloqueado per sprint contract §14.s10.1 zero
//! tolerance).
//!
//! The trait surface is identical at the production binding so the
//! orchestrator's audit-fail-CLOSED envelope (audit `stripe_paused`
//! emit BEFORE this `pause` call) holds at both fakes + production.

use std::collections::BTreeSet;
use std::sync::Mutex;

use uuid::Uuid;

use crate::error::ReconcileStripePauseError;

/// Outcome of a [`StripeSubmissionControl::pause`] call.
///
/// `Acked` on first sight (the canonical pause flag flipped from
/// `submission_open` to `submission_paused`); `AlreadyPaused` when the
/// flag was already in the paused state (idempotent — re-running the
/// SEV-1 arm at the next cron tick is a no-op).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StripePauseOutcome {
    /// Pause flag flipped on this call.
    Acked,
    /// Pause flag was already set; this call is idempotent.
    AlreadyPaused,
}

/// Stripe-submission control surface trait. Production wiring binds
/// to the canonical D1 `stripe_submission_state` flag table; the
/// trait surface is the abstraction the orchestrator depends on.
pub trait StripeSubmissionControl: Send + Sync + core::fmt::Debug {
    /// Pause Stripe usage_record submissions for `(tenant_id,
    /// billing_period)`. Production wiring writes a row into
    /// `stripe_submission_state` so the WI-S10-003 adapter cron reads
    /// it at next tick + halts; the in-memory fake stores the same
    /// canonical (tenant, period) tuple in a `BTreeSet`.
    ///
    /// # Errors
    ///
    /// [`ReconcileStripePauseError::Backend`] on backend transport
    /// failure (D1 UPDATE rejected / network partition). Note: per
    /// WI-S10-004 §1 the canonical SEV-1 audit row landed BEFORE this
    /// call (fail-CLOSED envelope) — even if pause fails, the alert
    /// fired + the operator triages.
    fn pause(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Result<StripePauseOutcome, ReconcileStripePauseError>;

    /// Whether `(tenant_id, billing_period)` is currently paused.
    ///
    /// # Errors
    ///
    /// [`ReconcileStripePauseError::Backend`] on backend transport
    /// failure.
    fn is_paused(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Result<bool, ReconcileStripePauseError>;
}

/// In-memory Stripe-submission control surface fake.
#[derive(Debug, Default)]
pub struct InMemoryStripeSubmissionControl {
    inner: Mutex<BTreeSet<(Uuid, String)>>,
}

impl InMemoryStripeSubmissionControl {
    /// Construct an empty (all submissions open) control surface.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of paused (tenant, period) tuples (diagnostic).
    #[must_use]
    pub fn paused_count(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }
}

impl StripeSubmissionControl for InMemoryStripeSubmissionControl {
    fn pause(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Result<StripePauseOutcome, ReconcileStripePauseError> {
        let mut guard = self.inner.lock().map_err(|_| {
            ReconcileStripePauseError::Backend(
                "billing-reconcile stripe-pause mutex poisoned".to_string(),
            )
        })?;
        let key = (tenant_id, billing_period.to_string());
        if guard.contains(&key) {
            return Ok(StripePauseOutcome::AlreadyPaused);
        }
        guard.insert(key);
        Ok(StripePauseOutcome::Acked)
    }

    fn is_paused(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Result<bool, ReconcileStripePauseError> {
        let guard = self.inner.lock().map_err(|_| {
            ReconcileStripePauseError::Backend(
                "billing-reconcile stripe-pause mutex poisoned".to_string(),
            )
        })?;
        Ok(guard.contains(&(tenant_id, billing_period.to_string())))
    }
}

/// Always-failing Stripe-submission control surface for adversarial
/// fail-CLOSED tests.
#[derive(Debug, Default)]
pub struct FailingStripeSubmissionControl;

impl FailingStripeSubmissionControl {
    /// Construct a fresh always-failing control surface.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl StripeSubmissionControl for FailingStripeSubmissionControl {
    fn pause(
        &self,
        _tenant_id: Uuid,
        _billing_period: &str,
    ) -> Result<StripePauseOutcome, ReconcileStripePauseError> {
        Err(ReconcileStripePauseError::Backend(
            "induced billing-reconcile stripe-pause failure (test fixture)".to_string(),
        ))
    }

    fn is_paused(
        &self,
        _tenant_id: Uuid,
        _billing_period: &str,
    ) -> Result<bool, ReconcileStripePauseError> {
        Err(ReconcileStripePauseError::Backend(
            "induced billing-reconcile stripe-pause failure (test fixture)".to_string(),
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

    #[test]
    fn first_pause_returns_acked() {
        let c = InMemoryStripeSubmissionControl::new();
        let t = Uuid::now_v7();
        assert_eq!(c.pause(t, "2026-05").unwrap(), StripePauseOutcome::Acked);
        assert_eq!(c.paused_count(), 1);
    }

    #[test]
    fn second_pause_returns_already_paused() {
        let c = InMemoryStripeSubmissionControl::new();
        let t = Uuid::now_v7();
        c.pause(t, "2026-05").unwrap();
        assert_eq!(
            c.pause(t, "2026-05").unwrap(),
            StripePauseOutcome::AlreadyPaused
        );
        assert_eq!(c.paused_count(), 1);
    }

    #[test]
    fn is_paused_reflects_state() {
        let c = InMemoryStripeSubmissionControl::new();
        let t = Uuid::now_v7();
        assert!(!c.is_paused(t, "2026-05").unwrap());
        c.pause(t, "2026-05").unwrap();
        assert!(c.is_paused(t, "2026-05").unwrap());
    }

    #[test]
    fn pause_isolated_per_tenant_period() {
        let c = InMemoryStripeSubmissionControl::new();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        c.pause(t1, "2026-05").unwrap();
        assert!(c.is_paused(t1, "2026-05").unwrap());
        assert!(!c.is_paused(t2, "2026-05").unwrap());
        assert!(!c.is_paused(t1, "2026-06").unwrap());
    }

    #[test]
    fn failing_control_returns_backend_error() {
        let c = FailingStripeSubmissionControl::new();
        let t = Uuid::now_v7();
        let err = c.pause(t, "2026-05").unwrap_err();
        let backend = matches!(err, ReconcileStripePauseError::Backend(_));
        assert!(backend);
    }
}
