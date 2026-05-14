//! [`PatSigningRotationAdapter`] — PAT signing key rotation adapter.
//!
//! Overlap: **24 hours** (CTRL-KEY-005/006; `key_management.md §3.2.1`).
//! Multi-key support via `signing_key_id` column (`data_model.md §4.1`).
//!
//! # Design
//!
//! PAT verify path accepts both Active + Overlap signing keys by
//! `signing_key_id` lookup (HMAC sig fast-fail ≤ 100µs before Argon2id
//! per S-03 cycle 9 SEAL decision (a)). No downstream re-keying required
//! — the PAT record carries the `signing_key_id` that signed it; the
//! verifier selects the right key from the multi-key keyring.
//!
//! # Production wiring (deferred to WI-S13-006)
//!
//! - Cloudflare Workers Secrets `pat_signing_<region>_active`.
//! - Daily cron 02:00 UTC; staggered per-region.
//! - D1 atomic batch [rotation_state UPDATE + audit_outbox INSERT].

use std::sync::{Arc, Mutex};

use crate::adapter::RotationAdapter;
use crate::error::RotationError;
use crate::types::{AssetClass, KeyHandle, KeyState};

/// In-memory state for PatSigning adapter.
#[derive(Debug)]
struct PatSigningStore {
    active: Option<KeyHandle>,
    overlap: Option<KeyHandle>,
    next_id: u64,
    error_rate: f64,
    in_flight: bool,
}

impl PatSigningStore {
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

/// PAT signing key rotation adapter (24h overlap; S-03 multi-key).
///
/// # Example
///
/// ```rust
/// use corelink_rotation_adapters::{PatSigningRotationAdapter, RotationAdapter, KeyState};
///
/// let adapter = PatSigningRotationAdapter::new("us-east".to_string());
/// let handle = adapter.generate(1_000_000).unwrap();
/// assert_eq!(handle.state, KeyState::Pending);
/// let promoted = adapter.promote(&handle, 1_000_001).unwrap();
/// assert_eq!(promoted.state, KeyState::Active);
/// ```
#[derive(Debug)]
pub struct PatSigningRotationAdapter {
    region: String,
    store: Arc<Mutex<PatSigningStore>>,
}

impl PatSigningRotationAdapter {
    /// Construct a new PAT signing rotation adapter for the given region.
    #[must_use]
    pub fn new(region: String) -> Self {
        Self {
            region,
            store: Arc::new(Mutex::new(PatSigningStore::new())),
        }
    }

    /// Set the simulated downstream error rate (test injection).
    pub fn set_error_rate(&self, rate: f64) {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        store.error_rate = rate;
    }

    /// Return the region string.
    #[must_use]
    pub fn region(&self) -> &str {
        &self.region
    }
}

impl RotationAdapter for PatSigningRotationAdapter {
    fn asset_class(&self) -> AssetClass {
        AssetClass::PatSigning
    }

    fn generate(&self, now_ms: u64) -> Result<KeyHandle, RotationError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;
        if store.in_flight {
            return Err(RotationError::RotationInFlight(AssetClass::PatSigning));
        }
        let key_id = store.next_id;
        store.next_id += 1;
        store.in_flight = true;
        Ok(KeyHandle {
            key_id,
            asset_class: AssetClass::PatSigning,
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
        let overlap_secs = AssetClass::PatSigning.overlap_seconds();
        if overlap_secs > AssetClass::PatSigning.hard_upper_bound_seconds() {
            return Err(RotationError::OverlapExceedsHardUpper {
                seconds: overlap_secs,
            });
        }
        let overlap_until_ms = now_ms + overlap_secs * 1_000;

        let mut store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;

        if let Some(current_active) = store.active.take() {
            store.overlap = Some(KeyHandle {
                state: KeyState::Overlap,
                overlap_until_ms: Some(overlap_until_ms),
                ..current_active
            });
        }

        let promoted = KeyHandle {
            key_id: new.key_id,
            asset_class: AssetClass::PatSigning,
            state: KeyState::Active,
            created_at_ms: new.created_at_ms,
            promoted_at_ms: Some(now_ms),
            overlap_until_ms: None,
            retired_at_ms: None,
        };
        store.active = Some(promoted.clone());
        // Clear in_flight: D1 UNIQUE is on state='pending'; post-promote
        // a new generate() is permitted.
        store.in_flight = false;
        Ok(promoted)
    }

    fn rekey_downstream(
        &self,
        _new: &KeyHandle,
        _progress_callback: &dyn Fn(f64),
    ) -> Result<(), RotationError> {
        // PAT signing keys do NOT require downstream re-keying: PAT
        // records carry `signing_key_id`; verifier selects the right key
        // from the multi-key keyring at verify time. No-op.
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
