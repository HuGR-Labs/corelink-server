//! Canonical replay forensic types: [`ReplayRequest`] +
//! [`ReplayReason`] + [`ReplayDecision`] + [`ReconstructedLayers`] +
//! [`ReplayConfig`].
//!
//! ## Why typed enums (NOT string discriminants)
//!
//! Per Lote 10.9-quinquies NEW-P0-2 (absorbed via WI-S10-006 §1
//! invariant 6): the replay reports stored in the canonical S-09
//! append-only audit chain (R2 Object Lock 7y) must be byte-deterministic
//! across the auditor evidence trail. Untyped `serde_json::Value`
//! defeats compile-time taxonomy enforcement; the typed enum +
//! per-variant payload struct is the canonical Lote 10.9-quinquies
//! absorption pattern (mirrors `UsageEventKind`, `AggregationDecision`,
//! `StripeAdapterDecision`, `ReconcileDecision`, `QuotaTransition`).
//!
//! All public enums are `#[non_exhaustive]` so follow-on WIs can extend
//! the taxonomy additively without breaking downstream `match` sites
//! (e.g. WI-S10-007 PRR ship gate may add a `LegalDiscoveryRequest`
//! reason variant; S-13 admin plane may add a `BulkReplay` decision
//! arm; S-20 GA gate may add a `LighthouseCustomerWalkthrough` reason).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use corelink_billing_reconcile::ReconcileDecision;

/// Canonical 4-element replay-reason taxonomy. Per WI-S10-006 §1: every
/// replay request carries an explicit reason so the auditor evidence
/// trail is unambiguous (a forensic replay without a recorded reason is
/// indistinguishable from operator-side curiosity, defeating the SOC 2
/// CC1.4 review purpose).
///
/// `#[non_exhaustive]` reserves additive growth for follow-on WIs (e.g.
/// `LegalDiscoveryRequest` for the WI-S10-007 PRR ship gate; `BulkReplay`
/// for S-13 admin plane).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReplayReason {
    /// Internal drift investigation: WI-S10-004 reconciliation worker
    /// flagged a divergence; the replay reconstructs the canonical
    /// layer outputs from the R2 raw events archive so the on-call
    /// engineer can isolate which layer (1, 2, or 3) introduced the
    /// drift.
    DriftInvestigation,
    /// Customer dispute / chargeback: customer claims overcharge for
    /// `(tenant, billing_period)`; Finance triggers the replay to
    /// produce an audit-grade reconstruction; the diff against the
    /// production Stripe invoice is forwarded to the customer with the
    /// chain of evidence.
    CustomerDispute,
    /// Compliance audit (SOC 2 CC1.4 walkthrough; GDPR Art. 22
    /// automated decision review): the auditor demands an independent
    /// reconstruction of an arbitrary invoice from the immutable 7y
    /// archive; the replay produces a byte-deterministic report that
    /// satisfies the control evidence requirement.
    ComplianceAudit,
    /// Dry-run / rehearsal: an operator wants to verify the replay
    /// pipeline itself (e.g. before a real customer dispute lands)
    /// without producing an auditor-grade forensic report. The
    /// canonical [`ReplayDecision::DryRunPlan`] arm fires; no state
    /// mutation; no append to the production audit chain (the
    /// `dry_run_planned` audit row IS appended to the chain so the
    /// rehearsal itself is auditable).
    DryRun,
}

impl ReplayReason {
    /// Canonical lower-snake-case mnemonic. Pinned for D1 CHECK
    /// constraints + dashboard widget grouping + cross-component
    /// regression tests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DriftInvestigation => "drift_investigation",
            Self::CustomerDispute => "customer_dispute",
            Self::ComplianceAudit => "compliance_audit",
            Self::DryRun => "dry_run",
        }
    }
}

impl core::fmt::Display for ReplayReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical 4-element replay-reason list. Pinned for cardinality
/// estimate + cross-component regression tests.
#[must_use]
pub const fn canonical_replay_reasons() -> &'static [ReplayReason; 4] {
    &[
        ReplayReason::DriftInvestigation,
        ReplayReason::CustomerDispute,
        ReplayReason::ComplianceAudit,
        ReplayReason::DryRun,
    ]
}

