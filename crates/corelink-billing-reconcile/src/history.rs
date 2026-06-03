//! Drift-history ledger trait + InMemory orchestrator-side fake.
//!
//! Per WI-S10-004 §6.1.7 + §12 the canonical SOC 2 CC1.4 + GAAP ASC
//! 606 evidence trail requires a 7-year-retained drift-history ledger
//! per (tenant, billing_period). Production wiring at WI-S10-007 binds
//! this to the canonical D1 `billing_reconciliation_drift` table
//! (additive migration `migrations/d1/0019_billing_reconciliation_drift.sql`)
//! whose `(tenant_id, billing_period, run_started_at)` UNIQUE PK pins
//! INV-BILLING-NO-DUP at the storage layer (re-running the cron for the
//! same period is idempotent over the canonical 4-tuple).
//!
//! The trait surface is identical to the production binding so the
//! orchestrator's audit-fail-CLOSED envelope (audit emit BEFORE this
//! INSERT) holds at both fakes + the production wiring.

use std::collections::BTreeMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::error::ReconcileDriftHistoryError;
use crate::event::ReconcileDecision;

/// Canonical drift-history row shape (mirrors the D1 column set
/// landed by `migrations/d1/0019_billing_reconciliation_drift.sql`).
#[derive(Clone, Debug, PartialEq)]
pub struct DriftHistoryRow {
    /// Tenant id.
    pub tenant_id: Uuid,
    /// Canonical billing period (`YYYY-MM`).
    pub billing_period: String,
    /// Run-started watermark (Unix epoch ms; pinned BEFORE any layer
    /// query per the canonical RunStarted audit envelope).
    pub run_started_at: u64,
    /// Decision arm for the run.
    pub decision: ReconcileDecision,
    /// Free-form context (e.g. drift_pct value, primary layer).
    pub context: String,
}

/// Outcome of a drift-history INSERT. `Inserted` on first sight;
/// `AlreadyExistsIdempotent` when the canonical
/// `(tenant_id, billing_period, run_started_at)` UNIQUE PK is hit on
/// a re-run with the same watermark (rare; production wiring's CF Cron
/// DO uses `corelink_time::next_month_first_utc_midnight()` so the
/// watermark is canonical per cron tick — re-runs produce the same
/// row by construction).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DriftHistoryInsertOutcome {
    /// First-sight INSERT.
    Inserted,
    /// Same canonical row already present (idempotent re-run).
    AlreadyExistsIdempotent,
}

/// Drift-history ledger trait. Production wiring binds to the
/// canonical D1 `billing_reconciliation_drift` table; this trait
/// surface is the abstraction the orchestrator depends on.
pub trait DriftHistoryLedger: Send + Sync + core::fmt::Debug {
    /// INSERT a fresh drift-history row. Returns
    /// [`DriftHistoryInsertOutcome::AlreadyExistsIdempotent`] when the
    /// canonical 4-tuple PK already holds the byte-identical row
    /// (re-run idempotency).
    ///
    /// # Errors
    ///
    /// [`ReconcileDriftHistoryError::Backend`] on backend transport
    /// failure (D1 INSERT rejected / network partition / batch
    /// failure).
    fn insert(
        &self,
        row: &DriftHistoryRow,
    ) -> Result<DriftHistoryInsertOutcome, ReconcileDriftHistoryError>;

    /// Look up the most recent drift-history row for `(tenant_id,
    /// billing_period)`. Used by the SEV-1 escalation arm + the
    /// reconciliation idempotency check.
    ///
    /// # Errors
    ///
    /// [`ReconcileDriftHistoryError::Backend`] on backend transport
    /// failure.
    fn latest(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Result<Option<DriftHistoryRow>, ReconcileDriftHistoryError>;
}

/// In-memory drift-history ledger fake. Keyed by `(tenant_id,
/// billing_period, run_started_at)` so the canonical 4-tuple PK + the
/// idempotency check fall out naturally.
#[derive(Debug, Default)]
pub struct InMemoryDriftHistoryLedger {
    inner: Mutex<BTreeMap<(Uuid, String, u64), DriftHistoryRow>>,
}

impl InMemoryDriftHistoryLedger {
    /// Construct an empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every row (diagnostic).
    #[must_use]
    pub fn snapshot(&self) -> Vec<DriftHistoryRow> {
        match self.inner.lock() {
            Ok(g) => g.values().cloned().collect(),
            Err(p) => p.into_inner().values().cloned().collect(),
        }
    }

    /// Number of rows captured.
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

impl DriftHistoryLedger for InMemoryDriftHistoryLedger {
    fn insert(
        &self,
        row: &DriftHistoryRow,
    ) -> Result<DriftHistoryInsertOutcome, ReconcileDriftHistoryError> {
        let mut guard = self.inner.lock().map_err(|_| {
            ReconcileDriftHistoryError::Backend(
                "billing-reconcile drift history mutex poisoned".to_string(),
            )
        })?;
        let key = (
            row.tenant_id,
            row.billing_period.clone(),
            row.run_started_at,
        );
        if let Some(existing) = guard.get(&key) {
            if existing == row {
                return Ok(DriftHistoryInsertOutcome::AlreadyExistsIdempotent);
            }
            // Same PK, divergent content — this is a CRITICAL signal in
            // production wiring (the canonical watermark monotonicity
            // is broken). The trait surface returns Backend so the
            // orchestrator surfaces it via fail-CLOSED.
            return Err(ReconcileDriftHistoryError::Backend(format!(
                "drift-history PK collision with diverged content: tenant={} billing_period={} run_started_at={}",
                row.tenant_id, row.billing_period, row.run_started_at
            )));
        }
        guard.insert(key, row.clone());
        Ok(DriftHistoryInsertOutcome::Inserted)
    }

