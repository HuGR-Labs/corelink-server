//! BLAKE3-256 idempotency-key derivation from the JCS-canonical
//! [`corelink_billing_aggregator::AggregatedCounter`] bytes.
//!
//! ## Canonical formula
//!
//! Per WI-S10-003 §6.1 + sprint contract §5.3 R-S10-6 the canonical
//! Stripe `Idempotency-Key` HTTP header MUST be derived deterministically
//! from the aggregate the usage record summarizes:
//!
//! ```text
//! idempotency_key = BLAKE3-256(JCS-canonicalize(aggregated_counter))
//! ```
//!
//! - The input is the full canonical bytes of the
//!   [`corelink_billing_aggregator::AggregatedCounter`] shape (CloudEvents
//!   1.0 envelope + chain link slot + typed `data` payload). JCS (RFC
//!   8785) sorts keys lexicographically + emits canonical numeric / string
//!   forms so two byte-equivalent aggregates with field-ordering
//!   differences produce the SAME canonical bytes (= the same
//!   idempotency key).
//! - The 32-byte BLAKE3-256 digest is hex-rendered (64 chars
//!   lowercase, RFC 4648 §8) for the on-the-wire `Idempotency-Key`
//!   header value.
//!
//! ## Why BLAKE3 (vs UUID v4 vs raw aggregate bytes)
//!
//! - UUID v4 would NOT survive Stripe API retry: each retry produces a
//!   fresh UUID, so Stripe's idempotency window degenerates to a no-op +
//!   the customer is charged twice (chargeback storm). The whole point
//!   of the canonical derivation is REPLAY SAFETY: same aggregate
//!   reproduces the same key regardless of retry storm.
//! - Raw aggregate bytes are unbounded length (arbitrary
//!   `idem_keys_seen` list); Stripe caps `Idempotency-Key` at 255 chars.
//!   BLAKE3-256 hex (64 chars) is well below the cap + uniform across
//!   aggregate sizes.
//! - BLAKE3 has 128-bit collision security at 256-bit output (same as
//!   SHA-256) + is 5x faster + parallelizable. CoreLink already pins
//!   BLAKE3 for CAS digests (S-01) + AC `result_hash` (S-04) + dedup
//!   fingerprint (S-07) + audit chain links (S-09) + idem_key derivation
//!   (WI-S10-001) + counter aggregator chain (WI-S10-002); using BLAKE3
//!   here keeps the cryptographic discipline uniform.
//!
//! Collision probability: birthday-bound for 2^N aggregates with 2^256
//! output is 2^(2N - 256). For N = 30 (10^9 aggregates/yr × 7y retention
//! ≈ 2^33), expected collisions ≈ 2^-190 — orders of magnitude below ANY
//! detection threshold. Adversarial collisions require 2^128 work
//! (BLAKE3 collision-resistance security); not exploitable.

use blake3::Hasher;

use corelink_billing_aggregator::AggregatedCounter;

use crate::error::StripeError;
use crate::event::IdempotencyKey;

/// Compute the JCS-canonical UTF-8 bytes of an aggregated counter per
/// RFC 8785. Mirrors
/// `corelink_billing_aggregator::compute_canonical_bytes` discipline.
///
/// # Errors
///
/// - [`StripeError::Canonicalization`] when `serde_jcs` rejects the
///   value (e.g. NaN floats, non-string map keys). The aggregate data
///   tree never contains non-finite floats by construction (every
///   field is `u64` / `u128` / `String` / `Vec<IdemKey>`); this is a
///   defensive guard.
pub fn compute_canonical_aggregate_bytes(
    aggregate: &AggregatedCounter,
) -> Result<Vec<u8>, StripeError> {
    serde_jcs::to_vec(aggregate).map_err(|e| StripeError::Canonicalization(format!("{e}")))
}

/// Derive the canonical Stripe `Idempotency-Key` from an aggregated
/// counter. Pure BLAKE3-256 of the JCS-canonical aggregate bytes.
///
/// # Errors
///
/// - [`StripeError::Canonicalization`] when JCS canonicalization of
///   `aggregate` fails.
pub fn derive_idempotency_key(
    aggregate: &AggregatedCounter,
) -> Result<IdempotencyKey, StripeError> {
    let canonical = compute_canonical_aggregate_bytes(aggregate)?;
    Ok(derive_idempotency_key_from_canonical(&canonical))
}

