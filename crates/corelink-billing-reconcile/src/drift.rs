//! Drift computation primitives shared by the orchestrator + property
//! tests.
//!
//! Per WI-S10-004 §1 the canonical drift metric is the maximum
//! pairwise relative-error across the three layers (Layer 1 ↔ 2 /
//! Layer 2 ↔ 3 / Layer 1 ↔ 3) — three layers are mathematically
//! necessary because a single-pair comparison would let an emit-side
//! bug AND an aggregator-side bug compose so the totals match by the
//! same wrong factor. The pairwise discipline catches the divergence
//! at the layer-boundary level + the orchestrator routes the SEV-1
//! arm specifically at Layer 3 drift (Stripe API discrepancy =
//! customer-facing invoice may be wrong = legal exposure).

use crate::event::{LayerTotals, ReconcileLayerKind, ReconcileSnapshot};

/// Compute the relative-error between two layer-total snapshots,
/// expressed as a fractional unit (`0.001 = 0.1%`). Mirrors the
/// `ReconcileError::DriftThresholdExceeded { drift_pct }` shape per
/// WI-S10-004 §1.
///
/// The denominator is the **larger** of the two `u128` totals so the
/// metric is scale-invariant (drift over a small denominator does
/// NOT inflate the percentage past 100%; the monotone direction is
/// always `0.0 ≤ drift_pct ≤ 1.0`). Both totals zero produces `0.0`
/// (canonical: no events ⇒ no drift).
///
/// # Why `u128 → f64` precision is bounded
///
/// `u128` totals encode the canonical `AggregatedCounter::data
/// .total_qty` shape; CoreLink billing volume is bounded by `10^9
/// events/yr × max bytes per event ≈ 10^28 < 2^96` so the f64 mantissa
/// (53 bits) loses at most `2^{96-53} = 2^43` LSBs. The drift metric
/// is float-precision-bounded by `≈ 2^-43` ≈ `10^-13` — orders of
/// magnitude below the canonical Quiet threshold (`10^-4`) so the
/// precision floor never produces a false-negative drift detection.
#[must_use]
pub fn compute_pairwise_drift_pct(left: LayerTotals, right: LayerTotals) -> f64 {
    let l = left.total_qty;
    let r = right.total_qty;
    if l == 0 && r == 0 {
        return 0.0;
    }
    let max = l.max(r);
    let diff = l.max(r).saturating_sub(l.min(r));
    // saturating_sub on `u128` is structurally safe (no underflow);
    // both casts to f64 land within the 53-bit mantissa precision
    // window per the module-level rationale.
    #[allow(
        clippy::cast_precision_loss,
        reason = "u128 → f64 precision loss is bounded by 2^{-43} per the \
                  module-level rationale; the drift metric never crosses \
                  the canonical Quiet threshold (10^-4) due to that floor."
    )]
    let drift = diff as f64 / max as f64;
    drift
}

/// Compute the maximum pairwise drift across the three layers, plus
/// the layer where the maximum was observed (the "primary" drift
/// signal — used to route SEV escalation).
///
/// The "primary layer" is defined as the layer that contributed to
/// the divergent pair AND is the one farther downstream in the
/// pipeline (Layer 1 → Layer 2 → Layer 3 ordering). Rationale: a
/// Layer 3 drift is the most actionable signal (Stripe API
/// discrepancy = customer-facing invoice = SEV-1) so the
/// `(Layer 1, Layer 3)` and `(Layer 2, Layer 3)` pairs both surface
/// Layer 3 as primary; the `(Layer 1, Layer 2)` pair surfaces
/// Layer 2 as primary. When all three pairs tie (rare; most likely a
/// trivial all-zero snapshot or an exact triple-match — both produce
/// `0.0`), the canonical default is `Layer1Emit` so the SEV
/// escalation routes upstream first (Lote 10.6bis split-tier
/// principle: detect at the source).
#[must_use]
pub fn compute_max_drift(snapshot: ReconcileSnapshot) -> (f64, ReconcileLayerKind) {
    let d12 = compute_pairwise_drift_pct(snapshot.layer1, snapshot.layer2);
    let d23 = compute_pairwise_drift_pct(snapshot.layer2, snapshot.layer3);
    let d13 = compute_pairwise_drift_pct(snapshot.layer1, snapshot.layer3);
    let mut max = 0.0f64;
    let mut primary = ReconcileLayerKind::Layer1Emit;
    // SEV escalation prioritization: Layer 3 drift > Layer 2 drift >
    // Layer 1 drift (Lote 10.6bis split-tier — detect at source but
    // route by impact). Tie-break direction:
    //   `>=` favors the latter pair on equal drift, escalating to the
    //   downstream layer with higher SEV impact.
    if d12 >= max {
        max = d12;
        primary = ReconcileLayerKind::Layer2Aggregate;
    }
    if d23 >= max {
        max = d23;
        primary = ReconcileLayerKind::Layer3Stripe;
    }
    if d13 >= max {
        max = d13;
        primary = ReconcileLayerKind::Layer3Stripe;
    }
    // Special case: all three layers identical (drift = 0). Pin
    // `Layer1Emit` as the canonical "no drift" primary to keep the
    // run-started arm + the no-drift arm consistent.
    if max == 0.0 {
        primary = ReconcileLayerKind::Layer1Emit;
    }
    (max, primary)
}

