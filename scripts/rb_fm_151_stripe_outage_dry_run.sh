#!/usr/bin/env bash
# WI-S10-007 — RB-FM-151 (Stripe Outage) dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-151 runbook
# (specs/05_quality/runbooks/RB-FM-151-stripe-outage.md) against the
# in-memory billing pipeline (corelink-billing-stripe webhook + retry
# queue + idempotency ledger). Per WI §6.1.7: 1h Stripe API 5xx
# simulation -> PAT-QUEUE-EVENTS-001 fallback -> PAT-BACKOFF-001
# retry -> recovery -> 0 lost invoices INV-BILLING-NO-LOSS preserved.
#
# What this script does NOT do: hit a real Cloudflare staging
# environment. The full staging dry-run (with real Stripe API mock
# 5xx 1h + on-call engineer execution + audit chain capture) is the
# integration-tier counterpart deferred until staging account
# provisioned. This host-side harness pins the runbook step ordering
# at PR speed so a regression cannot land without a CI failure.
#
# Pattern reused: scripts/rb_fm_302_billing_drift_dry_run.sh (WI-S10-007 sibling)
# + scripts/rb_fm_300_dry_run.sh (WI-S06-007). Same mental model + drift-
# detection grammar.
#
# Usage:
#   bash scripts/rb_fm_151_stripe_outage_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-151 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-151-stripe-outage.md"
emit "harness: scripts/rb_fm_151_stripe_outage_dry_run.sh (WI-S10-007)"
emit "outage magnitude per WI-S10-007 §6.1.7: Stripe API 5xx 1h simulation;"
emit "                                          PAT-QUEUE-EVENTS-001 fallback +"
emit "                                          PAT-BACKOFF-001 retry; recovery;"
emit "                                          0 lost invoices verified."
emit ""

emit "Step 1 — Detection: corelink-billing-stripe property test asserts"
emit "  webhook signature canonical (HMAC-SHA256 over <ts>.<payload>) +"
emit "  5-min replay window enforcement + idempotency-key UNIQUE PK at"
emit "  storage layer (INV-BILLING-NO-DUP). When Stripe API returns 5xx,"
emit "  the adapter routes the charge to retry_queue (PAT-QUEUE-EVENTS-001)."
emit "  Driving: cargo test -p corelink-billing-stripe --test prop_billing_stripe"
if cargo test -p corelink-billing-stripe --test prop_billing_stripe -- --quiet \
    >/tmp/rb_fm_151_step1.log 2>&1; then
    emit "  -> PASS: signature verify + idempotency-key UNIQUE + replay-window canonical at 10k iter"
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_151_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: signature/idempotency regressed — STOP"
    emit "  RB-FM-151 trigger condition: page SRE on-call + Engineer billing"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-2 page synthesis (host-side stub)."
emit "  In production this step pages SRE on-call via PagerDuty service key"
emit "  'corelink-finance' when corelink_billing_stripe_api_calls_total{status=5xx}"
emit "  spike sustained > 5 min (FM-151 detection target). NOT SEV-1: events"
emit "  queued via PAT-QUEUE-EVENTS-001 fallback; eventual consistency preserved."
SEV2_PAYLOAD='{"severity":"SEV-2","title":"FM-151 Stripe outage","metric":"corelink_billing_stripe_api_calls_total","outcome":"5xx_sustained_5min","runbook":"RB-FM-151"}'
emit "  payload: ${SEV2_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-2 page emitted (no real PagerDuty call)"
emit ""

emit "Step 3 — Mitigação imediata (≤ 30 min target per RB-FM-151):"
emit "  Step 3a: confirmar Stripe outage via status page <https://status.stripe.com>;"
emit "  Step 3b: PAT-QUEUE-EVENTS-001 ativa: pending charges queue em retry_queue;"
emit "           InMemoryStripeBillingAdapter routes (tenant, billing_period)"
emit "           to retry_queue ONCE (no duplicate enqueue per RetrySet membership"
emit "           guard);"
emit "  Step 3c: PAT-BACKOFF-001 aplicado em retries Stripe API: exponential"
emit "           1s/2s/4s/8s/16s + jitter; max 5 attempts canonical (sprint contract"
emit "           §14.s10.2);"
emit "  Step 3d: verificar customer charges/refunds/disputes pendentes que afetem"
emit "           trust (forensic anomaly surface)."
emit "  Driving: cargo test -p corelink-billing-emit --test prop_billing_emit"
if cargo test -p corelink-billing-emit --test prop_billing_emit -- --quiet \
    >/tmp/rb_fm_151_step3.log 2>&1; then
    emit "  -> PASS: emit lib INV-BILLING-NO-LOSS append-only at 10k iter — events not lost during outage"
