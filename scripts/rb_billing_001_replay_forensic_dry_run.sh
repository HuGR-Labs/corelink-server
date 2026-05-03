#!/usr/bin/env bash
# WI-S10-007 — RB-BILLING-001 (Audit-Grade Invoice Replay) dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-BILLING-001 runbook
# (specs/05_quality/runbooks/RB-BILLING-001.md) against the in-memory
# replay engine (corelink-billing-replay) so the runbook step ordering
# can be regression-tested in CI **before** the staging Finance walkthrough
# lands. Per WI §6.1.8: Finance team + mock SOC 2 auditor reconstrói 1
# invoice em < 30min via WI-S10-006 replay endpoint cooperation; auditor
# signs SOC 2 CC1.4 control evidence.
#
# This is the 3rd dry-run in the WI-S10-007 ship gate ladder:
#   - RB-FM-302 (Billing Drift; counter drift detection)
#   - RB-FM-151 (Stripe Outage; PAT-QUEUE-EVENTS-001 fallback)
#   - RB-BILLING-001 (Replay Forensic; Finance walkthrough; this script)
#
# What this script does NOT do: hit a real Cloudflare staging
# environment OR engage a real SOC 2 auditor. The full Finance
# walkthrough exercise (with paid mock auditor + 1 fake customer
# invoice generated in staging + < 30min reconstruction SLA + auditor
# sign-off SOC 2 CC1.4 control evidence) is the integration-tier
# counterpart deferred until staging account provisioned + auditor
# scheduled.
#
# Pattern reused: scripts/rb_fm_302_billing_drift_dry_run.sh +
# scripts/rb_fm_151_stripe_outage_dry_run.sh.
#
# Usage:
#   bash scripts/rb_billing_001_replay_forensic_dry_run.sh [--evidence <path>]
#
# Exit codes:
#   0 — dry-run succeeded; every step emitted the expected outcome.
#   1 — drift detected: one or more steps did not match the runbook.
#   2 — environment / build failure prevented the run.

set -euo pipefail

EVIDENCE_PATH="${EVIDENCE_PATH:-}"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --evidence)
            EVIDENCE_PATH="${2:-}"
            shift 2
            ;;
        *)
            echo "unknown flag: $1" >&2
            echo "usage: $0 [--evidence <path>]" >&2
            exit 2
            ;;
    esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

LOG_LINES=()

emit() {
    local stamp
    stamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    local line="${stamp} | ${1}"
    echo "$line"
    LOG_LINES+=("$line")
}

emit "== RB-BILLING-001 host-side dry-run starting (Finance walkthrough EVT-017) =="
emit "runbook: specs/05_quality/runbooks/RB-BILLING-001.md"
emit "harness: scripts/rb_billing_001_replay_forensic_dry_run.sh (WI-S10-007)"
emit "scenario: Finance + mock SOC 2 auditor reconstrói 1 invoice em < 30min"
emit "          via WI-S10-006 replay endpoint cooperation; auditor signs"
emit "          SOC 2 CC1.4 control evidence."
emit ""

emit "Step 1 — Authenticate + Authorize (≤ 1 min)."
emit "  In production this step authenticates Operator via PAT scope"
emit "  'billing_forensics_admin' (CTRL-AUTHZ-002 separate from regular admin —"
emit "  least-privilege-bounded audit-grade replay capability per WI-S10-006 §1)."
emit "  MFA re-auth required (CTRL-AUTH-010 freshness 30 min)."
emit "  corelink-billing-replay::ReplayEngine emits"
emit "  corelink.billing_replay.request_authorized BEFORE Authorized return."
emit "  Driving: cargo test -p corelink-billing-replay --test prop_billing_replay -- prop_authorized_role_only_executes prop_audit_emit_per_decision_arm"
if cargo test -p corelink-billing-replay --test prop_billing_replay -- \
    prop_authorized_role_only_executes prop_audit_emit_per_decision_arm --quiet \
    >/tmp/rb_billing_001_step1.log 2>&1; then
    emit "  -> PASS: role-only-execute + audit-per-decision-arm canonical at 10k iter"
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_billing_001_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: replay role authz regressed — STOP"
    exit 1
fi
emit ""

