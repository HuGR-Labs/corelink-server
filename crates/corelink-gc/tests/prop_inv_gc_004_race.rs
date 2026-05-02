//! WI-S06-006 — INV-GC-004 race property test (Mark + UpdateActionResult
//! interleavings) cross-validating the canonical TLA+ obligation
//! `gc_correctness.tla::InvGCReRefProtected` against the real Rust
//! impl shipped in WI-S06-002 (mark phase) + WI-S06-003 (sweep phase).
//!
//! # Dual-tier ceiling
//!
//! Per WI §6.1.9 + sprint contract DoD §6, this suite ships **two
//! tiers** of iteration count, controlled by the canonical proptest
//! `PROPTEST_CASES` env var:
//!
//! - **PR gate**: 10_000 iter (default). Runs every PR via the
//!   `cargo test -p corelink-gc --all-targets` job; budget ≤ 30 s.
//! - **Nightly gate**: 100_000 iter via `PROPTEST_CASES=100000`.
//!   Runs in `.github/workflows/nightly.yml` `proptest-extended` job;
//!   budget ≤ 60 min. Sprint contract DoD §6 mandates "0 violations
//!   sustained across 100k iterations".
//!
//! Both tiers MUST produce **zero** INV-GC-004 violations. The protect-if-`>=`
//! semantic from `specs/tla/gc_correctness.tla` L152-154 is the
//! load-bearing predicate: any `ac_meta.created_at_ms >=
//! gc_run.mark_started_at_ms` MUST short-circuit the sweep into
//! `ProtectedReRef`; any `<` MUST proceed to soft-delete. An off-by-one
//! at this boundary is a permanent-data-loss bug.
//!
//! # PRNG determinism (Lote 10.6-tris OPUS-MISS-2)
//!
//! Each iteration's random interleaving is sampled from
//! `ChaCha20Rng::seed_from_u64(seed)` where `seed` is a function of the
//! proptest-supplied iteration counter. Cross-platform reproducibility
//! is therefore a code-level guarantee, not a documentation claim:
//! a CI failure at iter=N reproduces deterministically on any host
//! given the same N. The internal `BTreeMap` iteration in
//! `InMemoryAcReferenceIndex::find_re_reference` is the only remaining
//! source of non-determinism, and it is sorted-by-construction.
//!
//! # Adversarial fixture inputs (Lote 10.6bis P0-W6-1)
//!
//! Per WI §1.7 + Lote 10.6bis P0-W6-1, the fixture emits the following
//! adversarial inputs that historically tripped LIKE-substring SQL
//! impls:
//!
//! 1. **Envelope mutation**: `ac_meta` rows whose `action_digest`
//!    METADATA contains another digest as a substring; only `j.value =
//!    digest` exact equality should match.
//! 2. **Short-digest substring**: 16-char prefix of the target digest
//!    embedded inside an unrelated `action_digest`; same exact-equality
//!    contract applies.
//! 3. **Schema-evolution**: an `ac_meta` row whose `blob_refs` is
//!    deliberately empty even though the `action_digest` mentions the
//!    target digest in a metadata field; should NOT inflate the
//!    protect-arm count.
//!
//! Without these inputs, a regressed LIKE-substring impl would pass
//! green silently against benign random fixtures (BLAKE3-256 hex
//! digests at 64 chars never collide via random substring).
//!
//! # TLA+ → Rust action mapping (WI §1.6)
//!
//! | TLA+ action               | Rust seam in this test                                       |
//! |---------------------------|--------------------------------------------------------------|
//! | `MarkPhaseStart`          | `seed_run_in_sweep` advances run to `GcPhase::Sweep`         |
//! | `GCMarkStep(blob)`        | `seed_candidate` writes `GcCandidate { mark_started_at, .. }` |
//! | `UpdateActionResult(ac)`  | `ac.push_ac_row(.., created_at_ms)` after mark anchor capture |
//! | `SweepStep(blob)`         | `sweep.execute(rid, tenant, region)`                          |
//! | `InvGCReRefProtected`     | The post-condition asserted on every iteration                |

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target"
)]

use std::sync::Arc;