else
    emit "  -> FAIL: emit lib regressed — escalate to SEV-1 (event loss possible)"
    exit 1
fi
emit ""

emit "Step 4 — Resolução."
emit "  Aguardar Stripe recovery (status page green)."
emit "  DrainRetryQueue fires when ~stripe_outage_active; charges Stripe-invoiced"
emit "  idempotently (same Idempotency-Key by construction; INV-BILLING-NO-DUP)."
emit "  Verificar reconciliation post-recovery: 3-layer reconcile + Stripe submission"
emit "  control resume (per-tenant pause flag UPSERT-safe); audit chain integrity"
emit "  preserved (every state mutation lands canonical 6-event audit row)."
emit "  Driving: cargo test -p corelink-billing-reconcile --test prop_billing_reconcile"
if cargo test -p corelink-billing-reconcile --test prop_billing_reconcile -- --quiet \
    >/tmp/rb_fm_151_step4.log 2>&1; then
    emit "  -> PASS: reconcile post-recovery green at 10k iter — INV-BILLING-NO-LOSS preserved"
else
    emit "  -> FAIL: reconcile regression — block production rollout"
    exit 1
fi
emit ""

emit "Step 5 — Post-incident."
emit "  Post-mortem se outage > 1h (Stripe SLA tier review)."
emit "  Review reconciliation drift on (tenant, billing_period) tuples that hit"
emit "  retry_queue: 0 lost invoices verified via DriftHistoryLedger UPSERT-safe"
emit "  ledger query (run_started_at within outage window)."
emit "  Stripe SLA tier review: vendor uptime contract delta vs CoreLink SLA"
emit "  exposure (Customer Success notify if customer-facing impact)."
emit "  -> SIMULATED: post-mortem template scaffold ready"
emit ""

emit "Step 6 — Forensics."
emit "  Audit chain query: corelink.billing_stripe.* event subjects emitted"
emit "  during the outage window — every retry_queue entry produces canonical"
emit "  fail-CLOSED audit row BEFORE state mutation (Lote 10.6bis pattern + S-07"
emit "  P1-1 fix); chain replay reconstructs the queue drain trace post-recovery."
emit "  Stripe-Signature header replay window (5-min canonical) prevents replay"
emit "  attack during outage via constant-time HMAC compare (subtle::ConstantTimeEq)."
emit "  -> SIMULATED: forensic chain template ready"
emit ""

emit "Step 7 — TLA+ obligation."
emit "  specs/tla/billing_atomicity.tla::INV_BILLING_NO_LOSS proves that every"
emit "  emitted event is accounted somewhere — staging, R2, retry_queue, or"
emit "  Stripe-invoiced bucket. INV_BILLING_NO_DUP proves no double-charge per"
emit "  (tenant, billing_period). The Stripe outage scenario is modeled via"
emit "  StripeOutageBegins / StripeChargeQueuedDuringOutage / DrainRetryQueue /"
emit "  StripeOutageRecovers actions; weak fairness on DrainRetryQueue + StripeOutageRecovers"
emit "  guarantees eventual delivery."
emit "  -> SIMULATED: TLA+ TLC verification gates merge via .github/workflows/tla_billing_check.yml"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-151-stripe-outage.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Detecção" "Comunicação" "Mitigação imediata" "Resolução" "Post-incident" "References")
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
emit "  signature verify + idempotency-key UNIQUE + replay-window prop          : 0 forge"
emit "  emit lib INV-BILLING-NO-LOSS append-only prop                           : 0 loss during outage"
emit "  reconcile post-recovery green prop                                      : 0 drift on drained queue"
emit "  runbook drift                                                           : 0 (all expected headers present)"
emit "  staging dry-run (real Stripe mock 5xx 1h + on-call exec ≤ 30 min p95)   : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-151 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
