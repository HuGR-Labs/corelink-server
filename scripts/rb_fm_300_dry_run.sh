#!/usr/bin/env bash
# WI-S06-007 — RB-FM-300 (GC refcount bug) dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-300 runbook
# (specs/05_quality/runbooks/RB-FM-300-gc-refcount-bug.md) against
# the in-memory GC stack (mark + sweep + physical-delete + reconcile +
# audit + metrics) so the runbook step ordering can be regression-tested
# in CI **before** the staging dry-run lands. Per WI §6.1.4.RB-FM-300:
# detection ≤ 5 min p95 via reconcile SEV-2 alert; remediation ≤ 30 min
# p95; customer notification ≤ 1h p95; ≥ 3 independent runs with
# documented seed variance per Lote 10.6bis P0-W7-4.
#
# What this script does NOT do: hit a real Cloudflare staging
# environment. The full staging dry-run (with PagerDuty synthetic +
# audit chain capture + chaos PR introducing 0.5% per-tenant refcount
# drift + on-call engineer execution) is the integration-tier counter-
# part deferred until staging account provisioned. This host-side harness
# pins the runbook step ordering at PR speed so a regression cannot
# land without a CI failure.
#
# Pattern reused: scripts/rb_fm_060_dry_run.sh (WI-S05-006) +
# scripts/rb_fm_303_dry_run.sh (WI-S04-006) +
# scripts/rb_fm_160_dry_run.sh (WI-S03-008) +
# scripts/rb_fm_253_dry_run.sh (WI-S02-006). Same mental model + drift-
# detection grammar.
#
# Usage:
#   bash scripts/rb_fm_300_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-300 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-300-gc-refcount-bug.md"
emit "harness: scripts/rb_fm_300_dry_run.sh (WI-S06-007)"
emit "chaos magnitude per Lote 10.6bis P0-W7-4: refcount drift = 0.5% per-tenant"
emit "                                          (above SEV-2 0.1%, below SEV-1 1%)"
emit ""

emit "Step 1 — Detection: reconcile property test asserts the canonical"
emit "  json_each JSON-aware membership idiom catches drift > 0 with"
emit "  zero false-positives (substring collisions structurally"
emit "  impossible). Also pins the SEV-2 / SEV-1 threshold gate."
emit "  Driving: cargo test -p corelink-gc --test prop_reconcile"
emit "            -- prop_json_each_semantics_not_like prop_no_drift_no_mutation"
if cargo test -p corelink-gc --test prop_reconcile -- \
    prop_json_each_semantics_not_like prop_no_drift_no_mutation --quiet \
    >/tmp/rb_fm_300_step1.log 2>&1; then
    emit "  -> PASS: json_each canonical idiom holds; drift detection canonical."
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_300_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: drift detection regressed — STOP"
    emit "  RB-FM-300 trigger condition: page SRE + Architect"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-2 page synthesis (host-side stub)."
emit "  In production this step pages SRE on-call via PagerDuty when"
emit "  the GC_RefcountDriftGlobalHigh alert fires (global drift > 0.1%"
emit "  sustained 1h) OR GC_RefcountDriftPerTenantHigh fires (per-tenant"
emit "  drift > 1% sustained 1h)."
SEV2_PAYLOAD='{"severity":"SEV-2","title":"FM-300 GC refcount drift","metric":"corelink_gc_refcount_drift_percent","outcome":"drift_0.5%_per_tenant","runbook":"RB-FM-300"}'
emit "  payload: ${SEV2_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-2 page emitted (no real PagerDuty call)"
emit ""

emit "Step 3 — Mitigação imediata (≤ 30 min p95 target per RB-FM-300):"
emit "  Step 3a: pausar GC sweep globalmente via degrade-mode"
emit "           gc-pause (PAT-DEGRADE-001 alignment);"
emit "  Step 3b: identificar universo: query D1 reconcile audit chain"
emit "           via corelink.gc.reconcile.refcount_manual_review_required"
emit "           events emitted in the last hour;"
emit "  Step 3c: undelete via tombstone reversão se ainda no grace"
emit "           period (72h CAS / 24h AC) per WI-S06-003 sweep CAP-GC-002"
emit "           reversibility seam BlobMetaStore::undelete."
emit "  Driving: cargo test -p corelink-gc --test prop_sweep \\"
emit "            -- prop_soft_delete_reversible"
if cargo test -p corelink-gc --test prop_sweep -- \
    prop_soft_delete_reversible --quiet \
    >/tmp/rb_fm_300_step3.log 2>&1; then
    emit "  -> PASS: soft-delete reversibility holds (CAP-GC-002 round-trip)"
