//! Replay idempotency ledger trait + in-memory fake.
//!
//! ## Why idempotency on `request_id` is the canonical forensic
//! ## determinism rationale (WI-S10-006 §1 invariant 6 / §6.1)
//!
//! Replay is the audit-grade dispute-resolution primitive: the same
//! customer dispute / SOC 2 audit / drift investigation may be re-run
//! multiple times across the 7y retention window (auditor sampling +
//! customer follow-up + SEV-1 post-mortem). The canonical contract is
//!
//! > **same `request_id` → byte-identical outcome**
//!
//! A re-submission of the same logical request that produced a
//! divergent outcome (different reconstruction; different decision arm)
//! would itself be a tampering signal — the auditor evidence trail
//! would lose its anchor. The idempotency ledger short-circuits the
//! second submission to the prior outcome so the orchestrator NEVER
//! double-executes.
//!
//! Production wiring at WI-S10-007 binds this to the canonical D1
//! `billing_replay_audit` table (additive migration
//! `migrations/d1/0021_billing_replay_audit.sql`); the in-memory fake
//! here pins the same trait surface so the orchestrator's idempotency
//! arm exercises the same fail-CLOSED envelope discipline.

use std::collections::BTreeMap;
use std::sync::Mutex;

use uuid::Uuid;

use super::error::ReplayIdempotencyError;
use super::event::{ReplayOutcome, ReplayRequest};

/// Whether a `record_outcome` insert was the FIRST sighting of a
/// `request_id` (canonical execution arm) or a re-submission that
/// reused the prior outcome (canonical idempotent-replay arm).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecordOutcome {
    /// First sighting: the orchestrator inserted a fresh row. The
    /// canonical `executed` audit envelope fires alongside.
    Inserted,
    /// Idempotent re-submission: a row with the same `request_id`
    /// already existed; the orchestrator returns the prior outcome
    /// without re-running the pipeline. The canonical
    /// `request_authorized` audit envelope fires with
    /// `idempotent_replay = true`.
    AlreadyExistsIdempotent {
        /// The prior outcome that the orchestrator reused.
        prior: ReplayOutcome,
    },
}

/// Replay idempotency ledger trait. Production wiring composes the
/// canonical D1 `billing_replay_audit` table at WI-S10-007 PRR ship
/// gate per the `trait-abstraction-defer` charter pattern.
pub trait ReplayIdempotencyLedger: Send + Sync + core::fmt::Debug {
    /// Look up the canonical prior outcome for `request_id` (returns
    /// `Ok(None)` when this is a first-sighting `request_id`).
    ///
    /// # Errors
    ///
    /// Returns [`ReplayIdempotencyError::Backend`] on backend failure.
    fn lookup(&self, request_id: Uuid) -> Result<Option<ReplayOutcome>, ReplayIdempotencyError>;

    /// Record a fresh outcome for `request` + `outcome`. Returns
    /// [`RecordOutcome::Inserted`] on first-sighting; returns
    /// [`RecordOutcome::AlreadyExistsIdempotent`] on re-submission
    /// (the prior outcome is reused).
    ///
    /// Caller must verify that the canonical request payload (the
    /// (tenant, billing_period, reason) tuple) MATCHES the prior
    /// outcome's payload; a mismatch surfaces as
    /// [`ReplayIdempotencyError::DivergentPayload`] (SEV-1 forensic
    /// anomaly).
    ///
    /// # Errors
    ///
    /// - [`ReplayIdempotencyError::Backend`] on backend failure.
    /// - [`ReplayIdempotencyError::DivergentPayload`] when a row with
    ///   the same `request_id` exists with a divergent
    ///   `(tenant, billing_period, reason)` tuple.
    fn record_outcome(
        &self,
        request: &ReplayRequest,
        outcome: ReplayOutcome,
    ) -> Result<RecordOutcome, ReplayIdempotencyError>;
}

/// In-memory replay idempotency ledger backed by `BTreeMap<request_id,
/// LedgerRow>`. Cloning shares the underlying map so orchestrator +
/// verifier can hold separate handles.
#[derive(Clone, Debug, Default)]
pub struct InMemoryReplayIdempotencyLedger {
    inner: std::sync::Arc<Mutex<BTreeMap<Uuid, LedgerRow>>>,
}

