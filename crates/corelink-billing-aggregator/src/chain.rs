//! BLAKE3-256 hash chain over JCS-canonical
//! [`crate::event::AggregatedCounter`] bytes.
//!
//! ## Canonical formulae
//!
//! - **Per-aggregate canonical bytes** (RFC 8785 JCS via `serde_jcs`):
//!
//!   ```text
//!   canonical_bytes(aggregate) = JCS-canonicalize(aggregate)
//!   ```
//!
//!   The chain link input is the JCS-canonical bytes of the FULL
//!   `AggregatedCounter` shape — the `prev_hash` slot is part of the
//!   canonical bytes (so a tamper that flipped a single bit of
//!   `prev_hash` would change the recomputed link hash too). This
//!   matches the Bitcoin block-header pattern (the `prev_hash` field
//!   IS part of the digest input) + mirrors `corelink-audit-chain`
//!   discipline.
//!
//! - **Chain link** (between consecutive aggregates):
//!
//!   ```text
//!   next_hash = BLAKE3(prev_hash_bytes || canonical_bytes(aggregate))
//!   ```
//!
//!   The producer concatenates the 32-byte previous-aggregate hash with
//!   the JCS-canonical UTF-8 byte stream of the current aggregate and
//!   BLAKE3-256 hashes the buffer. The 32-byte digest is the `prev_hash`
//!   slot of the NEXT aggregate in the chain. The verifier never
//!   re-canonicalizes — it reads the persisted JCS bytes off the
//!   counter store + recomputes BLAKE3 directly.
//!
//! - **Genesis link**: the genesis aggregate has `prev_hash =
//!   [0u8; 32]` (Bitcoin-genesis-block convention; mirrors
//!   `corelink_audit_chain::GENESIS_PREV_HASH`). The first
//!   non-genesis aggregate's `prev_hash` is the BLAKE3 link hash of the
//!   genesis aggregate's canonical bytes.
//!
//! ## Why BLAKE3-256 (vs SHA-256)
//!
//! Per WI-S10-002 §1 invariant 7: BLAKE3-256 has the same security
//! level (256-bit) + 5x faster + parallelizable. CoreLink already pins
//! BLAKE3 as the canonical hash family (CAS digests in S-01 + AC
//! `result_hash` in S-04 + dedup fingerprint in S-07 + audit chain
//! links in S-09 + idem_key derivation in WI-S10-001). Using BLAKE3
//! here keeps the cryptographic discipline uniform across the data
//! plane + audit plane + billing plane.
//!
//! ## Determinism property (idempotent re-aggregation)
//!
//! The orchestrator's deterministic input ordering primitive (sort by
//! `(time_ms, idem_key)` lexicographic) + the JCS canonicalization
//! (sorted keys + canonical numeric / string forms) + BLAKE3 (pure
//! function) compose to give the canonical idempotent re-aggregation
//! property: re-running the aggregator over the same input set
//! reproduces the same chain digest (WI-S10-002 §1 invariant 9). The
//! property test `prop_idempotent_rerun_same_chain_hash` (10k iter PR
//! gate; 100k nightly via `PROPTEST_CASES`) pins this directly.

use blake3::Hasher;

use crate::error::AggregatorError;
use crate::event::{AggregatedCounter, ChainHash};

/// Compute the JCS-canonical UTF-8 bytes of an aggregate per RFC 8785.
///
/// # Errors
///
/// - [`AggregatorError::Canonicalization`] when `serde_jcs` rejects the
///   value (e.g. NaN floats, non-string map keys). The aggregate data
///   tree never contains non-finite floats by construction (every
///   field is `u64` / `u128` / `String` / `Vec<IdemKey>`); this is a
///   defensive guard.
pub fn compute_canonical_bytes(
    aggregate: &AggregatedCounter,
) -> Result<Vec<u8>, AggregatorError> {
    serde_jcs::to_vec(aggregate)
        .map_err(|e| AggregatorError::Canonicalization(format!("{e}")))
}

