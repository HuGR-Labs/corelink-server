//! BLAKE3-256 idempotency-key derivation + per-tenant set-membership
//! tracker.
//!
//! ## Canonical formula
//!
//! Per WI-S10-001 §1 invariant 2 + sprint contract §5.1 R-S10-2 the
//! idempotency key is derived deterministically from the canonical
//! event payload so a Worker retry reproduces the SAME key:
//!
//! ```text
//! idem_key = BLAKE3-256(JCS-canonicalize(event with idem_key slot zeroed))
//! ```
//!
//! - The `idem_key` slot is zeroed during the link input
//!   canonicalization (mirrors the Bitcoin-genesis `prev_hash = [0u8; 32]`
//!   pattern from `corelink-audit-chain`); the derived 32-byte digest is
//!   then written back into the slot for the on-the-wire shape.
//! - JCS (RFC 8785) sorts keys lexicographically + emits canonical
//!   numeric / string forms so two byte-equivalent events with field
//!   ordering differences produce the SAME canonical bytes (= the same
//!   idem_key). This is the canonical determinism property the dedup
//!   relies on.
//!
//! ## Why BLAKE3-256 (vs SHA-256 / random UUID)
//!
//! - Random UUIDs would NOT survive Worker retry — each retry produces
//!   a fresh UUID, so deduplication degenerates to a no-op + the
//!   billing pipeline observes a duplicate. The whole point of the
//!   idempotency layer is REPLAY SAFETY: the SAME billable surface
//!   reproduces the SAME key.
//! - BLAKE3 has 128-bit collision security at 256-bit output (same as
//!   SHA-256) + is 5x faster + parallelizable. CoreLink already pins
//!   BLAKE3 for CAS digests (S-01) + AC `result_hash` (S-04) + dedup
//!   fingerprint (S-07) + audit chain links (S-09); using BLAKE3 here
//!   keeps the cryptographic discipline uniform.
//! - Collision rate: birthday-bound for 2^N events with 2^256 output is
//!   2^(2N - 256). For N = 30 (10^9 events/yr × 7y retention ≈ 2^33),
//!   expected collisions ≈ 2^-190 — orders of magnitude below ANY
//!   detection threshold. Adversarial collisions require 2^128 work
//!   (BLAKE3 collision-resistance security); not exploitable.
//!
//! ## Per-tenant partition
//!
//! The tracker partitions idem_key set membership PER TENANT — so
//! tenant A's reuse of a request_id never affects tenant B's emit
//! decision (`prop_tenant_isolation`). Production wiring at the
//! `(tenant_id, request_id) UNIQUE` D1 staging table pins the same
//! partition discipline (sprint contract §5.1 R-S10-2).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use blake3::Hasher;
use uuid::Uuid;

use crate::error::BillingEmitError;
use crate::event::{IdemKey, UsageEvent, GENESIS_IDEM_KEY};

/// Decision returned by [`IdempotencyTracker::insert`]: either the
/// `idem_key` was unseen for the (tenant, key) pair (the emit may
/// proceed) or a replay was detected (the caller's emit is treated as
/// a no-op + the audit fires for visibility).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdempotencyDecision {
    /// The key was unseen for the tenant; the caller MUST proceed
    /// with the R2 NDJSON write + state advance.
    Accepted,
    /// The key was already accepted for the tenant (replay-safe
    /// duplicate); the caller MUST treat the emit as a no-op + audit
    /// fires per `corelink.billing.duplicate_rejected`.
    DuplicateRejected,
}

/// Compute the JCS-canonical UTF-8 bytes of a usage event with the
/// `idem_key` slot zeroed for the link input. Mirrors
/// `corelink_audit_chain::compute_canonical_bytes` discipline (chain
/// link input zeroes the `prev_hash` slot via the Bitcoin-genesis
/// convention).
///
/// # Errors
///
/// - [`BillingEmitError::Canonicalization`] when `serde_jcs` rejects
///   the value (e.g. NaN floats, non-string map keys). Production
///   wiring at the construction surface ensures the data tree never
///   contains non-finite floats by construction.
pub fn compute_canonical_bytes_for_idem(
    event: &UsageEvent,
) -> Result<Vec<u8>, BillingEmitError> {
    // Zero the idem_key slot for the link input (canonical formula
    // input); the derived digest is then written back into the slot
    // post-derivation by the orchestrator.
    let mut zeroed = event.clone();
    zeroed.idem_key = IdemKey(GENESIS_IDEM_KEY);
    serde_jcs::to_vec(&zeroed)
        .map_err(|e| BillingEmitError::Canonicalization(format!("{e}")))
}

