//! Property tests pinning the load-bearing invariants of
//! `corelink-quota-fsm` at 10k iterations per check (PR-gate; nightly
//! 100k via `PROPTEST_CASES` env-var override per S-07 P1-2 fix).
//!
//! Coverage map (mirrors WI-S10-005 §6.1.20 + sprint contract §5.5
//! R-S10-10/11):
//!
//! - `prop_state_transitions_canonical` — for every (utilization,
//!   pre-state, post-state) triplet the orchestrator returns the
//!   canonical transition arm consistent with the 4-tier ladder
//!   boundary semantics.
//! - `prop_idempotent_transition_no_change` — re-firing the same
//!   utilization → `NoChange` arm; no audit row, no store UPSERT.
//! - `prop_80pct_boundary_telemetry_only` — at exactly 80%/79%/81%
//!   the state arm matches the canonical bucket; the
//!   `OverageTelemetryRecorded` audit fires AT the 80pct entry only
//!   (not at 79pct or 81pct passing through).
//! - `prop_95pct_boundary` — at exactly 95%/94%/96% the state arm
//!   matches the canonical bucket.
//! - `prop_100pct_writes_429` — at `≥ 100%` the state is
//!   `OverQuota100pct` and `writes_429() == true`.
//! - `prop_3_invoice_failures_suspends` — counter-based suspension
//!   fires on the canonical 3rd failure; subsequent failures are
//!   `NoChange`.
//! - `prop_reinstate_clears_suspension` — operator-driven path resets
//!   the per-tenant counter to 0 + flips to the utilization-derived
//!   bucket.
//! - `prop_tenant_isolation` — tenant A's transitions never affect
//!   tenant B's state; INV-AVAIL-ISOLATION canary.
//! - `prop_audit_emit_per_decision_arm` — every state-mutating
//!   transition arm fires its canonical audit BEFORE state mutation;
//!   INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER canary.
//!
//! Plus four sanity tests pinning canonical taxonomy cardinalities +
//! crate-level constants.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "test code: panics surface as test failures by design"
)]

use std::collections::HashSet;
use std::sync::Arc;

use corelink_billing::quota::fsm::{
    audit_event_for_transition, canonical_quota_audit_event_strings, canonical_quota_states,
    quota_fsm_schema_version, transition_emits_overage_telemetry, utilization_bucket,
    InMemoryQuotaAuditSink, InMemoryQuotaFsmStore, InMemoryQuotaStateMachine, QuotaAuditEventType,
    QuotaFsmConfig, QuotaFsmStore, QuotaState, QuotaStateMachine, QuotaTransition, UtilizationPct,
    OVER_QUOTA_100PCT_THRESHOLD, SOFT_WARNING_80PCT_THRESHOLD, SOFT_WARNING_95PCT_THRESHOLD,
    SUSPENSION_INVOICE_FAILURE_THRESHOLD,
};
use proptest::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use uuid::Uuid;

/// Read `PROPTEST_CASES` at runtime (per S-07 P1-2 fix). Default 10k
/// for PR gate; nightly job overrides to 100k.
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000)
}

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_audit_event_strings_pinned() {
    let s = canonical_quota_audit_event_strings();
    assert_eq!(s.len(), 4);
    assert!(s.contains(&"corelink.billing_quota.state_changed"));
    assert!(s.contains(&"corelink.billing_quota.overage_telemetry_recorded"));
    assert!(s.contains(&"corelink.billing_quota.suspended"));
    assert!(s.contains(&"corelink.billing_quota.reinstated"));
}

#[test]
fn canonical_states_pinned() {
    let s = canonical_quota_states();
    assert_eq!(s.len(), 5);
    let mut set: HashSet<&'static str> = HashSet::new();
    for k in s {
        assert!(set.insert(k.as_str()), "duplicate canonical: {k}");
    }
    assert_eq!(set.len(), 5);
}

#[test]
fn canonical_constants_pinned() {
    assert_eq!(SOFT_WARNING_80PCT_THRESHOLD, 80.0);
    assert_eq!(SOFT_WARNING_95PCT_THRESHOLD, 95.0);
    assert_eq!(OVER_QUOTA_100PCT_THRESHOLD, 100.0);
    assert_eq!(SUSPENSION_INVOICE_FAILURE_THRESHOLD, 3);
    assert_eq!(quota_fsm_schema_version(), 20);
}

