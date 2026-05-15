//! Property tests pinning INV-ADMIN-DUAL-APPROVAL across the dry-run
//! harness pre-flight predicate (WI-PROPTEST-FU-002 — DEBT-009).
//!
//! Closes the proptest-density gap identified in
//! `specs/_audits/2026-05-15-proptest-density.md` (ratio 0/1 → 3/1).
//!
//! # Invariant coverage
//!
//! | Test                                                       | Invariant pinned                          |
//! |------------------------------------------------------------|-------------------------------------------|
//! | `prop_inv_admin_dual_approval_single_approver_rejected`    | INV-ADMIN-DUAL-APPROVAL (n=1 rejected)    |
//! | `prop_inv_admin_dual_approval_self_approval_rejected`      | INV-ADMIN-DUAL-APPROVAL (self-approval)   |
//! | `prop_inv_admin_dual_approval_distinct_approvers_required` | INV-ADMIN-DUAL-APPROVAL (a == b rejected) |
//! | `prop_inv_admin_dry_run_short_circuit_on_rejection`        | INV-ADMIN-DUAL-APPROVAL (read-only DR)    |
//!
//! Adversarial fixtures:
//! - Random `(requester, approver_a, approver_b)` permutations sampled
//!   from a 12-element actor pool — covers the case `requester ∈
//!   {approver_a, approver_b}` with non-trivial probability.
//! - Same actor in both approver slots (`approver_a == approver_b`).
//! - Single approver (n=1) and zero approvers (n=0) boundary inputs.
//! - Note on UTF-8 lookalikes (`admin` vs Cyrillic `аdmin`): the dry-run
//!   harness identifies actors by `Uuid`, NEVER by name string, so the
//!   lookalike attack vector is structurally impossible at this layer.
//!   This is itself a property: same-name-different-UUID actors MUST
//!   produce distinct preflight outcomes (covered as a unit canary).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target: panics surface as test failures by design"
)]

use corelink_admin_dry_run::{
    dual_approval_preflight, is_dry_run_short_circuit, DualApprovalPreflight,
};
use proptest::prelude::*;
use proptest::test_runner::Config;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use uuid::Uuid;

// =====================================================================
// PROPTEST_CASES runtime knob (S-07 P1-2 contract — runtime fn, NOT const).
// =====================================================================

fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_048)
}

/// Deterministic actor pool — 12 distinct UUIDs derived from a small
/// integer alphabet so the proptest can sample from them by index.
const ACTOR_POOL_SIZE: usize = 12;

fn actor(idx: usize) -> Uuid {
    // Tag the high 64 bits with a magic constant to avoid the all-zero
    // Uuid (Uuid::nil), which has semantic meaning in some downstream
    // consumers (e.g. "unset actor"). The proptest doesn't care, but
    // the canary tests below need a non-nil pool.
    Uuid::from_u64_pair(0xCDA1_F00D_DEAD_BEEF, idx as u64 + 1)
}