/// Derive the canonical idempotency key from the usage event. Pure
/// BLAKE3-256 of the JCS-canonical bytes (with the idem_key slot
/// zeroed for the link input).
///
/// # Errors
///
/// - [`BillingEmitError::Canonicalization`] when JCS canonicalization
///   of `event` fails.
pub fn derive_idem_key(event: &UsageEvent) -> Result<IdemKey, BillingEmitError> {
    let canonical = compute_canonical_bytes_for_idem(event)?;
    Ok(derive_idem_key_from_canonical(&canonical))
}

/// Derive the canonical idempotency key from already-canonicalized
/// bytes. Used by the tests + production wiring once the bytes are
/// available; avoids re-running JCS.
#[must_use]
pub fn derive_idem_key_from_canonical(canonical_bytes: &[u8]) -> IdemKey {
    let mut h = Hasher::new();
    h.update(canonical_bytes);
    let digest = h.finalize();
    IdemKey(*digest.as_bytes())
}

/// Per-tenant idempotency tracker trait. Production wiring composes:
///
/// - `D1IdempotencyTracker` — `(tenant_id, request_id)` UNIQUE D1
///   staging table mirror (sprint contract §5.1 R-S10-2; deferred to
///   WI-S10-007 PRR ship gate per the `trait-abstraction-defer`
///   charter pattern).
pub trait IdempotencyTracker: Send + Sync + core::fmt::Debug {
    /// Attempt to insert `idem_key` into the tenant's accepted set.
    /// Returns [`IdempotencyDecision::Accepted`] if the key was
    /// unseen + the canonical bytes attempted to land match what the
    /// orchestrator passed (collision-safe path). Returns
    /// [`IdempotencyDecision::DuplicateRejected`] if the key was
    /// already accepted with the SAME canonical bytes (replay-safe
    /// duplicate).
    ///
    /// # Errors
    ///
    /// - [`BillingEmitError::IdempotencyCollision`] when the same
    ///   `idem_key` was already accepted but the canonical bytes
    ///   diverged (probability < 2^-128 under BLAKE3-256; defensive
    ///   guard).
    /// - [`BillingEmitError::Internal`] when the per-instance mutex
    ///   is poisoned.
    fn insert(
        &self,
        tenant_id: Uuid,
        idem_key: IdemKey,
        canonical_bytes: &[u8],
    ) -> Result<IdempotencyDecision, BillingEmitError>;

    /// Borrow the count of accepted keys for the tenant (used by tests
    /// + ops counters).
    fn accepted_count(&self, tenant_id: Uuid) -> usize;
}

/// In-memory idempotency tracker. Per-instance `Arc<Mutex<>>` per the
/// F-001 closure discipline; tests instantiate fresh trackers per case
/// so cross-test contamination is structurally impossible.
#[derive(Clone, Default, Debug)]
pub struct InMemoryIdempotencyTracker {
    state: Arc<Mutex<TrackerState>>,
}

#[derive(Debug, Default)]
struct TrackerState {
    // Per-tenant set of accepted idem_keys.
    accepted: HashMap<Uuid, HashSet<[u8; 32]>>,
    // Per-(tenant, key) canonical bytes used at the moment the key was
    // first accepted; collision-detection input.
    canonical: HashMap<(Uuid, [u8; 32]), Vec<u8>>,
}

