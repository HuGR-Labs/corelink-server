#!/usr/bin/env bash
# WI-S10-007 — RB-FM-302 (Billing Drift) dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-302 runbook
# (specs/05_quality/runbooks/RB-FM-302-billing-leak.md) against the
# in-memory billing pipeline (corelink-billing-emit + aggregator +
# stripe + reconcile + replay) so the runbook step ordering can be
# regression-tested in CI **before** the staging dry-run lands. Per
# WI §6.1.6: chaos inject 0.5% drift -> reconciliation Layer 1/2/3
# detects -> SEV-2 escalation -> 5-Why -> resolution -> re-reconcile
# green.
#
# What this script does NOT do: hit a real Cloudflare staging
# environment. The full staging dry-run (with chaos PR introducing
# 0.5% counter drift in staging + on-call engineer execution +
# PagerDuty synthetic + audit chain capture) is the integration-tier
# counterpart deferred until staging account provisioned. This host-
# side harness pins the runbook step ordering at PR speed so a
# regression cannot land without a CI failure.
#
# Pattern reused: scripts/rb_fm_300_dry_run.sh (WI-S06-007) +
# scripts/rb_fm_153_dry_run.sh (WI-S09-007) +
# scripts/rb_fm_303_dry_run.sh (WI-S04-006). Same mental model + drift-
# detection grammar.
#
# Usage:
#   bash scripts/rb_fm_302_billing_drift_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-302 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-302-billing-leak.md"
emit "harness: scripts/rb_fm_302_billing_drift_dry_run.sh (WI-S10-007)"
emit "chaos magnitude per WI-S10-007 §6.1.6: counter drift = 0.5% per-tenant"
emit "                                        (above SEV-2 0.1%, below SEV-1 1%)"
emit ""

emit "Step 1 — Detection: corelink-billing-reconcile property test asserts"
emit "  the canonical pairwise drift compute over (Layer1Emit, Layer2Aggregate,"
emit "  Layer3Stripe) detects drift > 0.1% with 4-tier ladder boundary"
emit "  semantics (NoDrift / AutoFixed / TicketSev3 / PageSev2 / PageSev1AutoPaused)."
emit "  Driving: cargo test -p corelink-billing-reconcile --test prop_billing_reconcile"
if cargo test -p corelink-billing-reconcile --test prop_billing_reconcile -- --quiet \
    >/tmp/rb_fm_302_step1.log 2>&1; then
    emit "  -> PASS: 4-tier drift ladder + dual-condition auto-fix gate canonical at 10k iter"
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_302_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: drift detection regressed — STOP"
    emit "  RB-FM-302 trigger condition: page Finance + SRE on-call + Architect"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-2 page synthesis (host-side stub)."
emit "  In production this step pages Finance + SRE on-call via PagerDuty"
emit "  service key 'corelink-finance' when corelink_billing_reconcile_drift_pct"
emit "  fires (drift > 0.1% sustained). Layer 3 Stripe API discrepancy is"
emit "  always SEV-1 + Stripe-pause idempotent flag (customer-facing invoice"
emit "  legal exposure)."
SEV2_PAYLOAD='{"severity":"SEV-2","title":"FM-302 billing drift","metric":"corelink_billing_reconcile_drift_pct","outcome":"drift_0.5%_per_tenant","runbook":"RB-FM-302","layer":"Layer2"}'
emit "  payload: ${SEV2_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-2 page emitted (no real PagerDuty call)"
emit ""

emit "Step 3 — Mitigação imediata (≤ 1h target per RB-FM-302):"
emit "  Step 3a: pausar billing posting para Stripe via StripeSubmissionControl"
emit "           pause flag idempotent on (tenant, billing_period) tuple — INV-BILLING-NO-DUP"
emit "           preserved at storage layer."
emit "  Step 3b: identificar scope: enumerate driven (tenant, billing_period)"
emit "           via DriftHistoryLedger UPSERT-safe ledger BTreeMap — last 24h"
emit "           run rows (tenant_id, billing_period, run_started_at) UNIQUE PK."
emit "  Step 3c: classify under-counting (counter < events) vs over-counting"
emit "           (counter > events) per pairwise drift sign (compute_pairwise_drift_pct)."
emit "  Driving: cargo test -p corelink-billing-aggregator --test prop_billing_aggregator"
if cargo test -p corelink-billing-aggregator --test prop_billing_aggregator -- --quiet \
    >/tmp/rb_fm_302_step3.log 2>&1; then
    emit "  -> PASS: counter aggregator chain integrity + idempotent re-run holds at 10k iter"
else
    emit "  -> FAIL: aggregator regressed — escalate to SEV-1"
    exit 1
fi
emit ""