/// Compute the chain link hash from `(prev_hash, aggregate)`.
///
/// `next_hash = BLAKE3(prev_hash_bytes || canonical_bytes(aggregate))`.
///
/// The output is the 32-byte BLAKE3-256 digest the NEXT aggregate in
/// the chain MUST carry as its `prev_hash` slot.
///
/// # Errors
///
/// - [`AggregatorError::Canonicalization`] when JCS canonicalization
///   of `aggregate` fails.
pub fn link_chain_hash(
    prev_hash: &ChainHash,
    aggregate: &AggregatedCounter,
) -> Result<ChainHash, AggregatorError> {
    let canonical = compute_canonical_bytes(aggregate)?;
    Ok(link_chain_hash_from_canonical(prev_hash, &canonical))
}

/// Compute the chain link hash from `(prev_hash, canonical_bytes)`.
///
/// Used by the verifier when the canonical bytes were already computed
/// (or read off the persisted store) so we don't re-run JCS.
#[must_use]
pub fn link_chain_hash_from_canonical(
    prev_hash: &ChainHash,
    canonical_bytes: &[u8],
) -> ChainHash {
    let mut h = Hasher::new();
    h.update(prev_hash.as_bytes());
    h.update(canonical_bytes);
    let digest = h.finalize();
    ChainHash(*digest.as_bytes())
}

/// Verify a chain link — given the previous chain hash, the aggregate,
/// and the claimed link hash, confirm `claimed` matches the canonical
/// formula.
///
/// Used by the daily ChainVerifier (deferred to WI-S10-007 alongside
/// the production CF Cron DO binding); exposed here so the
/// orchestrator's tests can also exercise the formula.
///
/// # Errors
///
/// - [`AggregatorError::Canonicalization`] when JCS canonicalization
///   of `aggregate` fails.
pub fn verify_chain_link(
    prev_hash: &ChainHash,
    aggregate: &AggregatedCounter,
    claimed: &ChainHash,
) -> Result<bool, AggregatorError> {
    let computed = link_chain_hash(prev_hash, aggregate)?;
    Ok(computed.as_bytes() == claimed.as_bytes())
}

/// Per-(tenant, billing_period) hash-chain builder. Tracks the chain
/// head (`prev_hash` of the next aggregate) + monotonic sequence
/// number. Mirrors `corelink_audit_chain::HashChainBuilder` discipline:
/// production wiring instantiates one builder per (tenant,
/// billing_period) DO actor; F-001 closure pins each builder to its DO
/// so cross-tenant contamination is structurally impossible.
///
/// ## State invariants
///
/// - `next_sequence` is strictly monotonic increasing by 1 per
///   aggregate.
/// - `head` is the BLAKE3 link hash of the previous aggregate (or
///   `[0u8; 32]` if the chain is empty / about to emit the genesis).
/// - The first aggregate ever appended MUST have `sequence_number == 0`
///   AND `prev_hash == [0u8; 32]`; the builder enforces this in
///   [`HashChainBuilder::append`].
#[derive(Clone, Debug)]
pub struct HashChainBuilder {
    head: ChainHash,
    next_sequence: u64,
}

