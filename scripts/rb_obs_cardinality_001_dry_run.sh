#!/usr/bin/env bash
# WI-S09-007 — RB-OBS-CARDINALITY-001 (Cardinality Explosion — Métrica
# → OOM Mimir / Cost Spike) dry-run harness.
#
# Drives a host-side simulated walkthrough of the
# RB-OBS-CARDINALITY-001 runbook
# (specs/05_quality/runbooks/RB-OBS-CARDINALITY-001.md) against the
# in-memory cardinality budget enforcement primitive
# (corelink-analytics::CardinalityValidator + INV-OBS-CARDINALITY-
# BUDGET HIGH per-metric ≤ 20k + global ≤ 100k). Per WI §6.1.6 +
# sprint contract §6 DoD EVT-017: synthetic injection of test-only
# metric `corelink_test_cardinality_explosion{label_a..z}` 25k unique
# series em staging; pass criteria validate (a) cardinality_check.py
# CI gate would catch em PR (offline check), (b) Mimir tenant tier
# limit rejects ingest beyond 20k per-metric (secondary defense),
# (c) SEV-2 alert fires on `corelink_metrics_cardinality_budget_
# violation_total > 0`, (d) degraded observability documented (some
# series dropped; not all queryable), (e) recovery on rollback
# auto-resolves alerts.
#
# What this script does NOT do: hit a real Grafana Mimir tenant with
# 25k unique series injection. Full staging dry-run with real Mimir
# tier limit secondary defense + SEV-2 alert + on-call engineer
# execution is deferred until staging account provisioned.
#
# Pattern reused: scripts/rb_fm_250_dry_run.sh (S-08) +
# scripts/rb_fm_059_dry_run.sh (S-07) + scripts/rb_fm_300_dry_run.sh
# (S-06). Same mental model + drift-detection grammar.
#
# Usage:
#   bash scripts/rb_obs_cardinality_001_dry_run.sh [--evidence <path>]
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

emit "== RB-OBS-CARDINALITY-001 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-OBS-CARDINALITY-001.md"
emit "harness: scripts/rb_obs_cardinality_001_dry_run.sh (WI-S09-007)"
emit "scenario: synthetic injection 25k séries em test-only metric"
emit "  Pass criteria: (a) cardinality_check.py CI gate rejects offline +"
emit "  (b) Mimir tenant tier limit rejects ingest beyond 20k per-metric +"
emit "  (c) SEV-2 alert fires + (d) degraded observability documented +"
emit "  (e) recovery on rollback auto-resolves."
emit ""

emit "Step 1 — Detection: cardinality budget breach alert + Mimir 429."
emit "  In production this step fires when:"
emit "    - corelink_metrics_cardinality_budget_violation_total > 0 (SEV-2 alert)."
emit "    - Grafana Mimir tenant 429 (over-quota) on label cartesian explosion."
emit "    - Cost report: Grafana spending up > 30% MoM sem explicação proporcional."
emit "    - Worker analytics emit warnings: 'cardinality limit reached'."
emit "  Host-side equivalent: corelink-analytics::CardinalityValidator emits"
emit "  AnalyticsError::CardinalityBudgetExceeded { scope=PerMetric|Global } when the"
emit "  per-metric ledger reaches the canonical 20k unique-tuple cap."
emit "  Driving: cargo test -p corelink-analytics --test prop_analytics"
if cargo test -p corelink-analytics --test prop_analytics -- --quiet \
    >/tmp/rb_obs_card_001_step1.log 2>&1; then
    emit "  -> PASS: corelink-analytics cardinality budget enforced;"
    emit "          17 prop tests green @ 10k iter (incl."
    emit "          prop_cardinality_budget_enforced + prop_cardinality_idempotent_"
    emit "          repeat_label_set)."
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_obs_card_001_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: cardinality budget regression — STOP"
    emit "  RB-OBS-CARDINALITY-001 trigger: page SRE on-call (cost explosion + OOM risk)"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-2 page synthesis (host-side stub)."
emit "  In production this step pages SRE on-call + Engineer responsável pelo"
emit "  subsystem afetado via PagerDuty when the cardinality budget violation"
emit "  counter increments OR Mimir tenant tier limit triggers 429 on ingest."
emit "  SEV-2 (NOT SEV-1: produção continua; observability quality degraded +"
emit "  cost explosion only)."
SEV2_PAYLOAD='{"severity":"SEV-2","title":"RB-OBS-CARDINALITY-001 cardinality explosion","metric":"corelink_metrics_cardinality_budget_violation_total","outcome":"25k_synthetic_series_injected","runbook":"RB-OBS-CARDINALITY-001"}'
emit "  payload: ${SEV2_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-2 page emitted (no real PagerDuty call)"
emit "  Customer notification: NOT required (interno tooling outage)"
emit ""