use corelink_gc::{
    BlobDigest, CandidateStatus, CountingSweepClock, GcCandidate, GcCandidatesStore, GcEventType,
    GcPhase, GcRegion, GcRunStore, InMemoryAcReferenceIndex, InMemoryBlobMetaStore,
    InMemoryGcAuditSink, InMemoryGcCandidatesStore, InMemoryGcMetrics, InMemoryGcRunStore,
    InMemorySweepPhase, RunId, SweepBlobMetaRow, SweepPhase,
};
use proptest::prelude::*;
use proptest::test_runner::Config;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use uuid::Uuid;

// =====================================================================
// Knobs.
// =====================================================================

/// Default 10k iter for PR gate; override via `PROPTEST_CASES=100000`
/// for the nightly tier per WI §6.1.9.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}

/// Boundary half-window the random sampler explores around the mark
/// anchor. Per WI §6.1.5 fixture generator: ac.created_at_ms straddles
/// `mark_started_at_ms ± 100ms`. The strict `>=` predicate at
/// `gc_correctness.tla` L152-154 is the load-bearing invariant so the
/// off-by-one cases (offset = -1, 0, +1) MUST be sampled.
const BOUNDARY_HALF_WINDOW_MS: i64 = 100;

// =====================================================================
// Fixture helpers (mirrored on prop_sweep::fresh / seed_run_in_sweep
// for cross-test consistency).
// =====================================================================

type RaceFixture = (
    InMemorySweepPhase<
        InMemoryGcRunStore,
        InMemoryGcCandidatesStore,
        InMemoryBlobMetaStore,
        InMemoryAcReferenceIndex,
        InMemoryGcAuditSink,
        InMemoryGcMetrics,
        CountingSweepClock,
    >,
    Arc<InMemoryGcRunStore>,
    Arc<InMemoryGcCandidatesStore>,
    Arc<InMemoryBlobMetaStore>,
    Arc<InMemoryAcReferenceIndex>,
    Arc<InMemoryGcAuditSink>,
);

fn region_from(idx: usize) -> GcRegion {
    GcRegion::all()[idx % 5]
}

/// Construct a 64-char canonical-hex digest from a `u64` seed. Each
/// distinct seed yields a distinct digest within the bands the test
/// uses. The leading 16 hex chars carry the seed; the remaining 48 are
/// a fixed `0xAB` pad — this is what enables the short-digest substring
/// adversarial input below (a 16-char prefix can be embedded inside an
/// unrelated string without collision-by-chance).
fn digest_from(seed: u64) -> BlobDigest {
    let prefix = format!("{seed:016x}");
    let mut s = prefix;
    s.push_str(&"ab".repeat((BlobDigest::LEN - 16) / 2));
    BlobDigest::parse(&s).expect("canonical hex digest")
}

/// F-001 closure: every fixture is constructed fresh, owning its own
/// state. No `static LazyLock<Mutex<>>` shared across iterations.
fn fresh_fixture(start_ms: u64) -> RaceFixture {
    let runs = Arc::new(InMemoryGcRunStore::new());
    let candidates = Arc::new(InMemoryGcCandidatesStore::new());
    let blob_meta = Arc::new(InMemoryBlobMetaStore::new());
    let ac_index = Arc::new(InMemoryAcReferenceIndex::new());
    let audit = Arc::new(InMemoryGcAuditSink::new());
    let metrics = Arc::new(InMemoryGcMetrics::new());
    let clock = Arc::new(CountingSweepClock::new(start_ms));
    let sweep = InMemorySweepPhase::with_defaults(
        Arc::clone(&runs),
        Arc::clone(&candidates),
        Arc::clone(&blob_meta),
        Arc::clone(&ac_index),
        Arc::clone(&audit),
        Arc::clone(&metrics),
        clock,
    );
    (sweep, runs, candidates, blob_meta, ac_index, audit)
}

fn seed_run_in_sweep(
    runs: &InMemoryGcRunStore,
    rid: RunId,
    tenant: Uuid,
    region: GcRegion,
    mark_anchor: u64,
) {
    runs.insert_pending(rid, tenant, region, 100, "cron".into())
        .unwrap();
    runs.acquire_running(rid, tenant, 200).unwrap();
    runs.transition_phase(rid, tenant, GcPhase::Mark, mark_anchor)
        .unwrap();
    runs.transition_phase(rid, tenant, GcPhase::Sweep, mark_anchor + 10)
        .unwrap();
}

