//! [`ByokRotationAdapter`] — BYOK customer CMK rotation adapter stub.
//!
//! Overlap: **7 days** (CTRL-KEY-010/011/012; `key_management.md
//! §3.2.1`). Customer-trigger via `POST /v1/customer/byok/rotate`;
//! NOT a cron-triggered rotation.
//!
//! # Design
//!
//! This is a **forward stub** for S-14 (BYOK full flow). The adapter
//! skeleton satisfies the `RotationAdapter` trait contract with correct
//! 7d overlap semantics and the canonical state machine transitions.
//! The actual customer CMK re-wrap + customer notification + CoreLink
//! DEK cache invalidation are deferred to S-14.
//!
//! Customer revoke mid-overlap: handled via `retire(old)` early call by
//! the rotation worker; INV-BYOK-CRYPTO-SOVEREIGNTY preserved
//! (intentional customer-triggered data inaccessibility).
//!
//! # Production wiring (deferred to S-14)
//!
//! - Per-customer Cloudflare Workers KV `byok_<tenant_id>_active`.
//! - Customer notification webhook on promotion + retire.
//! - CoreLink-side DEK cache invalidation 5 min hard on revoke.
//! - D1 atomic batch [rotation_state UPDATE + audit_outbox INSERT].

use std::sync::{Arc, Mutex};

use crate::adapter::RotationAdapter;
use crate::error::RotationError;
use crate::types::{AssetClass, KeyHandle, KeyState};

/// In-memory state for BYOK adapter.
#[derive(Debug)]
struct ByokStore {
    active: Option<KeyHandle>,
    overlap: Option<KeyHandle>,
    next_id: u64,
    error_rate: f64,
    in_flight: bool,
}

impl ByokStore {
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

/// BYOK customer CMK rotation adapter stub (7d overlap; S-14 forward).
///
/// # Example
///
/// ```rust
/// use corelink_rotation_adapters::{ByokRotationAdapter, RotationAdapter, KeyState};
///
/// let adapter = ByokRotationAdapter::new("tenant-abc".to_string());
/// let handle = adapter.generate(1_000_000).unwrap();
/// assert_eq!(handle.state, KeyState::Pending);
/// ```
#[derive(Debug)]
pub struct ByokRotationAdapter {
    /// Tenant ID or customer context identifier.
    tenant_id: String,
    store: Arc<Mutex<ByokStore>>,
}

impl ByokRotationAdapter {
    /// Construct a new BYOK rotation adapter for the given tenant.
    #[must_use]
    pub fn new(tenant_id: String) -> Self {
        Self {
            tenant_id,
            store: Arc::new(Mutex::new(ByokStore::new())),
        }
    }

    /// Set the simulated downstream error rate (test injection).
    pub fn set_error_rate(&self, rate: f64) {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        store.error_rate = rate;
    }

    /// Return the tenant ID.
    #[must_use]
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }
}

impl RotationAdapter for ByokRotationAdapter {
    fn asset_class(&self) -> AssetClass {
        AssetClass::Byok
    }

    fn generate(&self, now_ms: u64) -> Result<KeyHandle, RotationError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| RotationError::Storage(e.to_string()))?;
        if store.in_flight {
            return Err(RotationError::RotationInFlight(AssetClass::Byok));
        }
        let key_id = store.next_id;
        store.next_id += 1;
        store.in_flight = true;
        Ok(KeyHandle {
            key_id,
            asset_class: AssetClass::Byok,
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
        let overlap_secs = AssetClass::Byok.overlap_seconds();
        if overlap_secs > AssetClass::Byok.hard_upper_bound_seconds() {
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
            asset_class: AssetClass::Byok,
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
        // Stub: S-14 forward. Real implementation re-wraps per-customer
        // DEK cache with new CMK + sends customer notification webhook.
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
