//! `e2e-failover-router` — DR-16 end-to-end active-failover harness.
//!
//! Composes the canonical pure-logic orchestrators from
//! [`corelink_failover_router`] (multi-signal detection + read routing +
//! write blocking) under a **logical clock** so the suite is byte-for-byte
//! reproducible across runs.
//!
//! # Scenarios pinned (see `tests/scenarios.rs`)
//!
//! 1. **primary-up — no failover.** Probe reports healthy; reads route to
//!    primary; writes allowed; no audit event emitted.
//! 2. **primary-degraded — failover trigger.** Probe reports
//!    `RegionHealth::Degraded` (all three signals); reads route to sibling;
//!    writes blocked; `failover.detected` emitted BEFORE the route mutation
//!    (fail-CLOSED ordering).
//! 3. **split-brain prevention.** During the failover window, the primary
//!    MUST never accept writes simultaneously with the sibling holding the
//!    write lease. Assertion: every write decision against the primary
//!    returns `WriteMode::Blocked` while the sibling is the active lease
//!    holder.
//! 4. **failback after recovery.** Once the primary returns to
//!    `RegionHealth::Healthy`, reads re-route back to primary; writes
//!    re-allowed; the audit record sequence shows
//!    `failover.detected` → `failover.resolved` in order.
//! 5. **partial-region.** Some Durable Objects in the primary remain
//!    healthy while the region-level health probe flags Degraded. The
//!    router MUST treat the region uniformly (route ALL reads to sibling)
//!    rather than per-DO — partial-region health does NOT short-circuit
//!    the failover decision because residency invariants are evaluated at
//!    the region tier.
//!
//! # Charter constraints
//!
//! - `#![forbid(unsafe_code)]`
//! - No `unwrap` / `expect` / `panic` outside `#[cfg(test)]` (enforced via
//!   `Cargo.toml` lints).
//! - No `tokio` in `src/` (orchestrators are sync).
//! - Logical clock only — never `SystemTime::now()`.
//! - Audit fail-CLOSED ordering enforced + tested (scenario 2 + 4).
//! - No split-brain enforced + tested (scenario 3).

#![forbid(unsafe_code)]

use std::sync::Mutex;

use thiserror::Error;

/// Logical clock pinned at `t0_ms`; advances via
/// [`LogicalClock::advance_ms`]. Used in lieu of `SystemTime::now()` so
/// scenarios are byte-for-byte reproducible.
#[derive(Debug)]
pub struct LogicalClock {
    inner: Mutex<u64>,
}

impl LogicalClock {
    /// Construct a fresh clock pinned at `t0_ms` (Unix-epoch ms).
    #[must_use]
    pub fn new(t0_ms: u64) -> Self {
        Self {
            inner: Mutex::new(t0_ms),
        }
    }

    /// Read the current logical instant.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::ClockPoisoned`] when the inner mutex is
    /// poisoned (only reachable on a panic in a prior `now_ms` /
    /// `advance_ms` call — impossible in production because no method
    /// holds the lock across a panic).
    pub fn now_ms(&self) -> Result<u64, HarnessError> {
        self.inner
            .lock()
            .map(|g| *g)
            .map_err(|_| HarnessError::ClockPoisoned)
    }

    /// Advance the clock by `delta_ms`.
    ///
    /// # Errors
    ///
    /// Same as [`now_ms`].
    pub fn advance_ms(&self, delta_ms: u64) -> Result<(), HarnessError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| HarnessError::ClockPoisoned)?;
        *g = g.saturating_add(delta_ms);
        Ok(())
    }
}

/// Write-lease holder — tracks which region currently owns writes. The
/// harness uses this to assert split-brain absence: at any logical instant
/// the lease holder is exactly one region.
#[derive(Debug)]
pub struct WriteLeaseLedger {
    inner: Mutex<Vec<LeaseEntry>>,
}

/// One row in the write-lease audit ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseEntry {
    /// Logical instant of the lease event (start of the epoch).
    pub at_ms: u64,
    /// Region that holds the lease starting at `at_ms`.
    pub holder: String,
    /// Reason for the lease change (`bootstrap`, `failover.promoted`,
    /// `failback.demoted`).
    pub reason: String,
}

impl WriteLeaseLedger {
    /// Construct a fresh ledger with `initial_holder` set at `t0_ms`.
    #[must_use]
    pub fn bootstrap(initial_holder: impl Into<String>, t0_ms: u64) -> Self {
        Self {
            inner: Mutex::new(vec![LeaseEntry {
                at_ms: t0_ms,
                holder: initial_holder.into(),
                reason: "bootstrap".to_owned(),
            }]),
        }
    }

    /// Record a lease handover at `at_ms` with `reason`.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::LedgerPoisoned`] when the inner mutex is
    /// poisoned.
    pub fn handover(
        &self,
        new_holder: impl Into<String>,
        at_ms: u64,
        reason: impl Into<String>,
    ) -> Result<(), HarnessError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| HarnessError::LedgerPoisoned)?;
        g.push(LeaseEntry {
            at_ms,
            holder: new_holder.into(),
            reason: reason.into(),
        });
        Ok(())
    }

    /// Returns the current write-lease holder at `at_ms`.
    ///
    /// # Errors
    ///
    /// Same as [`handover`].
    pub fn holder_at(&self, at_ms: u64) -> Result<Option<String>, HarnessError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| HarnessError::LedgerPoisoned)?;
        Ok(g.iter()
            .filter(|e| e.at_ms <= at_ms)
            .next_back()
            .map(|e| e.holder.clone()))
    }

    /// Returns a snapshot of all lease entries (cloned). Used by
    /// scenario 3 to assert split-brain absence (lease epochs do not
    /// overlap).
    ///
    /// # Errors
    ///
    /// Same as [`handover`].
    pub fn snapshot(&self) -> Result<Vec<LeaseEntry>, HarnessError> {
        let g = self
            .inner
            .lock()
            .map_err(|_| HarnessError::LedgerPoisoned)?;
        Ok(g.clone())
    }

    /// Returns the number of distinct holders observed in the ledger.
    ///
    /// # Errors
    ///
    /// Same as [`handover`].
    pub fn distinct_holder_count(&self) -> Result<usize, HarnessError> {
        let snap = self.snapshot()?;
        let mut seen: Vec<String> = Vec::with_capacity(snap.len());
        for entry in &snap {
            if !seen.iter().any(|s| s == &entry.holder) {
                seen.push(entry.holder.clone());
            }
        }
        Ok(seen.len())
    }
}

/// Error taxonomy for the harness.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HarnessError {
    /// Logical clock mutex poisoned.
    #[error("logical clock mutex poisoned")]
    ClockPoisoned,
    /// Write-lease ledger mutex poisoned.
    #[error("write-lease ledger mutex poisoned")]
    LedgerPoisoned,
    /// Scenario invariant violated (split-brain, ordering, etc.).
    #[error("invariant violated: {0}")]
    InvariantViolated(String),
}
