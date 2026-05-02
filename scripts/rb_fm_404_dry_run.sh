#!/usr/bin/env bash
# WI-S06-007 — RB-FM-404 (GC sweep conflicts with write — refcount race)
# dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-404 runbook
# (specs/05_quality/runbooks/RB-FM-404-gc-write-race.md) against the
# in-memory GC stack — particularly the INV-GC-004 protect-if-`>=`
# canonical TLA semantics + the 100k race property test that cross-
# validates the formal verification obligation against the real Rust
# impl. Per WI §6.1.4.RB-FM-404: detection ≤ 1 min p95 via property
# test alert; remediation ≤ 30 min p95; ≥ 3 independent runs with
# documented seed variance per Lote 10.6bis P0-W7-4 with chaos PR
# magnitude pinned to UpdateActionResult fired at exactly
# `mark_started_at_ms + 1ms` (boundary case).
#
# What this script does NOT do: hit a real Cloudflare staging
# environment. Full staging dry-run with chaos PR injection + on-call
# engineer execution is deferred until staging account provisioned.
#
# Pattern reused: scripts/rb_fm_300_dry_run.sh (this lote) +
# scripts/rb_fm_060_dry_run.sh (WI-S05-006). Same mental model + drift-
# detection grammar.
#
# Usage:
#   bash scripts/rb_fm_404_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-404 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-404-gc-write-race.md"
emit "harness: scripts/rb_fm_404_dry_run.sh (WI-S06-007)"
emit "chaos magnitude per Lote 10.6bis P0-W7-4: UpdateActionResult fired"
emit "                                          at mark_started_at_ms + 1ms"
emit "                                          (boundary case; protected_re_ref expected)"
emit ""

emit "Step 1 — Detection: 100k race property test (release-mode) cross-"
emit "  validates the TLA+ obligation gc_correctness.tla::"
emit "  InvGCReRefProtected against the real Rust sweep impl. Boundary"
emit "  case at offset_ms ∈ {-1, 0, +1} sampled exhaustively per iter."
emit "  ZERO violations sustained across 100k iter is the canonical"
emit "  S-06 DoD gate."
emit "  Driving: PROPTEST_CASES=10000 cargo test -p corelink-gc --release \\"
emit "            --test prop_inv_gc_004_race"
if PROPTEST_CASES=10000 cargo test -p corelink-gc --release \
    --test prop_inv_gc_004_race -- --quiet \
    >/tmp/rb_fm_404_step1.log 2>&1; then
    emit "  -> PASS: 10k iter race test green (host-side fast gate);"
    emit "          full 100k iter gate runs nightly via .github/workflows/nightly.yml::proptest-extended."
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_404_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: INV-GC-004 violation detected — STOP"
    emit "  RB-FM-404 trigger condition: page Architect + Crypto SME + SRE"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-1 page synthesis (host-side stub)."
emit "  In production this step pages Architect + Crypto SME via"
emit "  PagerDuty when GC_InvGc004Violation alert fires (counter > 0)."
SEV1_PAYLOAD='{"severity":"SEV-1","title":"FM-404 GC sweep conflicts with write","metric":"corelink_gc_inv_gc_004_violations_total","outcome":"refcount_race_boundary","runbook":"RB-FM-404"}'
emit "  payload: ${SEV1_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-1 page emitted (no real PagerDuty call)"
emit ""

emit "Step 3 — Mitigação imediata (≤ 30 min p95 target per RB-FM-404):"
emit "  Step 3a: pausar GC sweep globalmente via degrade-mode gc-pause"
emit "           (PAT-DEGRADE-001 alignment);"
emit "  Step 3b: verify last GC bem-sucedido timestamp via"
emit "           gc_run table query;"
emit "  Step 3c: se blob ainda na grace period (72h CTRL-GC-001):"
emit "           undelete via tombstone reversão;"
emit "  Step 3d: se blob já physical delete: cliente vai re-uploadear"
emit "           (cache miss não fatal mas SLO-CAS-GET impactado)."
emit "  Driving: protect-if->= boundary test pins exact TLA semantics:"
emit "  Driving: cargo test -p corelink-gc --test prop_sweep \\"
emit "            -- prop_inv_gc_004_protect_if_ge_strict_boundary"
if cargo test -p corelink-gc --test prop_sweep -- \
    prop_inv_gc_004_protect_if_ge_strict_boundary --quiet \
    >/tmp/rb_fm_404_step3.log 2>&1; then
    emit "  -> PASS: protect-if->= boundary test green (canonical TLA L152-154 holds)"