/// Canonical 4-element replay-decision taxonomy. Per WI-S10-006 §6.1:
/// every replay invocation lands in exactly one of these arms; the
/// canonical audit envelope `corelink.billing_replay.<arm>` fires
/// BEFORE the state mutation on each arm.
///
/// `#[non_exhaustive]` reserves additive growth (e.g. follow-on WIs
/// may add `PartialExecuted` for the layer-by-layer drift investigation
/// arm).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReplayDecision {
    /// Authorization passed but the request reused a prior idempotent
    /// outcome (same `request_id` re-submitted; the orchestrator
    /// refuses to double-execute; reuses the prior outcome via the
    /// canonical idempotency ledger).
    ///
    /// The canonical audit row `request_authorized` lands so the
    /// auditor sees the re-submission attempt; no second `executed`
    /// row lands (the prior `executed` row is the authoritative one).
    Authorized {
        /// Whether this is a re-submission of an already-executed
        /// request (the orchestrator returned the prior outcome
        /// without re-running the pipeline).
        idempotent_replay: bool,
    },
    /// Authorization REJECTED: the requesting principal does not
    /// hold the canonical `billing_forensics_admin` role per
    /// CTRL-AUTHZ-002 (separate from regular admin per WI-S10-006 §1
    /// invariant 3).
    Denied403 {
        /// The role string the request presented (logged for forensic
        /// trail; never used for authorization). The canonical role
        /// strings are `billing_forensics_admin` (allowed) and any
        /// other (denied).
        presented_role: String,
    },
    /// Authorization passed + `ReplayReason::DryRun` arm: the
    /// orchestrator computed the replay plan but did NOT execute the
    /// pipeline. No production audit chain mutation past the
    /// `dry_run_planned` audit envelope. Used for rehearsal +
    /// pre-customer-dispute verification.
    DryRunPlan {
        /// The number of canonical layers that WOULD be reconstructed
        /// if the request were executed (canonical 3-layer pipeline:
        /// emit / aggregator / reconcile).
        layers_planned: u8,
    },
    /// Authorization passed + non-dry-run reason: the replay
    /// pipeline ran end-to-end producing reconstructed outputs across
    /// the canonical 3 layers. The orchestrator records the outcome
    /// in the idempotency ledger so future re-submissions of the same
    /// `request_id` short-circuit to [`ReplayDecision::Authorized`]
    /// `idempotent_replay = true`.
    Executed {
        /// Whether the reconstructed layer outputs diverged from the
        /// production reference values. `true` triggers the canonical
        /// `layer_diverged` audit envelope (separate from the
        /// `executed` envelope; surfaces forensic anomaly to the
        /// auditor evidence trail).
        layer_diverged: bool,
    },
}

impl ReplayDecision {
    /// Canonical lower-snake-case mnemonic. Pinned for D1 CHECK
    /// constraints + cross-component regression tests.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Authorized { .. } => "authorized",
            Self::Denied403 { .. } => "denied_403",
            Self::DryRunPlan { .. } => "dry_run_plan",
            Self::Executed { .. } => "executed",
        }
    }

    /// Whether this arm represents an authorization rejection.
    /// Pinned by `prop_authorized_role_only_executes`.
    #[must_use]
    pub const fn is_denied(&self) -> bool {
        matches!(self, Self::Denied403 { .. })
    }

    /// Whether this arm represents a state-mutating execution
    /// (orchestrator wrote a row to the idempotency ledger).
    #[must_use]
    pub const fn is_executed(&self) -> bool {
        matches!(self, Self::Executed { .. })
    }
}

/// Canonical billing-forensics admin role string. Per CTRL-AUTHZ-002
/// (security_model.md): the replay capability is a separate role from
/// regular admin so the SOC 2 CC1.4 + GDPR Art. 22 + GAAP ASC 606
/// audit-grade replay primitive is least-privilege-bounded. A
/// compromised regular admin account cannot trigger forensic replays;
/// the dedicated role is bound only to Finance + Compliance Officer.
///
/// Production wiring binds this string to the canonical Tower
/// middleware role check at the
/// `POST /v1/billing/replay` route (deferred to WI-S10-007 PRR ship
/// gate per the `trait-abstraction-defer` charter pattern). The trait
/// surface here treats the role as opaque + matches against the
/// canonical string by exact equality.
pub const BILLING_FORENSICS_ADMIN_ROLE: &str = "billing_forensics_admin";

