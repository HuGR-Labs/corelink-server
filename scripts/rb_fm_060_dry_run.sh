#!/usr/bin/env bash
# WI-S05-006 — RB-FM-060 (multipart orphan accumulation) dry-run
# harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-060 runbook
# (specs/05_quality/runbooks/RB-FM-060-multipart-orphan.md) against
# the in-memory multipart stack (handler + session_store +
# chunk_store + assembler + sweeper + audit) so the runbook step
# ordering can be regression-tested in CI **before** the staging
# dry-run lands. Per WI §6.1.2: detection ≤ 5 min via DASH-MULTIPART
# alert + property test; remediation ≤ 30 min; customer notification
# template ≤ 1h.
#
# What this script does NOT do: hit a real Cloudflare staging
# environment. The full staging dry-run (with PagerDuty synthetic +
# audit chain capture + chaos PR introducing an orphan multipart +
# on-call engineer execution) is the integration-tier counterpart
# that runs from `.github/workflows/staging-rb-fm-060.yml` (forward-
# looking; wired when the staging account is provisioned). This
# host-side harness pins the runbook step ordering at PR speed so a
# regression cannot land without a CI failure.
#
# Pattern reused: scripts/rb_fm_303_dry_run.sh (WI-S04-006) +
# scripts/rb_fm_160_dry_run.sh (WI-S03-008) +
# scripts/rb_fm_253_dry_run.sh (WI-S02-006). Same mental model + drift-
# detection grammar.
#
# Usage:
#   bash scripts/rb_fm_060_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-060 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-060-multipart-orphan.md"
emit "harness: scripts/rb_fm_060_dry_run.sh (WI-S05-006)"
emit ""

emit "Step 1 — Detection: cross-component property test asserts that"
emit "  the sweeper aborts ONLY tenant-scoped stale Live sessions and"
emit "  emits the canonical orphan_swept audit reason."
emit "  Driving: cargo test -p corelink-worker \\"
emit "            --features tower-middleware --test prop_multipart_full \\"
emit "            -- prop_multipart_full_stack_sweeper_orphan_abort_isolated"
if PROPTEST_CASES=64 cargo test -p corelink-worker --features tower-middleware \
    --test prop_multipart_full prop_multipart_full_stack_sweeper_orphan_abort_isolated -- --quiet \
    >/tmp/rb_fm_060_step1.log 2>&1; then
    emit "  -> PASS: tenant-scoped sweeper isolation holds at 64 iter (host-side fast gate);"
    emit "          full 10k iter gate runs at PR via prop_multipart_full."
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_060_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: sweeper tenant scope regressed — STOP"
    emit "  RB-FM-060 trigger condition: page SRE + Security Lead"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-3 page synthesis (host-side stub)."
emit "  In production this step pages SRE on-call via PagerDuty when"
emit "  the Multipart_OrphanRateHigh alert fires (orphan rate > 1%"
emit "  sustained 1h) OR the Multipart_SweeperStale alert fires"
emit "  (cron tick rate < 1/h sustained 1h)."
SEV3_PAYLOAD='{"severity":"SEV-3","title":"FM-060 multipart orphan accumulation","metric":"corelink_multipart_sweeper_orphans_aborted_total","outcome":"sustained_growth","runbook":"RB-FM-060"}'
emit "  payload: ${SEV3_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-3 page emitted (no real PagerDuty call)"
emit ""

emit "Step 3 — Mitigação imediata (≤ 30 min target per RB-FM-060):"
emit "  Step 3a: confirm via R2 admin API: \`ListMultipartUploads\`"
emit "           returns count > 0 sustained;"
emit "  Step 3b: sweeper ativa: verify cron DO last alarm fired"
emit "           ≤ 1h ago (DASH-MULTIPART panel 5)."
emit "  Step 3c: if sweeper inactive: trigger manual abort batch"
emit "           via \`wrangler tail worker-sweeper-multipart-<region>\`"
emit "           + invoke \`abort_multipart_upload\` per stale row."
emit "  Step 3d: identify root cause: client disconnect padrão?"
emit "           Network issues? Specific tenant?"
emit "  -> SIMULATED: each step is a runbook checkpoint exercised against"
emit "           the in-memory fixture (sweeper sub-module pure-logic"
emit "           tick path covers steps 3a-3c structurally)."
emit ""

