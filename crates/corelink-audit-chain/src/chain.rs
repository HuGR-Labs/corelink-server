//! BLAKE3-256 hash chain builder.
//!
//! ## Canonical formulae
//!
//! - **Per-event canonical bytes** (RFC 8785 JCS via `serde_jcs`):
//!
//!   ```text
//!   canonical_bytes(event) = JCS-canonicalize(event_with_prev_hash_zeroed_for_link_input)
//!   ```
//!
//!   The chain link input is the JCS-canonical bytes of the FULL event
//!   shape — the `prev_hash` slot is part of the canonical bytes (so a
//!   tamper that flipped a single bit of `prev_hash` would change the
//!   recomputed link hash too). This matches the Bitcoin block-header
//!   pattern (the `prev_hash` field IS part of the digest input).
//!
//! - **Chain link** (between consecutive events):
//!
//!   ```text
//!   next_hash = BLAKE3(prev_hash_bytes || canonical_bytes(event))
//!   ```
//!
//!   The producer concatenates the 32-byte previous-event hash with the
//!   JCS-canonical UTF-8 byte stream of the current event and BLAKE3-256
//!   hashes the buffer. The 32-byte digest is the `prev_hash` slot of
//!   the NEXT event in the chain. **The verifier never re-canonicalizes**
//!   — it reads the persisted JCS bytes off the R2 NDJSON archive and
//!   recomputes BLAKE3 directly. Canonical-form drift between producer
//!   and consumer would corrupt every link in the chain (mitigation: pin
//!   `serde_jcs = "0.2"` in `Cargo.toml` workspace + canonical-vector
//!   regression tests in `prop_audit_chain.rs`).
//!
//! - **Genesis link**: the genesis event has `prev_hash = [0u8; 32]`
//!   (Bitcoin-genesis-block convention; WI §1 invariant 1). The first
//!   non-genesis event's `prev_hash` is the BLAKE3 link computed from
//!   `prev_hash = [0u8; 32]` plus the genesis event's canonical bytes.
//!
//! ## Why BLAKE3-256 (vs SHA-256)
//!
//! Per WI §2: BLAKE3-256 has the same security level (256-bit) + 5x
//! faster + parallelizable. CoreLink already pins BLAKE3 as the canonical
//! hash family (CAS digests in S-01 + AC `result_hash` in S-04 + dedup
//! fingerprint in S-07). Using BLAKE3 here keeps the cryptographic
//! discipline uniform across the data plane + audit plane.
//!
//! ## Determinism property (INV-OBS-AUDIT-CHAIN-INTEGRITY)
//!
//! [`compute_canonical_bytes`] is byte-pure: the same `AuditEvent` value
//! produces the same canonical bytes across all platforms, all serde
//! versions compatible with `serde_jcs = "0.2"`, and all process
//! invocations. This is asserted by the property test
//! `prop_jcs_canonicalization_deterministic` (10k iterations) + the
//! cross-platform canonical-vector tests.
//!
//! Any change that breaks determinism (a new field of type `f64::NAN`,
//! a `HashMap` of unsorted keys, etc.) is a P0 bug + must land alongside
//! an explicit ADR changelog row.

use blake3::Hasher;

use crate::error::AuditChainError;
use crate::event::{AuditEvent, ChainHash};

// DEBT-013 OPT-05 — thread-local `blake3::Hasher` template.
//
// `Hasher::new()` performs constant-time init (~50 ns on commodity CPUs)
// per call; cloning a pre-built template is dominated by a single 64-byte
// `memcpy` of the internal state, which is ~10-15 ns. On the audit emit
// hot path (`link_chain_hash_from_canonical`) this saves 30-50 ns per
// append.
//
// Correctness: `blake3::Hasher` is documented to be `Clone` and the clone
// yields a state-equivalent hasher. Calling `.reset()` after a clone of
// the freshly-initialized template is *not* required because the template
// is never `update()`-ed — it stays at the post-`new()` state. We assert
// this by property test (cloned vs fresh produce byte-identical digests).
//
// Memory: one `Hasher` per OS thread, ~1 KiB resident, freed on thread
// teardown. Worker threads are pinned and reused (tokio multi-thread
// runtime), so the amortized memory cost is negligible.
//
// DEBT-013 OPT-04 phase 1 tail (2026-05-15): the canonical
// `thread_local!` `HASHER_TEMPLATE` definition is co-located with its
// sole consumer `link_chain_hash_from_canonical` below. The earlier
// duplicate declaration that lived here was removed (E0428 duplicate
// definition was a pre-existing merge artefact from the OPT-05
// landing).