impl HashChainBuilder {
    /// Construct a fresh builder pointing at the canonical genesis
    /// state: `head = [0u8; 32]`, `next_sequence = 0`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            head: ChainHash::genesis(),
            next_sequence: 0,
        }
    }

    /// Resume a builder from the persisted (head, next_sequence) tuple.
    /// Used by the production wiring at cold-start to rehydrate the
    /// chain head from the durable mirror after a worker restart.
    #[must_use]
    pub const fn resume(head: ChainHash, next_sequence: u64) -> Self {
        Self { head, next_sequence }
    }

    /// Borrow the current chain head (the `prev_hash` slot of the next
    /// aggregate to be appended).
    #[must_use]
    pub const fn head(&self) -> &ChainHash {
        &self.head
    }

    /// The sequence number of the next aggregate to be appended.
    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Append `aggregate` to the chain. Asserts the aggregate's
    /// `sequence_number` matches `self.next_sequence` AND the aggregate's
    /// `prev_hash` matches `self.head`; on success advances the head +
    /// returns the link hash the NEXT aggregate will carry as its
    /// `prev_hash`.
    ///
    /// # Errors
    ///
    /// - [`AggregatorError::ChainBreak`] when the aggregate's
    ///   `sequence_number` doesn't match `self.next_sequence` OR the
    ///   `prev_hash` doesn't match `self.head` (so a partially-applied
    ///   tamper would surface immediately at append time).
    /// - [`AggregatorError::Canonicalization`] when JCS canonicalization
    ///   of `aggregate` fails.
    pub fn append(
        &mut self,
        aggregate: &AggregatedCounter,
    ) -> Result<ChainHash, AggregatorError> {
        if aggregate.sequence_number != self.next_sequence {
            return Err(AggregatorError::ChainBreak(format!(
                "sequence ordering violation: expected {} observed {}",
                self.next_sequence, aggregate.sequence_number
            )));
        }
        if aggregate.prev_hash.as_bytes() != self.head.as_bytes() {
            return Err(AggregatorError::ChainBreak(format!(
                "prev_hash mismatch at sequence {}: head={} aggregate.prev_hash={}",
                aggregate.sequence_number,
                self.head.to_hex(),
                aggregate.prev_hash.to_hex()
            )));
        }
        let next = link_chain_hash(&self.head, aggregate)?;
        self.head = next;
        // u64::MAX overflow is structurally unreachable: 1 aggregate per
        // (tenant, period, kind) per cron run; even a degenerate 1Hz cron
        // would take 5×10^11 years to overflow.
        self.next_sequence = self.next_sequence.saturating_add(1);
        Ok(self.head)
    }
}

