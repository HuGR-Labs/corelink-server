//! Canonical reconciliation types: [`ReconcileLayerKind`] +
//! [`ReconcileDecision`] + [`LayerTotals`] + [`ReconcileSnapshot`] +
//! [`ReconcileConfig`].
//!
//! ## Why typed enums (NOT string discriminants)
//!
//! Per Lote 10.9-quinquies NEW-P0-2 (absorbed via WI-S10-004 §1
//! invariant 6): the reconciliation reports stored in R2 Object Lock 7y
//! must be byte-deterministic across the auditor evidence trail.
//! Untyped `serde_json::Value` defeats compile-time taxonomy
//! enforcement; the typed enum + per-variant payload struct is the
//! canonical Lote 10.9-quinquies absorption pattern (mirrors WI-S10-001
//! `UsageEventKind` + WI-S10-002 `AggregationDecision` + WI-S10-003
//! `StripeAdapterDecision`).
//!
//! All public enums are `#[non_exhaustive]` so follow-on WIs can extend
//! the taxonomy additively without breaking downstream `match` sites
//! (e.g. WI-S10-005 quota state-machine drift signals; WI-S10-006
//! replay forensic feed; S-13 admin plane).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical 3-element layer-kind taxonomy. Per WI-S10-004 §1: the
/// 3 layers are mathematically necessary for full coverage —
/// single-layer reconciliation can mask a bug between layers (emit bug
/// → counter low → invoice low → both wrong by the same factor; only
/// pairwise comparison at each layer detects the divergence).
///
/// `#[non_exhaustive]` reserves additive growth for follow-on WIs (e.g.
/// a Layer 4 = Stripe-paid-invoice ↔ bank-settlement reconciliation in
/// a future enterprise SKU).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReconcileLayerKind {
    /// Layer 1: Σ raw usage events emitted by `corelink-billing-emit`
    /// (`UsageEvent::data.qty` summed across the canonical R2 NDJSON
    /// bucket layout `usage/{tenant_id}/{billing_period}/*.usage.ndjson`)
    /// vs the same Σ rolled up at the aggregator surface. Detects
    /// emit-side bugs / aggregator-side filtering bugs.
    Layer1Emit,
    /// Layer 2: Σ aggregated counters from
    /// `corelink-billing-aggregator::AggregatedCounter::data.total_qty`
    /// (chain-linked per-(tenant, billing_period, event_kind)) vs Σ
    /// the Stripe-side ledger snapshot. Detects aggregator → Stripe
    /// adapter routing bugs / canonical-bytes regressions.
    Layer2Aggregate,
    /// Layer 3: Σ Stripe usage_records from
    /// `corelink-billing-stripe::UsageRecordRequest::total_qty` (the
    /// post-API-emit ledger snapshot) vs the upstream aggregator
    /// totals. Detects Stripe API ingest discrepancy. Drift here is
    /// SEV-1 (legal exposure: customer-facing invoice may be wrong).
    Layer3Stripe,
}

impl ReconcileLayerKind {
    /// Canonical lower-snake-case mnemonic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Layer1Emit => "layer1_emit",
            Self::Layer2Aggregate => "layer2_aggregate",
            Self::Layer3Stripe => "layer3_stripe",
        }
    }
}

impl core::fmt::Display for ReconcileLayerKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 3-element layer-kind list. Pinned for cardinality estimate +
/// cross-component regression tests.
#[must_use]
pub const fn canonical_reconcile_layer_kinds() -> &'static [ReconcileLayerKind; 3] {
    &[
        ReconcileLayerKind::Layer1Emit,
        ReconcileLayerKind::Layer2Aggregate,
        ReconcileLayerKind::Layer3Stripe,
    ]
}