else
    emit "  -> FAIL: undelete path regressed — escalate to SEV-1"
    exit 1
fi
emit ""

emit "Step 4 — Diagnóstico: 100k race property test asserts INV-GC-004"
emit "  (mark-phase-aware re-ref protection) holds at PR speed; if"
emit "  refcount drift correlates with race signature, FM-404 is the"
emit "  upstream cause and RB-FM-404 escalation applies."
emit "  Driving: cargo test -p corelink-gc --test prop_inv_gc_004_race \\"
emit "            -- --quiet"
if cargo test -p corelink-gc --release --test prop_inv_gc_004_race -- --quiet \
    >/tmp/rb_fm_300_step4.log 2>&1; then
    emit "  -> PASS: INV-GC-004 race test green (no upstream FM-404 signature)"
else
    emit "  -> FAIL: INV-GC-004 violation detected — escalate to RB-FM-404"
    exit 1
fi
emit ""

emit "Step 5 — Mitigação completa (≤ 4h p95)."
emit "  Hot fix: reconcile auto-fix path runs with dual-condition gate"
emit "           (count ≤ 5 AND percent ≤ 0.01% per Lote 10.6bis P0-6);"
emit "           larger drifts paused for manual review via"
emit "           refcount_manual_review_required audit event."
emit "  Cold fix:"
emit "    - patch refcount-write path bug (sweep / UpdateAR / DeleteAR);"
emit "    - re-enable GC só com TLA+ CI green + 100k race test green +"
emit "      chaos test 1 week sustained sem violation."
emit "  Driving: cargo test -p corelink-gc --test prop_reconcile \\"
emit "            -- prop_auto_fix_bounded_dual_condition prop_idempotent_re_run"
if cargo test -p corelink-gc --test prop_reconcile -- \
    prop_auto_fix_bounded_dual_condition prop_idempotent_re_run --quiet \
    >/tmp/rb_fm_300_step5.log 2>&1; then
    emit "  -> PASS: dual-condition gate holds + reconcile idempotent"
else
    emit "  -> FAIL: auto-fix gate regressed — block production rollout"
    exit 1
fi
emit ""

emit "Step 6 — Forensics + Notificação (≤ 1h p95 customer notif)."
emit "  Forensics: TLA+ scenario replay via TLC nightly (4h timeout);"
emit "             corelink.gc.reconcile.refcount_manual_review_required"
emit "             chain processor reconstructs the drift trail per row."
emit "  Notificação: customer email per RB template if data loss"
emit "             unrecoverable (post-grace physical-delete + R2"
emit "             versioning exhausted)."
emit "  Notificação: ANPD/DPC notification se PII material affected"
emit "             (LGPD Art. 16 retention compliance review)."
emit "  -> SIMULATED: post-mortem template scaffold + customer"
emit "             notification draft ready (gc-feature-overview.md)."
emit ""

emit "Step 7 — Post-mortem hooks."
emit "  TLA+ spec gc_correctness.tla precisa cobrir o cenário que foi"
emit "  violado se modelo formal não capturou."
emit "  Adicionar regression test ao prop_reconcile suite + grace"
emit "  period re-avaliado (72h mantido ou estendido para 96h?)."
emit "  Code review de GC: quem revisou PR que introduziu bug?"
emit "  -> SIMULATED: post-mortem template + regression-test harness ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-300-gc-refcount-bug.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Detecção" "Comunicação" "Mitigação imediata" "Mitigação completa" "Forensics" "Post-mortem obrigatório" "Prevenção")
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
emit "  reconcile json_each canonical idiom + no-drift prop      : 0 false drift"
emit "  soft-delete reversibility CAP-GC-002 prop                : 0 round-trip miss"
emit "  100k race INV-GC-004 prop (release-mode)                 : 0 violations"
emit "  reconcile dual-condition auto-fix gate prop              : 0 boundary leak"
emit "  reconcile idempotent re-run prop                         : 0 divergence"
emit "  runbook drift                                            : 0 (all expected headers present)"
emit "  staging dry-run (chaos PR + on-call exec ≤ 30 min p95)   : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-300 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
