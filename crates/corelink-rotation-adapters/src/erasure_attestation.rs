//! [`ErasureAttestationRotationAdapter`] — Ed25519 attestation key rotation.
//!
//! Overlap: **30 days** canonical (per `key_management.md §3.2.1` +
//! ADR-0018 + ADR-S14-007). The long window preserves verifiability of
//! attestations signed pre-rotation: the verify endpoint accepts both the
//! Active and Overlap keys during the 30d window; post-overlap only the
//! current Active key is accepted.
//!
//! # Per-region isolation
//!
//! Each region runs an independent adapter instance with staggered
//! rotation cron (01:00 UTC region-offset) to avoid simultaneous
//! transitions. INV-KEY-OVERLAP is enforced via the canonical
//! [`is_valid_read_state`] predicate.
//!
//! # Production wiring (deferred to GA ship gate)
//!
//! - Per-region Cloudflare Workers Secret `erasure_attestation_<region>_active`.
//! - D1 `erasure_public_keys` table (migration `0032_erasure_attestation.sql`).
//! - D1 atomic batch [rotation_state UPDATE + audit_outbox INSERT].
//! - Public key endpoint `GET /v1/public/keys/erasure/{region}.pub` serves
//!   Active + Overlap PEM keys from D1 during the 30d window.

use std::sync::{Arc, Mutex};

use crate::adapter::RotationAdapter;
use crate::error::RotationError;
use crate::types::{AssetClass, KeyHandle, KeyState};

/// In-memory state for erasure attestation key adapter.
#[derive(Debug)]
struct ErasureAttestationStore {
    active: Option<KeyHandle>,
    overlap: Option<KeyHandle>,
    next_id: u64,
    error_rate: f64,
    in_flight: bool,
}

impl ErasureAttestationStore {
    fn new() -> Self {
        Self {
            active: None,
            overlap: None,
            next_id: 1,
            error_rate: 0.0,
            in_flight: false,
        }
    }
}

/// Ed25519 attestation key rotation adapter (30d overlap; per-region;
/// WI-S14-007 erasure attestation).
///
/// # INV-KEY-OVERLAP (30d)
///
/// During the 30d overlap window both the previous Active key (now
/// Overlap) and the new Active key are accepted for signature
/// verification. Only the new Active key signs new attestations.
/// This ensures that a customer who receives an attestation just before
/// rotation can still verify it offline 28 days later.
///
/// # Example
///
/// ```rust
/// use corelink_rotation_adapters::{ErasureAttestationRotationAdapter, RotationAdapter, KeyState};
///
/// let adapter = ErasureAttestationRotationAdapter::new("weur".to_string());
/// let handle = adapter.generate(1_000_000).unwrap();
/// assert_eq!(handle.state, KeyState::Pending);
///
/// let active = adapter.promote(&handle, 2_000_000).unwrap();
/// assert_eq!(active.state, KeyState::Active);
/// ```
#[derive(Debug)]
pub struct ErasureAttestationRotationAdapter {
    region: String,
    store: Arc<Mutex<ErasureAttestationStore>>,
}

impl ErasureAttestationRotationAdapter {
    /// Construct a new erasure attestation key rotation adapter for the
    /// given region (e.g. `"weur"`, `"wnam"`, `"enam"`, `"sam"`).
    #[must_use]
    pub fn new(region: String) -> Self {
        Self {
            region,
            store: Arc::new(Mutex::new(ErasureAttestationStore::new())),
        }
    }

    /// Return the current active key handle, if any.
    ///
    /// Used by [`corelink_erasure_attestation`] to retrieve the active
    /// signing key for the attestation path.
    #[must_use]
    pub fn active_key(&self) -> Option<KeyHandle> {
        self.store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active
            .clone()
    }

    /// Return the current overlap key handle, if any.
    ///
    /// Used by the public key endpoint to serve both Active + Overlap
    /// public keys during the 30d window.
    #[must_use]
    pub fn overlap_key(&self) -> Option<KeyHandle> {
        self.store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .overlap
            .clone()
    }

    /// Set the simulated downstream error rate (test injection only).
    pub fn set_error_rate(&self, rate: f64) {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        store.error_rate = rate;
    }