/// Per-(tenant, billing_period) input snapshot: the three independent
/// totals the orchestrator compares to detect drift.
///
/// `total_qty` is `u128` because the upstream
/// `AggregatedCounter::data.total_qty` is `u128` + the saturating-coerce
/// to `u64` only happens at the Stripe API boundary; reconciliation
/// stays at `u128` to preserve the canonical-bytes discipline.
///
/// `drift_record_count` is the per-tenant cardinality of records that
/// disagreed (the "≤ 5 absolute" arm of the dual-condition auto-fix gate
/// per Lote 10.6bis P0-6 inheritance + WI-S10-004 §1 invariant 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerTotals {
    /// Σ qty across all events / counter / Stripe usage_records for
    /// this layer (post-aggregation).
    pub total_qty: u128,
    /// Cardinality of contributing rows (the "absolute count" arm of
    /// the dual-condition auto-fix gate; mirrors WI-S06 reconcile +
    /// the canonical Lote 10.6bis P0-6 scale-invariant pattern).
    pub record_count: u64,
}

impl LayerTotals {
    /// Construct a fresh totals snapshot.
    #[must_use]
    pub const fn new(total_qty: u128, record_count: u64) -> Self {
        Self {
            total_qty,
            record_count,
        }
    }

    /// Genesis (zero) totals.
    #[must_use]
    pub const fn zero() -> Self {
        Self::new(0, 0)
    }
}

/// Per-(tenant, billing_period) reconciliation snapshot. The
/// orchestrator pulls one of these from the three trait surfaces
/// (Layer 1 emit projection / Layer 2 aggregator projection / Layer 3
/// Stripe ledger projection) at run time + computes drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcileSnapshot {
    /// Tenant id of the snapshot.
    pub tenant_id: Uuid,
    /// Layer 1 totals (Σ R2 events).
    pub layer1: LayerTotals,
    /// Layer 2 totals (Σ aggregated counters).
    pub layer2: LayerTotals,
    /// Layer 3 totals (Σ Stripe usage_records).
    pub layer3: LayerTotals,
}

impl ReconcileSnapshot {
    /// Construct a fresh snapshot with all three layers.
    #[must_use]
    pub const fn new(
        tenant_id: Uuid,
        layer1: LayerTotals,
        layer2: LayerTotals,
        layer3: LayerTotals,
    ) -> Self {
        Self {
            tenant_id,
            layer1,
            layer2,
            layer3,
        }
    }

    /// Read totals for the requested layer.
    #[must_use]
    pub const fn totals_for(&self, kind: ReconcileLayerKind) -> LayerTotals {
        match kind {
            ReconcileLayerKind::Layer1Emit => self.layer1,
            ReconcileLayerKind::Layer2Aggregate => self.layer2,
            ReconcileLayerKind::Layer3Stripe => self.layer3,
        }
    }
}