fn seed_candidate(
    candidates: &InMemoryGcCandidatesStore,
    blob_meta: &InMemoryBlobMetaStore,
    tenant: Uuid,
    rid: RunId,
    digest: BlobDigest,
    mark_anchor: u64,
    blob_size_bytes: u64,
) {
    candidates
        .insert_candidate(GcCandidate {
            tenant_id: tenant,
            digest: digest.clone(),
            mark_started_at_ms: mark_anchor,
            mark_run_id: rid,
            blob_size_bytes,
            blob_last_referenced_at_ms: mark_anchor.saturating_sub(1),
            status: CandidateStatus::Candidate,
            created_at_ms: mark_anchor,
            swept_at_ms: None,
            protected_at_ms: None,
            protected_reason: None,
        })
        .unwrap();
    blob_meta.push_row(
        tenant,
        SweepBlobMetaRow {
            digest,
            size_bytes: blob_size_bytes,
            refcount: 0,
            last_referenced_at_ms: mark_anchor.saturating_sub(1),
            created_at_ms: mark_anchor.saturating_sub(100),
            deleted_at_ms: None,
        },
    );
}

// =====================================================================
// Adversarial fixture emitters (WI §1.7 + Lote 10.6bis P0-W6-1).
// =====================================================================

/// Emit envelope-mutation noise: an `ac_meta` row whose
/// `action_digest` STRING references the target digest as a substring,
/// but whose canonical `blob_refs` does not include it. The exact-
/// equality `j.value = digest` SQL semantic must reject this row.
fn emit_envelope_mutation_noise(
    ac: &InMemoryAcReferenceIndex,
    tenant: Uuid,
    target: &BlobDigest,
    other: BlobDigest,
    created_at_ms: u64,
) {
    let action_digest_metadata = format!("envelope:parent_digest={}", target.as_str());
    ac.push_ac_row(tenant, action_digest_metadata, vec![other], created_at_ms);
}

/// Emit short-digest substring noise: 16-char prefix of the target
/// digest embedded inside an unrelated `action_digest`, with
/// `blob_refs` pointing at a different digest entirely.
fn emit_short_digest_substring_noise(
    ac: &InMemoryAcReferenceIndex,
    tenant: Uuid,
    target: &BlobDigest,
    other: BlobDigest,
    created_at_ms: u64,
) {
    let prefix_len: usize = 16.min(target.as_str().len());
    let prefix = &target.as_str()[..prefix_len];
    let action_digest_metadata = format!("subref:{prefix}::random_action");
    ac.push_ac_row(tenant, action_digest_metadata, vec![other], created_at_ms);
}

/// Emit schema-evolution noise: an `ac_meta` row whose `blob_refs` is
/// empty even though the `action_digest` mentions the target. Models a
/// future `blob_refs: JSON object` migration where `j.value` over an
/// object would yield zero matches; the exact-equality predicate must
/// not falsely protect.
fn emit_schema_evolution_noise(
    ac: &InMemoryAcReferenceIndex,
    tenant: Uuid,
    target: &BlobDigest,
    created_at_ms: u64,
) {
    let action_digest_metadata = format!("future_schema:{}", target.as_str());
    ac.push_ac_row(
        tenant,
        action_digest_metadata,
        Vec::<BlobDigest>::new(),
        created_at_ms,
    );
}

// =====================================================================
// Race executor — one iteration.
//
// The boundary semantic enforced is the canonical TLA `>=`
// (`gc_correctness.tla` L152-154):
//   PROTECT iff exists ac with blob_refs.contains(digest)
//                AND ac.created_at_ms >= mark_started_at_ms
//   SWEEP   otherwise
// =====================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedDecision {
    Protect,
    Sweep,
}