#[derive(Clone, Debug)]
struct LedgerRow {
    tenant_id: Uuid,
    billing_period: String,
    reason: super::event::ReplayReason,
    outcome: ReplayOutcome,
}

impl InMemoryReplayIdempotencyLedger {
    /// Construct a fresh ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of distinct `request_id` rows. Useful for assertions in
    /// tests (e.g. idempotent re-submission keeps `len() == 1`).
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether the ledger is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ReplayIdempotencyLedger for InMemoryReplayIdempotencyLedger {
    fn lookup(&self, request_id: Uuid) -> Result<Option<ReplayOutcome>, ReplayIdempotencyError> {
        let guard = self.inner.lock().map_err(|_| {
            ReplayIdempotencyError::Backend(
                "billing-replay idempotency ledger mutex poisoned (lookup)".to_string(),
            )
        })?;
        Ok(guard.get(&request_id).map(|r| r.outcome.clone()))
    }

    fn record_outcome(
        &self,
        request: &ReplayRequest,
        outcome: ReplayOutcome,
    ) -> Result<RecordOutcome, ReplayIdempotencyError> {
        let mut guard = self.inner.lock().map_err(|_| {
            ReplayIdempotencyError::Backend(
                "billing-replay idempotency ledger mutex poisoned (record_outcome)".to_string(),
            )
        })?;
        if let Some(existing) = guard.get(&request.request_id) {
            // Divergent-payload check: the canonical idempotency
            // contract is "same `request_id` → same payload"; a
            // replayed `request_id` paired with a different
            // (tenant, billing_period, reason) tuple is a SEV-1
            // forensic anomaly the orchestrator surfaces immediately.
            if existing.tenant_id != request.tenant_id
                || existing.billing_period != request.billing_period
                || existing.reason != request.reason
            {
                return Err(ReplayIdempotencyError::DivergentPayload(
                    request.request_id.to_string(),
                ));
            }
            return Ok(RecordOutcome::AlreadyExistsIdempotent {
                prior: existing.outcome.clone(),
            });
        }
        guard.insert(
            request.request_id,
            LedgerRow {
                tenant_id: request.tenant_id,
                billing_period: request.billing_period.clone(),
                reason: request.reason,
                outcome,
            },
        );
        Ok(RecordOutcome::Inserted)
    }
}

/// Always-failing idempotency ledger for adversarial tests of the
/// fail-CLOSED envelope.
#[derive(Debug, Default)]
pub struct FailingReplayIdempotencyLedger;

impl FailingReplayIdempotencyLedger {
    /// Construct a fresh always-failing ledger.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ReplayIdempotencyLedger for FailingReplayIdempotencyLedger {
    fn lookup(&self, _request_id: Uuid) -> Result<Option<ReplayOutcome>, ReplayIdempotencyError> {
        Err(ReplayIdempotencyError::Backend(
            "induced billing-replay idempotency ledger failure (test fixture)".to_string(),
        ))
    }

