//! Property tests pinning the load-bearing invariants of
//! `corelink-dedup` at 10k iterations per check (PR-gate; nightly
//! 100k via env var override).
//!
//! Coverage map:
//!
//! - `prop_find_missing_returns_only_absent_digests` — for any
//!   `(present_set, queried_set)`, the response is exactly
//!   `queried_set ∖ present_set`. The set-difference identity is the
//!   load-bearing semantic guarantee.
//! - `prop_tenant_isolation` — Tenant A's chunks NEVER surface in
//!   Tenant B's `find_missing_blobs` result. CTRL-ISO-005 enforced at
//!   the public API surface.
//! - `prop_idempotent` — repeated calls with the same `(tenant_id,
//!   queried)` return the same `FindMissingBlobsOutcome` byte-for-byte.
//! - `prop_dedup_ratio_monotone` — repeated `Reused` writes never
//!   decrement `chunks_reused_total`; repeated `Inserted` writes
//!   never decrement `chunks_inserted_total`. The dedup ratio is
//!   monotone non-decreasing across the reused-only suffix of any
//!   write trace.
//! - `prop_input_order_preserved` — output `missing` order matches
//!   input order modulo membership filter.
//! - `prop_duplicate_input_duplicate_output` — N copies of an absent
//!   digest in the input yield N copies in the output (server NEVER
//!   collapses client-side dedup; ADR-0028 inheritance).
//!
//! ## Iteration count rationale
//!
//! 10k iter per property is the sprint contract §5 R-S07-8 floor; the
//! GC + AC + CAS sprints all ship at this density. Nightly bumps to
//! 100k iter via the `PROPTEST_CASES` env var (CI workflow override).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::collections::BTreeSet;

use corelink_cas::dedup::{
    record_chunk_write, BlobDigest, ChunkWriteOutcome, DedupIndex, DedupMetricKind,
    InMemoryDedupIndex, InMemoryDedupMetrics, MAX_FIND_MISSING_BATCH_SIZE,
};
use proptest::prelude::*;
use uuid::Uuid;

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

fn ten_a() -> Uuid {
    Uuid::from_u128(0xa)
}

fn ten_b() -> Uuid {
    Uuid::from_u128(0xb)
}

/// Generate a deterministic 64-char lower-hex digest from a seed byte.
fn dig_from_seed(seed: u8) -> BlobDigest {
    let bytes = [seed; 32];
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    BlobDigest::new(s).unwrap()
}

/// Strategy: a sub-batch of distinct digest seeds (`u8`); cap at
/// `MAX_FIND_MISSING_BATCH_SIZE` so the trait's batch-size precondition
/// is honoured by every generated input.
fn distinct_seeds_strategy(
    max_size: usize,
) -> impl Strategy<Value = Vec<u8>> {
    let cap = max_size.min(usize::from(u8::MAX));
    prop::collection::btree_set(0u8..=u8::MAX, 0..=cap)
        .prop_map(|s| s.into_iter().collect::<Vec<u8>>())
}