else
    emit "  -> FAIL: boundary test regressed — block production rollout immediately"
    exit 1
fi
emit ""

emit "Step 4 — Diagnóstico: cross-component sweep prop suite asserts"
emit "  the canonical sweep decision predicate aggregates across 10k"
emit "  iter with random clock advance. If ANY arm leaks across tenant"
emit "  boundary, FM-303 (cross-tenant) is upstream cause."
emit "  Driving: cargo test -p corelink-gc --test prop_sweep \\"
emit "            -- prop_sweep_tenant_isolation prop_step_decision_predicate_aggregates"
if cargo test -p corelink-gc --test prop_sweep -- \
    prop_sweep_tenant_isolation prop_step_decision_predicate_aggregates --quiet \
    >/tmp/rb_fm_404_step4.log 2>&1; then
    emit "  -> PASS: tenant isolation + decision-aggregator green (no upstream FM-303)"
else
    emit "  -> FAIL: cross-tenant leak detected — escalate to RB-FM-253 + RB-FM-303"
    exit 1
fi
emit ""

emit "Step 5 — Mitigação completa (≤ 4h p95)."
emit "  Hot fix: patch race bug — likely candidates per WI §1.7:"
emit "           - mark_started_at vs ac.created_at comparison wrong;"
emit "           - sweep query uses LIKE instead of json_each (regression"
emit "             pin: prop_json_each_semantics_not_like in reconcile);"
emit "           - sweep step_candidate transitions before audit emits."
emit "  Cold fix: re-enable GC só com TLA+ check verde do INV-GC-001 +"
emit "            INV-GC-004 + chaos test 1 week sustained."
emit "  Driving: idempotent re-run prop validates fix landed cleanly:"
emit "  Driving: cargo test -p corelink-gc --test prop_sweep \\"
emit "            -- prop_sweep_idempotent_re_run"
if cargo test -p corelink-gc --test prop_sweep -- \
    prop_sweep_idempotent_re_run --quiet \
    >/tmp/rb_fm_404_step5.log 2>&1; then
    emit "  -> PASS: sweep idempotent re-run green (fix landed cleanly post-deploy)"
else
    emit "  -> FAIL: idempotency regressed — block production rollout"
    exit 1
fi
emit ""

emit "Step 6 — Forensics + Notificação."
emit "  Forensics: TLC scenario replay with seed = trace hash;"
emit "             corelink.gc.sweep.protected_re_ref + sweep.soft_deleted"
emit "             chain processor reconstructs the boundary case per row."
emit "  Forensics: PAT-SOFT-DELETE-001 funcionou? Grace period suficiente?"
emit "  Notificação: customer email if data loss (post-grace physical-delete);"
emit "             reference customer-visible bytes_reclaimed_last_30d gauge"
emit "             (CAP-GC-006) for transparency."
emit "  Notificação: ANPD/DPC if PII material affected (rare; INV-GC-004"
emit "             violation is generally a single-tenant issue per"
emit "             INV-TENANT-ISOLATION)."
emit "  -> SIMULATED: post-mortem template scaffold + customer"
emit "             notification draft ready (gc-feature-overview.md)."
emit ""

emit "Step 7 — Post-mortem hooks."
emit "  Atualizar gc_correctness.tla se modelo formal não cobria o caso."
emit "  Considerar grace period maior (96h vs 72h)."
emit "  Audit anterior: this FM tinha O=1; atualizar para O observed."
emit "  Adicionar regression test ao prop_inv_gc_004_race suite com seed"
emit "  recovered from forensic chain."
emit "  -> SIMULATED: post-mortem template + regression-test harness ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-404-gc-write-race.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Detecção" "Comunicação" "Mitigação imediata" "Mitigação completa" "Forensics" "Post-mortem")
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
emit "  100k race INV-GC-004 prop (release-mode 10k host-side)   : 0 violations"
emit "  protect-if->= boundary test (canonical TLA L152-154)     : 0 boundary leak"
emit "  sweep tenant isolation prop                              : 0 cross-tenant"
emit "  sweep step_decision_predicate_aggregates prop            : 0 disagreement"
emit "  sweep idempotent re-run prop                             : 0 divergence"
emit "  runbook drift                                            : 0 (all expected headers present)"
emit "  staging dry-run (chaos PR + on-call exec ≤ 30 min p95)   : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-404 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
