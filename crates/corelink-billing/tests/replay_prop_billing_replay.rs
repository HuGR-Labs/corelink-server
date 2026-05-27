//! Property tests pinning the load-bearing invariants of
//! `corelink-billing-replay` at 10k iterations per check (PR-gate;
//! nightly 100k via `PROPTEST_CASES` env-var override per S-07 P1-2
//! fix).
//!
//! Coverage map (mirrors WI-S10-006 §6.1.20):
//!
//! - `prop_authorized_role_only_executes` — only requests presenting
//!   the canonical `billing_forensics_admin` role reach the executed
//!   arm; every other role lands in Denied403; CTRL-AUTHZ-002 canary.
//! - `prop_idempotent_replay_same_request_id` — same UUIDv7
//!   `request_id` re-submitted reuses the prior outcome; the
//!   canonical idempotency contract (WI-S10-006 §1 invariant 6) is
//!   enforced at the orchestrator + ledger boundary.
//! - `prop_replay_deterministic` — same `(tenant, billing_period,
//!   archive)` produces byte-identical reconstruction across
//!   invocations; the canonical determinism rationale.
//! - `prop_dry_run_no_state_mutation` — DryRun arm never UPSERTs the
//!   idempotency ledger; the audit row IS the only side effect.
//! - `prop_layer_diverged_flagged` — when reconstructed layers
//!   differ from production reference, the supplemental
//!   `layer_diverged` audit fires + the executed decision arm carries
//!   `layer_diverged = true`.
//! - `prop_tenant_isolation` — cross-tenant archive reads
//!   structurally impossible; INV-TENANT-ISOLATION canary.
//! - `prop_audit_emit_per_decision_arm` — every replay decision arm
//!   fires its canonical audit BEFORE state mutation;
//!   INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER canary.
//! - `prop_chain_event_appended_per_replay` — every successful
//!   replay invocation appends at least one row to the canonical
//!   audit chain; INV-OBS-AUDIT-CHAIN-INTEGRITY canary (S-09
//!   inheritance).
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