/// A replay request: per (request_id, tenant, billing_period, reason)
/// the canonical input shape that the orchestrator dispatches.
///
/// `request_id` is canonical UUIDv7 (time-ordered + globally unique +
/// monotonic across emitters) so the idempotency ledger key is
/// time-localized + a duplicate re-submission is detected by exact
/// equality (a random UUIDv4 would have the same equality semantics
/// but lose the time-ordering forensic property — which arm landed
/// first under concurrent re-submissions; the audit chain timeline
/// would be ambiguous).
///
/// Per WI-S10-006 §6.1: idempotency-on-`request_id` is the canonical
/// forensic determinism rationale: the same dispute / audit /
/// investigation may be re-run multiple times across the 7y retention
/// window; every re-submission must reuse the prior outcome (otherwise
/// the auditor sees diverging replay outputs for the same logical
/// request — which is itself a tampering signal).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayRequest {
    /// Canonical UUIDv7 request id. Time-ordered; idempotency key for
    /// the canonical replay ledger.
    pub request_id: Uuid,
    /// Canonical UUIDv7 admin pubkey of the requesting principal.
    /// Production wiring extracts this from the JWT-authenticated
    /// `TenantCtx` middleware (S-03 inheritance Lote 10.4bis); the
    /// trait surface here treats it as opaque.
    pub requested_by: Uuid,
    /// Canonical role string presented by the requesting principal.
    /// Exact equality against [`BILLING_FORENSICS_ADMIN_ROLE`] gates
    /// the authorization arm.
    pub presented_role: String,
    /// Tenant id for the canonical (tenant, billing_period) replay
    /// scope. Per Lote 10.4bis: tenant id MUST come from the
    /// authenticated middleware (NOT the request body); the trait
    /// surface here treats it as opaque.
    pub tenant_id: Uuid,
    /// Canonical UTC month bucket (`YYYY-MM`). Validated upstream via
    /// [`corelink_billing_emit::validate_billing_period`].
    pub billing_period: String,
    /// Canonical replay reason taxonomy.
    pub reason: ReplayReason,
}

impl ReplayRequest {
    /// Construct a fresh replay request. The canonical orchestrator
    /// at [`super::engine::ReplayEngine::replay`] consumes the
    /// request directly; production wiring at the Worker route
    /// `POST /v1/billing/replay` deserializes the inbound JSON body +
    /// populates `requested_by` + `presented_role` + `tenant_id` from
    /// the authenticated middleware.
    #[must_use]
    pub fn new(
        request_id: Uuid,
        requested_by: Uuid,
        presented_role: impl Into<String>,
        tenant_id: Uuid,
        billing_period: impl Into<String>,
        reason: ReplayReason,
    ) -> Self {
        Self {
            request_id,
            requested_by,
            presented_role: presented_role.into(),
            tenant_id,
            billing_period: billing_period.into(),
            reason,
        }
    }

    /// Whether the presented role matches the canonical
    /// [`BILLING_FORENSICS_ADMIN_ROLE`]. Pinned by
    /// `prop_authorized_role_only_executes`.
    #[must_use]
    pub fn role_authorized(&self) -> bool {
        self.presented_role == BILLING_FORENSICS_ADMIN_ROLE
    }
}

/// Canonical 3-layer reconstructed-output snapshot: the per-layer
/// totals the orchestrator re-derives from the R2 NDJSON archive.
/// Per WI-S10-006 §6.1: the same code paths as production aggregation
/// (WI-S10-002) + invoice generation (WI-S10-003) MUST be re-applied;
/// otherwise reconstruction differs from "what should have been" + the
/// drift detection becomes ambiguous (reconstruction-side bug vs
/// production-side bug indistinguishable).
///
/// The trait surface here ships pure-logic re-aggregation; the
/// production CF Worker route at WI-S10-007 binds the actual R2 read
/// + the canonical aggregator + Stripe-fetch composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconstructedLayers {
    /// Layer 1: Σ raw usage events from the R2 NDJSON archive at
    /// `usage/{tenant_id}/{billing_period}/*.usage.ndjson`.
    pub layer1_emit_total_qty: u128,
    /// Layer 2: Σ aggregated counters re-derived via the canonical
    /// aggregator code path (WI-S10-002 deterministic aggregation).
    pub layer2_aggregator_total_qty: u128,
    /// Layer 3: Σ Stripe usage_records re-derived via the canonical
    /// Stripe adapter idempotency-key derivation (WI-S10-003).
    pub layer3_stripe_total_qty: u128,
}

impl ReconstructedLayers {
    /// Construct a fresh reconstructed-layers snapshot.
    #[must_use]
    pub const fn new(
        layer1_emit_total_qty: u128,
        layer2_aggregator_total_qty: u128,
        layer3_stripe_total_qty: u128,
    ) -> Self {
        Self {
            layer1_emit_total_qty,
            layer2_aggregator_total_qty,
            layer3_stripe_total_qty,
        }
    }