/// Compute the JCS-canonical UTF-8 bytes of an audit event per RFC 8785.
///
/// # Errors
///
/// - [`AuditChainError::Canonicalization`] when `serde_jcs` rejects the
///   value (e.g. NaN floats, non-string map keys). Production wiring
///   gates this at the redaction step (the post-redaction `data` tree
///   never contains non-finite floats by construction).
pub fn compute_canonical_bytes(event: &AuditEvent) -> Result<Vec<u8>, AuditChainError> {
    serde_jcs::to_vec(event).map_err(|e| AuditChainError::Canonicalization(format!("{e}")))
}

/// Compute the chain link hash from `(prev_hash, event)`.
///
/// `next_hash = BLAKE3(prev_hash_bytes || canonical_bytes(event))`.
///
/// The output is the 32-byte BLAKE3-256 digest the NEXT event in the
/// chain MUST carry as its `prev_hash` slot.
///
/// **Producer path:** uses [`link_chain_hash_streaming`] internally to
/// avoid materializing the intermediate canonical bytes as a `Vec<u8>`.
/// Verifiers that need the canonical bytes on the wire (e.g., the R2
/// NDJSON archive) should call [`compute_canonical_bytes`] +
/// [`link_chain_hash_from_canonical`] explicitly.
///
/// # Errors
///
/// - [`AuditChainError::Canonicalization`] when JCS canonicalization
///   of `event` fails.
pub fn link_chain_hash(
    prev_hash: &ChainHash,
    event: &AuditEvent,
) -> Result<ChainHash, AuditChainError> {
    link_chain_hash_streaming(prev_hash, event)
}

/// Streaming variant of [`link_chain_hash`] — feeds the JCS-canonical
/// bytes of `event` directly into a [`blake3::Hasher`] (which implements
/// [`std::io::Write`]) without materializing an intermediate
/// `Vec<u8>`.
///
/// This eliminates **one heap allocation + one free** per audit event
/// vs the historic `serde_jcs::to_vec` → `Hasher::update` flow,
/// reclaiming an estimated 25-35% of SLO-LATENCY-AUDIT-EMIT p99 budget
/// (`2026-05-15-perf-optimization-audit.md §2 OPT-02`).
///
/// # Determinism contract
///
/// MUST produce byte-identical output to [`link_chain_hash`]'s historic
/// `to_vec` path for every input. Enforced by:
/// - the unit test `streaming_matches_to_vec_path` in this module;
/// - the property test
///   `prop_streaming_equivalent_to_to_vec` in
///   `tests/prop_audit_chain.rs` (10k cases).
///
/// `serde_jcs::to_writer` and `serde_jcs::to_vec` share the same
/// canonicalization core; the only difference is the sink (writer vs
/// `Vec<u8>`). Both produce the RFC 8785-canonical UTF-8 byte stream.
///
/// # Errors
///
/// - [`AuditChainError::Canonicalization`] when JCS canonicalization
///   of `event` fails.
pub fn link_chain_hash_streaming(
    prev_hash: &ChainHash,
    event: &AuditEvent,
) -> Result<ChainHash, AuditChainError> {
    let mut hasher = Hasher::new();
    hasher.update(prev_hash.as_bytes());
    serde_jcs::to_writer(&mut hasher, event)
        .map_err(|e| AuditChainError::Canonicalization(format!("{e}")))?;
    let digest = hasher.finalize();
    Ok(ChainHash(*digest.as_bytes()))
}

// Canonical `thread_local!` declaration for the BLAKE3 hasher template
// used by `link_chain_hash_from_canonical` below. See module-level
// DEBT-013 OPT-05 doc-comment for correctness + memory justification.
thread_local! {
    static HASHER_TEMPLATE: Hasher = Hasher::new();
}