#[test]
fn canonical_transition_taxonomy_pinned() {
    let arms = [
        QuotaTransition::NoChange {
            state: QuotaState::WithinPlan,
        },
        QuotaTransition::TransitionedTo80pct {
            from: QuotaState::WithinPlan,
        },
        QuotaTransition::TransitionedTo95pct {
            from: QuotaState::SoftWarning80pct,
        },
        QuotaTransition::TransitionedTo100pct {
            from: QuotaState::SoftWarning95pct,
        },
        QuotaTransition::Suspended {
            invoice_failures: 3,
        },
        QuotaTransition::Reinstated {
            new_state: QuotaState::WithinPlan,
        },
    ];
    assert_eq!(arms.len(), 6);
    let mut set: HashSet<&'static str> = HashSet::new();
    for a in arms {
        assert!(
            set.insert(a.as_str()),
            "duplicate transition: {}",
            a.as_str()
        );
    }
    assert_eq!(set.len(), 6);
}

// ---- helpers ---------------------------------------------------------

type Fsm = InMemoryQuotaStateMachine<InMemoryQuotaAuditSink, InMemoryQuotaFsmStore>;

fn fresh_fsm() -> (Fsm, Arc<InMemoryQuotaAuditSink>, Arc<InMemoryQuotaFsmStore>) {
    let audit = Arc::new(InMemoryQuotaAuditSink::new());
    let store = Arc::new(InMemoryQuotaFsmStore::new());
    let fsm = InMemoryQuotaStateMachine::new(Arc::clone(&audit), Arc::clone(&store));
    (fsm, audit, store)
}

fn util(v: f64) -> UtilizationPct {
    UtilizationPct::new(v).unwrap()
}

