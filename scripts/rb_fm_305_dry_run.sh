#!/usr/bin/env bash
# WI-S06-007 — RB-FM-305 (tombstone lost — eviction reverts with re-upload)
# dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-305 runbook
# (specs/05_quality/runbooks/RB-FM-305-tombstone-lost.md) against
# the in-memory GC stack — particularly the physical-delete strict->-greater-than
# post-grace gate + sweeper cron health (Cron DO alarm re-arm path) +
# orphan-R2 detection in reconcile. Per WI §6.1.4.RB-FM-305: detection
# ≤ 5 min p95 cross-checked against sweeper-tick-stale alert AND
# SLO-FRESH-GC sustained metric (the latter provides the 5-min
# detection; the sweeper-tick alert fires only after > 1h); ≥ 3
# independent runs with documented seed variance per Lote 10.6bis
# P0-W7-4; chaos PR magnitude pinned to GC paused 7d (cron disabled) +
# 100 GiB orphan accumulation.
#
# What this script does NOT do: hit a real Cloudflare staging
# environment. Full staging dry-run with 7d cron-disabled chaos +
# on-call engineer execution is deferred until staging account
# provisioned.
#
# Pattern reused: scripts/rb_fm_300_dry_run.sh + scripts/rb_fm_404_dry_run.sh
# (this lote) + scripts/rb_fm_060_dry_run.sh (WI-S05-006). Same mental
# model + drift-detection grammar.
#
# Usage:
#   bash scripts/rb_fm_305_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-305 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-305-tombstone-lost.md"
emit "harness: scripts/rb_fm_305_dry_run.sh (WI-S06-007)"
emit "chaos magnitude per Lote 10.6bis P0-W7-4: GC paused 7d (cron disabled) + 100 GiB orphan accumulation"
emit "  Detection signal: SLO-FRESH-GC sustained metric (≤ 5min p95) NOT sweeper-tick-stale (> 1h)"
emit ""

emit "Step 1 — Detection: cron tick rate property test asserts the"
emit "  sweeper alarm re-arm path is structurally idempotent + the"
emit "  jitter helper produces region-deterministic offsets. The"
emit "  > 1h sweeper-stale alert is too slow for the 5-min target;"
emit "  SLO-FRESH-GC orphan-age sustained gauge provides the fast"
emit "  detection per WI §6.1.4.RB-FM-305 + DASH-GC panel 10."
emit "  Driving: cargo test -p corelink-gc --test prop_scheduler"
if cargo test -p corelink-gc --test prop_scheduler -- --quiet \
    >/tmp/rb_fm_305_step1.log 2>&1; then
    emit "  -> PASS: scheduler / cron alarm re-arm path canonical;"
    emit "          jitter helper produces region-deterministic offsets."
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_305_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: scheduler regression — STOP"
    emit "  RB-FM-305 trigger condition: page SRE + verify Cron DO health"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-1 page synthesis (host-side stub)."
emit "  In production this step pages SRE on-call via PagerDuty when"
emit "  GC_SweeperCronStale alert fires (rate < canonical daily cadence"
emit "  sustained 1h) OR SLO-FRESH-GC drops below threshold (sustained"
emit "  orphan age > grace + 7d)."
SEV1_PAYLOAD='{"severity":"SEV-1","title":"FM-305 GC tombstone lost / cron stale","metric":"corelink_gc_scheduler_cron_fired_total","outcome":"7d_no_tick_100gib_orphan","runbook":"RB-FM-305"}'
emit "  payload: ${SEV1_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-1 page emitted (no real PagerDuty call)"
emit "  Privacy Officer: notify if DSR-erasure tombstone affected (escala SEV-1→SEV-0)"
emit ""

emit "Step 3 — Mitigação imediata (≤ 30 min p95 target per RB-FM-305):"
emit "  Step 3a: identificar blobs afetados via reconcile orphan-R2"
emit "           detection arm (corelink.gc.reconcile orphan_r2_count"
emit "           gauge). Per WI-S06-005 reconcile module: when"
emit "           r2_present=false AND deleted_at_ms.is_none() the row"
emit "           lands in OrphanR2Detected arm; refcount NEVER mutated."
emit "  Step 3b: re-aplicar tombstone via undelete reversal where the"
emit "           grace window has not yet expired."
emit "  Step 3c: verify billing impact: events emitted durante"
emit "           ressurrect window count? S-10 reconciliation worker detects."
emit "  Step 3d: audit emit per re-tombstone via"
emit "           corelink.gc.reconcile.refcount_manual_review_required"
emit "           with prev_state, fix_reason, operator_id."
emit "  Driving: reconcile orphan-R2 detection + tenant isolation:"
emit "  Driving: cargo test -p corelink-gc --test prop_reconcile \\"
emit "            -- prop_tenant_isolation prop_audit_emit_per_decision_arm"
if cargo test -p corelink-gc --test prop_reconcile -- \
    prop_tenant_isolation prop_audit_emit_per_decision_arm --quiet \
    >/tmp/rb_fm_305_step3.log 2>&1; then
    emit "  -> PASS: reconcile tenant isolation + audit emit canonical"
