//! [`RotationStateMachine`] trait + [`InMemoryRotationStateMachine`]
//! orchestrator (D1-backed state machine driver; CI fake).
//!
//! ## Run pipeline
//!
//! Each rotation call follows the canonical fail-CLOSED audit envelope:
//!
//! 1. **F-001 Mutex acquired** (per-instance; prevents concurrent state
//!    transitions on the same asset class+region pair).
//! 2. **Lookup current keys** (active + overlap from store).
//! 3. **Validate transition** (via adapter `validate_transition`).
//! 4. **Audit emit BEFORE state mutation** (INV-KEY-AUDIT; fail-CLOSED:
//!    audit failure aborts the transition).
//! 5. **Store upsert** (D1 atomic batch in production).
//!
//! ## INV-KEY-AUDIT
//!
//! Every state transition emits a `RotationRecord` capturing:
//! `asset_class`, `region`, `key_id`, `state_from`, `state_to`,
//! `phase`, `now_ms`. In production this maps to a CloudEvent inserted
//! into the audit_outbox table in the same D1 batch as the
//! `rotation_state` UPDATE.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use corelink_rotation_adapters::{AssetClass, KeyHandle, KeyState, RotationError};

/// Canonical rotation phase taxonomy.
///
/// `#[non_exhaustive]` per CoreLink codex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RotationPhase {
    /// Key material generated (Pending state).
    Generate,
    /// Key promoted to Active; previous moved to Overlap.
    Promote,
    /// Downstream re-keying in progress (TDK envelope re-wrap).
    RekeyDownstream,
    /// Overlap key retired (Overlap → Retired).
    Retire,
    /// Key material destroyed (Retired → Destroyed).
    Destroy,
    /// PAT-ROLL-FORWARD-001 auto-rollback triggered.
    Rollback,
}

impl RotationPhase {
    /// Return the canonical phase string for metric labels + audit events.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generate => "generate",
            Self::Promote => "promote",
            Self::RekeyDownstream => "rekey_downstream",
            Self::Retire => "retire",
            Self::Destroy => "destroy",
            Self::Rollback => "rollback",
        }
    }
}

impl core::fmt::Display for RotationPhase {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A state transition record (INV-KEY-AUDIT).
///
/// In production: mapped to a CloudEvent inserted into `audit_outbox`
/// in the same D1 batch as the `rotation_state` UPDATE.
#[derive(Debug, Clone)]
pub struct RotationRecord {
    /// Asset class rotated.
    pub asset_class: AssetClass,
    /// Region (per-region key isolation).
    pub region: String,
    /// Key ID involved in the transition.
    pub key_id: u64,
    /// State before the transition.
    pub state_from: KeyState,
    /// State after the transition.
    pub state_to: KeyState,
    /// Phase that triggered this record.
    pub phase: RotationPhase,
    /// Timestamp milliseconds.
    pub now_ms: u64,
}

/// Rotation state-machine store (per-asset key store; CI fake uses
/// `InMemoryRotationStateMachine`).
pub trait RotationStateMachine: core::fmt::Debug {
    /// Record a state transition + emit audit (INV-KEY-AUDIT).
    /// Returns `Err(RotationError::Audit)` on audit emit failure
    /// (fail-CLOSED; state NOT mutated).
    ///
    /// # Errors
    ///
    /// [`RotationError::Audit`] on audit failure.
    /// [`RotationError::Storage`] on store failure.
    fn record_transition(
        &self,
        handle: &KeyHandle,
        state_to: KeyState,
        phase: RotationPhase,
        region: &str,
        now_ms: u64,
    ) -> Result<(), RotationError>;

    /// Return all recorded transitions (for tests + observability).
    fn snapshot(&self) -> Vec<RotationRecord>;
}

/// In-memory rotation state-machine (CI fake; production uses D1).
#[derive(Debug)]
pub struct InMemoryRotationStateMachine {
    records: Arc<Mutex<HashMap<u64, Vec<RotationRecord>>>>,
    /// Optionally inject audit failure for fail-CLOSED tests.
    fail_audit: bool,
}

impl InMemoryRotationStateMachine {
    /// Construct a new in-memory state machine.
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            fail_audit: false,
        }
    }

    /// Construct with audit-fail injection (for fail-CLOSED tests).
    #[must_use]
    pub fn with_failing_audit() -> Self {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            fail_audit: true,
        }
    }

    /// Return the number of recorded transitions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|v| v.len())
            .sum()
    }

    /// Return `true` if no transitions have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for InMemoryRotationStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl RotationStateMachine for InMemoryRotationStateMachine {
    fn record_transition(
        &self,
        handle: &KeyHandle,
        state_to: KeyState,
        phase: RotationPhase,
        region: &str,
        now_ms: u64,
    ) -> Result<(), RotationError> {
        if self.fail_audit {
            return Err(RotationError::Audit(
                "injected audit failure (fail-CLOSED test)".to_string(),
            ));
        }

        let record = RotationRecord {
            asset_class: handle.asset_class,
            region: region.to_string(),
            key_id: handle.key_id,
            state_from: handle.state,
            state_to,
            phase,
            now_ms,
        };

        let mut records = self
            .records
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;
        records.entry(handle.key_id).or_default().push(record);
        Ok(())
    }

    fn snapshot(&self) -> Vec<RotationRecord> {
        let records = self
            .records
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut all: Vec<RotationRecord> = records.values().flatten().cloned().collect();
        all.sort_by_key(|r| r.now_ms);
        all
    }
}
