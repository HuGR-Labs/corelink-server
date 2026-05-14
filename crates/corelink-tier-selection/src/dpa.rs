//! DPA acceptance gate — INV-ONBOARD-DPA-FIRST canonical primitive.
//!
//! This module exposes a small trait surface that wraps the WI-S19-002
//! DPA acceptance ledger. Production wiring (deferred to PRR ship
//! gate) reads `tenant.dpa_signed_ts` from D1 inside the same
//! `BEGIN IMMEDIATE TRANSACTION` that performs the tier-selection
//! check + UPDATE. In tests we use the [`InMemoryDpaGate`] fake.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::tenant::TenantId;

/// DPA acceptance lookup trait. Implementations MUST be fail-CLOSED:
/// any storage transient failure MUST return `Ok(false)` (deny) so
/// INV-ONBOARD-DPA-FIRST is preserved.
pub trait DpaAcceptanceGate: core::fmt::Debug + Send + Sync {
    /// Returns `true` iff `tenant_id` has accepted the current DPA
    /// version. MUST be invoked BEFORE any Stripe API call.
    fn is_accepted(&self, tenant_id: &TenantId, current_dpa_version: &str) -> bool;
}

/// In-memory DPA gate fake. Used in tests + property tests.
///
/// Mutating the map after a tier-selection in flight models the
/// concurrent flow exploitation scenario from WI §2 ("tab 1 user
/// accepts DPA at t1; tab 2 user starts Stripe Checkout at t1+1ms").
#[derive(Clone, Debug, Default)]
pub struct InMemoryDpaGate {
    accepted: Arc<Mutex<HashMap<(TenantId, String), bool>>>,
}

impl InMemoryDpaGate {
    /// Construct an empty gate (no tenant has DPA accepted).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark `tenant_id` as having accepted DPA version `version`.
    ///
    /// Idempotent; later calls overwrite the prior state.
    pub fn accept(&self, tenant_id: TenantId, version: impl Into<String>) {
        let key = (tenant_id, version.into());
        if let Ok(mut g) = self.accepted.lock() {
            g.insert(key, true);
        }
    }

    /// Explicitly revoke (return `false` from `is_accepted`).
    pub fn revoke(&self, tenant_id: TenantId, version: impl Into<String>) {
        let key = (tenant_id, version.into());
        if let Ok(mut g) = self.accepted.lock() {
            g.insert(key, false);
        }
    }
}

impl DpaAcceptanceGate for InMemoryDpaGate {
    fn is_accepted(&self, tenant_id: &TenantId, current_dpa_version: &str) -> bool {
        let key = (tenant_id.clone(), current_dpa_version.to_string());
        match self.accepted.lock() {
            // Fail-CLOSED on poisoned mutex: deny (preserves INV).
            Err(_) => false,
            Ok(g) => g.get(&key).copied().unwrap_or(false),
        }
    }
}

/// Adversarial fixture that always returns `false` (DPA never
/// accepted). Property tests use this to enforce
/// INV-ONBOARD-DPA-FIRST is NEVER bypassed.
#[derive(Clone, Debug, Default)]
pub struct AlwaysDenyDpaGate;

impl DpaAcceptanceGate for AlwaysDenyDpaGate {
    fn is_accepted(&self, _tenant_id: &TenantId, _current_dpa_version: &str) -> bool {
        false
    }
}