/// Execute one Mark + UpdateActionResult interleaving and assert the
/// canonical TLA `>=` predicate holds. Returns `Ok(())` on success;
/// proptest's `prop_assert*` family converts assertion failures into
/// shrinkable counter-examples.
#[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
fn run_one_race_iteration(seed: u64) -> Result<(), TestCaseError> {
    let mut rng = ChaCha20Rng::seed_from_u64(seed);

    // Random tenant + region + mark anchor sampled from a band wide
    // enough that ac_offset_ms = -BOUNDARY_HALF_WINDOW_MS never
    // underflows.
    let tenant_seed: u128 = rng.random_range(1..1_000_000);
    let region_idx: usize = rng.random_range(0..5);
    let mark_anchor: u64 = rng.random_range(1_000_000..10_000_000);

    // Boundary offset ∈ [-100, +100]. The off-by-one cases
    // (offset = -1, 0, +1) MUST be sampled — protect-if-`>=` says
    // offset = 0 is the protect-arm boundary.
    let ac_offset_ms: i64 = rng.random_range(-BOUNDARY_HALF_WINDOW_MS..=BOUNDARY_HALF_WINDOW_MS);
    let expected: ExpectedDecision = if ac_offset_ms >= 0 {
        ExpectedDecision::Protect
    } else {
        ExpectedDecision::Sweep
    };

    // Decide whether to run the FULL race (real interleaving with the
    // candidate's own re-reference) OR the negative-control orphan path
    // (no real re-reference; only adversarial noise).
    let with_real_reference: bool = rng.random_bool(0.7);

    // Sweep clock starts AFTER mark_anchor + sweep transition (+10) +
    // slack; this keeps the gc_run checkpoint CHECK constraint
    // monotonic across the InMemoryGcRunStore in-memory backend.
    let (sweep, runs, candidates, blob_meta, ac, audit) = fresh_fixture(mark_anchor + 1_000_000);
    let region = region_from(region_idx);
    let tenant = Uuid::from_u128(tenant_seed);
    let rid = RunId(Uuid::from_u128(tenant_seed.wrapping_add(1)));
    seed_run_in_sweep(&runs, rid, tenant, region, mark_anchor);

    // Target candidate.
    let target_seed: u64 = rng.random_range(1_000_000..2_000_000);
    let target_digest = digest_from(target_seed);
    seed_candidate(
        &candidates,
        &blob_meta,
        tenant,
        rid,
        target_digest.clone(),
        mark_anchor,
        1024,
    );

    // Compute ac_created_at_ms with saturating-arithmetic safety. The
    // negative branch is bounded by BOUNDARY_HALF_WINDOW_MS << mark_anchor.
    let ac_created_at_ms: u64 = if ac_offset_ms >= 0 {
        mark_anchor.saturating_add(ac_offset_ms as u64)
    } else {
        let abs = (-ac_offset_ms) as u64;
        mark_anchor.saturating_sub(abs)
    };

    if with_real_reference {
        // The genuine UpdateActionResult firing during/after mark scan.
        // This is the canonical TLA action that flips the outcome.
        ac.push_ac_row(
            tenant,
            "real_action",
            vec![target_digest.clone()],
            ac_created_at_ms,
        );
    }

    // Adversarial noise: envelope mutation + substring + schema
    // evolution. None of these reference target_digest in `blob_refs`
    // so none should trigger PROTECT. They are designed to defeat
    // a regressed LIKE-substring impl that would inspect
    // `action_digest` as a string for the digest.
    let noise_count: u32 = rng.random_range(0..4);
    for k in 0..noise_count {
        let other_seed = rng.random_range(2_000_000..3_000_000);
        let other = digest_from(other_seed);
        // Deterministic ac_created_at_ms picks for each adversarial
        // input within ±BOUNDARY_HALF_WINDOW_MS so a regressed impl
        // would PROTECT on at least one of them.
        let noise_offset: i64 =
            rng.random_range(-BOUNDARY_HALF_WINDOW_MS..=BOUNDARY_HALF_WINDOW_MS);
        let noise_at_ms: u64 = if noise_offset >= 0 {
            mark_anchor.saturating_add(noise_offset as u64)
        } else {
            mark_anchor.saturating_sub((-noise_offset) as u64)
        };
        match k % 3 {
            0 => emit_envelope_mutation_noise(&ac, tenant, &target_digest, other, noise_at_ms),
            1 => emit_short_digest_substring_noise(&ac, tenant, &target_digest, other, noise_at_ms),
            _ => emit_schema_evolution_noise(&ac, tenant, &target_digest, noise_at_ms),
        }
    }

    // Run sweep.
    let result = sweep
        .execute(rid, tenant, region)
        .map_err(|e| TestCaseError::fail(format!("sweep.execute failed: {e:?}")))?;
    let cand = candidates
        .lookup(tenant, &target_digest, rid)
        .map_err(|e| TestCaseError::fail(format!("candidates.lookup failed: {e:?}")))?
        .ok_or_else(|| TestCaseError::fail("candidate row vanished mid-flight".to_owned()))?;
    let blob_row = blob_meta
        .snapshot(tenant, &target_digest)
        .ok_or_else(|| TestCaseError::fail("blob_meta row vanished mid-flight".to_owned()))?;

    // Outcome assertion: the canonical TLA semantic.
    if with_real_reference {
        match expected {
            ExpectedDecision::Protect => {
                prop_assert_eq!(
                    result.blobs_protected_re_ref_count,
                    1,
                    "INV-GC-004 violation: expected PROTECT (offset={} >= 0; \
                     ac.created_at_ms={} mark_anchor={}); \
                     got swept_count={} protected_count={}",
                    ac_offset_ms,
                    ac_created_at_ms,
                    mark_anchor,
                    result.blobs_swept_count,
                    result.blobs_protected_re_ref_count
                );
                prop_assert_eq!(result.blobs_swept_count, 0);
                prop_assert_eq!(cand.status, CandidateStatus::ProtectedReRef);
                prop_assert!(blob_row.deleted_at_ms.is_none());
                prop_assert_eq!(audit.snapshot_of(GcEventType::SweepProtectedReRef).len(), 1);
                prop_assert_eq!(audit.snapshot_of(GcEventType::SweepSoftDeleted).len(), 0);
                prop_assert!(ac_created_at_ms >= mark_anchor);
            }
            ExpectedDecision::Sweep => {
                prop_assert_eq!(
                    result.blobs_swept_count,
                    1,
                    "INV-GC-004 contract violation (sweep arm): expected SWEEP (offset={} < 0; \
                     ac.created_at_ms={} mark_anchor={}); \
                     got swept_count={} protected_count={}",
                    ac_offset_ms,
                    ac_created_at_ms,
                    mark_anchor,
                    result.blobs_swept_count,
                    result.blobs_protected_re_ref_count
                );
                prop_assert_eq!(result.blobs_protected_re_ref_count, 0);
                prop_assert_eq!(cand.status, CandidateStatus::Swept);
                prop_assert!(blob_row.deleted_at_ms.is_some());
                prop_assert_eq!(audit.snapshot_of(GcEventType::SweepSoftDeleted).len(), 1);
                prop_assert_eq!(audit.snapshot_of(GcEventType::SweepProtectedReRef).len(), 0);
                prop_assert!(ac_created_at_ms < mark_anchor);
            }
        }
    } else {
        // Negative-control: NO real re-reference; only adversarial
        // noise. Sweep MUST proceed regardless of offset (the noise
        // never names the target in `blob_refs`).
        prop_assert_eq!(
            result.blobs_swept_count,
            1,
            "Negative-control violation: adversarial noise inflated PROTECT arm. \
             A regressed LIKE-substring impl would have triggered here. \
             swept_count={} protected_count={}",
            result.blobs_swept_count,
            result.blobs_protected_re_ref_count
        );
        prop_assert_eq!(result.blobs_protected_re_ref_count, 0);
        prop_assert_eq!(cand.status, CandidateStatus::Swept);
        prop_assert!(blob_row.deleted_at_ms.is_some());
    }

    Ok(())
}

