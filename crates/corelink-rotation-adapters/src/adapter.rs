//! [`RotationAdapter`] trait + canonical read/write state predicates
//! (INV-KEY-OVERLAP + INV-KEY-NO-SKIP enforcement at the trait surface).

use crate::error::RotationError;
use crate::types::{KeyHandle, KeyState};

/// Return `true` if `state` is valid for **write** operations.
///
/// Per INV-KEY-NO-SKIP (CRITICAL; invariant registry §3.13): only
/// `Active` keys may accept writes. Keys in any other state
/// (`Pending`, `Overlap`, `Retired`, `Destroyed`, `RolledBack`) MUST
/// reject write operations.
#[must_use]
pub fn is_valid_write_state(state: KeyState) -> bool {
    matches!(state, KeyState::Active)
}

/// Return `true` if `state` is valid for **read** operations.
///
/// Per INV-KEY-OVERLAP (HIGH; invariant registry §3.13): during the
/// overlap window both the previous `Overlap` key and the new `Active`
/// key are accepted for reads. `Pending`, `Retired`, `Destroyed`, and
/// `RolledBack` reject reads.
#[must_use]
pub fn is_valid_read_state(state: KeyState) -> bool {
    matches!(state, KeyState::Active | KeyState::Overlap)
}

/// Per-asset rotation adapter trait.
///
/// Each of the 5 asset classes has its own adapter implementation.
/// The trait is synchronous (no `async_trait` dependency, wasm32-clean)
/// because the in-memory fakes used in CI are fully synchronous; the
/// production wiring introduces the async binding at the Workers layer,
/// not here.
///
/// # State machine invariants enforced at the trait surface
///
/// - `generate` → new handle in `Pending` state.
/// - `promote(new)` → `new` moves to `Active`; previous `Active` moves
///   to `Overlap` with `overlap_until_ms` set.
/// - `retire(old)` → `Overlap` moves to `Retired` after overlap window.
/// - `destroy(retired)` → `Retired` moves to `Destroyed` after 90d
///   grace.
/// - `rollback(new, previous)` → `new` moves to `RolledBack`; `previous`
///   re-promoted to `Active` (PAT-ROLL-FORWARD-001).
///
/// Violating any transition returns [`RotationError::InvalidTransition`].
pub trait RotationAdapter: core::fmt::Debug {
    /// The asset class this adapter handles.
    fn asset_class(&self) -> crate::types::AssetClass;

    /// Generate new key material. Returns a [`KeyHandle`] in `Pending`
    /// state.
    ///
    /// # Errors
    ///
    /// - [`RotationError::Kms`] on key generation failure.
    /// - [`RotationError::RotationInFlight`] if another rotation is
    ///   already in-flight for this (asset_class, region) pair.
    fn generate(&self, now_ms: u64) -> Result<KeyHandle, RotationError>;

    /// Promote `new` key from `Pending` → `Active`; the current
    /// `Active` key transitions to `Overlap` with `overlap_until_ms`
    /// set to `now_ms + asset_class.overlap_seconds() * 1000`.
    ///
    /// Validates that `overlap_seconds ≤ hard_upper_bound_seconds`
    /// before accepting the promotion (returns
    /// [`RotationError::OverlapExceedsHardUpper`] on violation).
    ///
    /// # Errors
    ///
    /// - [`RotationError::InvalidTransition`] if `new.state != Pending`.
    /// - [`RotationError::OverlapExceedsHardUpper`] if overlap > 30d.
    /// - [`RotationError::Audit`] if audit outbox emit fails (fail-CLOSED).
    /// - [`RotationError::Storage`] on D1 batch failure.
    fn promote(&self, new: &KeyHandle, now_ms: u64) -> Result<KeyHandle, RotationError>;

    /// Re-key downstream resources (e.g., re-wrap TDK envelopes).
    /// Reports progress via `progress_callback` in `0.0..=1.0`. The
    /// callback is bounded by the caller's duration budget per asset class.
    ///
    /// For adapters that do not require downstream re-keying (e.g.,
    /// `PatSigning`, `AuditChain`), this is a no-op returning `Ok(())`.
    ///
    /// # Errors
    ///
    /// - [`RotationError::Storage`] on downstream re-key failure.
    fn rekey_downstream(
        &self,
        new: &KeyHandle,
        progress_callback: &dyn Fn(f64),
    ) -> Result<(), RotationError>;

    /// Move `old` from `Overlap` → `Retired` once the overlap window
    /// has expired.
    ///
    /// # Errors
    ///
    /// - [`RotationError::InvalidTransition`] if `old.state != Overlap`.
    /// - [`RotationError::Audit`] on audit emit failure.
    /// - [`RotationError::Storage`] on D1 failure.
    fn retire(&self, old: &KeyHandle, now_ms: u64) -> Result<KeyHandle, RotationError>;

    /// Destroy `retired` key material (`Retired` → `Destroyed`).
    /// Zeroizes key bytes + emits final audit.
    ///
    /// # Errors
    ///
    /// - [`RotationError::InvalidTransition`] if `retired.state != Retired`.
    /// - [`RotationError::Audit`] on audit emit failure.
    /// - [`RotationError::Storage`] on D1 failure.
    fn destroy(&self, retired: &KeyHandle, now_ms: u64) -> Result<KeyHandle, RotationError>;

    /// PAT-ROLL-FORWARD-001 auto-rollback: revert promotion.
    ///
    /// Transitions `new` → `RolledBack`; re-promotes `previous_active`
    /// from `Overlap` → `Active`. Preserves INV-KEY-NO-SKIP (the
    /// previous key becomes the only write-accepted key).
    ///
    /// # Errors
    ///
    /// - [`RotationError::InvalidTransition`] on invalid state.
    /// - [`RotationError::Audit`] on audit emit failure.
    /// - [`RotationError::Storage`] on D1 failure.
    fn rollback(
        &self,
        new: &KeyHandle,
        previous_active: &KeyHandle,
        now_ms: u64,
    ) -> Result<(KeyHandle, KeyHandle), RotationError>;

    /// Sample the downstream error rate (0.0..=1.0) for this asset
    /// class during a rotation. Used by the PAT-ROLL-FORWARD-001
    /// driver every 60 seconds. Returns `0.0` if no downstream
    /// health endpoint is available for this asset class.
    ///
    /// # Errors
    ///
    /// [`RotationError::Storage`] if the error-rate metric store is
    /// unavailable.
    fn downstream_error_rate(&self) -> Result<f64, RotationError>;

    /// Validate the canonical state transition `from → to` against the
    /// state machine. Returns `Err(InvalidTransition)` on violation.
    ///
    /// This is a shared helper used by all adapters; override only if
    /// the asset class has a non-canonical transition graph.
    fn validate_transition(
        &self,
        from: KeyState,
        to: KeyState,
    ) -> Result<(), RotationError> {
        let valid = matches!(
            (from, to),
            (KeyState::Pending, KeyState::Active)
                | (KeyState::Active, KeyState::Overlap)
                | (KeyState::Overlap, KeyState::Retired)
                | (KeyState::Retired, KeyState::Destroyed)
                | (KeyState::Active, KeyState::RolledBack)
                | (KeyState::Overlap, KeyState::Active) // rollback re-promotion
        );
        if valid {
            Ok(())
        } else {
            Err(RotationError::InvalidTransition { from, to })
        }
    }
}