    /// Genesis (zero) totals. Useful for the dry-run-plan arm.
    #[must_use]
    pub const fn zero() -> Self {
        Self::new(0, 0, 0)
    }

    /// Whether all three layers reconstructed to the same canonical
    /// total (byte-identical re-derivation). The canonical
    /// "byte-identical" semantic is `Σ qty equality across layers`;
    /// floating-point drift is NOT possible here because the layer
    /// totals are `u128` (lossless across the BLAKE3 chain).
    #[must_use]
    pub const fn three_layers_match(&self) -> bool {
        self.layer1_emit_total_qty == self.layer2_aggregator_total_qty
            && self.layer2_aggregator_total_qty == self.layer3_stripe_total_qty
    }
}

/// Canonical per-replay outcome stored in the idempotency ledger. The
/// ledger PRIMARY KEY is `request_id` UNIQUE; a duplicate re-submission
/// of the same `request_id` retrieves this record + short-circuits to
/// [`ReplayDecision::Authorized`] `idempotent_replay = true`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayOutcome {
    /// Wall-clock instant the canonical orchestrator landed the
    /// outcome (Unix epoch ms; canonical no `_ms` suffix per Lote
    /// 10.7bis P0-3).
    pub landed_at: u64,
    /// The canonical decision arm.
    pub decision: ReplayDecision,
    /// The reconstructed-layers snapshot (zero for the
    /// `Denied403` + `DryRunPlan` arms; non-zero for the `Executed`
    /// arm; the `Authorized` arm with `idempotent_replay = true`
    /// reuses the prior outcome's snapshot).
    pub layers: ReconstructedLayers,
    /// The reference layer totals against which the reconstruction
    /// was diffed (the canonical production layer totals; empty for
    /// the `Denied403` + `DryRunPlan` arms).
    pub reference_layers: ReconstructedLayers,
}

impl ReplayOutcome {
    /// Construct a fresh outcome.
    #[must_use]
    pub const fn new(
        landed_at: u64,
        decision: ReplayDecision,
        layers: ReconstructedLayers,
        reference_layers: ReconstructedLayers,
    ) -> Self {
        Self {
            landed_at,
            decision,
            layers,
            reference_layers,
        }
    }
}

/// Canonical 5-element decision taxonomy: a layer-by-layer diff
/// summary the orchestrator emits alongside the [`ReplayDecision`]
/// for forensic clarity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum LayerDriftSummary {
    /// All three layers reconstructed byte-identical to the reference
    /// production totals. Customer dispute resolved (CoreLink data
    /// correct).
    AllLayersMatch,
    /// Layer 1 (raw events) diverged: emit-side bug or R2 archive
    /// tampering signal.
    Layer1Diverged,
    /// Layer 2 (aggregator) diverged: aggregator-side bug; Layer 1
    /// matched.
    Layer2Diverged,
    /// Layer 3 (Stripe) diverged: Stripe API ingest discrepancy;
    /// upstream aggregator matched.
    Layer3Diverged,
    /// Multiple layers diverged: cascading bug or systemic tampering
    /// signal; SEV-1 forensic post-mortem.
    MultipleLayersDiverged,
}

impl LayerDriftSummary {
    /// Compute the canonical drift summary for a reconstructed-vs-reference
    /// pair. Pure function over the two snapshots; pinned by
    /// `prop_layer_diverged_flagged`.
    #[must_use]
    pub const fn classify(
        reconstructed: ReconstructedLayers,
        reference: ReconstructedLayers,
    ) -> Self {
        let l1 = reconstructed.layer1_emit_total_qty != reference.layer1_emit_total_qty;
        let l2 = reconstructed.layer2_aggregator_total_qty != reference.layer2_aggregator_total_qty;
        let l3 = reconstructed.layer3_stripe_total_qty != reference.layer3_stripe_total_qty;
        match (l1, l2, l3) {
            (false, false, false) => Self::AllLayersMatch,
            (true, false, false) => Self::Layer1Diverged,
            (false, true, false) => Self::Layer2Diverged,
            (false, false, true) => Self::Layer3Diverged,
            _ => Self::MultipleLayersDiverged,
        }
    }