    /// Return the region this adapter is bound to.
    #[must_use]
    pub fn region(&self) -> &str {
        &self.region
    }
}

impl RotationAdapter for ErasureAttestationRotationAdapter {
    fn asset_class(&self) -> AssetClass {
        AssetClass::ErasureAttestationKey
    }

    fn generate(&self, now_ms: u64) -> Result<KeyHandle, RotationError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;
        if store.in_flight {
            return Err(RotationError::RotationInFlight(
                AssetClass::ErasureAttestationKey,
            ));
        }
        let key_id = store.next_id;
        store.next_id += 1;
        store.in_flight = true;
        Ok(KeyHandle {
            key_id,
            asset_class: AssetClass::ErasureAttestationKey,
            state: KeyState::Pending,
            created_at_ms: now_ms,
            promoted_at_ms: None,
            overlap_until_ms: None,
            retired_at_ms: None,
        })
    }

    fn promote(&self, new: &KeyHandle, now_ms: u64) -> Result<KeyHandle, RotationError> {
        if new.state != KeyState::Pending {
            return Err(RotationError::InvalidTransition {
                from: new.state,
                to: KeyState::Active,
            });
        }
        let overlap_secs = AssetClass::ErasureAttestationKey.overlap_seconds();
        if overlap_secs > AssetClass::ErasureAttestationKey.hard_upper_bound_seconds() {
            return Err(RotationError::OverlapExceedsHardUpper {
                seconds: overlap_secs,
            });
        }
        let overlap_until_ms = now_ms + overlap_secs * 1_000;

        let mut store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;

        // Transition current active → overlap (30d window).
        if let Some(current_active) = store.active.take() {
            store.overlap = Some(KeyHandle {
                state: KeyState::Overlap,
                overlap_until_ms: Some(overlap_until_ms),
                ..current_active
            });
        }

        let promoted = KeyHandle {
            key_id: new.key_id,
            asset_class: AssetClass::ErasureAttestationKey,
            state: KeyState::Active,
            created_at_ms: new.created_at_ms,
            promoted_at_ms: Some(now_ms),
            overlap_until_ms: None,
            retired_at_ms: None,
        };
        store.active = Some(promoted.clone());
        // Allow a new generate() once promotion completes.
        store.in_flight = false;
        Ok(promoted)
    }

    fn rekey_downstream(
        &self,
        _new: &KeyHandle,
        _progress_callback: &dyn Fn(f64),
    ) -> Result<(), RotationError> {
        // No downstream re-keying: old attestation signatures remain
        // verifiable via the Overlap public key for 30d; no re-sign
        // needed. The verify endpoint simply serves both keys during
        // the overlap window. No-op.
        Ok(())
    }

    fn retire(&self, old: &KeyHandle, now_ms: u64) -> Result<KeyHandle, RotationError> {
        if old.state != KeyState::Overlap {
            return Err(RotationError::InvalidTransition {
                from: old.state,
                to: KeyState::Retired,
            });
        }
        let mut store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;
        store.overlap = None;
        store.in_flight = false;
        Ok(KeyHandle {
            state: KeyState::Retired,
            retired_at_ms: Some(now_ms),
            ..old.clone()
        })
    }

    fn destroy(&self, retired: &KeyHandle, _now_ms: u64) -> Result<KeyHandle, RotationError> {
        if retired.state != KeyState::Retired {
            return Err(RotationError::InvalidTransition {
                from: retired.state,
                to: KeyState::Destroyed,
            });
        }
        // Key material zeroized by the production wiring (Workers Secret
        // deletion). At the in-memory CI skeleton level we model the
        // state transition only.
        Ok(KeyHandle {
            state: KeyState::Destroyed,
            ..retired.clone()
        })
    }

    fn rollback(
        &self,
        new: &KeyHandle,
        previous_active: &KeyHandle,
        _now_ms: u64,
    ) -> Result<(KeyHandle, KeyHandle), RotationError> {
        if new.state != KeyState::Active {
            return Err(RotationError::InvalidTransition {
                from: new.state,
                to: KeyState::RolledBack,
            });
        }
        if previous_active.state != KeyState::Overlap {
            return Err(RotationError::InvalidTransition {
                from: previous_active.state,
                to: KeyState::Active,
            });
        }
        let mut store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;
        let rolled_back = KeyHandle {
            state: KeyState::RolledBack,
            ..new.clone()
        };
        let re_promoted = KeyHandle {
            state: KeyState::Active,
            overlap_until_ms: None,
            ..previous_active.clone()
        };
        store.active = Some(re_promoted.clone());
        store.overlap = None;
        store.in_flight = false;
        Ok((rolled_back, re_promoted))
    }

    fn downstream_error_rate(&self) -> Result<f64, RotationError> {
        let store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;
        Ok(store.error_rate)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "unit tests may use unwrap/expect"
)]
mod tests {
    use super::*;