/// Canonical 5-element decision taxonomy. Mirrors the canonical 4-tier
/// drift threshold ladder (`< 0.01%` Quiet / `< 0.1%` SEV-3 / `< 1%`
/// SEV-2 / `≥ 1%` SEV-1) **plus** the `AutoFixed` arm carved out from
/// the Quiet tier when the dual-condition auto-fix gate fires (drift
/// AND record-count both within the canonical bounds — Lote 10.6bis P0-6
/// scale-invariant).
///
/// `PageSev1AutoPaused` carries the `pause_acked: bool` so the caller
/// can distinguish a successful pause from a pause that was attempted +
/// audited but the control surface backend rejected it (per Lote 10.6bis
/// fail-CLOSED envelope: the audit row landed first; the pause attempt
/// is logged + bubbles up via [`crate::error::ReconcileError::StripePause`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReconcileDecision {
    /// All three layers within canonical float-precision tolerance
    /// (`drift_pct < 0.01%`); audit `no_drift` emitted (forensic
    /// trail; sustained < 0.01% gates the 30d clean streak DoD per
    /// sprint contract §6).
    NoDrift {
        /// Maximum drift percentage observed across the three pairwise
        /// layer comparisons (Layer 1 ↔ 2 / Layer 2 ↔ 3 / Layer 1 ↔ 3),
        /// expressed as a fractional unit (`0.0001 = 0.01%`).
        max_drift_pct: f64,
    },
    /// Drift detected AND within the dual-condition auto-fix gate
    /// (`record_count ≤ AUTO_FIX_MAX_RECORDS AND drift_pct ≤
    /// AUTO_FIX_MAX_PERCENT` per tenant — Lote 10.6bis P0-6
    /// scale-invariant). Auto-fix audit `auto_fixed` emitted BEFORE
    /// the corrective drift-history INSERT (fail-CLOSED envelope).
    AutoFixed {
        /// Maximum drift percentage observed.
        max_drift_pct: f64,
        /// Drift record count at the moment the gate fired.
        drift_record_count: u64,
    },
    /// `0.01% ≤ drift_pct < 0.1%` (statistical-bound breach but under
    /// the canonical SEV-2 trigger). SEV-3 monitor; drift-history
    /// ticket filed for Finance-triage backlog. Audit `ticket_filed`
    /// emitted BEFORE the drift-history INSERT.
    TicketSev3 {
        /// Maximum drift percentage observed.
        max_drift_pct: f64,
        /// Layer where the maximum drift was observed (the "primary"
        /// drift signal — used to direct Finance triage).
        primary_layer: ReconcileLayerKind,
    },
    /// `0.1% ≤ drift_pct < 1%` (canonical SEV-2 page; PagerDuty
    /// `corelink-finance` + `corelink-sre` services per sprint contract
    /// §17 inheritance from WI-S09-006). Audit `page_dispatched`
    /// emitted BEFORE the drift-history INSERT.
    PageSev2 {
        /// Maximum drift percentage observed.
        max_drift_pct: f64,
        /// Layer where the maximum drift was observed.
        primary_layer: ReconcileLayerKind,
    },
    /// `drift_pct ≥ 1%` (canonical SEV-1 page + Stripe-submission
    /// auto-pause). Audit `stripe_paused` emitted BEFORE the
    /// `StripeSubmissionControl::pause` call (fail-CLOSED envelope:
    /// audit row lands even if the pause attempt fails downstream).
    PageSev1AutoPaused {
        /// Maximum drift percentage observed.
        max_drift_pct: f64,
        /// Layer where the maximum drift was observed (Layer 3 = real
        /// Stripe API discrepancy = customer-facing invoice may be
        /// wrong = legal exposure).
        primary_layer: ReconcileLayerKind,
        /// `true` when the Stripe-submission control surface
        /// acknowledged the pause; `false` when the pause attempt
        /// failed at the backend (the audit row landed first, the
        /// SEV-1 alert is already dispatched, the pause failure
        /// surfaces as [`crate::error::ReconcileError::StripePause`]
        /// to the caller — the orchestrator does NOT silently swallow
        /// the failure).
        pause_acked: bool,
    },
}

impl ReconcileDecision {
    /// Canonical lower-snake-case mnemonic for metrics + audit context.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoDrift { .. } => "no_drift",
            Self::AutoFixed { .. } => "auto_fixed",
            Self::TicketSev3 { .. } => "ticket_sev3",
            Self::PageSev2 { .. } => "page_sev2",
            Self::PageSev1AutoPaused { .. } => "page_sev1_auto_paused",
        }
    }

    /// Maximum drift percentage observed for this decision.
    #[must_use]
    pub const fn max_drift_pct(self) -> f64 {
        match self {
            Self::NoDrift { max_drift_pct }
            | Self::AutoFixed { max_drift_pct, .. }
            | Self::TicketSev3 { max_drift_pct, .. }
            | Self::PageSev2 { max_drift_pct, .. }
            | Self::PageSev1AutoPaused { max_drift_pct, .. } => max_drift_pct,
        }
    }
}

/// Auto-fix gate (count arm; Lote 10.6bis Part 2a P0-6
/// scale-invariant). Auto-fix fires ONLY when
/// `drift_record_count ≤ AUTO_FIX_MAX_RECORDS` AND
/// `drift_pct ≤ AUTO_FIX_MAX_PERCENT` per tenant. Boundary `=5`
/// auto-fixes (operative bound `≤5`); `=6` rejects → escalate.
pub const AUTO_FIX_MAX_RECORDS: u64 = 5;

/// Auto-fix gate (percentage arm; Lote 10.6bis Part 2a P0-6
/// scale-invariant). Expressed as a fractional unit (`0.01 % =
/// 0.0001`).
pub const AUTO_FIX_MAX_PERCENT: f64 = 0.0001;