emit "Step 2 — Identify scope (≤ 2 min)."
emit "  Identify invoice_id + tenant_id + billing_period (canonical 7-char YYYY-MM)."
emit "  Cross-reference Stripe stripe_id ↔ CoreLink internal idempotency_key"
emit "  via corelink-billing-stripe::InMemoryStripeUsageLedger UPSERT-safe ledger."
emit "  Per ReplayReason taxonomy 4-element (DriftInvestigation / CustomerDispute /"
emit "  ComplianceAudit / DryRun): Finance walkthrough = ComplianceAudit reason."
emit "  -> SIMULATED: scope identification template ready"
emit ""

emit "Step 3 — Replay execution (≤ 5 min p99 SLA per WI §6.1.8 + RB-BILLING-001 §3)."
emit "  POST /v1/billing/replay reads R2 billing-events bucket per tenant + ts"
emit "  range; reconstructs invoice line items deterministically via:"
emit "    - Aggregation per SKU + region (4-tuple PK: tenant_id, region, sku, hour)"
emit "    - Pricing rules from D1 plan snapshot at billing_period boundary"
emit "    - Hash chain integrity verification (BLAKE3-256 link hash via"
emit "      InMemoryAggregatedCounterStore::ChainHeadRecord)"
emit "  Returns ReconstructedLayers (3-layer u128 totals snapshot)."
emit "  Driving: cargo test -p corelink-billing-replay --test prop_billing_replay -- prop_replay_deterministic prop_idempotent_replay_same_request_id"
if cargo test -p corelink-billing-replay --test prop_billing_replay -- \
    prop_replay_deterministic prop_idempotent_replay_same_request_id --quiet \
    >/tmp/rb_billing_001_step3.log 2>&1; then
    emit "  -> PASS: replay determinism + idempotent re-fire on (request_id) PK at 10k iter"
else
    emit "  -> FAIL: replay determinism regression — STOP investigation"
    exit 1
fi
emit ""

emit "Step 4 — Comparison (≤ 30 min p99 SLA)."
emit "  Fetch Stripe invoice via Stripe API; compare reconstructed JSON vs"
emit "  Stripe invoice JSON (modulo Stripe metadata: timestamps + internal Stripe IDs)."
emit "  LayerDriftSummary 5-element classification:"
emit "    - AllLayersMatch        -> ✅ PASS (Diff < 0.01%; expected outcome)"
emit "    - Layer1Diverged        -> R2 events ↔ counter D1 drift"
emit "    - Layer2Diverged        -> counter D1 ↔ invoice_line_item D1 drift"
emit "    - Layer3Diverged        -> invoice_line_item ↔ Stripe drift (CRITICAL)"
emit "    - MultipleLayersDiverged -> SEV-1 escalate Finance + Legal + Compliance"
emit "  Driving: cargo test -p corelink-billing-replay --test prop_billing_replay -- prop_layer_diverged_flagged prop_drift_summary_lifted_canonical prop_layer_drift_classify_consistent"
if cargo test -p corelink-billing-replay --test prop_billing_replay -- \
    prop_layer_diverged_flagged prop_drift_summary_lifted_canonical prop_layer_drift_classify_consistent --quiet \
    >/tmp/rb_billing_001_step4.log 2>&1; then
    emit "  -> PASS: layer-diverged classification + drift-summary lift canonical at 10k iter"
else
    emit "  -> FAIL: drift classification regression — block Finance close-of-month"
    exit 1
fi
emit ""

emit "Step 5 — Document outcome (≤ 1h p99)."
emit "  Audit emit corelink.billing_replay.executed via canonical S-09 audit"
emit "  chain (RFC 8785 JCS canonical determinism + BLAKE3-256 link hash)."
emit "  Stamp into compliance audit log (S-09 R2 audit bucket 7y Object Lock"
emit "  Governance Mode retention)."
emit "  Customer/auditor notification per RB-BILLING-001 §5: HTML + JSON"
emit "  diff report; embedded in Finance walkthrough exhibit."
emit "  Driving: cargo test -p corelink-billing-replay --test prop_billing_replay -- prop_chain_event_appended_per_replay prop_dry_run_no_state_mutation"
if cargo test -p corelink-billing-replay --test prop_billing_replay -- \
    prop_chain_event_appended_per_replay prop_dry_run_no_state_mutation --quiet \
    >/tmp/rb_billing_001_step5.log 2>&1; then
    emit "  -> PASS: per-replay chain extension + dry-run no-mutation canonical at 10k iter"