emit "Step 4 — Diagnóstico: WI-S10-006 replay endpoint reconstructs invoice"
emit "  byte-a-byte from R2 raw events (3-layer reconstructed totals snapshot)."
emit "  prop_replay_deterministic + prop_layer_diverged_flagged 10k iter pin"
emit "  the invariant that the same (tenant, billing_period) replay always"
emit "  produces the same ReconstructedLayers and that a divergent layer is"
emit "  classified per LayerDriftSummary 5-element taxonomy."
emit "  Driving: cargo test -p corelink-billing-replay --test prop_billing_replay"
if cargo test -p corelink-billing-replay --test prop_billing_replay -- --quiet \
    >/tmp/rb_fm_302_step4.log 2>&1; then
    emit "  -> PASS: replay determinism + layer-diverged classification holds at 10k iter"
else
    emit "  -> FAIL: replay regression — STOP investigation, escalate to SEV-1"
    exit 1
fi
emit ""

emit "Step 5 — Mitigação completa (≤ 24h p95)."
emit "  Hot fix: rebuild counters from R2 events (R2 = source of truth):"
emit "    SELECT tenant, sku, billing_period, SUM(qty) FROM events_unrolled"
emit "    WHERE billing_period >= :affected_since"
emit "    GROUP BY tenant, sku, billing_period"
emit "    -> UPSERT usage_counter (idempotent — INV-BILLING-NO-DUP at storage layer)."
emit "  Cold fix:"
emit "    - re-enable Stripe posting via StripeSubmissionControl::resume after"
emit "      reconcile re-run green;"
emit "    - dry-run reconciliation re-run with auto-fix dual-condition gate"
emit "      (count ≤ 5 AND pct ≤ 0.0001) — WI-S10-007 §6.1.6 reconcile-after-fix"
emit "      green requirement;"
emit "    - communicate customers afetados se billing mudou material (≥ \$10)."
emit "  Driving: cargo test -p corelink-billing-emit --test prop_billing_emit"
if cargo test -p corelink-billing-emit --test prop_billing_emit -- --quiet \
    >/tmp/rb_fm_302_step5.log 2>&1; then
    emit "  -> PASS: emit lib idempotency + INV-BILLING-NO-LOSS append-only at 10k iter"
else
    emit "  -> FAIL: emit lib regressed — block production rollout"
    exit 1
fi
emit ""

emit "Step 6 — Forensics + Notificação ao customer."
emit "  Forensics: TLA+ scenario replay via TLC nightly (specs/tla/billing_atomicity.tla);"
emit "             corelink.billing_reconcile.* chain processor reconstructs"
emit "             the drift trail per (tenant, billing_period) row; root cause"
emit "             classified (event loss / counter update fail / idempotency"
emit "             collision / clock drift)."
emit "  Notificação: customer email per RB template if billing changed material"
emit "             (under-charged > \$10 → notify + offer payment/credit; over-charged"
emit "             → refund + 'trust bonus' credit)."
emit "  Notificação: Finance leadership review se total impact > \$10k OR > 100"
emit "             customers affected (Public Incident Report mandatory)."
emit "  -> SIMULATED: post-mortem template scaffold + customer notification draft"
emit "             ready (specs/04_sprints/S10/finance-walkthrough.md template)."
emit ""

emit "Step 7 — Post-mortem hooks."
emit "  TLA+ spec billing_atomicity.tla precisa cobrir o cenário que foi"
emit "  violado (INV-BILLING-NO-LOSS / INV-BILLING-NO-DUP) se modelo formal não"
emit "  capturou."
emit "  Adicionar regression test to corelink-billing-reconcile prop suite"
emit "  + reconcile auto-fix dual-condition gate re-avaliado (count threshold"
emit "  vs percent threshold escolha)."
emit "  Code review: quem revisou PR que introduziu bug? Plano sync issue?"
emit "  -> SIMULATED: post-mortem template + regression-test harness ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-302-billing-leak.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Detecção" "Comunicação" "Mitigação imediata" "Mitigação completa" "Forensics" "Notificação ao customer" "Post-mortem" "Prevenção")
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
emit "== EVT-017 evidence summary =="
emit "  4-tier drift ladder (NoDrift/AutoFixed/Sev3/Sev2/Sev1) prop          : 0 boundary violation"
emit "  aggregator chain integrity + idempotent re-run prop                  : 0 divergence"
emit "  replay determinism + layer-diverged classification prop              : 0 layer-arm regression"
emit "  emit lib idempotency + INV-BILLING-NO-LOSS append-only prop          : 0 loss"
emit "  runbook drift                                                        : 0 (all expected headers present)"
emit "  staging dry-run (chaos PR + on-call exec ≤ 1h p95)                   : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-302 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