/// Drift threshold below which the run is silent (`< 0.01%` canonical
/// float-precision tolerance per WI-S10-004 §6.1 + sprint contract
/// §14.s10.1 zero-tolerance ladder). Boundary semantics: `drift_pct ≤
/// QUIET_THRESHOLD` is Quiet; `drift_pct > QUIET_THRESHOLD` advances to
/// the SEV-3 ticket arm.
pub const QUIET_THRESHOLD: f64 = 0.0001;

/// SEV-2 page threshold (`0.1%` canonical sprint contract §14.s10.1).
/// Boundary semantics: `drift_pct ≤ SEV3_TO_SEV2_THRESHOLD` is SEV-3
/// ticket; `drift_pct > SEV3_TO_SEV2_THRESHOLD` advances to SEV-2 page.
pub const SEV3_TO_SEV2_THRESHOLD: f64 = 0.001;

/// SEV-1 page + Stripe-pause threshold (`1%` canonical sprint contract
/// §14.s10.1). Boundary semantics: `drift_pct ≤ SEV2_TO_SEV1_THRESHOLD`
/// is SEV-2 page; `drift_pct > SEV2_TO_SEV1_THRESHOLD` advances to
/// SEV-1 page + Stripe auto-pause arm.
pub const SEV2_TO_SEV1_THRESHOLD: f64 = 0.01;

/// Knobs driving the reconciliation orchestrator. Defaults pin the
/// canonical thresholds from sprint contract §14.s10.1 + WI-S10-004 §1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReconcileConfig {
    auto_fix_max_records: u64,
    auto_fix_max_percent: f64,
    quiet_threshold: f64,
    sev3_to_sev2_threshold: f64,
    sev2_to_sev1_threshold: f64,
}

impl Default for ReconcileConfig {
    fn default() -> Self {
        Self {
            auto_fix_max_records: AUTO_FIX_MAX_RECORDS,
            auto_fix_max_percent: AUTO_FIX_MAX_PERCENT,
            quiet_threshold: QUIET_THRESHOLD,
            sev3_to_sev2_threshold: SEV3_TO_SEV2_THRESHOLD,
            sev2_to_sev1_threshold: SEV2_TO_SEV1_THRESHOLD,
        }
    }
}

impl ReconcileConfig {
    /// Construct a custom config. Production wiring lifts the values
    /// from `wrangler.toml` env overrides
    /// (`CORELINK_BILLING_RECONCILE_*`).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ReconcileError::Config`] when an
    /// invariant is violated (non-finite tolerance; negative percentage;
    /// non-monotonic ladder `quiet > sev3 > sev1`; auto-fix percentage
    /// gate above the SEV-3 ticket threshold which would defeat the
    /// purpose of the gate).
    pub fn new(
        auto_fix_max_records: u64,
        auto_fix_max_percent: f64,
        quiet_threshold: f64,
        sev3_to_sev2_threshold: f64,
        sev2_to_sev1_threshold: f64,
    ) -> Result<Self, crate::error::ReconcileError> {
        for (name, v) in [
            ("auto_fix_max_percent", auto_fix_max_percent),
            ("quiet_threshold", quiet_threshold),
            ("sev3_to_sev2_threshold", sev3_to_sev2_threshold),
            ("sev2_to_sev1_threshold", sev2_to_sev1_threshold),
        ] {
            if !v.is_finite() || v < 0.0 {
                return Err(crate::error::ReconcileError::Config(format!(
                    "{name} must be finite and >= 0; got {v}"
                )));
            }
        }
        // Monotonic ladder: quiet < sev3 < sev1 (strict).
        if quiet_threshold >= sev3_to_sev2_threshold {
            return Err(crate::error::ReconcileError::Config(format!(
                "quiet_threshold ({quiet_threshold}) must be strictly less than sev3_to_sev2_threshold ({sev3_to_sev2_threshold})"
            )));
        }
        if sev3_to_sev2_threshold >= sev2_to_sev1_threshold {
            return Err(crate::error::ReconcileError::Config(format!(
                "sev3_to_sev2_threshold ({sev3_to_sev2_threshold}) must be strictly less than sev2_to_sev1_threshold ({sev2_to_sev1_threshold})"
            )));
        }
        // Auto-fix percentage gate must NOT exceed the canonical
        // float-precision Quiet ceiling — the auto-fix arm carves a
        // narrow safe-fix band INSIDE the Quiet tier, so promoting it
        // above the SEV-3 trigger would defeat the gate.
        if auto_fix_max_percent > quiet_threshold {
            return Err(crate::error::ReconcileError::Config(format!(
                "auto_fix_max_percent ({auto_fix_max_percent}) must not exceed quiet_threshold ({quiet_threshold})"
            )));
        }
        Ok(Self {
            auto_fix_max_records,
            auto_fix_max_percent,
            quiet_threshold,
            sev3_to_sev2_threshold,
            sev2_to_sev1_threshold,
        })
    }