else
    emit "  -> FAIL: audit chain regression — STOP"
    exit 1
fi
emit ""

emit "Step 6 — SOC 2 CC1.4 control evidence (Finance walkthrough sign-off)."
emit "  Mock SOC 2 auditor reviews:"
emit "    a) Reconstructed invoice byte-equivalent to Stripe invoice (modulo metadata)"
emit "    b) Replay endpoint role-protected (CTRL-AUTHZ-002 billing_forensics_admin)"
emit "    c) Audit chain integrity preserved (per-replay event canonical INV-OBS-AUDIT-CHAIN-INTEGRITY)"
emit "    d) PII redaction at Replay output boundary (Lote 10.9-quinquies NEW-P0-2"
emit "       typed payload — NO raw PII at trait surface)"
emit "    e) 7-year retention (R2 Object Lock Governance Mode + D1 billing_replay_audit"
emit "       PRIMARY KEY (request_id) UNIQUE)"
emit "  Auditor signs sign-off section in specs/04_sprints/S10/finance-walkthrough.md."
emit "  Finance Officer signs sign-off section confirming reconstruction < 30min SLA met."
emit "  Driving: cargo test -p corelink-billing-replay --test prop_billing_replay -- prop_tenant_isolation"
if cargo test -p corelink-billing-replay --test prop_billing_replay -- \
    prop_tenant_isolation --quiet \
    >/tmp/rb_billing_001_step6.log 2>&1; then
    emit "  -> PASS: per-tenant replay isolation INV-TENANT-ISOLATION at 10k iter"
else
    emit "  -> FAIL: tenant isolation regression — block Finance walkthrough sign-off"
    exit 1
fi
emit ""

emit "Step 7 — Post-incident (if diff > threshold)."
emit "  Post-mortem within 7d. 5-Why focused on:"
emit "    - Layer 1 (events R2 ↔ counters D1) drift?"
emit "    - Layer 2 (counters D1 ↔ invoice_line_item D1) drift?"
emit "    - Layer 3 (invoice_line_item D1 ↔ Stripe) drift?"
emit "  Reconciliation worker review (corelink-billing-reconcile dual-condition"
emit "  auto-fix gate calibration: count ≤ 5 AND pct ≤ 0.0001)."
emit "  SOC 2 audit log update + ANPD/DPC notification per LGPD Art. 16 if PII"
emit "  material affected."
emit "  -> SIMULATED: post-mortem template + regression-test harness ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-BILLING-001.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Pré-condições" "Quando usar" "Procedure" "Outputs" "Acceptance criteria" "Escalation" "Recovery / Rollback" "Post-incident" "Evidence" "References")
DRIFT=0
for header in "${EXPECTED_HEADERS[@]}"; do
    if ! grep -q "^## ${header}" "$RUNBOOK_FILE"; then
        emit "  -> DRIFT: runbook is missing header '${header}'"
        DRIFT=1
    fi
done
if [[ $DRIFT -eq 0 ]]; then
    emit "  -> PASS: every expected runbook header present (no drift)"
else
    emit "  -> FAIL: runbook drift detected; harness MUST be updated together"
    exit 1
fi

emit ""
emit "== Finance walkthrough exhibit summary =="
emit "  role authz + audit-per-decision-arm prop                                : 0 unauthorized leakage"
emit "  replay determinism + idempotent re-fire (request_id PK) prop            : 0 divergence"
emit "  layer-diverged classification + drift-summary lift prop                 : 0 misroute"
emit "  per-replay chain extension + dry-run no-mutation prop                   : 0 chain break"
emit "  per-tenant replay isolation INV-TENANT-ISOLATION prop                   : 0 cross-tenant"
emit "  runbook drift                                                           : 0 (all expected headers present)"
emit "  staging Finance walkthrough (paid mock SOC 2 auditor + < 30min p99 SLA) : DEFERRED until staging account provisioned + auditor scheduled"
emit ""
emit "== RB-BILLING-001 host-side dry-run COMPLETE (Finance walkthrough exhibit ready) =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
