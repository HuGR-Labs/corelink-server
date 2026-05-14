//! [`TdkRotationAdapter`] — TDK (tenant derivation key) rotation adapter.
//!
//! Overlap: **7 days** (CTRL-KEY-005 + CTRL-KEY-006; `key_management.md
//! §3.2.1`). Re-wrap CAS envelope background job required; 24h would
//! cause starvation at TB scale.
//!
//! # Production wiring (deferred to WI-S13-006)
//!
//! - Cloudflare Workers Secrets `tdk_<region>_active` + `tdk_<region>_overlap`.
//! - Re-wrap: paginated background job reading all S-01 CAS envelopes +
//!   re-wrapping with new TDK; `corelink_key_rewrap_progress_ratio`
//!   metric updated per batch.
//! - D1 atomic batch [rotation_state UPDATE + audit_outbox INSERT].

use std::sync::{Arc, Mutex};

use crate::adapter::RotationAdapter;
use crate::error::RotationError;
use crate::types::{AssetClass, KeyHandle, KeyState};

/// In-memory state store for the TDK adapter (CI fake; production uses D1).
#[derive(Debug)]
struct TdkStore {
    /// Active key (None if no rotation has been performed yet).
    active: Option<KeyHandle>,
    /// Overlap key (previous active during rotation window).
    overlap: Option<KeyHandle>,
    /// Next key ID (monotonically increasing).
    next_id: u64,
    /// Simulated downstream error rate (0.0..=1.0) for testing.
    error_rate: f64,
    /// Whether a rotation is currently in-flight.
    in_flight: bool,
}

impl TdkStore {
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

/// TDK rotation adapter (7d overlap; S-01 envelope re-wrap).
///
/// # Example
///
/// ```rust
/// use corelink_rotation_adapters::{TdkRotationAdapter, RotationAdapter};
///
/// let adapter = TdkRotationAdapter::new("us-east".to_string());
/// let handle = adapter.generate(1_000_000).unwrap();
/// assert_eq!(handle.state, corelink_rotation_adapters::KeyState::Pending);
/// ```
#[derive(Debug)]
pub struct TdkRotationAdapter {
    region: String,
    store: Arc<Mutex<TdkStore>>,
}

impl TdkRotationAdapter {
    /// Construct a new TDK rotation adapter for the given region.
    #[must_use]
    pub fn new(region: String) -> Self {
        Self {
            region,
            store: Arc::new(Mutex::new(TdkStore::new())),
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

impl RotationAdapter for TdkRotationAdapter {
    fn asset_class(&self) -> AssetClass {
        AssetClass::Tdk
    }

    fn generate(&self, now_ms: u64) -> Result<KeyHandle, RotationError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;
        if store.in_flight {
            return Err(RotationError::RotationInFlight(AssetClass::Tdk));
        }
        let key_id = store.next_id;
        store.next_id += 1;
        store.in_flight = true;
        Ok(KeyHandle {
            key_id,
            asset_class: AssetClass::Tdk,
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
        let overlap_secs = AssetClass::Tdk.overlap_seconds();
        if overlap_secs > AssetClass::Tdk.hard_upper_bound_seconds() {
            return Err(RotationError::OverlapExceedsHardUpper {
                seconds: overlap_secs,
            });
        }
        let overlap_until_ms = now_ms + overlap_secs * 1_000;

        let mut store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;

        // Move current active → overlap.
        if let Some(current_active) = store.active.take() {
            store.overlap = Some(KeyHandle {
                state: KeyState::Overlap,
                overlap_until_ms: Some(overlap_until_ms),
                ..current_active
            });
        }

        let promoted = KeyHandle {
            key_id: new.key_id,
            asset_class: AssetClass::Tdk,
            state: KeyState::Active,
            created_at_ms: new.created_at_ms,
            promoted_at_ms: Some(now_ms),
            overlap_until_ms: None,
            retired_at_ms: None,
        };
        store.active = Some(promoted.clone());
        // Clear in_flight so the next rotation can generate a new Pending
        // key. The D1 UNIQUE constraint is on (asset_class, region) WHERE
        // state='pending'; once the key is promoted (no longer Pending), a
        // new generate() is permitted.
        store.in_flight = false;
        Ok(promoted)
    }

    fn rekey_downstream(
        &self,
        _new: &KeyHandle,
        progress_callback: &dyn Fn(f64),
    ) -> Result<(), RotationError> {
        // Simulate 10-step re-wrap progress (real: paginated CAS envelopes).
        for i in 0_u32..=10 {
            progress_callback(f64::from(i) / 10.0);
        }
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

        let retired = KeyHandle {
            state: KeyState::Retired,
            retired_at_ms: Some(now_ms),
            overlap_until_ms: old.overlap_until_ms,
            ..old.clone()
        };
        store.overlap = None;
        store.in_flight = false;
        Ok(retired)
    }

    fn destroy(&self, retired: &KeyHandle, _now_ms: u64) -> Result<KeyHandle, RotationError> {
        if retired.state != KeyState::Retired {
            return Err(RotationError::InvalidTransition {
                from: retired.state,
                to: KeyState::Destroyed,
            });
        }
        // In production: zeroize key bytes + emit final audit.
        // `_now_ms` is recorded as destroyed_at_ms in D1 at the
        // production binding layer (not stored in KeyHandle; separate
        // D1 column per schema §6.1.3).
        Ok(KeyHandle {
            state: KeyState::Destroyed,
            retired_at_ms: retired.retired_at_ms,
            overlap_until_ms: None,
            promoted_at_ms: retired.promoted_at_ms,
            created_at_ms: retired.created_at_ms,
            key_id: retired.key_id,
            asset_class: AssetClass::Tdk,
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