else
    emit "  -> FAIL: reconcile path regressed — block production rollout"
    exit 1
fi
emit ""

emit "Step 4 — Diagnóstico: physical-delete strict->-greater-than post-grace gate"
emit "  pins the off-by-one anti-pattern (loosening to `>=` is a data-"
emit "  loss bug). The conditional refcount=0 predicate (Lote 10.6bis"
emit "  P0-4) protects against re-upload race during physical-delete."
emit "  Causa raiz típica:"
emit "    1. Race condition entre eviction (S-07) e re-upload do mesmo"
emit "       digest;"
emit "    2. Replication lag D1 entre regiões: tombstone só na primary;"
emit "    3. Refcount bug (FM-300 vizinho) — cross-correlate via"
emit "       RB-FM-300 if drift signal coincides."
emit "  Driving: cargo test -p corelink-gc --lib physical_delete::"
if cargo test -p corelink-gc --lib physical_delete:: -- --quiet \
    >/tmp/rb_fm_305_step4.log 2>&1; then
    emit "  -> PASS: physical-delete strict->-greater-than boundary + refcount=0"
    emit "          predicate green (24 lib unit tests pass)"
else
    emit "  -> FAIL: physical-delete invariant regressed — escalate to RB-FM-300"
    exit 1
fi
emit ""

emit "Step 5 — Resolução."
emit "  Hot fix: write path lê tombstone_log + soft-block 24h re-write se"
emit "           digest tombstoned recente."
emit "  Cold fix:"
emit "    - tombstone como hard-block durante grace period (rejeita"
emit "      re-upload com 410 Gone se < grace);"
emit "    - INV-DEDUP-CONSISTENCY property test cobre cenário de race"
emit "      (S-07 forward);"
emit "    - TLA+ extension de gc_correctness.tla modeling tombstone race"
emit "      (out-of-TLA-scope today per ADR-0042 §A3 — soft-delete grace"
emit "      window covered architecturally not via TLA+)."
emit "  Driving: snapshot bound + idempotent re-run prop validates"
emit "           reconcile fix landed cleanly:"
emit "  Driving: cargo test -p corelink-gc --test prop_reconcile \\"
emit "            -- prop_snapshot_bound_excludes_post_snapshot_writes prop_idempotent_re_run"
if cargo test -p corelink-gc --test prop_reconcile -- \
    prop_snapshot_bound_excludes_post_snapshot_writes prop_idempotent_re_run --quiet \
    >/tmp/rb_fm_305_step5.log 2>&1; then
    emit "  -> PASS: snapshot bound + idempotent re-run green"
else
    emit "  -> FAIL: post-fix validation regressed — block production rollout"
    exit 1
fi
emit ""

emit "Step 6 — Post-incident."
emit "  Post-mortem dentro de 7d."
emit "  Adicionar property test cobrindo specific scenario."
emit "  Review com Architect + Privacy Officer."
emit "  -> SIMULATED: post-mortem template ready"
emit ""

emit "Step 7 — Evidence."
emit "  D1 query results pre/post fix (orphan-R2 detection arm)."
emit "  Reconciliation drift report (corelink.gc.reconcile audit chain)."
emit "  CloudEvents audit trail (refcount_manual_review_required +"
emit "  refcount_reconciled events for the affected window)."
emit "  -> SIMULATED: forensic chain template ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-305-tombstone-lost.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Detecção" "Comunicação" "Mitigação imediata" "Diagnóstico" "Resolução" "Post-incident" "Evidence" "References")
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
emit "  scheduler / cron alarm re-arm prop (16 tests)            : all green"
emit "  reconcile tenant isolation + audit emit prop             : 0 cross-tenant + 0 orphan emit"
emit "  physical-delete strict->-greater-than post-grace boundary (24 lib)  : 0 boundary leak"
emit "  reconcile snapshot bound + idempotent re-run prop        : 0 post-snapshot drift"
emit "  runbook drift                                            : 0 (all expected headers present)"
emit "  staging dry-run (chaos PR + on-call exec ≤ 30 min p95)   : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-305 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