/// Derive the canonical Stripe `Idempotency-Key` from already-
/// canonicalized aggregate bytes. Used by the in-memory adapter +
/// production wiring once the bytes are available; avoids re-running
/// JCS.
#[must_use]
pub fn derive_idempotency_key_from_canonical(canonical_bytes: &[u8]) -> IdempotencyKey {
    let mut h = Hasher::new();
    h.update(canonical_bytes);
    let digest = h.finalize();
    IdempotencyKey(*digest.as_bytes())
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
    use corelink_billing_aggregator::ChainHash;
    use corelink_billing_emit::{IdemKey, UsageEventKind};
    use uuid::Uuid;

    fn fresh_aggregate(seq: u64, prev: ChainHash, tenant: Uuid, qty: u128) -> AggregatedCounter {
        AggregatedCounter::new(
            "corelink/region/iad/aggregator",
            Uuid::now_v7(),
            1_700_000_000_000_u64.saturating_add(seq),
            seq,
            prev,
            tenant,
            "2026-05",
            UsageEventKind::CasPut,
            qty,
            1,
            0,
            1000,
            vec![IdemKey::genesis()],
        )
    }

    #[test]
    fn canonical_bytes_deterministic() {
        let tenant = Uuid::now_v7();
        let a = fresh_aggregate(0, ChainHash::genesis(), tenant, 42);
        let b1 = compute_canonical_aggregate_bytes(&a).unwrap();
        let b2 = compute_canonical_aggregate_bytes(&a).unwrap();
        assert_eq!(b1, b2);
    }

    #[test]
    fn idempotency_key_deterministic_for_same_aggregate() {
        let tenant = Uuid::now_v7();
        let a = fresh_aggregate(0, ChainHash::genesis(), tenant, 42);
        let k1 = derive_idempotency_key(&a).unwrap();
        let k2 = derive_idempotency_key(&a).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn idempotency_key_diverges_on_aggregate_change() {
        let tenant = Uuid::now_v7();
        let a1 = fresh_aggregate(0, ChainHash::genesis(), tenant, 42);
        let mut a2 = a1.clone();
        a2.data.total_qty = 99;
        let k1 = derive_idempotency_key(&a1).unwrap();
        let k2 = derive_idempotency_key(&a2).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn idempotency_key_diverges_per_tenant() {
        let t1 = Uuid::now_v7();
        let t2 = Uuid::now_v7();
        let a1 = fresh_aggregate(0, ChainHash::genesis(), t1, 42);
        let a2 = fresh_aggregate(0, ChainHash::genesis(), t2, 42);
        let k1 = derive_idempotency_key(&a1).unwrap();
        let k2 = derive_idempotency_key(&a2).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn idempotency_key_diverges_per_billing_period() {
        let tenant = Uuid::now_v7();
        let mut a1 = fresh_aggregate(0, ChainHash::genesis(), tenant, 42);
        a1.data.billing_period = "2026-05".to_string();
        let mut a2 = fresh_aggregate(0, ChainHash::genesis(), tenant, 42);
        a2.data.billing_period = "2026-06".to_string();
        let k1 = derive_idempotency_key(&a1).unwrap();
        let k2 = derive_idempotency_key(&a2).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn idempotency_key_hex_form_is_64_chars() {
        let tenant = Uuid::now_v7();
        let a = fresh_aggregate(0, ChainHash::genesis(), tenant, 42);
        let k = derive_idempotency_key(&a).unwrap();
        assert_eq!(k.to_hex().len(), 64);
        for c in k.to_hex().chars() {
            assert!(c.is_ascii_hexdigit() && (c.is_ascii_lowercase() || c.is_ascii_digit()));
        }
    }

    #[test]
    fn split_canonical_form_matches_full_pipeline() {
        let tenant = Uuid::now_v7();
        let a = fresh_aggregate(0, ChainHash::genesis(), tenant, 42);
        let canonical = compute_canonical_aggregate_bytes(&a).unwrap();
        let k_full = derive_idempotency_key(&a).unwrap();
        let k_split = derive_idempotency_key_from_canonical(&canonical);
        assert_eq!(k_full, k_split);
    }
}