    /// Auto-fix max records gate (count arm).
    #[must_use]
    pub const fn auto_fix_max_records(self) -> u64 {
        self.auto_fix_max_records
    }

    /// Auto-fix max percent gate (percentage arm).
    #[must_use]
    pub const fn auto_fix_max_percent(self) -> f64 {
        self.auto_fix_max_percent
    }

    /// Quiet threshold (canonical float-precision tolerance ceiling).
    #[must_use]
    pub const fn quiet_threshold(self) -> f64 {
        self.quiet_threshold
    }

    /// SEV-3 ticket → SEV-2 page boundary.
    #[must_use]
    pub const fn sev3_to_sev2_threshold(self) -> f64 {
        self.sev3_to_sev2_threshold
    }

    /// SEV-2 page → SEV-1 page + Stripe-pause boundary.
    #[must_use]
    pub const fn sev2_to_sev1_threshold(self) -> f64 {
        self.sev2_to_sev1_threshold
    }
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

    #[test]
    fn canonical_layer_kinds_pinned() {
        let s = canonical_reconcile_layer_kinds();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].as_str(), "layer1_emit");
        assert_eq!(s[1].as_str(), "layer2_aggregate");
        assert_eq!(s[2].as_str(), "layer3_stripe");
    }

    #[test]
    fn layer_kind_display_matches_str() {
        assert_eq!(
            format!("{}", ReconcileLayerKind::Layer3Stripe),
            "layer3_stripe"
        );
    }

    #[test]
    fn canonical_constants_pinned() {
        assert_eq!(AUTO_FIX_MAX_RECORDS, 5);
        assert_eq!(AUTO_FIX_MAX_PERCENT, 0.0001);
        assert_eq!(QUIET_THRESHOLD, 0.0001);
        assert_eq!(SEV3_TO_SEV2_THRESHOLD, 0.001);
        assert_eq!(SEV2_TO_SEV1_THRESHOLD, 0.01);
    }

    #[test]
    fn config_default_matches_canonical_constants() {
        let c = ReconcileConfig::default();
        assert_eq!(c.auto_fix_max_records(), AUTO_FIX_MAX_RECORDS);
        assert_eq!(c.auto_fix_max_percent(), AUTO_FIX_MAX_PERCENT);
        assert_eq!(c.quiet_threshold(), QUIET_THRESHOLD);
        assert_eq!(c.sev3_to_sev2_threshold(), SEV3_TO_SEV2_THRESHOLD);
        assert_eq!(c.sev2_to_sev1_threshold(), SEV2_TO_SEV1_THRESHOLD);
    }

    #[test]
    fn config_rejects_non_finite() {
        let r = ReconcileConfig::new(5, f64::NAN, 0.0001, 0.001, 0.01);
        assert!(matches!(r, Err(crate::error::ReconcileError::Config(_))));
    }

    #[test]
    fn config_rejects_negative() {
        let r = ReconcileConfig::new(5, -1.0, 0.0001, 0.001, 0.01);
        assert!(matches!(r, Err(crate::error::ReconcileError::Config(_))));
    }

    #[test]
    fn config_rejects_non_monotonic_ladder() {
        let r = ReconcileConfig::new(5, 0.0001, 0.001, 0.0005, 0.01);
        assert!(matches!(r, Err(crate::error::ReconcileError::Config(_))));
    }

    #[test]
    fn config_rejects_auto_fix_above_quiet() {
        let r = ReconcileConfig::new(5, 0.001, 0.0001, 0.001, 0.01);
        assert!(matches!(r, Err(crate::error::ReconcileError::Config(_))));
    }

    #[test]
    fn config_accepts_canonical() {
        ReconcileConfig::new(
            AUTO_FIX_MAX_RECORDS,
            AUTO_FIX_MAX_PERCENT,
            QUIET_THRESHOLD,
            SEV3_TO_SEV2_THRESHOLD,
            SEV2_TO_SEV1_THRESHOLD,
        )
        .unwrap();
    }

    #[test]
    fn snapshot_totals_for_dispatches_per_layer() {
        let t = Uuid::now_v7();
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(100, 1),
            LayerTotals::new(200, 2),
            LayerTotals::new(300, 3),
        );
        assert_eq!(
            s.totals_for(ReconcileLayerKind::Layer1Emit),
            LayerTotals::new(100, 1)
        );
        assert_eq!(
            s.totals_for(ReconcileLayerKind::Layer2Aggregate),
            LayerTotals::new(200, 2)
        );
        assert_eq!(
            s.totals_for(ReconcileLayerKind::Layer3Stripe),
            LayerTotals::new(300, 3)
        );
    }

    #[test]
    fn decision_max_drift_pct_lifts_per_arm() {
        assert_eq!(
            ReconcileDecision::NoDrift { max_drift_pct: 0.0 }.max_drift_pct(),
            0.0
        );
        assert_eq!(
            ReconcileDecision::AutoFixed {
                max_drift_pct: 0.00005,
                drift_record_count: 1
            }
            .max_drift_pct(),
            0.00005
        );
        assert_eq!(
            ReconcileDecision::PageSev1AutoPaused {
                max_drift_pct: 0.05,
                primary_layer: ReconcileLayerKind::Layer3Stripe,
                pause_acked: true,
            }
            .max_drift_pct(),
            0.05
        );
    }

    #[test]
    fn decision_as_str_pins_taxonomy() {
        assert_eq!(
            ReconcileDecision::NoDrift { max_drift_pct: 0.0 }.as_str(),
            "no_drift"
        );
        assert_eq!(
            ReconcileDecision::AutoFixed {
                max_drift_pct: 0.0,
                drift_record_count: 1
            }
            .as_str(),
            "auto_fixed"
        );
        assert_eq!(
            ReconcileDecision::TicketSev3 {
                max_drift_pct: 0.0005,
                primary_layer: ReconcileLayerKind::Layer1Emit,
            }
            .as_str(),
            "ticket_sev3"
        );
        assert_eq!(
            ReconcileDecision::PageSev2 {
                max_drift_pct: 0.005,
                primary_layer: ReconcileLayerKind::Layer2Aggregate,
            }
            .as_str(),
            "page_sev2"
        );
        assert_eq!(
            ReconcileDecision::PageSev1AutoPaused {
                max_drift_pct: 0.05,
                primary_layer: ReconcileLayerKind::Layer3Stripe,
                pause_acked: true,
            }
            .as_str(),
            "page_sev1_auto_paused"
        );
    }

    #[test]
    fn layer_totals_zero_genesis() {
        let t = LayerTotals::zero();
        assert_eq!(t.total_qty, 0);
        assert_eq!(t.record_count, 0);
    }

    #[test]
    fn snapshot_serde_round_trips() {
        let t = Uuid::now_v7();
        let s = ReconcileSnapshot::new(
            t,
            LayerTotals::new(100, 1),
            LayerTotals::new(100, 1),
            LayerTotals::new(100, 1),
        );
        let json = serde_json::to_string(&s).unwrap();
        let back: ReconcileSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn decision_serde_round_trips() {
        let d = ReconcileDecision::PageSev1AutoPaused {
            max_drift_pct: 0.05,
            primary_layer: ReconcileLayerKind::Layer3Stripe,
            pause_acked: true,
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: ReconcileDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