/// Strategy: an arbitrary order-preserving multiset of digest seeds
/// (allows duplicates; bounded to `MAX_FIND_MISSING_BATCH_SIZE`).
fn arbitrary_seeds_strategy(
    max_size: usize,
) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(0u8..=u8::MAX, 0..=max_size)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Set-difference identity: `find_missing_blobs(T, queried) =
    /// queried ∖ present_for_T`.
    #[test]
    fn prop_find_missing_returns_only_absent_digests(
        present in distinct_seeds_strategy(64),
        queried in arbitrary_seeds_strategy(MAX_FIND_MISSING_BATCH_SIZE),
    ) {
        let idx = InMemoryDedupIndex::new();
        for s in &present {
            idx.upsert_chunk(ten_a(), &dig_from_seed(*s)).unwrap();
        }
        let queried_digests: Vec<BlobDigest> =
            queried.iter().copied().map(dig_from_seed).collect();
        let out = idx.find_missing_blobs(ten_a(), &queried_digests).unwrap();
        // Each missing digest MUST NOT appear in the present set.
        let present_set: BTreeSet<u8> = present.into_iter().collect();
        for m in &out.missing {
            // Recover seed from hex (first hex byte).
            let seed_byte = u8::from_str_radix(&m.as_hex()[0..2], 16).unwrap();
            prop_assert!(
                !present_set.contains(&seed_byte),
                "missing digest {m} maps to seed {seed_byte} which IS in present set"
            );
        }
        // Every queried digest NOT in present_set MUST appear in
        // missing (with the same multiplicity as the input).
        let absent_in_input: Vec<u8> = queried
            .iter()
            .copied()
            .filter(|s| !present_set.contains(s))
            .collect();
        prop_assert_eq!(out.missing.len(), absent_in_input.len());
        prop_assert_eq!(out.queried_count, queried_digests.len());
    }

    /// CTRL-ISO-005 — Tenant B's lookup never surfaces Tenant A's
    /// chunks as PRESENT (i.e. they always appear in `missing` from
    /// B's perspective).
    #[test]
    fn prop_tenant_isolation(
        a_seeds in distinct_seeds_strategy(64),
        b_query in distinct_seeds_strategy(MAX_FIND_MISSING_BATCH_SIZE),
    ) {
        let idx = InMemoryDedupIndex::new();
        for s in &a_seeds {
            idx.upsert_chunk(ten_a(), &dig_from_seed(*s)).unwrap();
        }
        // Tenant B has uploaded NOTHING. Every query digest MUST
        // surface in `missing` regardless of whether tenant A has it.
        let queried: Vec<BlobDigest> =
            b_query.iter().copied().map(dig_from_seed).collect();
        let out = idx.find_missing_blobs(ten_b(), &queried).unwrap();
        prop_assert_eq!(out.missing.len(), queried.len());
        // The missing list MUST be input-order-preserving for B.
        for (m, q) in out.missing.iter().zip(queried.iter()) {
            prop_assert_eq!(m, q);
        }
    }

    /// Idempotency: repeated calls with same input yield identical
    /// outcome.
    #[test]
    fn prop_idempotent(
        present in distinct_seeds_strategy(64),
        queried in arbitrary_seeds_strategy(MAX_FIND_MISSING_BATCH_SIZE),
    ) {
        let idx = InMemoryDedupIndex::new();
        for s in &present {
            idx.upsert_chunk(ten_a(), &dig_from_seed(*s)).unwrap();
        }
        let q: Vec<BlobDigest> =
            queried.iter().copied().map(dig_from_seed).collect();
        let r1 = idx.find_missing_blobs(ten_a(), &q).unwrap();
        let r2 = idx.find_missing_blobs(ten_a(), &q).unwrap();
        let r3 = idx.find_missing_blobs(ten_a(), &q).unwrap();
        prop_assert_eq!(r1.clone(), r2.clone());
        prop_assert_eq!(r2, r3);
        prop_assert_eq!(r1.queried_count, q.len());
    }

    /// Dedup-on-write counters are monotone non-decreasing. For any
    /// trace of writes, the inserted+reused total equals the trace
    /// length (no double-counting / dropping).
    #[test]
    fn prop_dedup_ratio_monotone(
        outcomes in prop::collection::vec(any::<bool>(), 0..200),
    ) {
        let m = InMemoryDedupMetrics::new();
        let tenant = ten_a();
        let mut prev_inserted: u64 = 0;
        let mut prev_reused: u64 = 0;
        for is_inserted in &outcomes {
            let outcome = ChunkWriteOutcome::from_inserted_bool(*is_inserted);
            record_chunk_write(&m, tenant, outcome).unwrap();
            let cur_inserted = m.counter_total(DedupMetricKind::ChunksInserted);
            let cur_reused = m.counter_total(DedupMetricKind::ChunksReused);
            // Monotone non-decreasing.
            prop_assert!(cur_inserted >= prev_inserted);
            prop_assert!(cur_reused >= prev_reused);
            prev_inserted = cur_inserted;
            prev_reused = cur_reused;
        }
        // Total must equal trace length.
        let total = prev_inserted.saturating_add(prev_reused);
        prop_assert_eq!(total as usize, outcomes.len());
        // If there is at least one Reused outcome, the dedup ratio is
        // bounded `(0, 1]`; if zero, it is exactly 0.
        let reused_count = outcomes.iter().filter(|b| !**b).count() as u64;
        if total > 0 {
            let ratio = m.dedup_ratio().unwrap();
            prop_assert!((0.0..=1.0).contains(&ratio));
            // Ratio EQUALS reused_count / total (within f64 epsilon).
            let expected =
                (reused_count as f64) / (total as f64);
            prop_assert!((ratio - expected).abs() < 1e-9);
        }
    }

    /// Output `missing` order MUST match input order (modulo
    /// presence filter).
    #[test]
    fn prop_input_order_preserved(
        seeds in arbitrary_seeds_strategy(MAX_FIND_MISSING_BATCH_SIZE),
        present_seeds in distinct_seeds_strategy(64),
    ) {
        let idx = InMemoryDedupIndex::new();
        for s in &present_seeds {
            idx.upsert_chunk(ten_a(), &dig_from_seed(*s)).unwrap();
        }
        let queried: Vec<BlobDigest> =
            seeds.iter().copied().map(dig_from_seed).collect();
        let out = idx.find_missing_blobs(ten_a(), &queried).unwrap();
        let present_set: BTreeSet<u8> = present_seeds.into_iter().collect();
        // The expected `missing` list filters out present seeds while
        // preserving input order + multiplicity.
        let expected: Vec<BlobDigest> = seeds
            .iter()
            .copied()
            .filter(|s| !present_set.contains(s))
            .map(dig_from_seed)
            .collect();
        prop_assert_eq!(out.missing, expected);
    }

    /// N copies of an absent digest in the input yield N copies in
    /// the output (server NEVER collapses client-side dedup).
    #[test]
    fn prop_duplicate_input_duplicate_output(
        seed in 0u8..=u8::MAX,
        n in 1usize..=64,
    ) {
        let idx = InMemoryDedupIndex::new();
        let d = dig_from_seed(seed);
        // Don't upsert — d is absent.
        let queried: Vec<BlobDigest> = (0..n).map(|_| d.clone()).collect();
        let out = idx.find_missing_blobs(ten_a(), &queried).unwrap();
        prop_assert_eq!(out.missing.len(), n);
        for m in &out.missing {
            prop_assert_eq!(m, &d);
        }
        prop_assert_eq!(out.queried_count, n);
    }

    /// Soft-delete forces re-upload: a soft-deleted digest surfaces
    /// in `missing` for the SAME tenant that owns it. (No grace window
    /// short-circuit.)
    #[test]
    fn prop_soft_delete_surfaces_as_missing(
        seeds in distinct_seeds_strategy(MAX_FIND_MISSING_BATCH_SIZE),
    ) {
        let idx = InMemoryDedupIndex::new();
        for s in &seeds {
            idx.upsert_chunk(ten_a(), &dig_from_seed(*s)).unwrap();
        }
        // Soft-delete EVERY row.
        for s in &seeds {
            idx.soft_delete(ten_a(), &dig_from_seed(*s), 1).unwrap();
        }
        let queried: Vec<BlobDigest> =
            seeds.iter().copied().map(dig_from_seed).collect();
        let out = idx.find_missing_blobs(ten_a(), &queried).unwrap();
        prop_assert_eq!(out.missing.len(), queried.len());
    }

    /// Re-upload after soft-delete UNDELETES the row (refcount
    /// increments + `deleted_at` clears). The next `find_missing`
    /// reports the digest as PRESENT.
    #[test]
    fn prop_re_upload_undeletes(
        seeds in distinct_seeds_strategy(64),
    ) {
        let idx = InMemoryDedupIndex::new();
        for s in &seeds {
            idx.upsert_chunk(ten_a(), &dig_from_seed(*s)).unwrap();
            idx.soft_delete(ten_a(), &dig_from_seed(*s), 1).unwrap();
            idx.upsert_chunk(ten_a(), &dig_from_seed(*s)).unwrap();
        }
        let queried: Vec<BlobDigest> =
            seeds.iter().copied().map(dig_from_seed).collect();
        let out = idx.find_missing_blobs(ten_a(), &queried).unwrap();
        prop_assert!(out.missing.is_empty());
    }

    /// Batch above the max ceiling rejects with `BatchTooLarge`.
    #[test]
    fn prop_batch_above_max_rejected(
        n in (MAX_FIND_MISSING_BATCH_SIZE + 1)..=(MAX_FIND_MISSING_BATCH_SIZE + 5),
    ) {
        let idx = InMemoryDedupIndex::new();
        // Build a batch of size n via repeated digest reuse (the
        // size check fires BEFORE any per-digest work).
        let d = dig_from_seed(1);
        let queried: Vec<BlobDigest> = (0..n).map(|_| d.clone()).collect();
        let err = idx.find_missing_blobs(ten_a(), &queried).unwrap_err();
        match err {
            corelink_cas::dedup::DedupError::BatchTooLarge { got, limit } => {
                prop_assert_eq!(got, n);
                prop_assert_eq!(limit, MAX_FIND_MISSING_BATCH_SIZE);
            }
            other => prop_assert!(false, "expected BatchTooLarge, got {other:?}"),
        }
    }
}