    /// Canonical lower-snake-case mnemonic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllLayersMatch => "all_layers_match",
            Self::Layer1Diverged => "layer1_diverged",
            Self::Layer2Diverged => "layer2_diverged",
            Self::Layer3Diverged => "layer3_diverged",
            Self::MultipleLayersDiverged => "multiple_layers_diverged",
        }
    }

    /// Whether this summary represents a divergence (any layer ≠
    /// reference).
    #[must_use]
    pub const fn diverged(self) -> bool {
        !matches!(self, Self::AllLayersMatch)
    }
}

/// Canonical replay-engine config. Reserved for follow-on extensions;
/// the in-memory orchestrator currently has no tunable thresholds (the
/// canonical 3-layer reconstruction is a pure function over the input
/// archive; there's nothing to tune at the trait surface).
///
/// Production wiring at WI-S10-007 may bind:
/// - 30-min p99 SLA budget (sprint contract §6 DoD).
/// - 10 req/h rate limit per `billing_forensics_admin` user.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReplayConfig {
    /// Reserved. The current pure-logic skeleton has no tunables;
    /// follow-on production wiring at WI-S10-007 may add SLA budget
    /// + rate-limit hooks here.
    pub reserved: (),
}

impl ReplayConfig {
    /// Construct a fresh config with the canonical defaults.
    #[must_use]
    pub const fn new() -> Self {
        Self { reserved: () }
    }
}