    #[test]
    fn generate_returns_pending() {
        let adapter = ErasureAttestationRotationAdapter::new("weur".to_string());
        let handle = adapter.generate(1_000).expect("generate");
        assert_eq!(handle.state, KeyState::Pending);
        assert_eq!(handle.asset_class, AssetClass::ErasureAttestationKey);
    }

    #[test]
    fn promote_transitions_active_and_sets_overlap() {
        let adapter = ErasureAttestationRotationAdapter::new("weur".to_string());
        let h1 = adapter.generate(1_000).expect("gen 1");
        let active1 = adapter.promote(&h1, 2_000).expect("promote 1");
        assert_eq!(active1.state, KeyState::Active);

        let h2 = adapter.generate(3_000).expect("gen 2");
        let active2 = adapter.promote(&h2, 4_000).expect("promote 2");
        assert_eq!(active2.state, KeyState::Active);

        // old key now in overlap
        let overlap = adapter.overlap_key().expect("overlap key");
        assert_eq!(overlap.state, KeyState::Overlap);
        // 30d overlap window set correctly
        let expected_until = 4_000 + 30 * 24 * 3_600 * 1_000;
        assert_eq!(overlap.overlap_until_ms, Some(expected_until));
    }

    #[test]
    fn overlap_seconds_is_30d() {
        assert_eq!(
            AssetClass::ErasureAttestationKey.overlap_seconds(),
            30 * 24 * 3_600,
        );
    }

    #[test]
    fn retire_and_destroy_lifecycle() {
        let adapter = ErasureAttestationRotationAdapter::new("wnam".to_string());
        let h1 = adapter.generate(1_000).unwrap();
        let active1 = adapter.promote(&h1, 2_000).unwrap();
        let h2 = adapter.generate(3_000).unwrap();
        let _active2 = adapter.promote(&h2, 4_000).unwrap();

        let overlap_key = KeyHandle {
            state: KeyState::Overlap,
            ..active1
        };
        let retired = adapter.retire(&overlap_key, 5_000).unwrap();
        assert_eq!(retired.state, KeyState::Retired);

        let destroyed = adapter.destroy(&retired, 6_000).unwrap();
        assert_eq!(destroyed.state, KeyState::Destroyed);
    }

    #[test]
    fn rotation_in_flight_rejected() {
        let adapter = ErasureAttestationRotationAdapter::new("enam".to_string());
        let _h = adapter.generate(1_000).unwrap();
        // second generate while in_flight returns error
        let err = adapter.generate(2_000).unwrap_err();
        assert!(matches!(
            err,
            RotationError::RotationInFlight(AssetClass::ErasureAttestationKey)
        ));
    }

    #[test]
    fn rollback_re_promotes_previous() {
        let adapter = ErasureAttestationRotationAdapter::new("sam".to_string());
        let h1 = adapter.generate(1_000).unwrap();
        let active1 = adapter.promote(&h1, 2_000).unwrap();
        let h2 = adapter.generate(3_000).unwrap();
        let active2 = adapter.promote(&h2, 4_000).unwrap();

        let overlap_h = adapter.overlap_key().unwrap();
        let (rb, re_promoted) = adapter.rollback(&active2, &overlap_h, 5_000).unwrap();
        assert_eq!(rb.state, KeyState::RolledBack);
        assert_eq!(re_promoted.state, KeyState::Active);
        assert_eq!(re_promoted.key_id, active1.key_id);
    }
}