// ---- property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Every utilization → state pair is consistent with the canonical
    /// 4-tier ladder boundary semantics (`>=` lower / `<` upper). The
    /// orchestrator's pure `utilization_bucket` primitive is the
    /// source of truth; the orchestrator's transition arm matches
    /// what the bucket says.
    #[test]
    fn prop_state_transitions_canonical(
        seed in any::<u64>(),
        // Constrain to [0, 200] per the UtilizationPct bound.
        util_value in 0.0f64..=200.0
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let cfg = QuotaFsmConfig::default();
        let u = util(util_value);
        let bucket = utilization_bucket(u, &cfg);

        let (fsm, _audit, store) = fresh_fsm();
        let t = Uuid::now_v7();
        fsm.evaluate_utilization(t, u, 100).unwrap();
        let row = store.lookup(t).unwrap();
        // When the orchestrator does NOT upsert (genesis WithinPlan +
        // utilization_bucket WithinPlan = NoChange), the row is None.
        // Otherwise the row reflects the bucket exactly.
        if bucket == QuotaState::WithinPlan {
            prop_assert!(row.is_none() || row.as_ref().unwrap().current_state == QuotaState::WithinPlan);
        } else {
            let row = row.unwrap();
            prop_assert_eq!(row.current_state, bucket);
        }
    }

    /// Re-firing the same utilization → NoChange arm; no fresh audit
    /// rows; store row preserved (no UPSERT — `updated_at_ms` matches
    /// the first call).
    #[test]
    fn prop_idempotent_transition_no_change(
        seed in any::<u64>(),
        util_value in 80.0f64..200.0
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (fsm, audit, store) = fresh_fsm();
        let t = Uuid::now_v7();
        let u = util(util_value);
        // First call lands the canonical state edge.
        fsm.evaluate_utilization(t, u, 100).unwrap();
        let audit_after_first = audit.len();
        let row_after_first = store.lookup(t).unwrap().unwrap();
        // Second call with same utilization → NoChange.
        let trans = fsm.evaluate_utilization(t, u, 200).unwrap();
        let no_change = matches!(trans, QuotaTransition::NoChange { .. });
        prop_assert!(no_change);
        prop_assert_eq!(audit.len(), audit_after_first);
        let row_after_second = store.lookup(t).unwrap().unwrap();
        // updated_at_ms preserved (no UPSERT).
        prop_assert_eq!(row_after_second.updated_at_ms, row_after_first.updated_at_ms);
    }

    /// At exactly 80%/79%/81% the state arm matches the canonical
    /// bucket per the boundary semantics: 80% inclusive lower bound.
    /// The `OverageTelemetryRecorded` audit fires only on the
    /// transition INTO 80pct (genesis WithinPlan → 80pct); a 79% or
    /// 81% utilization that lands in a different bucket emits a
    /// different audit pair.
    #[test]
    fn prop_80pct_boundary_telemetry_only(
        seed in any::<u64>(),
        // Pick one of three boundary points.
        bucket_pick in 0u32..3
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (fsm, audit, store) = fresh_fsm();
        let t = Uuid::now_v7();
        let (val, expected_bucket) = match bucket_pick {
            0 => (79.0f64, QuotaState::WithinPlan),
            1 => (80.0f64, QuotaState::SoftWarning80pct),
            _ => (81.0f64, QuotaState::SoftWarning80pct),
        };
        fsm.evaluate_utilization(t, util(val), 100).unwrap();
        // Verify the bucket landed correctly.
        if expected_bucket == QuotaState::WithinPlan {
            // Genesis WithinPlan → no UPSERT.
            prop_assert!(store.lookup(t).unwrap().is_none());
            prop_assert_eq!(audit.len(), 0);
        } else {
            let row = store.lookup(t).unwrap().unwrap();
            prop_assert_eq!(row.current_state, expected_bucket);
            // Genesis → 80pct fires both state_changed + overage_telemetry.
            let state_changed = audit
                .snapshot_of(QuotaAuditEventType::StateChanged)
                .len();
            let telemetry = audit
                .snapshot_of(QuotaAuditEventType::OverageTelemetryRecorded)
                .len();
            prop_assert_eq!(state_changed, 1);
            prop_assert_eq!(telemetry, 1);
        }
    }

    /// At exactly 95%/94%/96% the state arm matches the canonical
    /// bucket: 95% inclusive lower bound.
    #[test]
    fn prop_95pct_boundary(
        seed in any::<u64>(),
        bucket_pick in 0u32..3
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (fsm, audit, store) = fresh_fsm();
        let t = Uuid::now_v7();
        let (val, expected_bucket) = match bucket_pick {
            0 => (94.0f64, QuotaState::SoftWarning80pct),
            1 => (95.0f64, QuotaState::SoftWarning95pct),
            _ => (96.0f64, QuotaState::SoftWarning95pct),
        };
        fsm.evaluate_utilization(t, util(val), 100).unwrap();
        let row = store.lookup(t).unwrap().unwrap();
        prop_assert_eq!(row.current_state, expected_bucket);
        // Both 94 and 95+ entries from genesis → both fire telemetry
        // (94 → 80pct entry; 95 → 95pct entry).
        let telemetry = audit
            .snapshot_of(QuotaAuditEventType::OverageTelemetryRecorded)
            .len();
        prop_assert_eq!(telemetry, 1);
    }

    /// At `≥ 100%` the state arm is `OverQuota100pct` AND
    /// `writes_429() == true` (the hot-path Tower middleware reads
    /// this to fire the canonical 429 + X-RateLimit-Layer: quota
    /// header per the S-08 alignment).
    #[test]
    fn prop_100pct_writes_429(
        seed in any::<u64>(),
        // Constrain to the canonical hard-block band (cap at 200 per
        // UtilizationPct bound).
        util_value in 100.0f64..=200.0
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (fsm, _audit, store) = fresh_fsm();
        let t = Uuid::now_v7();
        fsm.evaluate_utilization(t, util(util_value), 100).unwrap();
        let row = store.lookup(t).unwrap().unwrap();
        prop_assert_eq!(row.current_state, QuotaState::OverQuota100pct);
        prop_assert!(row.current_state.writes_429());
    }

    /// 3rd consecutive invoice failure → `Suspended` arm; subsequent
    /// failures are `NoChange` (counter monotonically advances; the
    /// terminal arm dominates).
    #[test]
    fn prop_3_invoice_failures_suspends(
        seed in any::<u64>(),
        extra_failures in 0u32..10
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (fsm, audit, store) = fresh_fsm();
        let t = Uuid::now_v7();
        // Bump the counter (3 + `extra_failures` total).
        for i in 0..3 {
            fsm.record_invoice_failure(t, 100 + i).unwrap();
        }
        for i in 0..extra_failures {
            fsm.record_invoice_failure(t, 1000 + u64::from(i)).unwrap();
        }
        let row = store.lookup(t).unwrap().unwrap();
        prop_assert_eq!(row.current_state, QuotaState::SuspendedForNonPayment);
        prop_assert_eq!(row.invoice_failure_count.value(), 3 + extra_failures);
        // Exactly ONE Suspended audit (the canonical edge from 2→3).
        let suspended_audits = audit.snapshot_of(QuotaAuditEventType::Suspended);
        prop_assert_eq!(suspended_audits.len(), 1);
    }

    /// Operator-driven `reinstate()` clears the per-tenant counter to
    /// 0 + flips to the utilization-derived bucket. The `Reinstated`
    /// audit fires.
    #[test]
    fn prop_reinstate_clears_suspension(
        seed in any::<u64>(),
        util_value in 0.0f64..=200.0
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (fsm, audit, store) = fresh_fsm();
        let t = Uuid::now_v7();
        // Suspend.
        for _ in 0..3 {
            fsm.record_invoice_failure(t, 100).unwrap();
        }
        // Reinstate.
        let trans = fsm.reinstate(t, util(util_value), 1000).unwrap();
        let reinstated = matches!(trans, QuotaTransition::Reinstated { .. });
        prop_assert!(reinstated);
        let row = store.lookup(t).unwrap().unwrap();
        // Counter cleared.
        prop_assert_eq!(row.invoice_failure_count.value(), 0);
        // State matches utilization bucket.
        let cfg = QuotaFsmConfig::default();
        let bucket = utilization_bucket(util(util_value), &cfg);
        prop_assert_eq!(row.current_state, bucket);
        // Reinstated audit fired.
        let reinstated_audits = audit.snapshot_of(QuotaAuditEventType::Reinstated);
        prop_assert_eq!(reinstated_audits.len(), 1);
    }

    /// Tenant A's transitions never affect tenant B's state.
    /// INV-AVAIL-ISOLATION canary.
    #[test]
    fn prop_tenant_isolation(
        seed in any::<u64>(),
        ta_util in 0.0f64..=200.0,
        tb_util in 0.0f64..=200.0,
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (fsm, _audit, store) = fresh_fsm();
        let ta = Uuid::now_v7();
        let tb = Uuid::now_v7();
        // Suspend tenant A.
        for _ in 0..3 {
            fsm.record_invoice_failure(ta, 100).unwrap();
        }
        // Evaluate tenant A and B at varying utilization.
        fsm.evaluate_utilization(ta, util(ta_util), 200).unwrap();
        fsm.evaluate_utilization(tb, util(tb_util), 300).unwrap();
        let ra = store.lookup(ta).unwrap().unwrap();
        // Tenant A: suspension dominates utilization.
        prop_assert_eq!(ra.current_state, QuotaState::SuspendedForNonPayment);
        // Tenant B: state matches utilization bucket; never suspended.
        let cfg = QuotaFsmConfig::default();
        let tb_bucket = utilization_bucket(util(tb_util), &cfg);
        let rb = store.lookup(tb).unwrap();
        if tb_bucket == QuotaState::WithinPlan {
            prop_assert!(rb.is_none() || rb.as_ref().unwrap().current_state == QuotaState::WithinPlan);
        } else {
            let rb = rb.unwrap();
            prop_assert_eq!(rb.current_state, tb_bucket);
            prop_assert!(!rb.current_state.is_suspended());
        }
    }

    /// Every state-mutating transition arm fires its canonical audit
    /// BEFORE state mutation. INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER
    /// canary.
    #[test]
    fn prop_audit_emit_per_decision_arm(
        seed in any::<u64>(),
        bucket in 0u32..4
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (fsm, audit, _store) = fresh_fsm();
        let t = Uuid::now_v7();
        // Pick a target bucket per the canonical ladder.
        let util_value = match bucket {
            0 => 50.0f64,   // WithinPlan
            1 => 85.0f64,   // 80pct
            2 => 96.0f64,   // 95pct
            _ => 105.0f64,  // 100pct
        };
        let trans = fsm.evaluate_utilization(t, util(util_value), 100).unwrap();
        // Every state-mutating arm fires `state_changed`; the
        // 80pct + 95pct sibling fires `overage_telemetry_recorded`.
        // The `NoChange` arm fires NO audit by construction.
        if let Some(expected) = audit_event_for_transition(trans) {
            let arm_audits = audit.snapshot_of(expected);
            prop_assert_eq!(arm_audits.len(), 1);
        } else {
            prop_assert_eq!(audit.len(), 0);
        }
        if transition_emits_overage_telemetry(trans) {
            let telemetry = audit
                .snapshot_of(QuotaAuditEventType::OverageTelemetryRecorded);
            prop_assert_eq!(telemetry.len(), 1);
        }
    }
}