    fn latest(
        &self,
        tenant_id: Uuid,
        billing_period: &str,
    ) -> Result<Option<DriftHistoryRow>, ReconcileDriftHistoryError> {
        let guard = self.inner.lock().map_err(|_| {
            ReconcileDriftHistoryError::Backend(
                "billing-reconcile drift history mutex poisoned".to_string(),
            )
        })?;
        // BTreeMap iteration is ordered by key; the third tuple slot is
        // `run_started_at` so the last matching key for the
        // (tenant, period) prefix is the most recent.
        let mut latest: Option<&DriftHistoryRow> = None;
        for ((t, p, _), row) in guard.iter() {
            if *t == tenant_id && p == billing_period {
                latest = Some(row);
            }
        }
        Ok(latest.cloned())
    }
}

/// Always-failing drift-history ledger for adversarial fail-CLOSED
/// tests.
#[derive(Debug, Default)]
pub struct FailingDriftHistoryLedger;

impl FailingDriftHistoryLedger {
    /// Construct a fresh always-failing ledger.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl DriftHistoryLedger for FailingDriftHistoryLedger {
    fn insert(
        &self,
        _row: &DriftHistoryRow,
    ) -> Result<DriftHistoryInsertOutcome, ReconcileDriftHistoryError> {
        Err(ReconcileDriftHistoryError::Backend(
            "induced billing-reconcile drift history failure (test fixture)".to_string(),
        ))
    }

    fn latest(
        &self,
        _tenant_id: Uuid,
        _billing_period: &str,
    ) -> Result<Option<DriftHistoryRow>, ReconcileDriftHistoryError> {
        Err(ReconcileDriftHistoryError::Backend(
            "induced billing-reconcile drift history failure (test fixture)".to_string(),
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

    fn row(t: Uuid, period: &str, run_at: u64, dec: ReconcileDecision) -> DriftHistoryRow {
        DriftHistoryRow {
            tenant_id: t,
            billing_period: period.to_string(),
            run_started_at: run_at,
            decision: dec,
            context: "test".to_string(),
        }
    }

    #[test]
    fn first_insert_returns_inserted() {
        let l = InMemoryDriftHistoryLedger::new();
        let t = Uuid::now_v7();
        let r = row(
            t,
            "2026-05",
            100,
            ReconcileDecision::NoDrift { max_drift_pct: 0.0 },
        );
        assert_eq!(l.insert(&r).unwrap(), DriftHistoryInsertOutcome::Inserted);
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn duplicate_same_content_returns_idempotent() {
        let l = InMemoryDriftHistoryLedger::new();
        let t = Uuid::now_v7();
        let r = row(
            t,
            "2026-05",
            100,
            ReconcileDecision::NoDrift { max_drift_pct: 0.0 },
        );
        l.insert(&r).unwrap();
        assert_eq!(
            l.insert(&r).unwrap(),
            DriftHistoryInsertOutcome::AlreadyExistsIdempotent
        );
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn duplicate_diverged_content_errors() {
        let l = InMemoryDriftHistoryLedger::new();
        let t = Uuid::now_v7();
        let r1 = row(
            t,
            "2026-05",
            100,
            ReconcileDecision::NoDrift { max_drift_pct: 0.0 },
        );
        l.insert(&r1).unwrap();
        let r2 = row(
            t,
            "2026-05",
            100,
            ReconcileDecision::PageSev2 {
                max_drift_pct: 0.005,
                primary_layer: crate::event::ReconcileLayerKind::Layer1Emit,
            },
        );
        let err = l.insert(&r2).unwrap_err();
        let backend = matches!(err, ReconcileDriftHistoryError::Backend(_));
        assert!(backend);
    }

    #[test]
    fn latest_returns_most_recent_per_tenant_period() {
        let l = InMemoryDriftHistoryLedger::new();
        let t = Uuid::now_v7();
        let r1 = row(
            t,
            "2026-05",
            100,
            ReconcileDecision::NoDrift { max_drift_pct: 0.0 },
        );
        let r2 = row(
            t,
            "2026-05",
            200,
            ReconcileDecision::TicketSev3 {
                max_drift_pct: 0.0005,
                primary_layer: crate::event::ReconcileLayerKind::Layer1Emit,
            },
        );
        l.insert(&r1).unwrap();
        l.insert(&r2).unwrap();
        let latest = l.latest(t, "2026-05").unwrap().unwrap();
        assert_eq!(latest.run_started_at, 200);
    }

    #[test]
    fn latest_none_for_unknown_tenant() {
        let l = InMemoryDriftHistoryLedger::new();
        let t = Uuid::now_v7();
        let other = Uuid::now_v7();
        l.insert(&row(
            t,
            "2026-05",
            100,
            ReconcileDecision::NoDrift { max_drift_pct: 0.0 },
        ))
        .unwrap();
        assert!(l.latest(other, "2026-05").unwrap().is_none());
    }

    #[test]
    fn failing_ledger_returns_backend_error() {
        let l = FailingDriftHistoryLedger::new();
        let t = Uuid::now_v7();
        let r = row(
            t,
            "2026-05",
            100,
            ReconcileDecision::NoDrift { max_drift_pct: 0.0 },
        );
        let err = l.insert(&r).unwrap_err();
        let backend = matches!(err, ReconcileDriftHistoryError::Backend(_));
        assert!(backend);
    }
}