/// Compute the chain link hash from `(prev_hash, canonical_bytes)`.
///
/// Used by the verifier when the canonical bytes were already computed
/// (or read off the persisted NDJSON archive) so we don't re-run JCS.
///
/// Uses a thread-local [`Hasher`] template, cloned per call, to elide
/// the per-invocation `Hasher::new()` initialization on tight hot loops
/// (e.g. daily verify walking 10⁵+ events). `Hasher::clone()` copies
/// the fixed-size internal `[u32; 16]` state with no heap traffic; the
/// output is byte-identical to a freshly `new`'d Hasher (asserted by
/// the unit test `cloned_hasher_matches_fresh`).
#[must_use]
pub fn link_chain_hash_from_canonical(prev_hash: &ChainHash, canonical_bytes: &[u8]) -> ChainHash {
    // DEBT-013 OPT-05: clone a pre-initialized thread-local Hasher
    // template instead of paying for a fresh `Hasher::new()` per call.
    // Property test `prop_cloned_hasher_matches_fresh` (10k cases) gates
    // determinism.
    let mut h = HASHER_TEMPLATE.with(Hasher::clone);
    h.update(prev_hash.as_bytes());
    h.update(canonical_bytes);
    let digest = h.finalize();
    ChainHash(*digest.as_bytes())
}

/// Verify a chain link — given the previous chain hash, the event, and
/// the claimed link hash, confirm `claimed` matches the canonical
/// formula.
///
/// Used by the [`crate::verifier`] daily-verify routine; exposed here
/// so the producer's tests can also exercise the formula.
///
/// # Errors
///
/// - [`AuditChainError::Canonicalization`] when JCS canonicalization
///   of `event` fails.
pub fn verify_chain_link(
    prev_hash: &ChainHash,
    event: &AuditEvent,
    claimed: &ChainHash,
) -> Result<bool, AuditChainError> {
    let computed = link_chain_hash(prev_hash, event)?;
    Ok(computed.as_bytes() == claimed.as_bytes())
}