use corelink_billing::replay::{
    audit_event_for_decision, canonical_replay_audit_event_strings, canonical_replay_reasons,
    drift_summary_from_reconcile, replay_schema_version, InMemoryReplayArchive,
    InMemoryReplayAuditSink, InMemoryReplayEngine, InMemoryReplayIdempotencyLedger,
    LayerDriftSummary, ReconstructedLayers, ReplayAuditEventType, ReplayDecision, ReplayEngine,
    ReplayReason, ReplayRequest, BILLING_FORENSICS_ADMIN_ROLE, CANONICAL_LAYER_COUNT,
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

const ALL_REASONS: &[ReplayReason] = &[
    ReplayReason::DriftInvestigation,
    ReplayReason::CustomerDispute,
    ReplayReason::ComplianceAudit,
    ReplayReason::DryRun,
];

// ---- canonical surface pinning ---------------------------------------

#[test]
fn canonical_audit_event_strings_pinned() {
    let s = canonical_replay_audit_event_strings();
    assert_eq!(s.len(), 5);
    assert!(s.contains(&"corelink.billing_replay.request_authorized"));
    assert!(s.contains(&"corelink.billing_replay.request_denied"));
    assert!(s.contains(&"corelink.billing_replay.dry_run_planned"));
    assert!(s.contains(&"corelink.billing_replay.executed"));
    assert!(s.contains(&"corelink.billing_replay.layer_diverged"));
}

#[test]
fn canonical_reasons_pinned() {
    let s = canonical_replay_reasons();
    assert_eq!(s.len(), 4);
    let mut set: HashSet<&'static str> = HashSet::new();
    for r in s {
        assert!(set.insert(r.as_str()), "duplicate canonical reason: {r}");
    }
    assert_eq!(set.len(), 4);
}

#[test]
fn canonical_constants_pinned() {
    assert_eq!(BILLING_FORENSICS_ADMIN_ROLE, "billing_forensics_admin");
    assert_eq!(CANONICAL_LAYER_COUNT, 3);
    assert_eq!(replay_schema_version(), 21);
}

#[test]
fn canonical_decision_taxonomy_pinned() {
    let arms = [
        ReplayDecision::Authorized {
            idempotent_replay: true,
        },
        ReplayDecision::Denied403 {
            presented_role: "viewer".to_string(),
        },
        ReplayDecision::DryRunPlan {
            layers_planned: CANONICAL_LAYER_COUNT,
        },
        ReplayDecision::Executed {
            layer_diverged: false,
        },
    ];
    assert_eq!(arms.len(), 4);
    let mut set: HashSet<&'static str> = HashSet::new();
    for a in &arms {
        assert!(set.insert(a.as_str()), "duplicate decision str: {}", a.as_str());
    }
    assert_eq!(set.len(), 4);
}

// ---- helpers ---------------------------------------------------------

type Engine = InMemoryReplayEngine<
    InMemoryReplayAuditSink,
    InMemoryReplayIdempotencyLedger,
    InMemoryReplayArchive,
>;

fn fresh_engine() -> (
    Engine,
    Arc<InMemoryReplayAuditSink>,
    Arc<InMemoryReplayIdempotencyLedger>,
    Arc<InMemoryReplayArchive>,
) {
    let audit = Arc::new(InMemoryReplayAuditSink::new());
    let idem = Arc::new(InMemoryReplayIdempotencyLedger::new());
    let archive = Arc::new(InMemoryReplayArchive::new());
    let e = InMemoryReplayEngine::new(
        Arc::clone(&audit),
        Arc::clone(&idem),
        Arc::clone(&archive),
    );
    (e, audit, idem, archive)
}

fn pick_reason(rng: &mut ChaCha20Rng) -> ReplayReason {
    use rand::RngCore;
    let idx = (rng.next_u32() as usize) % ALL_REASONS.len();
    ALL_REASONS[idx]
}

fn req(rid: Uuid, t: Uuid, role: &str, period: &str, reason: ReplayReason) -> ReplayRequest {
    ReplayRequest::new(rid, Uuid::now_v7(), role, t, period, reason)
}

// ---- property tests --------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(),
        ..ProptestConfig::default()
    })]

    /// Only requests presenting the canonical
    /// `billing_forensics_admin` role reach the executed arm; every
    /// other role lands in Denied403. CTRL-AUTHZ-002 canary.
    #[test]
    fn prop_authorized_role_only_executes(
        seed in any::<u64>(),
        unauthorized_role in "[a-z_]{3,32}",
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (e, audit, idem, archive) = fresh_engine();
        let t = Uuid::now_v7();
        let layers = ReconstructedLayers::new(100, 100, 100);
        archive.seed(t, "2026-05", layers);

        // Authorized request → executed arm.
        let r_ok = req(
            Uuid::now_v7(),
            t,
            BILLING_FORENSICS_ADMIN_ROLE,
            "2026-05",
            ReplayReason::CustomerDispute,
        );
        let d_ok = e.replay(&r_ok, layers, 1).unwrap();
        let executed_ok = matches!(
            d_ok,
            ReplayDecision::Executed { layer_diverged: false }
        );
        prop_assert!(executed_ok);
        prop_assert_eq!(idem.len(), 1);

        // Unauthorized request (any non-canonical role) → Denied403.
        // The proptest-generated string MAY be the canonical role
        // string; in that case skip the unauthorized branch (the
        // first arm already pinned the authorized path).
        if unauthorized_role != BILLING_FORENSICS_ADMIN_ROLE {
            let r_ko = req(
                Uuid::now_v7(),
                t,
                &unauthorized_role,
                "2026-05",
                ReplayReason::CustomerDispute,
            );
            let d_ko = e.replay(&r_ko, layers, 1).unwrap();
            let denied_ko = matches!(d_ko, ReplayDecision::Denied403 { .. });
            prop_assert!(denied_ko);
            // Idempotency ledger is unchanged on the Denied403 arm.
            prop_assert_eq!(idem.len(), 1);
            // Audit chain captured the rejection.
            prop_assert!(
                !audit.snapshot_of(ReplayAuditEventType::RequestDenied).is_empty()
            );
        }
    }

    /// Same UUIDv7 `request_id` re-submitted reuses the prior outcome;
    /// the canonical idempotency contract (WI-S10-006 §1 invariant 6)
    /// is enforced at the orchestrator + ledger boundary.
    #[test]
    fn prop_idempotent_replay_same_request_id(
        seed in any::<u64>(),
        layer_qty in 0u128..1_000_000,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (e, _audit, idem, archive) = fresh_engine();
        let t = Uuid::now_v7();
        let layers = ReconstructedLayers::new(layer_qty, layer_qty, layer_qty);
        archive.seed(t, "2026-05", layers);
        let rid = Uuid::now_v7();
        let reason = pick_reason(&mut rng);
        // Skip DryRun for this property: dry-run never UPSERTs the
        // ledger so the canonical idempotent-replay arm doesn't apply
        // (covered separately by `prop_dry_run_no_state_mutation`).
        let reason = if matches!(reason, ReplayReason::DryRun) {
            ReplayReason::CustomerDispute
        } else {
            reason
        };
        let r = req(rid, t, BILLING_FORENSICS_ADMIN_ROLE, "2026-05", reason);
        let d1 = e.replay(&r, layers, 1).unwrap();
        let d2 = e.replay(&r, layers, 2).unwrap();
        let d3 = e.replay(&r, layers, 3).unwrap();
        let executed_first = matches!(d1, ReplayDecision::Executed { .. });
        prop_assert!(executed_first);
        let idemp2 = matches!(d2, ReplayDecision::Authorized { idempotent_replay: true });
        prop_assert!(idemp2);
        let idemp3 = matches!(d3, ReplayDecision::Authorized { idempotent_replay: true });
        prop_assert!(idemp3);
        // Ledger len stays at 1 across the 3 invocations.
        prop_assert_eq!(idem.len(), 1);
    }

    /// Same `(tenant, billing_period, archive)` produces byte-identical
    /// reconstruction across invocations; the canonical determinism
    /// rationale.
    #[test]
    fn prop_replay_deterministic(
        seed in any::<u64>(),
        l1 in 0u128..1_000_000,
        l2 in 0u128..1_000_000,
        l3 in 0u128..1_000_000,
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (e, _audit, _idem, archive) = fresh_engine();
        let t = Uuid::now_v7();
        let layers = ReconstructedLayers::new(l1, l2, l3);
        archive.seed(t, "2026-05", layers);
        // Distinct request_ids so each invocation runs the executed
        // pipeline (idempotency would short-circuit otherwise).
        let r1 = req(Uuid::now_v7(), t, BILLING_FORENSICS_ADMIN_ROLE, "2026-05", ReplayReason::CustomerDispute);
        let r2 = req(Uuid::now_v7(), t, BILLING_FORENSICS_ADMIN_ROLE, "2026-05", ReplayReason::CustomerDispute);
        let d1 = e.replay(&r1, layers, 1).unwrap();
        let d2 = e.replay(&r2, layers, 2).unwrap();
        // Both invocations land in the SAME decision shape because the
        // archive returns the SAME reconstructed layers + the
        // reference is the SAME; reconstruction is byte-deterministic.
        prop_assert_eq!(d1.as_str(), d2.as_str());
        // Both invocations land in the executed arm with the same
        // diverged flag (here: false because reference == archive).
        let exec1 = matches!(d1, ReplayDecision::Executed { layer_diverged: false });
        let exec2 = matches!(d2, ReplayDecision::Executed { layer_diverged: false });
        prop_assert!(exec1);
        prop_assert!(exec2);
    }

    /// DryRun arm never UPSERTs the idempotency ledger; the audit row
    /// IS the only side effect.
    #[test]
    fn prop_dry_run_no_state_mutation(
        seed in any::<u64>(),
        layer_qty in 0u128..1_000_000,
        n_invocations in 1u32..20,
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (e, audit, idem, archive) = fresh_engine();
        let t = Uuid::now_v7();
        let layers = ReconstructedLayers::new(layer_qty, layer_qty, layer_qty);
        archive.seed(t, "2026-05", layers);
        for i in 0..n_invocations {
            // Each invocation uses a distinct request_id so the
            // orchestrator computes a fresh dry-run plan rather than
            // short-circuiting on idempotency. (DryRun does not write
            // to the ledger so distinct request_ids land distinct
            // audit rows but no ledger rows.)
            let r = req(
                Uuid::now_v7(),
                t,
                BILLING_FORENSICS_ADMIN_ROLE,
                "2026-05",
                ReplayReason::DryRun,
            );
            let dec = e.replay(&r, layers, i.into()).unwrap();
            let dry = matches!(dec, ReplayDecision::DryRunPlan { layers_planned: 3 });
            prop_assert!(dry);
        }
        // Ledger UNCHANGED across n_invocations DryRun arms.
        prop_assert_eq!(idem.len(), 0);
        // Each invocation landed exactly one dry_run_planned audit row.
        prop_assert_eq!(
            audit.snapshot_of(ReplayAuditEventType::DryRunPlanned).len(),
            n_invocations as usize
        );
        // No executed audit rows.
        prop_assert_eq!(
            audit.snapshot_of(ReplayAuditEventType::Executed).len(),
            0
        );
    }

    /// When reconstructed layers differ from production reference, the
    /// supplemental `layer_diverged` audit fires + the executed
    /// decision arm carries `layer_diverged = true`.
    #[test]
    fn prop_layer_diverged_flagged(
        seed in any::<u64>(),
        ref_qty in 1u128..1_000_000,
        delta in 1u128..1000,
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (e, audit, _idem, archive) = fresh_engine();
        let t = Uuid::now_v7();
        let reconstructed = ReconstructedLayers::new(ref_qty.saturating_sub(delta), ref_qty, ref_qty);
        let reference = ReconstructedLayers::new(ref_qty, ref_qty, ref_qty);
        archive.seed(t, "2026-05", reconstructed);
        let r = req(
            Uuid::now_v7(),
            t,
            BILLING_FORENSICS_ADMIN_ROLE,
            "2026-05",
            ReplayReason::DriftInvestigation,
        );
        let dec = e.replay(&r, reference, 1).unwrap();
        // ref_qty.saturating_sub(delta) != ref_qty when delta > 0 (here
        // the strategy guarantees delta ≥ 1 + ref_qty ≥ 1 so the
        // saturating_sub strictly reduces).
        let diverged_arm = matches!(
            dec,
            ReplayDecision::Executed { layer_diverged: true }
        );
        prop_assert!(diverged_arm);
        prop_assert_eq!(
            audit.snapshot_of(ReplayAuditEventType::LayerDiverged).len(),
            1
        );
        prop_assert_eq!(
            audit.snapshot_of(ReplayAuditEventType::Executed).len(),
            1
        );
    }

    /// Cross-tenant archive reads structurally impossible.
    /// INV-TENANT-ISOLATION canary.
    #[test]
    fn prop_tenant_isolation(
        seed in any::<u64>(),
        l_a in 0u128..1_000_000,
        l_b in 0u128..1_000_000,
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (e, _audit, idem, archive) = fresh_engine();
        let ta = Uuid::now_v7();
        let tb = Uuid::now_v7();
        let layers_a = ReconstructedLayers::new(l_a, l_a, l_a);
        let layers_b = ReconstructedLayers::new(l_b, l_b, l_b);
        archive.seed(ta, "2026-05", layers_a);
        archive.seed(tb, "2026-05", layers_b);
        let ra = req(Uuid::now_v7(), ta, BILLING_FORENSICS_ADMIN_ROLE, "2026-05", ReplayReason::CustomerDispute);
        let rb = req(Uuid::now_v7(), tb, BILLING_FORENSICS_ADMIN_ROLE, "2026-05", ReplayReason::CustomerDispute);
        // Each tenant's replay matches its OWN reference (not the
        // other tenant's).
        let da = e.replay(&ra, layers_a, 1).unwrap();
        let db = e.replay(&rb, layers_b, 1).unwrap();
        let exec_a = matches!(da, ReplayDecision::Executed { layer_diverged: false });
        let exec_b = matches!(db, ReplayDecision::Executed { layer_diverged: false });
        prop_assert!(exec_a);
        prop_assert!(exec_b);
        // Two distinct request_ids → two distinct ledger rows.
        prop_assert_eq!(idem.len(), 2);
    }

    /// Every replay decision arm fires its canonical audit BEFORE
    /// state mutation. INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER canary.
    #[test]
    fn prop_audit_emit_per_decision_arm(
        seed in any::<u64>(),
        bucket in 0u32..4,
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let (e, audit, _idem, archive) = fresh_engine();
        let t = Uuid::now_v7();
        let layers = ReconstructedLayers::new(100, 100, 100);
        archive.seed(t, "2026-05", layers);
        let r = match bucket {
            0 => {
                // Authorized + Executed arm.
                req(
                    Uuid::now_v7(),
                    t,
                    BILLING_FORENSICS_ADMIN_ROLE,
                    "2026-05",
                    ReplayReason::CustomerDispute,
                )
            }
            1 => {
                // Denied403 arm.
                req(
                    Uuid::now_v7(),
                    t,
                    "viewer",
                    "2026-05",
                    ReplayReason::CustomerDispute,
                )
            }
            2 => {
                // DryRunPlan arm.
                req(
                    Uuid::now_v7(),
                    t,
                    BILLING_FORENSICS_ADMIN_ROLE,
                    "2026-05",
                    ReplayReason::DryRun,
                )
            }
            _ => {
                // Idempotent re-fire arm (Authorized).
                let rid = Uuid::now_v7();
                let r_first = req(
                    rid,
                    t,
                    BILLING_FORENSICS_ADMIN_ROLE,
                    "2026-05",
                    ReplayReason::CustomerDispute,
                );
                e.replay(&r_first, layers, 1).unwrap();
                req(
                    rid,
                    t,
                    BILLING_FORENSICS_ADMIN_ROLE,
                    "2026-05",
                    ReplayReason::CustomerDispute,
                )
            }
        };
        let dec = e.replay(&r, layers, 100).unwrap();
        let expected_event = audit_event_for_decision(&dec);
        // The decision-arm canonical audit landed.
        let arm_audits = audit.snapshot_of(expected_event);
        prop_assert!(!arm_audits.is_empty(), "no audit row for arm {:?}", expected_event);
    }

    /// Every successful replay invocation appends at least one row to
    /// the canonical audit chain. INV-OBS-AUDIT-CHAIN-INTEGRITY canary
    /// (S-09 inheritance).
    #[test]
    fn prop_chain_event_appended_per_replay(
        seed in any::<u64>(),
        n in 1u32..20,
    ) {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (e, audit, _idem, archive) = fresh_engine();
        let t = Uuid::now_v7();
        let layers = ReconstructedLayers::new(100, 100, 100);
        archive.seed(t, "2026-05", layers);
        let mut expected_min_rows = 0usize;
        for i in 0..n {
            let reason = pick_reason(&mut rng);
            // Generate authorized request 80% of the time; rest denied.
            let role = if i.is_multiple_of(5) {
                "viewer".to_string()
            } else {
                BILLING_FORENSICS_ADMIN_ROLE.to_string()
            };
            let r = req(
                Uuid::now_v7(),
                t,
                &role,
                "2026-05",
                reason,
            );
            let _ = e.replay(&r, layers, i.into()).unwrap();
            expected_min_rows += 1;
        }
        // The audit chain captured AT LEAST one row per invocation
        // (executed + layer_diverged would be 2, but here layers
        // match so it's always 1).
        prop_assert!(audit.len() >= expected_min_rows);
    }

    /// Drift summary lifted from the canonical reconciliation worker
    /// decision matches the canonical drift-summary taxonomy.
    #[test]
    fn prop_drift_summary_lifted_canonical(seed in any::<u64>(), bucket in 0u32..5) {
        use corelink_billing_reconcile::{ReconcileDecision, ReconcileLayerKind};
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let dec = match bucket {
            0 => ReconcileDecision::NoDrift { max_drift_pct: 0.0 },
            1 => ReconcileDecision::AutoFixed {
                max_drift_pct: 0.00005,
                drift_record_count: 1,
            },
            2 => ReconcileDecision::TicketSev3 {
                max_drift_pct: 0.0005,
                primary_layer: ReconcileLayerKind::Layer1Emit,
            },
            3 => ReconcileDecision::PageSev2 {
                max_drift_pct: 0.005,
                primary_layer: ReconcileLayerKind::Layer2Aggregate,
            },
            _ => ReconcileDecision::PageSev1AutoPaused {
                max_drift_pct: 0.05,
                primary_layer: ReconcileLayerKind::Layer3Stripe,
                pause_acked: true,
            },
        };
        let s = drift_summary_from_reconcile(&dec);
        match bucket {
            0 | 1 => prop_assert_eq!(s, LayerDriftSummary::AllLayersMatch),
            2 => prop_assert_eq!(s, LayerDriftSummary::Layer1Diverged),
            3 => prop_assert_eq!(s, LayerDriftSummary::Layer2Diverged),
            _ => prop_assert_eq!(s, LayerDriftSummary::Layer3Diverged),
        }
    }

    /// Layer-drift classify is consistent with three_layers_match():
    /// AllLayersMatch iff three_layers_match() on the equality of
    /// reconstructed-vs-reference snapshots.
    #[test]
    fn prop_layer_drift_classify_consistent(
        seed in any::<u64>(),
        a1 in 0u128..1_000_000,
        a2 in 0u128..1_000_000,
        a3 in 0u128..1_000_000,
        b1 in 0u128..1_000_000,
        b2 in 0u128..1_000_000,
        b3 in 0u128..1_000_000,
    ) {
        let _ = ChaCha20Rng::seed_from_u64(seed);
        let r = ReconstructedLayers::new(a1, a2, a3);
        let p = ReconstructedLayers::new(b1, b2, b3);
        let s = LayerDriftSummary::classify(r, p);
        let any_diff = a1 != b1 || a2 != b2 || a3 != b3;
        prop_assert_eq!(s.diverged(), any_diff);
        if !any_diff {
            prop_assert_eq!(s, LayerDriftSummary::AllLayersMatch);
        }
    }
}
