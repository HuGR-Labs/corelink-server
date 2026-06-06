//! Per-tenant policy ledger for the Restriction + Objection DSR arms.
//!
//! WI-S11-001 ships Restriction + Objection as **policy-only** DSR arms
//! (no data mutation; flag flips on the tenant policy table). The
//! production wiring binds these to the canonical
//! `tenant_processing_policy` Neon table — deferred to WI-S11-008 PRR
//! ship gate. The harness mirrors the canonical effect:
//!
//! - **Restriction**: subsequent writes for the tenant return
//!   `processing_restricted` (decision = [`PolicyDecision::Restricted`]);
//!   reads remain allowed.
//! - **Objection**: per-tenant + per-processing-purpose pause flag;
//!   pipelines matching `(tenant_id, purpose)` decide
//!   [`PolicyDecision::Objected`].

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use uuid::Uuid;

/// Canonical processing-restriction flag for a tenant. A tenant with
/// `Paused` writes returns `processing_restricted` (403) on every
/// write attempt; reads remain allowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RestrictionFlag {
    /// Default arm — writes + reads both allowed.
    Allowed,
    /// Writes are paused; reads remain allowed.
    Paused,
}

/// Canonical decision returned by [`TenantPolicyLedger::evaluate_write`]
/// + [`TenantPolicyLedger::evaluate_purpose`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PolicyDecision {
    /// Write / processing allowed — happy path.
    Allowed,
    /// Restriction in effect; future writes return 403
    /// `processing_restricted`. Reads still allowed.
    Restricted,
    /// Objection in effect for `(tenant, purpose)`; the matching
    /// pipeline pauses.
    Objected {
        /// Canonical processing purpose mnemonic (e.g.
        /// `marketing_analytics`, `aggregate_telemetry`).
        purpose: String,
    },
}

/// Per-tenant policy ledger. Cloning the inner `Arc` handles keeps
/// observability state shared across the orchestrator + tests.
#[derive(Clone, Debug, Default)]
pub struct TenantPolicyLedger {
    restriction: Arc<Mutex<HashMap<Uuid, RestrictionFlag>>>,
    objection: Arc<Mutex<HashMap<Uuid, BTreeSet<String>>>>,
}

impl TenantPolicyLedger {
    /// Construct a fresh ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Flip the canonical restriction flag for `tenant_id`. Returns
    /// the prior flag (default [`RestrictionFlag::Allowed`]).
    pub fn set_restriction(&self, tenant_id: Uuid, flag: RestrictionFlag) -> RestrictionFlag {
        let mut guard = match self.restriction.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .insert(tenant_id, flag)
            .unwrap_or(RestrictionFlag::Allowed)
    }

    /// Canonical restriction flag (default [`RestrictionFlag::Allowed`]).
    #[must_use]
    pub fn restriction(&self, tenant_id: Uuid) -> RestrictionFlag {
        let guard = match self.restriction.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .get(&tenant_id)
            .copied()
            .unwrap_or(RestrictionFlag::Allowed)
    }

    /// Record an objection to `(tenant_id, purpose)`. Subsequent
    /// [`Self::evaluate_purpose`] calls for the matching purpose
    /// return [`PolicyDecision::Objected`].
    pub fn record_objection(&self, tenant_id: Uuid, purpose: impl Into<String>) {
        let mut guard = match self.objection.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.entry(tenant_id).or_default().insert(purpose.into());
    }

    /// Whether `(tenant_id, purpose)` is currently under objection.
    #[must_use]
    pub fn is_objected(&self, tenant_id: Uuid, purpose: &str) -> bool {
        let guard = match self.objection.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .get(&tenant_id)
            .is_some_and(|set| set.contains(purpose))
    }

    /// Snapshot the canonical objection set for `tenant_id`.
    #[must_use]
    pub fn snapshot_objections(&self, tenant_id: Uuid) -> Vec<String> {
        let guard = match self.objection.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .get(&tenant_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Canonical "should this write proceed?" decision. Mirrors the
    /// production middleware: a paused tenant returns
    /// [`PolicyDecision::Restricted`]; otherwise
    /// [`PolicyDecision::Allowed`].
    #[must_use]
    pub fn evaluate_write(&self, tenant_id: Uuid) -> PolicyDecision {
        match self.restriction(tenant_id) {
            RestrictionFlag::Allowed => PolicyDecision::Allowed,
            RestrictionFlag::Paused => PolicyDecision::Restricted,
        }
    }

    /// Canonical "should this processing pipeline proceed?" decision
    /// for `(tenant_id, purpose)`.
    #[must_use]
    pub fn evaluate_purpose(&self, tenant_id: Uuid, purpose: &str) -> PolicyDecision {
        if self.is_objected(tenant_id, purpose) {
            return PolicyDecision::Objected {
                purpose: purpose.to_string(),
            };
        }
        PolicyDecision::Allowed
    }
}