/// Bridge type that lifts a [`ReconcileDecision`] into the
/// canonical [`LayerDriftSummary`] taxonomy. Used by the orchestrator
/// at the `Executed` arm so the production reconciliation worker's
/// classification (WI-S10-004) flows into the replay forensic report
/// without schema duplication.
///
/// The wildcard arm catches any future additive variant per the
/// `#[non_exhaustive]` discipline; an unknown decision conservatively
/// classifies as `MultipleLayersDiverged` (the canonical "investigate
/// further" sentinel) so the auditor evidence trail surfaces an
/// anomaly rather than silently mis-classifying.
#[must_use]
pub fn drift_summary_from_reconcile(decision: &ReconcileDecision) -> LayerDriftSummary {
    use corelink_billing_reconcile::ReconcileLayerKind;
    match decision {
        ReconcileDecision::NoDrift { .. } | ReconcileDecision::AutoFixed { .. } => {
            LayerDriftSummary::AllLayersMatch
        }
        ReconcileDecision::TicketSev3 { primary_layer, .. }
        | ReconcileDecision::PageSev2 { primary_layer, .. }
        | ReconcileDecision::PageSev1AutoPaused { primary_layer, .. } => match primary_layer {
            ReconcileLayerKind::Layer1Emit => LayerDriftSummary::Layer1Diverged,
            ReconcileLayerKind::Layer2Aggregate => LayerDriftSummary::Layer2Diverged,
            ReconcileLayerKind::Layer3Stripe => LayerDriftSummary::Layer3Diverged,
            _ => LayerDriftSummary::MultipleLayersDiverged,
        },
        _ => LayerDriftSummary::MultipleLayersDiverged,
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

    #[test]
    fn replay_reason_canonical_strings_unique() {
        let v = canonical_replay_reasons();
        assert_eq!(v.len(), 4);
        let mut set = std::collections::HashSet::new();
        for r in v {
            assert!(set.insert(r.as_str()), "duplicate: {r}");
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn replay_decision_canonical_strings_unique() {
        let arms = [
            ReplayDecision::Authorized {
                idempotent_replay: false,
            },
            ReplayDecision::Denied403 {
                presented_role: String::new(),
            },
            ReplayDecision::DryRunPlan { layers_planned: 3 },
            ReplayDecision::Executed {
                layer_diverged: false,
            },
        ];
        let mut set = std::collections::HashSet::new();
        for a in &arms {
            assert!(set.insert(a.as_str()), "duplicate: {}", a.as_str());
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn role_authorization_uses_exact_equality() {
        let r = ReplayRequest::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            BILLING_FORENSICS_ADMIN_ROLE,
            Uuid::now_v7(),
            "2026-05",
            ReplayReason::CustomerDispute,
        );
        assert!(r.role_authorized());

        let r = ReplayRequest::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            "regular_admin",
            Uuid::now_v7(),
            "2026-05",
            ReplayReason::CustomerDispute,
        );
        assert!(!r.role_authorized());
    }

    #[test]
    fn role_authorization_is_case_sensitive() {
        let r = ReplayRequest::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            "Billing_Forensics_Admin",
            Uuid::now_v7(),
            "2026-05",
            ReplayReason::CustomerDispute,
        );
        assert!(!r.role_authorized());
    }

    #[test]
    fn three_layers_match_returns_true_only_for_exact_equality() {
        let l = ReconstructedLayers::new(100, 100, 100);
        assert!(l.three_layers_match());
        let l = ReconstructedLayers::new(100, 100, 101);
        assert!(!l.three_layers_match());
        let l = ReconstructedLayers::zero();
        assert!(l.three_layers_match());
    }

    #[test]
    fn layer_drift_summary_classifies_each_arm() {
        let r = ReconstructedLayers::new(100, 100, 100);
        let p = ReconstructedLayers::new(100, 100, 100);
        assert_eq!(LayerDriftSummary::classify(r, p), LayerDriftSummary::AllLayersMatch);

        let r = ReconstructedLayers::new(99, 100, 100);
        let p = ReconstructedLayers::new(100, 100, 100);
        assert_eq!(LayerDriftSummary::classify(r, p), LayerDriftSummary::Layer1Diverged);

        let r = ReconstructedLayers::new(100, 99, 100);
        assert_eq!(LayerDriftSummary::classify(r, p), LayerDriftSummary::Layer2Diverged);

        let r = ReconstructedLayers::new(100, 100, 99);
        assert_eq!(LayerDriftSummary::classify(r, p), LayerDriftSummary::Layer3Diverged);

        let r = ReconstructedLayers::new(99, 99, 100);
        assert_eq!(
            LayerDriftSummary::classify(r, p),
            LayerDriftSummary::MultipleLayersDiverged
        );
    }

    #[test]
    fn replay_decision_is_denied_pins() {
        let d = ReplayDecision::Denied403 {
            presented_role: "viewer".to_string(),
        };
        assert!(d.is_denied());
        assert!(!d.is_executed());

        let d = ReplayDecision::Authorized {
            idempotent_replay: true,
        };
        assert!(!d.is_denied());
    }

    #[test]
    fn replay_decision_is_executed_pins() {
        let d = ReplayDecision::Executed {
            layer_diverged: false,
        };
        assert!(d.is_executed());
        assert!(!d.is_denied());

        let d = ReplayDecision::DryRunPlan { layers_planned: 3 };
        assert!(!d.is_executed());
    }

    #[test]
    fn role_string_pinned_canonical() {
        assert_eq!(BILLING_FORENSICS_ADMIN_ROLE, "billing_forensics_admin");
    }

    #[test]
    fn drift_summary_lifted_from_reconcile_no_drift() {
        let d = ReconcileDecision::NoDrift { max_drift_pct: 0.0 };
        assert_eq!(drift_summary_from_reconcile(&d), LayerDriftSummary::AllLayersMatch);
    }

    #[test]
    fn drift_summary_lifted_from_reconcile_layer_routes() {
        use corelink_billing_reconcile::ReconcileLayerKind;
        let d = ReconcileDecision::PageSev1AutoPaused {
            max_drift_pct: 0.05,
            primary_layer: ReconcileLayerKind::Layer3Stripe,
            pause_acked: true,
        };
        assert_eq!(drift_summary_from_reconcile(&d), LayerDriftSummary::Layer3Diverged);

        let d = ReconcileDecision::TicketSev3 {
            max_drift_pct: 0.0005,
            primary_layer: ReconcileLayerKind::Layer1Emit,
        };
        assert_eq!(drift_summary_from_reconcile(&d), LayerDriftSummary::Layer1Diverged);

        let d = ReconcileDecision::PageSev2 {
            max_drift_pct: 0.005,
            primary_layer: ReconcileLayerKind::Layer2Aggregate,
        };
        assert_eq!(drift_summary_from_reconcile(&d), LayerDriftSummary::Layer2Diverged);
    }

    #[test]
    fn replay_outcome_constructs_with_canonical_landed_at() {
        let o = ReplayOutcome::new(
            42,
            ReplayDecision::Executed {
                layer_diverged: false,
            },
            ReconstructedLayers::new(1, 1, 1),
            ReconstructedLayers::new(1, 1, 1),
        );
        assert_eq!(o.landed_at, 42);
        assert!(o.decision.is_executed());
        assert!(o.layers.three_layers_match());
    }

    #[test]
    fn replay_config_default_constructs() {
        let c = ReplayConfig::default();
        let c2 = ReplayConfig::new();
        assert_eq!(c, c2);
    }

    #[test]
    fn replay_reason_display_matches_as_str() {
        assert_eq!(format!("{}", ReplayReason::CustomerDispute), "customer_dispute");
    }
}