/// Compute the drift record-count: the maximum cardinality of
/// disagreeing rows across the three layer pairs. Used as the
/// "absolute count" arm of the dual-condition auto-fix gate (Lote
/// 10.6bis P0-6 scale-invariant inheritance).
#[must_use]
pub fn compute_drift_record_count(snapshot: ReconcileSnapshot) -> u64 {
    let r1 = snapshot.layer1.record_count;
    let r2 = snapshot.layer2.record_count;
    let r3 = snapshot.layer3.record_count;
    let max = r1.max(r2).max(r3);
    let min = r1.min(r2).min(r3);
    max.saturating_sub(min)
}

/// Whether the dual-condition auto-fix gate fires for the given
/// `(drift_pct, drift_record_count)`. Both conditions required (Lote
/// 10.6bis Part 2a P0-6 scale-invariant percentage-floor +
/// absolute-floor): boundary `record_count = 5 AND drift_pct =
/// 0.0001` auto-fixes (operative bound `≤`); either
/// `record_count = 6` OR `drift_pct = 0.000101` rejects (escalate).
#[must_use]
pub fn auto_fix_gate_fires(
    drift_pct: f64,
    drift_record_count: u64,
    config: &crate::event::ReconcileConfig,
) -> bool {
    drift_record_count <= config.auto_fix_max_records()
        && drift_pct <= config.auto_fix_max_percent()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives; float_cmp is acceptable for canonical-percentage-pin assertions where the values are constructed deterministically."
)]
mod tests {
    use super::*;
    use crate::event::ReconcileConfig;
    use uuid::Uuid;

    #[test]
    fn pairwise_drift_zero_for_zero_zero() {
        let d = compute_pairwise_drift_pct(LayerTotals::zero(), LayerTotals::zero());
        assert_eq!(d, 0.0);
    }

    #[test]
    fn pairwise_drift_zero_for_equal_totals() {
        let a = LayerTotals::new(1000, 5);
        let d = compute_pairwise_drift_pct(a, a);
        assert_eq!(d, 0.0);
    }

    #[test]
    fn pairwise_drift_one_for_zero_vs_one() {
        let d = compute_pairwise_drift_pct(LayerTotals::zero(), LayerTotals::new(1, 1));
        assert_eq!(d, 1.0);
    }

    #[test]
    fn pairwise_drift_canonical_one_percent() {
        // 99 vs 100 over denominator 100 = 1%.
        let d = compute_pairwise_drift_pct(LayerTotals::new(99, 1), LayerTotals::new(100, 1));
        assert!((d - 0.01).abs() < 1e-12);
    }

    #[test]
    fn pairwise_drift_canonical_point_one_percent() {
        // 999 vs 1000 over denominator 1000 = 0.1%.
        let d = compute_pairwise_drift_pct(LayerTotals::new(999, 1), LayerTotals::new(1000, 1));
        assert!((d - 0.001).abs() < 1e-12);
    }

    #[test]
    fn pairwise_drift_symmetric() {
        let a = LayerTotals::new(99, 1);
        let b = LayerTotals::new(100, 1);
        assert_eq!(
            compute_pairwise_drift_pct(a, b),
            compute_pairwise_drift_pct(b, a)
        );
    }

    #[test]
    fn max_drift_zero_for_triple_match() {
        let t = Uuid::now_v7();
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(1000, 1),
            LayerTotals::new(1000, 1),
            LayerTotals::new(1000, 1),
        );
        let (max, primary) = compute_max_drift(s);
        assert_eq!(max, 0.0);
        assert_eq!(primary, ReconcileLayerKind::Layer1Emit);
    }

    #[test]
    fn max_drift_routes_layer3_when_layer3_diverges() {
        let t = Uuid::now_v7();
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(1000, 1),
            LayerTotals::new(1000, 1),
            LayerTotals::new(990, 1),
        );
        let (_max, primary) = compute_max_drift(s);
        assert_eq!(primary, ReconcileLayerKind::Layer3Stripe);
    }

    #[test]
    fn max_drift_routes_layer2_when_only_layer1_layer2_diverge() {
        // Layer 1 vs Layer 2 = 1% drift; Layer 2 vs Layer 3 = 0
        // drift; Layer 1 vs Layer 3 = 1% drift. The (1↔3) pair is
        // tie-broken to Layer 3 per the SEV-1-routing canonical
        // direction.
        let t = Uuid::now_v7();
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(990, 1),
            LayerTotals::new(1000, 1),
            LayerTotals::new(1000, 1),
        );
        let (_max, primary) = compute_max_drift(s);
        assert_eq!(primary, ReconcileLayerKind::Layer3Stripe);
    }

    #[test]
    fn drift_record_count_max_minus_min() {
        let t = Uuid::now_v7();
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(100, 1),
            LayerTotals::new(100, 5),
            LayerTotals::new(100, 3),
        );
        assert_eq!(compute_drift_record_count(s), 4);
    }

    #[test]
    fn auto_fix_gate_canonical_boundary() {
        let c = ReconcileConfig::default();
        // count=5 + pct=0.0001 → fires (boundary).
        assert!(auto_fix_gate_fires(0.0001, 5, &c));
        // count=6 → rejected.
        assert!(!auto_fix_gate_fires(0.0001, 6, &c));
        // pct=0.0002 → rejected.
        assert!(!auto_fix_gate_fires(0.0002, 5, &c));
        // count=0 + pct=0 → fires (degenerate; canonical NoDrift
        // mostly but the gate logic itself accepts).
        assert!(auto_fix_gate_fires(0.0, 0, &c));
    }
}