impl InMemoryIdempotencyTracker {
    /// Construct a fresh tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl IdempotencyTracker for InMemoryIdempotencyTracker {
    fn insert(
        &self,
        tenant_id: Uuid,
        idem_key: IdemKey,
        canonical_bytes: &[u8],
    ) -> Result<IdempotencyDecision, BillingEmitError> {
        let mut g = self.state.lock().map_err(|_| {
            BillingEmitError::Internal(
                "idempotency tracker mutex poisoned".to_string(),
            )
        })?;
        let key_bytes = *idem_key.as_bytes();
        let canonical_key = (tenant_id, key_bytes);

        let already_accepted = g
            .accepted
            .get(&tenant_id)
            .is_some_and(|set| set.contains(&key_bytes));

        if already_accepted {
            // Defensive collision check: same idem_key for divergent
            // canonical bytes is an SEV-1 source.
            let stored = g.canonical.get(&canonical_key).cloned();
            return match stored {
                Some(prior) if prior.as_slice() == canonical_bytes => {
                    Ok(IdempotencyDecision::DuplicateRejected)
                }
                Some(_) => Err(BillingEmitError::IdempotencyCollision {
                    tenant_id: tenant_id.to_string(),
                    idem_key: idem_key.to_hex(),
                }),
                None => Err(BillingEmitError::Internal(format!(
                    "idempotency tracker invariant violated: idem_key {} accepted for tenant {} without canonical record",
                    idem_key.to_hex(),
                    tenant_id
                ))),
            };
        }

        // First sight: accept + record canonical bytes for future
        // collision detection.
        g.accepted.entry(tenant_id).or_default().insert(key_bytes);
        g.canonical.insert(canonical_key, canonical_bytes.to_vec());
        Ok(IdempotencyDecision::Accepted)
    }