impl Default for HashChainBuilder {
    fn default() -> Self {
        Self::new()
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
    use crate::event::AggregatedCounter;
    use corelink_billing_emit::{IdemKey, UsageEventKind};
    use uuid::Uuid;

    fn fresh_aggregate(seq: u64, prev: ChainHash, tenant: Uuid) -> AggregatedCounter {
        AggregatedCounter::new(
            "corelink/region/iad/aggregator",
            Uuid::now_v7(),
            1_700_000_000_000_u64.saturating_add(seq),
            seq,
            prev,
            tenant,
            "2026-05",
            UsageEventKind::CasPut,
            42,
            1,
            0,
            1000,
            vec![IdemKey::genesis()],
        )
    }

    #[test]
    fn jcs_canonicalization_byte_equal_on_repeated_compute() {
        let a = fresh_aggregate(0, ChainHash::genesis(), Uuid::now_v7());
        let b1 = compute_canonical_bytes(&a).unwrap();
        let b2 = compute_canonical_bytes(&a).unwrap();
        assert_eq!(b1, b2);
    }

    #[test]
    fn jcs_canonicalization_sorts_keys_lexicographically() {
        let a = fresh_aggregate(0, ChainHash::genesis(), Uuid::now_v7());
        let bytes = compute_canonical_bytes(&a).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        // JCS sorts keys lexicographically: `data` (`d`) appears before
        // `specversion` (`s`).
        let data_pos = s.find("\"data\"").unwrap();
        let spec_pos = s.find("\"specversion\"").unwrap();
        assert!(data_pos < spec_pos);
    }

    #[test]
    fn link_chain_hash_deterministic() {
        let a = fresh_aggregate(0, ChainHash::genesis(), Uuid::now_v7());
        let h1 = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        let h2 = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn link_chain_hash_diverges_on_aggregate_change() {
        let tenant = Uuid::now_v7();
        let a1 = fresh_aggregate(0, ChainHash::genesis(), tenant);
        let mut a2 = a1.clone();
        a2.data.total_qty = 99;
        let h1 = link_chain_hash(&ChainHash::genesis(), &a1).unwrap();
        let h2 = link_chain_hash(&ChainHash::genesis(), &a2).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn link_chain_hash_diverges_on_prev_change() {
        let a = fresh_aggregate(0, ChainHash::genesis(), Uuid::now_v7());
        let h_genesis = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        let other = ChainHash([0xFF; 32]);
        let h_other = link_chain_hash(&other, &a).unwrap();
        assert_ne!(h_genesis, h_other);
    }

    #[test]
    fn link_chain_hash_from_canonical_matches_full_pipeline() {
        let a = fresh_aggregate(0, ChainHash::genesis(), Uuid::now_v7());
        let canonical = compute_canonical_bytes(&a).unwrap();
        let h_full = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        let h_split = link_chain_hash_from_canonical(&ChainHash::genesis(), &canonical);
        assert_eq!(h_full, h_split);
    }

    #[test]
    fn verify_chain_link_accepts_correct_link() {
        let a = fresh_aggregate(0, ChainHash::genesis(), Uuid::now_v7());
        let h = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        assert!(verify_chain_link(&ChainHash::genesis(), &a, &h).unwrap());
    }

    #[test]
    fn verify_chain_link_rejects_tampered_link() {
        let a = fresh_aggregate(0, ChainHash::genesis(), Uuid::now_v7());
        let h = link_chain_hash(&ChainHash::genesis(), &a).unwrap();
        let mut tampered_bytes = *h.as_bytes();
        tampered_bytes[0] ^= 0xFF;
        let tampered = ChainHash(tampered_bytes);
        assert!(!verify_chain_link(&ChainHash::genesis(), &a, &tampered).unwrap());
    }

    #[test]
    fn builder_starts_at_genesis_state() {
        let b = HashChainBuilder::new();
        assert_eq!(b.head().as_bytes(), &crate::event::GENESIS_PREV_HASH);
        assert_eq!(b.next_sequence(), 0);
    }

    #[test]
    fn builder_appends_genesis_aggregate() {
        let mut b = HashChainBuilder::new();
        let a = fresh_aggregate(0, ChainHash::genesis(), Uuid::now_v7());
        let after = b.append(&a).unwrap();
        assert_eq!(b.next_sequence(), 1);
        assert_eq!(b.head(), &after);
        assert_ne!(after.as_bytes(), &crate::event::GENESIS_PREV_HASH);
    }

    #[test]
    fn builder_rejects_wrong_sequence() {
        let mut b = HashChainBuilder::new();
        let a = fresh_aggregate(5, ChainHash::genesis(), Uuid::now_v7());
        let err = b.append(&a).unwrap_err();
        assert!(matches!(err, AggregatorError::ChainBreak(_)));
    }

    #[test]
    fn builder_rejects_wrong_prev_hash() {
        let mut b = HashChainBuilder::new();
        let bad_prev = ChainHash([0xAA; 32]);
        let a = fresh_aggregate(0, bad_prev, Uuid::now_v7());
        let err = b.append(&a).unwrap_err();
        assert!(matches!(err, AggregatorError::ChainBreak(_)));
    }

    #[test]
    fn builder_advances_chain_correctly_over_three_aggregates() {
        let mut b = HashChainBuilder::new();
        let tenant = Uuid::now_v7();
        // Aggregate 0 (genesis).
        let mut a0 = fresh_aggregate(0, ChainHash::genesis(), tenant);
        a0.data.total_qty = 1;
        let h0 = b.append(&a0).unwrap();
        // Aggregate 1.
        let mut a1 = fresh_aggregate(1, h0, tenant);
        a1.data.total_qty = 2;
        let h1 = b.append(&a1).unwrap();
        // Aggregate 2.
        let mut a2 = fresh_aggregate(2, h1, tenant);
        a2.data.total_qty = 3;
        let h2 = b.append(&a2).unwrap();
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
        assert_ne!(h0, h2);
        assert_eq!(b.next_sequence(), 3);
    }

    #[test]
    fn builder_resume_rehydrates_state() {
        let mut b1 = HashChainBuilder::new();
        let tenant = Uuid::now_v7();
        let a0 = fresh_aggregate(0, ChainHash::genesis(), tenant);
        let h0 = b1.append(&a0).unwrap();
        let mut b2 = HashChainBuilder::resume(h0, 1);
        let mut a1 = fresh_aggregate(1, h0, tenant);
        a1.data.total_qty = 99;
        let h1 = b2.append(&a1).unwrap();
        let mut b_combined = HashChainBuilder::new();
        b_combined.append(&a0).unwrap();
        let h_combined = b_combined.append(&a1).unwrap();
        assert_eq!(h1, h_combined);
    }
}