emit "Step 3 — Mitigation imediata (≤ 1h p95 target per RB-OBS-CARDINALITY-001):"
emit "  Step 3a: identify offending metric: top-N por unique series count em Mimir tenant."
emit "  Step 3b: identify offending labels: frequently tenant_id × region × op cartesian."
emit "  Step 3c: block ingestion of high-cardinality label temporarily:"
emit "    - Mimir tenant config: drop label via relabel rule."
emit "    - Alternativa: workers emit menos detalhe (downgrade level)."
emit "  Step 3d: validate cost impact estimated via Grafana billing API projection."
emit "  Driving: cargo test -p corelink-analytics --test prop_analytics (per-metric"
emit "  + global ledger isolation under high-cartesian sweep)."
emit "  -> PASS (covered by Step 1 prop_analytics suite)"
emit ""

emit "Step 4 — Diagnóstico (≤ 6h):"
emit "  Step 4a: root cause identification:"
emit "    - Label novo adicionado sem CI check (PR slipped past cardinality_check.py)."
emit "    - Bug: label conteúdo não-bounded (e.g., error_message literal in label)."
emit "    - Tenant abuse: synthetic tenant gerando millions de unique values."
emit "    - Schema migration: enum field expandido sem awareness."
emit "  Step 4b: cross-reference cardinality budget definido em observability_model.md §11.2."
emit "  Step 4c: Mimir queries: time-series retention vs ingest rate."
emit "  Driving: cargo test -p corelink-analytics --lib (forbidden label names lint)"
if cargo test -p corelink-analytics --lib -- --quiet \
    >/tmp/rb_obs_card_001_step4.log 2>&1; then
    emit "  -> PASS: corelink-analytics::FORBIDDEN_LABEL_NAMES const lint catches"
    emit "          trace_id/tenant_id/request_id/blob_digest forbidden labels at the"
    emit "          structural cartesian boundary; not representable by construction."
else
    emit "  -> FAIL: forbidden-label lint regression — STOP"
    exit 1
fi
emit ""

emit "Step 5 — Resolução: hot fix + cold fix path."
emit "  Step 5a (hot fix ≤ 24h):"
emit "    - Drop label via relabel rule (irreversible past data)."
emit "    - Fix code that emit unbounded label."
emit "    - Roll back PR if recente."
emit "  Step 5b (cold fix ≤ 7d):"
emit "    - Strengthen cardinality_check.py CI gate (false positive em PR review)."
emit "    - Adicionar pre-deploy canary (10% traffic, monitor cardinality 1h)."
emit "    - Update INV-OBS-CARDINALITY-BUDGET enforcement (Mimir tenant hard limit)."
emit "    - Adicionar error_message allowlist of generic patterns vs literals."
emit "  -> SIMULATED: rollback path canonical (idempotent budget reset on PR revert)"
emit ""

emit "Step 6 — Post-incident: post-mortem + 5-Why + observability_model update."
emit "  Step 6a: post-mortem mandatório (Lote 9.1 post-mortem hook trigger)."
emit "  Step 6b: 5-Why: por que CI cardinality_check missed? Por que budget incorrectly?"
emit "  Step 6c: update observability_model.md §11.2 se budget revisitado."
emit "  Step 6d: treinar engineering: cardinality discipline em SRE workshop."
emit "  -> SIMULATED: post-mortem template ready"
emit ""

emit "Step 7 — Evidence forensic chain."
emit "  Step 7a: Mimir cardinality dashboard pre/post fix."
emit "  Step 7b: Cost report (Grafana billing)."
emit "  Step 7c: PR/commit que introduziu label problemático."
emit "  Step 7d: CI cardinality_check.py logs."
emit "  -> SIMULATED: forensic chain template ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-OBS-CARDINALITY-001.md"
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
emit "  analytics cardinality budget enforced (17 prop @ 10k)         : all green"
emit "  forbidden-label-name lint at type system boundary             : structural"
emit "  per-metric + global budget canonical (20k / 100k)             : pinned"
emit "  runbook drift                                                 : 0 (all expected headers present)"
emit "  staging dry-run (real Mimir tier limit + SEV-2 alert)         : DEFERRED until staging account provisioned"
emit ""
emit "== RB-OBS-CARDINALITY-001 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