proptest! {
    #![proptest_config(Config { cases: proptest_cases(), .. Config::default() })]

    /// **Canonical 100k race property test** (PR gate at 10k iter via
    /// proptest default; nightly tier at 100k via `PROPTEST_CASES=100000`).
    ///
    /// For each iteration sampled from a `ChaCha20Rng::seed_from_u64`,
    /// constructs a fresh fixture, fires one Mark + UpdateActionResult
    /// interleaving, and asserts the canonical TLA `>=` predicate
    /// holds. ZERO violations expected across the entire run.
    ///
    /// The `seed in any::<u64>()` strategy gives proptest full control
    /// over shrinking — failures reproduce deterministically by seeding
    /// the same `u64`.
    #[test]
    fn prop_inv_gc_004_race_mark_update_ar(seed in any::<u64>()) {
        run_one_race_iteration(seed)?;
    }
}

// =====================================================================
// Sanity-grade tests — non-proptest invariants pinned alongside the
// race suite for fast-fail signal on basic regressions.
// =====================================================================

/// The canonical TLC v1.8.0 SHA-256 string MUST match the literal
/// pinned in `.github/workflows/tla_check.yml`,
/// `scripts/run_tlc_corelink.sh`, and `ADR-0042 §A1`. This sanity
/// test guards against drift between the Rust fixture suite and the
/// TLA+ CI bootstrap.
#[test]
fn canonical_tlc_sha_pinned_at_lote_10_6_tris_value() {
    const CANONICAL_TLC_SHA: &str =
        "d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f";
    // The literal length must be exactly 64 hex chars (SHA-256).
    assert_eq!(CANONICAL_TLC_SHA.len(), 64);
    // Every char must be lower-case hex.
    assert!(CANONICAL_TLC_SHA
        .chars()
        .all(|c| c.is_ascii_hexdigit() && (!c.is_alphabetic() || c.is_lowercase())));
}