    fn accepted_count(&self, tenant_id: Uuid) -> usize {
        match self.state.lock() {
            Ok(g) => g.accepted.get(&tenant_id).map_or(0, HashSet::len),
            Err(p) => p.into_inner().accepted.get(&tenant_id).map_or(0, HashSet::len),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::event::{UsageEvent, UsageEventKind};
    use corelink_analytics::Region;

    fn fresh_event(tenant: Uuid, qty: u64, period: &str) -> UsageEvent {
        UsageEvent::new(
            "corelink/region/iad",
            Uuid::now_v7(),
            1_700_000_000_000,
            Region::Iad,
            tenant,
            UsageEventKind::CasPut,
            qty,
            period,
        )
        .unwrap()
    }

    #[test]
    fn derive_idem_key_deterministic() {
        let tenant = Uuid::now_v7();
        let mut e1 = fresh_event(tenant, 4096, "2026-05");
        let e2 = e1.clone();
        // Even after pre-populating the slot with garbage, derivation
        // re-zeros so result is invariant to slot content.
        e1.idem_key = IdemKey([0xFF; 32]);
        let k1 = derive_idem_key(&e1).unwrap();
        let k2 = derive_idem_key(&e2).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn derive_idem_key_diverges_on_data_change() {
        let tenant = Uuid::now_v7();
        let e1 = fresh_event(tenant, 4096, "2026-05");
        let e2 = fresh_event(tenant, 8192, "2026-05");
        let k1 = derive_idem_key(&e1).unwrap();
        let k2 = derive_idem_key(&e2).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn derive_idem_key_diverges_on_billing_period_change() {
        let tenant = Uuid::now_v7();
        let e1 = fresh_event(tenant, 4096, "2026-05");
        let e2 = fresh_event(tenant, 4096, "2026-06");
        let k1 = derive_idem_key(&e1).unwrap();
        let k2 = derive_idem_key(&e2).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn derive_idem_key_diverges_on_tenant_change() {
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let e1 = fresh_event(tenant_a, 4096, "2026-05");
        let e2 = fresh_event(tenant_b, 4096, "2026-05");
        let k1 = derive_idem_key(&e1).unwrap();
        let k2 = derive_idem_key(&e2).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn derive_from_canonical_matches_full_pipeline() {
        let tenant = Uuid::now_v7();
        let e = fresh_event(tenant, 1, "2026-05");
        let canonical = compute_canonical_bytes_for_idem(&e).unwrap();
        let k_full = derive_idem_key(&e).unwrap();
        let k_canonical = derive_idem_key_from_canonical(&canonical);
        assert_eq!(k_full, k_canonical);
    }

    #[test]
    fn tracker_first_insert_accepted() {
        let tracker = InMemoryIdempotencyTracker::new();
        let tenant = Uuid::now_v7();
        let e = fresh_event(tenant, 1, "2026-05");
        let canonical = compute_canonical_bytes_for_idem(&e).unwrap();
        let key = derive_idem_key_from_canonical(&canonical);
        let d = tracker.insert(tenant, key, &canonical).unwrap();
        assert_eq!(d, IdempotencyDecision::Accepted);
        assert_eq!(tracker.accepted_count(tenant), 1);
    }

    #[test]
    fn tracker_replay_returns_duplicate_rejected() {
        let tracker = InMemoryIdempotencyTracker::new();
        let tenant = Uuid::now_v7();
        let e = fresh_event(tenant, 1, "2026-05");
        let canonical = compute_canonical_bytes_for_idem(&e).unwrap();
        let key = derive_idem_key_from_canonical(&canonical);
        let _ = tracker.insert(tenant, key, &canonical).unwrap();
        let d2 = tracker.insert(tenant, key, &canonical).unwrap();
        assert_eq!(d2, IdempotencyDecision::DuplicateRejected);
        // Set membership did not grow.
        assert_eq!(tracker.accepted_count(tenant), 1);
    }

    #[test]
    fn tracker_collision_surfaces_sev1_error() {
        let tracker = InMemoryIdempotencyTracker::new();
        let tenant = Uuid::now_v7();
        let e1 = fresh_event(tenant, 1, "2026-05");
        let canonical1 = compute_canonical_bytes_for_idem(&e1).unwrap();
        let key1 = derive_idem_key_from_canonical(&canonical1);
        // Force a synthetic collision: insert key1 with canonical1
        // first (accepted), then try to insert key1 with DIFFERENT
        // canonical bytes (`canonical2`); the tracker MUST surface the
        // collision.
        let canonical2 = b"distinct canonical bytes synthetic for collision test".to_vec();
        let _ = tracker.insert(tenant, key1, &canonical1).unwrap();
        let err = tracker.insert(tenant, key1, &canonical2).unwrap_err();
        assert!(matches!(err, BillingEmitError::IdempotencyCollision { .. }));
    }

    #[test]
    fn tracker_per_tenant_partition() {
        let tracker = InMemoryIdempotencyTracker::new();
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let e_a = fresh_event(tenant_a, 1, "2026-05");
        let e_b = fresh_event(tenant_b, 1, "2026-05");
        let canonical_a = compute_canonical_bytes_for_idem(&e_a).unwrap();
        let canonical_b = compute_canonical_bytes_for_idem(&e_b).unwrap();
        let key_a = derive_idem_key_from_canonical(&canonical_a);
        let key_b = derive_idem_key_from_canonical(&canonical_b);
        let _ = tracker.insert(tenant_a, key_a, &canonical_a).unwrap();
        let _ = tracker.insert(tenant_b, key_b, &canonical_b).unwrap();
        assert_eq!(tracker.accepted_count(tenant_a), 1);
        assert_eq!(tracker.accepted_count(tenant_b), 1);
        // Re-inserting tenant A's key under tenant B is treated as a
        // FIRST sight for tenant B (per-tenant partition).
        let synthetic_canonical_for_b = canonical_a.clone();
        let d = tracker
            .insert(tenant_b, key_a, &synthetic_canonical_for_b)
            .unwrap();
        assert_eq!(d, IdempotencyDecision::Accepted);
        assert_eq!(tracker.accepted_count(tenant_b), 2);
    }

    #[test]
    fn tracker_clone_shares_state() {
        let t1 = InMemoryIdempotencyTracker::new();
        let t2 = t1.clone();
        let tenant = Uuid::now_v7();
        let e = fresh_event(tenant, 1, "2026-05");
        let canonical = compute_canonical_bytes_for_idem(&e).unwrap();
        let key = derive_idem_key_from_canonical(&canonical);
        let _ = t1.insert(tenant, key, &canonical).unwrap();
        // Cloned handle observes the same accepted set.
        assert_eq!(t2.accepted_count(tenant), 1);
        let d2 = t2.insert(tenant, key, &canonical).unwrap();
        assert_eq!(d2, IdempotencyDecision::DuplicateRejected);
    }
}