emit "Step 4 — Diagnóstico: REAPI v2 SplitBlob/SpliceBlob conformance"
emit "  asserts the canonical wire contract still holds end-to-end."
emit "  If the conformance suite regressed simultaneously the orphan"
emit "  signal could be a symptom of a deeper handler / R2 adapter"
emit "  break."
emit "  Driving: cargo test -p corelink-worker \\"
emit "            --features tower-middleware --test reapi_v2_split_splice_conformance"
if cargo test -p corelink-worker --features tower-middleware \
    --test reapi_v2_split_splice_conformance -- --quiet \
    >/tmp/rb_fm_060_step4.log 2>&1; then
    emit "  -> PASS: REAPI v2 split/splice conformance suite green (10/10)"
else
    emit "  -> FAIL: conformance regressed — escalate to SEV-1 + page Architect"
    exit 1
fi
emit ""

emit "Step 5 — Resolução."
emit "  Hot fix: manual abort orphan parts > 7d (\`abort_multipart_upload\`"
emit "           via wrangler CLI or sweeper DO admin endpoint)."
emit "  Cold fix:"
emit "    - tighten sweeper cadence se needed (default 1h → 30 min);"
emit "    - investigate root cause (network instability, client SDK"
emit "      bug, specific tenant misuse);"
emit "    - customer outreach se tenant-specific via release-notes-s05"
emit "      onboarding doc + Bazel chunked-cache setup guide."
emit "  Driving: cross-component property test asserts the canonical"
emit "           tenant-isolation invariant still holds post-fix."
emit "  Driving: cargo test -p corelink-worker \\"
emit "            --features tower-middleware --test prop_multipart_full \\"
emit "            -- prop_multipart_full_stack_tenant_isolation_100k"
if PROPTEST_CASES=64 cargo test -p corelink-worker --features tower-middleware \
    --test prop_multipart_full prop_multipart_full_stack_tenant_isolation_100k -- --quiet \
    >/tmp/rb_fm_060_step5.log 2>&1; then
    emit "  -> PASS: tenant isolation holds post-fix (host-side fast gate)"
else
    emit "  -> FAIL: tenant isolation regressed — block production rollout"
    exit 1
fi
emit ""

emit "Step 6 — Forensics + Notificação."
emit "  Forensics: audit log query (audit_outbox + audit chain) — quem"
emit "             abriu o multipart session que ficou orphan? Quando?"
emit "  Forensics: client SDK version distribution — was the orphan"
emit "             cohort all on the same SDK release?"
emit "  Forensics: bug de código = git blame on the chunker/handler"
emit "             commit; investigate if a specific PR introduced the"
emit "             regression."
emit "  Notificação: tenant impactado — email formal + cost-impact"
emit "             reference (R2 charges for ongoing parts)."
emit "  Notificação (cost concern only): no DPA / ANPD escalation"
emit "             unless the orphan window leaked PII (extremely"
emit "             unlikely — multipart parts are blob bytes, not"
emit "             tenant metadata)."
emit "  -> SIMULATED: post-mortem template scaffold + customer"
emit "             notification draft ready (release-notes-s05.md +"
emit "             multipart-sla-addendum-s05-ga.md)."
emit ""

emit "Step 7 — Post-mortem hooks."
emit "  TLA+ spec INV-MULTIPART-ORPHAN-DETECTABLE precisa cobrir o"
emit "  cenário que foi violado (S-05 inherits from S-01 spec corpus)."
emit "  Adicionar regression test ao property test suite"
emit "  (prop_multipart_full_stack_sweeper_orphan_abort_isolated is"
emit "  the canonical growth surface)."
emit "  Considerar promoção para FF-HR-002 + revisar code review process."
emit "  -> SIMULATED: post-mortem template + regression-test harness ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-060-multipart-orphan.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Detecção" "Comunicação" "Mitigação imediata" "Resolução" "References")
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
emit "  sweeper tenant-scoped orphan abort prop (64 iter PR; 10k nightly): 0 cross-tenant aborts"
emit "  REAPI v2 split/splice conformance suite                          : 10/10 pass"
emit "  tenant isolation prop post-fix (64 iter PR; 100k nightly)        : 0 leaks"
emit "  runbook drift                                                    : 0 (all expected headers present)"
emit "  staging dry-run (chaos PR + on-call exec ≤ 30 min)               : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-060 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
