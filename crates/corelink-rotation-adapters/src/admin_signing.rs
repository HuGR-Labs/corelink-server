//! [`AdminSigningRotationAdapter`] — admin signing key rotation adapter.
//!
//! Overlap: **24 hours** (per-region HMAC-SHA256; `key_management.md
//! §3.2.1`; ADR-0018 5th asset class added Lote 10.13 codex P0).
//!
//! # Design
//!
//! The admin signing key is a per-region HMAC-SHA256 key used by the
//! WI-S13-002 dual-approval gate to sign `op_payload || nonce || ts`.
//! Consumed by WI-S13-002 via a key cache (per-region pull). Multi-key
//! support by `key_id` index: the dual-approval verifier accepts both
//! Active + Overlap signing keys during the 24h window.
//!
//! # Production wiring (deferred to WI-S13-006)
//!
//! - Per-region Cloudflare Workers Secrets `admin_signing_<region>_active`.
//! - New signing key pushed to dual-approval gate DO cache via DO
//!   subscribe-pub on promotion.
//! - D1 atomic batch [rotation_state UPDATE + audit_outbox INSERT].

use std::sync::{Arc, Mutex};

use crate::adapter::RotationAdapter;
use crate::error::RotationError;
use crate::types::{AssetClass, KeyHandle, KeyState};

/// In-memory state for AdminSigning adapter.
#[derive(Debug)]
struct AdminSigningStore {
    active: Option<KeyHandle>,
    overlap: Option<KeyHandle>,
    next_id: u64,
    error_rate: f64,
    in_flight: bool,
}

impl AdminSigningStore {
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

/// Admin signing key rotation adapter (24h overlap; per-region
/// HMAC-SHA256; dual-approval WI-S13-002).
///
/// # Example
///
/// ```rust
/// use corelink_rotation_adapters::{AdminSigningRotationAdapter, RotationAdapter, KeyState};
///
/// let adapter = AdminSigningRotationAdapter::new("us-east".to_string());
/// let handle = adapter.generate(1_000_000).unwrap();
/// assert_eq!(handle.state, KeyState::Pending);
/// let promoted = adapter.promote(&handle, 1_000_001).unwrap();
/// assert_eq!(promoted.state, KeyState::Active);
/// ```
#[derive(Debug)]
pub struct AdminSigningRotationAdapter {
    region: String,
    store: Arc<Mutex<AdminSigningStore>>,
}

impl AdminSigningRotationAdapter {
    /// Construct a new admin signing rotation adapter for the given region.
    #[must_use]
    pub fn new(region: String) -> Self {
        Self {
            region,
            store: Arc::new(Mutex::new(AdminSigningStore::new())),
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

impl RotationAdapter for AdminSigningRotationAdapter {
    fn asset_class(&self) -> AssetClass {
        AssetClass::AdminSigning
    }

    fn generate(&self, now_ms: u64) -> Result<KeyHandle, RotationError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;
        if store.in_flight {
            return Err(RotationError::RotationInFlight(AssetClass::AdminSigning));
        }
        let key_id = store.next_id;
        store.next_id += 1;
        store.in_flight = true;
        Ok(KeyHandle {
            key_id,
            asset_class: AssetClass::AdminSigning,
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
        let overlap_secs = AssetClass::AdminSigning.overlap_seconds();
        if overlap_secs > AssetClass::AdminSigning.hard_upper_bound_seconds() {
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
            asset_class: AssetClass::AdminSigning,
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
        // Admin signing keys do NOT require downstream re-keying: the
        // dual-approval DO cache is updated via subscribe-pub on promotion
        // (production wiring; deferred to WI-S13-006). No-op here.
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