proptest! {
    #![proptest_config(Config { cases: proptest_cases(), .. Config::default() })]

    /// INV-ADMIN-DUAL-APPROVAL: a single approver MUST never be sufficient.
    /// Random (requester, approver) sampled from the 12-actor pool;
    /// regardless of whether they are distinct, the preflight MUST
    /// return `InsufficientApprovers { n: 1 }`.
    #[test]
    fn prop_inv_admin_dual_approval_single_approver_rejected(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let req_idx = rng.random_range(0..ACTOR_POOL_SIZE);
        let app_idx = rng.random_range(0..ACTOR_POOL_SIZE);
        let req = actor(req_idx);
        let app = actor(app_idx);

        let p = dual_approval_preflight(req, &[app]);
        prop_assert_eq!(
            p,
            DualApprovalPreflight::InsufficientApprovers { n: 1 },
            "INV-ADMIN-DUAL-APPROVAL violation: single approver accepted (req={}, app={})",
            req_idx,
            app_idx
        );
        let short = is_dry_run_short_circuit(p);
        prop_assert!(short,
            "INV-ADMIN-DUAL-APPROVAL: single-approver case did not short-circuit dry-run");
    }

    /// INV-ADMIN-DUAL-APPROVAL: an approver MUST NOT also be the requester
    /// (self-approval forbidden). Random {requester, approver_a, approver_b}
    /// permutations; the case `requester ∈ {approver_a, approver_b}` MUST
    /// be rejected with `SelfApproval`.
    #[test]
    fn prop_inv_admin_dual_approval_self_approval_rejected(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        // Pick a requester. Place the requester at one of the approver
        // slots; fill the other with a distinct random actor (so the
        // outcome is unambiguously `SelfApproval`, not `DuplicateApprovers`
        // or `Accepted`).
        let req_idx = rng.random_range(0..ACTOR_POOL_SIZE);
        // Force `other` distinct from `req` by sampling from the
        // remaining 11 slots.
        let other_idx = {
            let mut o = rng.random_range(0..(ACTOR_POOL_SIZE - 1));
            if o >= req_idx {
                o += 1;
            }
            o
        };
        let req = actor(req_idx);
        let other = actor(other_idx);
        prop_assert_ne!(req, other, "actor pool collision (impossible by construction)");

        // Place requester at slot 0 (approver_a = requester).
        let p_a = dual_approval_preflight(req, &[req, other]);
        prop_assert_eq!(
            p_a,
            DualApprovalPreflight::SelfApproval,
            "INV-ADMIN-DUAL-APPROVAL violation: self-approval accepted at slot 0"
        );

        // Place requester at slot 1 (approver_b = requester).
        let p_b = dual_approval_preflight(req, &[other, req]);
        prop_assert_eq!(
            p_b,
            DualApprovalPreflight::SelfApproval,
            "INV-ADMIN-DUAL-APPROVAL violation: self-approval accepted at slot 1"
        );
    }

    /// INV-ADMIN-DUAL-APPROVAL: the two approvers MUST be distinct.
    /// Random pairs `(approver_a, approver_b)` over the 12-actor pool;
    /// when `approver_a == approver_b` the preflight MUST reject with
    /// `DuplicateApprovers`. When they differ AND neither is the
    /// requester, the preflight MUST accept.
    #[test]
    fn prop_inv_admin_dual_approval_distinct_approvers_required(seed in any::<u64>()) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let req_idx = rng.random_range(0..ACTOR_POOL_SIZE);
        // Sample approvers from the (pool \ {requester}) set so the
        // self-approval path never fires; we isolate the
        // distinct-approvers axis.
        let pool_without_req: Vec<usize> = (0..ACTOR_POOL_SIZE)
            .filter(|&i| i != req_idx)
            .collect();
        let a_idx_pos = rng.random_range(0..pool_without_req.len());
        let b_idx_pos = rng.random_range(0..pool_without_req.len());
        let a_idx = pool_without_req[a_idx_pos];
        let b_idx = pool_without_req[b_idx_pos];

        let req = actor(req_idx);
        let a = actor(a_idx);
        let b = actor(b_idx);

        let p = dual_approval_preflight(req, &[a, b]);
        if a_idx == b_idx {
            prop_assert_eq!(
                p,
                DualApprovalPreflight::DuplicateApprovers,
                "INV-ADMIN-DUAL-APPROVAL violation: duplicate approvers accepted (a_idx=b_idx={})",
                a_idx
            );
        } else {
            prop_assert_eq!(
                p,
                DualApprovalPreflight::Accepted,
                "INV-ADMIN-DUAL-APPROVAL violation: distinct non-self approvers REJECTED (req={}, a={}, b={})",
                req_idx, a_idx, b_idx
            );
            let short = is_dry_run_short_circuit(p);
            prop_assert!(!short,
                "INV-ADMIN-DUAL-APPROVAL: accepted case incorrectly short-circuited");
        }
    }

    /// INV-ADMIN-DUAL-APPROVAL (read-only invariant): EVERY non-Accepted
    /// preflight outcome MUST short-circuit the dry-run plan. The
    /// `is_dry_run_short_circuit` predicate MUST agree with
    /// `dual_approval_preflight` exactly: `Accepted ⟺ ¬short_circuit`.
    ///
    /// Adversarial input: random integer slot counts in [0, 4] paired
    /// with random actor indices.
    #[test]
    fn prop_inv_admin_dry_run_short_circuit_on_rejection(
        seed in any::<u64>(),
        n_approvers in 0u8..=4u8,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let req_idx = rng.random_range(0..ACTOR_POOL_SIZE);
        let req = actor(req_idx);
        let approvers: Vec<Uuid> = (0..n_approvers)
            .map(|_| actor(rng.random_range(0..ACTOR_POOL_SIZE)))
            .collect();
        let p = dual_approval_preflight(req, &approvers);
        let short = is_dry_run_short_circuit(p);
        let accepted = matches!(p, DualApprovalPreflight::Accepted);

        prop_assert!(
            accepted != short,
            "INV-ADMIN-DUAL-APPROVAL: short-circuit / accepted predicate disagreement \
             (preflight={p:?}, short={short}, accepted={accepted})"
        );
    }
}

// =====================================================================
// Determinism canary — PRNG seed reproducibility.
// =====================================================================

#[test]
fn prng_seed_is_deterministic_across_invocations() {
    let mut a = ChaCha20Rng::seed_from_u64(0xADA1_F00D_DEAD_BEEF);
    let mut b = ChaCha20Rng::seed_from_u64(0xADA1_F00D_DEAD_BEEF);
    for _ in 0..128 {
        let av: u64 = a.random();
        let bv: u64 = b.random();
        assert_eq!(av, bv, "ChaCha20Rng output must be deterministic per seed");
    }
}

/// Canary: actor pool yields 12 distinct UUIDs (no collisions).
#[test]
fn actor_pool_has_no_collisions() {
    let mut seen = std::collections::HashSet::new();
    for i in 0..ACTOR_POOL_SIZE {
        let u = actor(i);
        assert!(seen.insert(u), "actor({i}) collided");
    }
    assert_eq!(seen.len(), ACTOR_POOL_SIZE);
}

/// Canary: same-name-different-UUID actors produce distinct preflight
/// outcomes (refutes the UTF-8 lookalike attack at this layer — actors
/// are identified by Uuid, never by display string).
#[test]
fn uuid_identity_independent_of_display_string() {
    let req = actor(0);
    let alice = actor(1);
    let alice_lookalike = actor(2); // same display "alice" hypothetically, distinct UUID
    let p1 = dual_approval_preflight(req, &[alice, alice_lookalike]);
    assert_eq!(p1, DualApprovalPreflight::Accepted,
        "distinct UUIDs must accept regardless of display string");

    // Same UUID twice → DuplicateApprovers, regardless of any external
    // name binding.
    let p2 = dual_approval_preflight(req, &[alice, alice]);
    assert_eq!(p2, DualApprovalPreflight::DuplicateApprovers);
}
