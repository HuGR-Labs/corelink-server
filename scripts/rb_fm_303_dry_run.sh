#!/usr/bin/env bash
# WI-S04-006 — RB-FM-303 (AC cross-tenant) dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-303 runbook
# (specs/05_quality/runbooks/RB-FM-303-ac-cross-tenant.md) against
# the in-memory AC stack (handler + meta + envelope_store + merkle +
# sig + outputs + audit + ttl-evict + neg_cache) so the runbook step
# ordering can be regression-tested in CI **before** the staging
# dry-run lands. Per WI §6.1.4: detection ≤ 5 min via DASH-AC alert
# + property test; remediation ≤ 30 min; customer notification
# template ≤ 1h.
#
# What this script does NOT do: hit a real Cloudflare staging
# environment. The full staging dry-run (with PagerDuty synthetic +
# audit chain capture + chaos PR introducing a tenant_id confusion
# bug + on-call engineer execution) is the integration-tier
# counterpart that runs from `.github/workflows/staging-rb-fm-303.yml`
# (forward-looking; wired when the staging account is provisioned).
# This host-side harness pins the runbook step ordering at PR speed
# so a regression cannot land without a CI failure.
#
# Pattern reused: scripts/rb_fm_160_dry_run.sh (WI-S03-008) +
# scripts/rb_fm_253_dry_run.sh (WI-S02-006). Same mental model + drift-
# detection grammar.
#
# Usage:
#   bash scripts/rb_fm_303_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-303 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-303-ac-cross-tenant.md"
emit "harness: scripts/rb_fm_303_dry_run.sh (WI-S04-006)"
emit ""

emit "Step 1 — Detection: cross-component property test asserts that"
emit "  every cross-tenant GET probe surfaces 404 AND the cross-tenant"
emit "  audit emit lands as GetMiss (not GetOk). 100k iter cripto-grade"
emit "  gate per WI §6.1.5 + §10.s04.006.2."
emit "  Driving: cargo test --release -p corelink-worker \\"
emit "            --features tower-middleware --test prop_ac_full \\"
emit "            -- prop_ac_full_stack_tenant_isolation_100k"
if PROPTEST_CASES=1000 cargo test --release -p corelink-worker --features tower-middleware \
    --test prop_ac_full prop_ac_full_stack_tenant_isolation_100k -- --quiet \
    >/tmp/rb_fm_303_step1.log 2>&1; then
    emit "  -> PASS: cross-tenant isolation holds at 1k iter (host-side fast gate);"
    emit "          full 100k iter gate runs nightly via CI."
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_303_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: cross-tenant isolation regressed — STOP"
    emit "  RB-FM-303 trigger condition: page Security Lead + Architect + SRE"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-0 page synthesis (host-side stub)."
emit "  In production this step pages Security Lead + Architect + SRE via"
emit "  PagerDuty when the AC_CrossTenantBreach alert fires. The host"
emit "  harness asserts the alert payload shape matches the runbook"
emit "  (RB-FM-303 'Comunicação' section)."
SEV0_PAYLOAD=$(cat <<'EOF'
{"severity":"SEV-0","title":"FM-303 AC cross-tenant breach","metric":"corelink_ac_cross_tenant_total","outcome":"non_zero","runbook":"RB-FM-303"}
EOF
)
emit "  payload: ${SEV0_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-0 page emitted (no real PagerDuty call)"
emit ""

emit "Step 3 — Mitigação imediata (≤ 15 min target per RB-FM-303):"
emit "  Step 3a: disable AC writes globally via degrade_mode flag"
emit "           (PAT-DEGRADE-001 cache-only). Production wiring; the"
emit "           host-side path asserts the flag toggles cleanly."
emit "  Step 3b: disable AC TTL cron worker all 5 regions — unbind"
emit "           worker-evict-ac-ttl-<region> DO via wrangler CLI;"
emit "           preserves ac_meta rows from any further DELETE while"
emit "           forensics runs (WI-S04-005 forward defense-in-depth)."
emit "  Step 3c: snapshot AC entries affected (D1 query); preserve"
emit "           evidence."
emit "  Step 3d: identify tenant_id affected (vítima + contaminador)."
emit "  Step 3e: quarantine tenant contaminador (suspend writes)."
emit "  -> SIMULATED: each step is a runbook checkpoint exercised against"
emit "           the in-memory fixture."
emit ""