    fn record_outcome(
        &self,
        _request: &ReplayRequest,
        _outcome: ReplayOutcome,
    ) -> Result<RecordOutcome, ReplayIdempotencyError> {
        Err(ReplayIdempotencyError::Backend(
            "induced billing-replay idempotency ledger failure (test fixture)".to_string(),
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
    use super::super::event::{ReconstructedLayers, ReplayDecision, ReplayReason};

    fn req(rid: Uuid, t: Uuid, period: &str, reason: ReplayReason) -> ReplayRequest {
        ReplayRequest::new(
            rid,
            Uuid::now_v7(),
            super::super::event::BILLING_FORENSICS_ADMIN_ROLE,
            t,
            period,
            reason,
        )
    }

    fn outcome(landed_at: u64) -> ReplayOutcome {
        ReplayOutcome::new(
            landed_at,
            ReplayDecision::Executed {
                layer_diverged: false,
            },
            ReconstructedLayers::new(100, 100, 100),
            ReconstructedLayers::new(100, 100, 100),
        )
    }

    #[test]
    fn first_sighting_inserts() {
        let l = InMemoryReplayIdempotencyLedger::new();
        let rid = Uuid::now_v7();
        let r = req(rid, Uuid::now_v7(), "2026-05", ReplayReason::CustomerDispute);
        let o = outcome(1);
        let r_out = l.record_outcome(&r, o.clone()).unwrap();
        assert!(matches!(r_out, RecordOutcome::Inserted));
        assert_eq!(l.len(), 1);
        let prior = l.lookup(rid).unwrap();
        assert_eq!(prior, Some(o));
    }

    #[test]
    fn re_submission_returns_prior_outcome_idempotent() {
        let l = InMemoryReplayIdempotencyLedger::new();
        let rid = Uuid::now_v7();
        let r = req(rid, Uuid::now_v7(), "2026-05", ReplayReason::CustomerDispute);
        let o1 = outcome(1);
        let o2 = outcome(2);
        l.record_outcome(&r, o1.clone()).unwrap();
        let r_out = l.record_outcome(&r, o2).unwrap();
        match r_out {
            RecordOutcome::AlreadyExistsIdempotent { prior } => assert_eq!(prior, o1),
            RecordOutcome::Inserted => unreachable!("expected idempotent re-fire"),
        }
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn divergent_payload_surfaces_error() {
        let l = InMemoryReplayIdempotencyLedger::new();
        let rid = Uuid::now_v7();
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        let r1 = req(rid, t1, "2026-05", ReplayReason::CustomerDispute);
        let r2 = req(rid, t2, "2026-05", ReplayReason::CustomerDispute);
        l.record_outcome(&r1, outcome(1)).unwrap();
        let err = l.record_outcome(&r2, outcome(2)).unwrap_err();
        assert!(matches!(err, ReplayIdempotencyError::DivergentPayload(_)));
        // Ledger len unchanged.
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn divergent_billing_period_surfaces_error() {
        let l = InMemoryReplayIdempotencyLedger::new();
        let rid = Uuid::now_v7();
        let t = Uuid::now_v7();
        let r1 = req(rid, t, "2026-05", ReplayReason::CustomerDispute);
        let r2 = req(rid, t, "2026-06", ReplayReason::CustomerDispute);
        l.record_outcome(&r1, outcome(1)).unwrap();
        let err = l.record_outcome(&r2, outcome(2)).unwrap_err();
        assert!(matches!(err, ReplayIdempotencyError::DivergentPayload(_)));
    }

    #[test]
    fn divergent_reason_surfaces_error() {
        let l = InMemoryReplayIdempotencyLedger::new();
        let rid = Uuid::now_v7();
        let t = Uuid::now_v7();
        let r1 = req(rid, t, "2026-05", ReplayReason::CustomerDispute);
        let r2 = req(rid, t, "2026-05", ReplayReason::ComplianceAudit);
        l.record_outcome(&r1, outcome(1)).unwrap();
        let err = l.record_outcome(&r2, outcome(2)).unwrap_err();
        assert!(matches!(err, ReplayIdempotencyError::DivergentPayload(_)));
    }

    #[test]
    fn lookup_returns_none_for_unknown_request_id() {
        let l = InMemoryReplayIdempotencyLedger::new();
        let r = l.lookup(Uuid::now_v7()).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn cloned_ledger_shares_storage() {
        let l1 = InMemoryReplayIdempotencyLedger::new();
        let l2 = l1.clone();
        let rid = Uuid::now_v7();
        let r = req(rid, Uuid::now_v7(), "2026-05", ReplayReason::CustomerDispute);
        l1.record_outcome(&r, outcome(1)).unwrap();
        assert_eq!(l2.len(), 1);
    }

    #[test]
    fn failing_ledger_returns_backend_error_on_lookup() {
        let l = FailingReplayIdempotencyLedger::new();
        let err = l.lookup(Uuid::now_v7()).unwrap_err();
        assert!(matches!(err, ReplayIdempotencyError::Backend(_)));
    }

    #[test]
    fn failing_ledger_returns_backend_error_on_record() {
        let l = FailingReplayIdempotencyLedger::new();
        let r = req(Uuid::now_v7(), Uuid::now_v7(), "2026-05", ReplayReason::CustomerDispute);
        let err = l.record_outcome(&r, outcome(1)).unwrap_err();
        assert!(matches!(err, ReplayIdempotencyError::Backend(_)));
    }

    #[test]
    fn empty_ledger_reports_empty() {
        let l = InMemoryReplayIdempotencyLedger::new();
        assert!(l.is_empty());
        assert_eq!(l.len(), 0);
    }
}