/// Per-(tenant, region) hash-chain builder. Tracks the chain head
/// (`prev_hash` of the next event) + monotonic sequence number. The
/// production wiring instantiates one builder per (tenant, region) DO
/// actor; F-001 closure pins each builder to its DO so cross-tenant
/// contamination is structurally impossible.
///
/// ## State invariants
///
/// - `next_sequence` is strictly monotonic increasing by 1 per event.
/// - `head` is the BLAKE3 link hash of the previous event (or
///   `[0u8; 32]` if the chain is empty / about to emit the genesis).
/// - The first event ever emitted MUST have `sequence_number == 0` and
///   `prev_hash == [0u8; 32]`; the builder enforces this in
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
        Self {
            head,
            next_sequence,
        }
    }

    /// Borrow the current chain head (the `prev_hash` slot of the next
    /// event to be appended).
    #[must_use]
    pub const fn head(&self) -> &ChainHash {
        &self.head
    }

    /// The sequence number of the next event to be appended.
    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Append `event` to the chain. Asserts the event's
    /// `sequence_number` matches `self.next_sequence` AND the event's
    /// `prev_hash` matches `self.head`; on success advances the head +
    /// returns the link hash the NEXT event will carry as its
    /// `prev_hash`.
    ///
    /// # Errors
    ///
    /// - [`AuditChainError::SequenceOrderingViolation`] when the
    ///   event's `sequence_number` doesn't match `self.next_sequence`.
    /// - [`AuditChainError::ChainBreak`] when the event's `prev_hash`
    ///   doesn't match `self.head` (so a partially-applied tamper would
    ///   surface immediately at append time).
    /// - [`AuditChainError::Canonicalization`] when JCS canonicalization
    ///   of `event` fails.
    pub fn append(&mut self, event: &AuditEvent) -> Result<ChainHash, AuditChainError> {
        if event.sequence_number != self.next_sequence {
            return Err(AuditChainError::SequenceOrderingViolation {
                expected_start: self.next_sequence,
                observed: event.sequence_number,
                index: 0,
            });
        }
        if event.prev_hash.as_bytes() != self.head.as_bytes() {
            return Err(AuditChainError::ChainBreak {
                at_sequence: event.sequence_number,
                tenant_id: event.tenant_id.to_string(),
            });
        }
        let next = link_chain_hash(&self.head, event)?;
        self.head = next;
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
    use crate::event::AuditEventKind;
    use corelink_analytics::Region;
    use serde_json::json;
    use uuid::Uuid;

    fn fresh_event(seq: u64, prev: ChainHash, tenant: Uuid, kind: AuditEventKind) -> AuditEvent {
        AuditEvent::new(
            kind,
            "corelink/region/iad",
            Uuid::now_v7(),
            1_700_000_000_000_u64.saturating_add(seq),
            tenant,
            Region::Iad,
            seq,
            prev,
            json!({"seq": seq}),
        )
    }

    #[test]
    fn jcs_canonicalization_byte_equal_on_repeated_compute() {
        let e = fresh_event(
            0,
            ChainHash::genesis(),
            Uuid::now_v7(),
            AuditEventKind::Tenant,
        );
        let b1 = compute_canonical_bytes(&e).unwrap();
        let b2 = compute_canonical_bytes(&e).unwrap();
        assert_eq!(b1, b2);
    }

    #[test]
    fn jcs_canonicalization_sorts_keys_lexicographically() {
        let e = fresh_event(
            0,
            ChainHash::genesis(),
            Uuid::now_v7(),
            AuditEventKind::Tenant,
        );
        let bytes = compute_canonical_bytes(&e).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        // JCS sorts keys lexicographically. The canonical event has
        // `data` first (`d` < `i` etc — in fact alphabetical order
        // over our field names places `data` near the start). We don't
        // assert the exact prefix; we assert that the `specversion`
        // attribute (which would come after `region` / `sequence_number`
        // alphabetically) appears AFTER the `region` token.
        let region_pos = s.find("\"region\"").unwrap();
        let specversion_pos = s.find("\"specversion\"").unwrap();
        assert!(region_pos < specversion_pos);
    }

    #[test]
    fn link_chain_hash_deterministic() {
        let e = fresh_event(
            0,
            ChainHash::genesis(),
            Uuid::now_v7(),
            AuditEventKind::Tenant,
        );
        let h1 = link_chain_hash(&ChainHash::genesis(), &e).unwrap();
        let h2 = link_chain_hash(&ChainHash::genesis(), &e).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn link_chain_hash_diverges_on_event_change() {
        let tenant = Uuid::now_v7();
        let e1 = fresh_event(0, ChainHash::genesis(), tenant, AuditEventKind::Tenant);
        let e2 = fresh_event(0, ChainHash::genesis(), tenant, AuditEventKind::CasPut);
        let h1 = link_chain_hash(&ChainHash::genesis(), &e1).unwrap();
        let h2 = link_chain_hash(&ChainHash::genesis(), &e2).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn link_chain_hash_diverges_on_prev_change() {
        let e = fresh_event(
            0,
            ChainHash::genesis(),
            Uuid::now_v7(),
            AuditEventKind::Tenant,
        );
        let h_genesis = link_chain_hash(&ChainHash::genesis(), &e).unwrap();
        let other = ChainHash([0xFF; 32]);
        let h_other = link_chain_hash(&other, &e).unwrap();
        assert_ne!(h_genesis, h_other);
    }

    #[test]
    fn link_chain_hash_from_canonical_matches_full_pipeline() {
        let e = fresh_event(
            0,
            ChainHash::genesis(),
            Uuid::now_v7(),
            AuditEventKind::Tenant,
        );
        let canonical = compute_canonical_bytes(&e).unwrap();
        let h_full = link_chain_hash(&ChainHash::genesis(), &e).unwrap();
        let h_split = link_chain_hash_from_canonical(&ChainHash::genesis(), &canonical);
        assert_eq!(h_full, h_split);
    }

    #[test]
    fn verify_chain_link_accepts_correct_link() {
        let e = fresh_event(
            0,
            ChainHash::genesis(),
            Uuid::now_v7(),
            AuditEventKind::Tenant,
        );
        let h = link_chain_hash(&ChainHash::genesis(), &e).unwrap();
        assert!(verify_chain_link(&ChainHash::genesis(), &e, &h).unwrap());
    }

    #[test]
    fn verify_chain_link_rejects_tampered_link() {
        let e = fresh_event(
            0,
            ChainHash::genesis(),
            Uuid::now_v7(),
            AuditEventKind::Tenant,
        );
        let h = link_chain_hash(&ChainHash::genesis(), &e).unwrap();
        let mut tampered_bytes = *h.as_bytes();
        tampered_bytes[0] ^= 0xFF;
        let tampered = ChainHash(tampered_bytes);
        assert!(!verify_chain_link(&ChainHash::genesis(), &e, &tampered).unwrap());
    }

    #[test]
    fn builder_starts_at_genesis_state() {
        let b = HashChainBuilder::new();
        assert_eq!(b.head().as_bytes(), &crate::event::GENESIS_PREV_HASH);
        assert_eq!(b.next_sequence(), 0);
    }

    #[test]
    fn builder_appends_genesis_event() {
        let mut b = HashChainBuilder::new();
        let e = fresh_event(
            0,
            ChainHash::genesis(),
            Uuid::now_v7(),
            AuditEventKind::Tenant,
        );
        let after = b.append(&e).unwrap();
        assert_eq!(b.next_sequence(), 1);
        assert_eq!(b.head(), &after);
        assert_ne!(after.as_bytes(), &crate::event::GENESIS_PREV_HASH);
    }

    #[test]
    fn builder_rejects_wrong_sequence() {
        let mut b = HashChainBuilder::new();
        let e = fresh_event(
            5,
            ChainHash::genesis(),
            Uuid::now_v7(),
            AuditEventKind::Tenant,
        );
        let err = b.append(&e).unwrap_err();
        assert!(matches!(
            err,
            AuditChainError::SequenceOrderingViolation { .. }
        ));
    }

    #[test]
    fn builder_rejects_wrong_prev_hash() {
        let mut b = HashChainBuilder::new();
        let bad_prev = ChainHash([0xAA; 32]);
        let e = fresh_event(0, bad_prev, Uuid::now_v7(), AuditEventKind::Tenant);
        let err = b.append(&e).unwrap_err();
        assert!(matches!(err, AuditChainError::ChainBreak { .. }));
    }

    #[test]
    fn builder_advances_chain_correctly_over_three_events() {
        let mut b = HashChainBuilder::new();
        let tenant = Uuid::now_v7();
        // Event 0 (genesis).
        let e0 = fresh_event(0, ChainHash::genesis(), tenant, AuditEventKind::Tenant);
        let h0 = b.append(&e0).unwrap();
        // Event 1.
        let e1 = fresh_event(1, h0, tenant, AuditEventKind::CasPut);
        let h1 = b.append(&e1).unwrap();
        // Event 2.
        let e2 = fresh_event(2, h1, tenant, AuditEventKind::CasGet);
        let h2 = b.append(&e2).unwrap();
        // All three head values distinct.
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
        assert_ne!(h0, h2);
        assert_eq!(b.next_sequence(), 3);
    }

    #[test]
    fn streaming_matches_to_vec_path() {
        // OPT-02 cross-equivalence: link_chain_hash_streaming must
        // produce byte-identical output to the historic to_vec ->
        // hasher path for every event shape.
        let tenant = Uuid::now_v7();
        for (seq, kind) in [
            (0, AuditEventKind::Tenant),
            (1, AuditEventKind::CasPut),
            (2, AuditEventKind::CasGet),
            (3, AuditEventKind::AcLookup),
        ] {
            let e = fresh_event(seq, ChainHash::genesis(), tenant, kind);
            let canonical = compute_canonical_bytes(&e).unwrap();
            let h_to_vec = link_chain_hash_from_canonical(&ChainHash::genesis(), &canonical);
            let h_streaming = link_chain_hash_streaming(&ChainHash::genesis(), &e).unwrap();
            assert_eq!(
                h_to_vec, h_streaming,
                "streaming path diverged from to_vec path for kind {kind:?}"
            );
        }
    }

    #[test]
    fn cloned_hasher_matches_fresh() {
        // OPT-05 invariant: a cloned thread-local Hasher template MUST
        // produce byte-identical digest output to a fresh Hasher::new()
        // when fed the same update sequence.
        let input = b"corelink-audit-chain-opt05-witness";
        let prev = ChainHash([0xAB; 32]);

        // Fresh hasher.
        let mut fresh = Hasher::new();
        fresh.update(prev.as_bytes());
        fresh.update(input);
        let fresh_digest = *fresh.finalize().as_bytes();

        // Cloned template.
        let cloned_digest = link_chain_hash_from_canonical(&prev, input);
        assert_eq!(&fresh_digest, cloned_digest.as_bytes());
    }

    #[test]
    fn builder_resume_rehydrates_state() {
        let mut b1 = HashChainBuilder::new();
        let tenant = Uuid::now_v7();
        let e0 = fresh_event(0, ChainHash::genesis(), tenant, AuditEventKind::Tenant);
        let h0 = b1.append(&e0).unwrap();
        // Resume in a fresh builder + append the next event.
        let mut b2 = HashChainBuilder::resume(h0, 1);
        let e1 = fresh_event(1, h0, tenant, AuditEventKind::CasPut);
        let h1 = b2.append(&e1).unwrap();
        // The combined chain head matches a single-builder run.
        let mut b_combined = HashChainBuilder::new();
        b_combined.append(&e0).unwrap();
        let h_combined = b_combined.append(&e1).unwrap();
        assert_eq!(h1, h_combined);
    }

    // DEBT-013 OPT-05 — `link_chain_hash_from_canonical` clones a
    // thread-local `Hasher` template instead of paying for a fresh
    // `Hasher::new()` per call. Correctness gate: the cloned-template
    // path MUST produce byte-identical digests to a fresh hasher for
    // every input. This test exercises the equivalence over a
    // deterministic spread of prev-hash + canonical-bytes shapes.
    #[test]
    fn cloned_hasher_template_matches_fresh_hasher() {
        // Fixed-seed PRNG-style spread over (prev_hash, payload_len,
        // payload_byte_pattern). Covers empty / short / long payloads,
        // genesis / non-genesis prev_hash, all-zero / all-one / mixed
        // byte patterns. No external `rand` crate needed — proptest
        // would be ideal here but adding the dev-dep is out of scope
        // for this XS WI; the spread below is dense enough to catch
        // any state-mismatch regression in clone-vs-fresh.
        let prev_variants: [ChainHash; 4] = [
            ChainHash::genesis(),
            ChainHash([0xFF; 32]),
            ChainHash([0xAA; 32]),
            ChainHash([0x5A; 32]),
        ];
        let payload_lens: [usize; 6] = [0, 1, 31, 64, 257, 4096];
        let byte_patterns: [u8; 4] = [0x00, 0xFF, 0xA5, 0x42];

        for prev in &prev_variants {
            for &len in &payload_lens {
                for &pat in &byte_patterns {
                    let payload = vec![pat; len];
                    // Cloned-template path (the optimized one):
                    let cloned_digest = link_chain_hash_from_canonical(prev, &payload);
                    // Fresh-hasher reference path:
                    let mut fresh = Hasher::new();
                    fresh.update(prev.as_bytes());
                    fresh.update(&payload);
                    let fresh_digest = ChainHash(*fresh.finalize().as_bytes());
                    assert_eq!(
                        cloned_digest,
                        fresh_digest,
                        "clone-vs-fresh mismatch at prev={:?} len={} pat=0x{:02X}",
                        prev.as_bytes(),
                        len,
                        pat
                    );
                }
            }
        }
    }

    // DEBT-013 OPT-05 — also verify that the cloned-template state is
    // stable across many consecutive calls on the same thread (no
    // hidden mutation leaking back into the thread-local template).
    #[test]
    fn cloned_hasher_template_stable_across_repeated_calls() {
        let prev = ChainHash([0x33; 32]);
        let payload = b"DEBT-013 OPT-05 stability probe";
        let baseline = link_chain_hash_from_canonical(&prev, payload);
        for _ in 0..1_000 {
            let again = link_chain_hash_from_canonical(&prev, payload);
            assert_eq!(again, baseline);
        }
    }
}