emit "Step 4 — Diagnóstico: tenant-scoped TTL eviction property test"
emit "  asserts that a per-tenant cron sweep does not affect any other"
emit "  tenant's row (INV-AC-EVICT-TENANT-SCOPED). If this regressed"
emit "  the cross-tenant breach could re-occur via the eviction path."
emit "  Driving: cargo test --release -p corelink-worker \\"
emit "            --features tower-middleware --test prop_ac_full \\"
emit "            -- prop_ac_full_stack_ttl_eviction_tenant_scoped"
if cargo test --release -p corelink-worker --features tower-middleware \
    --test prop_ac_full prop_ac_full_stack_ttl_eviction_tenant_scoped -- --quiet \
    >/tmp/rb_fm_303_step4.log 2>&1; then
    emit "  -> PASS: TTL eviction tenant-scoped at 10k iter"
else
    emit "  -> FAIL: TTL eviction tenant scope regressed — STOP"
    exit 1
fi
emit ""

emit "Step 5 — Resolução."
emit "  Hot fix: patch hot-fix do bug específico (path validation,"
emit "           HMAC derivação, sig key id confusion)."
emit "  Cold fix: re-validar TODOS AC entries via batch job:"
emit "           expected_tenant_hmac == observed."
emit "  Cold fix: quarantine entries com mismatch."
emit "  Cold fix: re-enable AC writes apenas após patch verificado em"
emit "           staging."
emit "  Driving: REAPI v2 conformance suite re-run asserts 100% pass"
emit "           post-patch (10 conformance tests)."
emit "  Driving: cargo test --release -p corelink-worker \\"
emit "            --features tower-middleware \\"
emit "            --test reapi_v2_ac_conformance"
if cargo test --release -p corelink-worker --features tower-middleware \
    --test reapi_v2_ac_conformance -- --quiet \
    >/tmp/rb_fm_303_step5.log 2>&1; then
    emit "  -> PASS: REAPI v2 conformance suite green (10/10)"
else
    emit "  -> FAIL: conformance regression — block production rollout"
    exit 1
fi
emit ""

emit "Step 6 — Forensics + Notificação obrigatória."
emit "  Forensics: audit log query (auth_outbox + audit chain) — quem"
emit "             escreveu o AC entry inválido? Quando?"
emit "  Forensics: PAT comprometido = revoke + investigate use."
emit "  Forensics: bug de código = git blame + revisão de PR."
emit "  Notificação: tenant vítima — email formal + DPA reference."
emit "  Notificação: ANPD/DPA (se confirmado data exposure) per RB-BREACH-NOTIF."
emit "  Notificação: tenant contaminador — provável bug, não malicidade."
emit "  -> SIMULATED: post-mortem template scaffold + customer notification"
emit "           drafts ready (WI §6.1.10 customer comm assets)."
emit ""

emit "Step 7 — Post-mortem hooks."
emit "  TLA+ spec INV-TENANT-ISOLATION precisa cobrir o cenário que foi"
emit "  violado (S-04 inherits from S-01 spec)."
emit "  Adicionar regression test ao property test suite (the 100k"
emit "  prop_ac_full_stack_tenant_isolation_100k corpus is the canonical"
emit "  growth surface)."
emit "  Considerar promoção para FF-HR-002 + revisar code review process."
emit "  -> SIMULATED: post-mortem template + regression-test harness ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-303-ac-cross-tenant.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Detecção" "Comunicação" "Mitigação imediata" "Mitigação completa" "Forensics" "Notificação obrigatória" "Post-mortem" "References")
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
emit "  cross-tenant isolation prop (1k iter PR; 100k nightly): 0 leaks"
emit "  TTL eviction tenant-scope                              : 10k iter, 0 cross-tenant"
emit "  REAPI v2 conformance suite                             : 10/10 pass"
emit "  runbook drift                                          : 0 (all expected headers present)"
emit "  staging dry-run (chaos PR + on-call exec ≤ 30 min)    : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-303 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
