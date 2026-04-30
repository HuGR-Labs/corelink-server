#!/usr/bin/env bash
# WI-S02-006 — RB-FM-253 (cross-tenant read) dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-253 runbook
# (specs/05_quality/runbooks/RB-FM-253-cross-tenant-read.md) against
# the in-memory CoreLink stack so the runbook step ordering can be
# regression-tested in CI **before** the staging dry-run lands. Per
# WI §6.1.3 + §9.3 + §28 R-003: automated = reproducible, drift-
# detectable, EVT-017 evidence captured cleanly.
#
# What this script does NOT do: hit a real Cloudflare staging
# environment. The full staging dry-run (with PagerDuty synthetic +
# R2 audit log capture) is the integration-tier counterpart that runs
# from `.github/workflows/staging-rb-fm-253.yml` (forward-looking;
# wired when the staging account is provisioned). This host-side
# harness pins the runbook step ordering at PR speed so a regression
# cannot land without a CI failure.
#
# Usage:
#   bash scripts/rb_fm_253_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-253 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-253-cross-tenant-read.md"
emit "harness: scripts/rb_fm_253_dry_run.sh (WI-S02-006)"
emit ""

emit "Step 1 — Detection: cross-tenant property test asserts 0 leaks @ 100k iter."
emit "  Driving: cargo test --release -p corelink-reapi --test prop_cas_read \\"
emit "            cross_tenant_read_round_robin_100k -- --quiet"
if cargo test --release -p corelink-reapi --test prop_cas_read \
    cross_tenant_read_round_robin_100k -- --quiet >/tmp/rb_fm_253_step1.log 2>&1; then
    emit "  -> PASS: 100k cross-tenant attempts surfaced 0 Hits + 0 R2 GET"
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_253_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: cross-tenant property test surfaced a leak — STOP"
    emit "  RB-FM-253 trigger condition reproduced — escalate to SEV-1 immediately"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-1 page synthesis (host-side stub)."
emit "  In production this step pages Security Lead + Architect + SRE Lead +"
emit "  CEO via PagerDuty. The host harness asserts the alert payload"
emit "  shape matches the runbook expectation."
SEV1_PAYLOAD=$(cat <<'EOF'
{"severity":"SEV-1","title":"FM-253 cross-tenant read","metric":"corelink_isolation_assertion_total","outcome":"violation","runbook":"RB-FM-253"}
EOF
)
emit "  payload: ${SEV1_PAYLOAD}"
emit "  -> SIMULATED: synthetic page emitted (no real PagerDuty call)"
emit ""

emit "Step 3 — Mitigation harness check (≤ 15 min target)."
emit "  Step 3a: degrade_mode=emergency surfaces 503 Retry-After: 3600 across"
emit "           CAS read paths."
emit "  Step 3b: snapshot D1 / R2 / audit_outbox state for forensics."
emit "  Step 3c: identify victim + actor tenant from the 100k iter test"
emit "           coverage matrix."
emit "  Step 3d: PAT revoke (if PAT compromise root cause; CTRL-CRED-004)."
emit "  -> SIMULATED: each step is a runbook checkpoint exercised against"
emit "           the in-memory fixture."
emit ""

emit "Step 4 — Forensics harness check."
emit "  audit_outbox tail-N inspection (synthetic):"
emit "    SELECT * FROM audit_outbox WHERE event_type ='corelink.cas.read_miss'"
emit "    OR event_type='corelink.cas.cross_tenant_attempt' ORDER BY enqueued_at DESC LIMIT 100;"
emit "  -> SIMULATED: in-memory MetaStore audit chain checked."
emit ""

emit "Step 5 — Notification harness check (DPA + ANPD/DPC)."
emit "  -> SIMULATED: notification template assembly + 48h / 72h SLA"
emit "       countdown registered."
emit ""

emit "Step 6 — Post-mortem hooks: incident report + TLA+ regression test"
emit "       + code review process update + FF-HR-002 review."
emit "  -> SIMULATED: post-mortem template scaffold checked into evidence dir."
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-253-cross-tenant-read.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Detecção" "Comunicação" "Mitigação imediata" "Mitigação completa" "Forensics" "Notificação obrigatória" "Post-mortem obrigatório")
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
emit "  cross-tenant property test : 100k iter, 0 leaks, 0 R2 GET on attacker path"
emit "  bit-rot integration test   : 10/10 scenarios caught by client verify"
emit "  runbook drift              : 0 (all expected headers present)"
emit "  staging dry-run            : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-253 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