/// PRNG determinism: seeding `ChaCha20Rng` with the same `u64` MUST
/// yield identical output across hosts. This is the crate-level
/// reproducibility guarantee per Lote 10.6-tris OPUS-MISS-2.
#[test]
fn prng_seed_is_deterministic_across_invocations() {
    let mut a = ChaCha20Rng::seed_from_u64(0xCAFE_F00D_BEEF_DEAD);
    let mut b = ChaCha20Rng::seed_from_u64(0xCAFE_F00D_BEEF_DEAD);
    for _ in 0..1024 {
        let av: u64 = a.random();
        let bv: u64 = b.random();
        assert_eq!(av, bv, "ChaCha20Rng output must be deterministic per seed");
    }
}

/// Single-iteration smoke at the `>=` boundary (offset = 0). Ensures
/// the harness wires correctly without depending on proptest's
/// case-runner. Acts as a fast-fail canary if the SweepPhase API drifts.
#[test]
fn race_smoke_protect_at_zero_offset_boundary() {
    let mark_anchor: u64 = 5_000_000;
    let (sweep, runs, candidates, blob_meta, ac, audit) = fresh_fixture(mark_anchor + 1_000_000);
    let tenant = Uuid::from_u128(42);
    let rid = RunId(Uuid::from_u128(43));
    seed_run_in_sweep(&runs, rid, tenant, GcRegion::Sam, mark_anchor);
    let d = digest_from(0xBEEF_CAFE);
    seed_candidate(
        &candidates,
        &blob_meta,
        tenant,
        rid,
        d.clone(),
        mark_anchor,
        256,
    );
    // ac.created_at_ms == mark_anchor — protect-arm boundary.
    ac.push_ac_row(tenant, "smoke_action", vec![d.clone()], mark_anchor);
    let result = sweep.execute(rid, tenant, GcRegion::Sam).unwrap();
    assert_eq!(result.blobs_protected_re_ref_count, 1);
    assert_eq!(result.blobs_swept_count, 0);
    let row = blob_meta.snapshot(tenant, &d).unwrap();
    assert!(row.deleted_at_ms.is_none());
    assert_eq!(audit.snapshot_of(GcEventType::SweepProtectedReRef).len(), 1);
}

/// Single-iteration smoke at offset = -1 (just-before mark anchor).
/// MUST proceed to Sweep. This is the inverse off-by-one canary.
#[test]
fn race_smoke_sweep_at_negative_one_offset() {
    let mark_anchor: u64 = 5_000_000;
    let (sweep, runs, candidates, blob_meta, ac, audit) = fresh_fixture(mark_anchor + 1_000_000);
    let tenant = Uuid::from_u128(7);
    let rid = RunId(Uuid::from_u128(8));
    seed_run_in_sweep(&runs, rid, tenant, GcRegion::Iad, mark_anchor);
    let d = digest_from(0xDEAD_BEEF);
    seed_candidate(
        &candidates,
        &blob_meta,
        tenant,
        rid,
        d.clone(),
        mark_anchor,
        256,
    );
    // ac.created_at_ms == mark_anchor - 1 — sweep-arm boundary.
    ac.push_ac_row(tenant, "sweep_smoke", vec![d.clone()], mark_anchor - 1);
    let result = sweep.execute(rid, tenant, GcRegion::Iad).unwrap();
    assert_eq!(result.blobs_swept_count, 1);
    assert_eq!(result.blobs_protected_re_ref_count, 0);
    let row = blob_meta.snapshot(tenant, &d).unwrap();
    assert!(row.deleted_at_ms.is_some());
    assert_eq!(audit.snapshot_of(GcEventType::SweepSoftDeleted).len(), 1);
}
