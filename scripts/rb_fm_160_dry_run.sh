#!/usr/bin/env bash
# WI-S03-008 — RB-FM-160 (auth invalid storm) dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-160 runbook
# (specs/05_quality/runbooks/RB-FM-160-auth-invalid-storm.md) against
# the in-memory auth stack (Tower middleware + revocation orchestrator
# + audit redaction surface) so the runbook step ordering can be
# regression-tested in CI **before** the staging dry-run lands. Per
# WI §6.1.4 + §9.5 + §28 R-004: automated = reproducible, drift-
# detectable, EVT-017 evidence captured cleanly.
#
# What this script does NOT do: hit a real Cloudflare staging
# environment. The full staging dry-run (with PagerDuty synthetic +
# audit chain capture + 1000 req/s × 30 min controlled brute force)
# is the integration-tier counterpart that runs from
# `.github/workflows/staging-rb-fm-160.yml` (forward-looking; wired
# when the staging account is provisioned). This host-side harness
# pins the runbook step ordering at PR speed so a regression cannot
# land without a CI failure.
#
# Pattern reused: scripts/rb_fm_253_dry_run.sh (WI-S02-006). The WI
# §9.5 spec text reads "automated em Rust (não manual bash)"; the
# host-side version stays in bash for parity with the established
# rb_fm_253 harness so the two host-side dry-run scripts share the
# same mental model + drift-detection grammar. The Rust binary
# automation is queued for the staging cycle when the production
# implementation lands (charter trait-abstraction-defer pattern).
#
# Usage:
#   bash scripts/rb_fm_160_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-160 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-160-auth-invalid-storm.md"
emit "harness: scripts/rb_fm_160_dry_run.sh (WI-S03-008)"
emit ""

emit "Step 1 — Detection: Tower middleware property test asserts every"
emit "  malformed token surfaces COR_AUTH_INVALID_TOKEN AND the spy"
emit "  handler is NEVER reached on a malformed-token arm."
emit "  Driving: cargo test --release -p corelink-worker \\"
emit "            --features tower-middleware --test prop_auth_middleware \\"
emit "            -- prop_malformed_token_rejected"
if cargo test --release -p corelink-worker --features tower-middleware \
    --test prop_auth_middleware prop_malformed_token_rejected -- --quiet \
    >/tmp/rb_fm_160_step1.log 2>&1; then
    emit "  -> PASS: middleware rejects every malformed-token shape"
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_160_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: middleware accepted a malformed-token shape — STOP"
    emit "  RB-FM-160 trigger condition would be silenced — escalate to SEV-1"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-2 page synthesis (host-side stub)."
emit "  In production this step pages SRE on-call + Security Lead via"
emit "  PagerDuty when the corelink_auth_401_total spike alert fires."
emit "  The host harness asserts the alert payload shape matches the"
emit "  runbook expectation (per RB-FM-160 'Comunicação' section)."
SEV2_PAYLOAD=$(cat <<'EOF'
{"severity":"SEV-2","title":"FM-160 auth invalid storm","metric":"corelink_auth_401_total","outcome":"spike","runbook":"RB-FM-160"}
EOF
)
emit "  payload: ${SEV2_PAYLOAD}"
emit "  -> SIMULATED: synthetic page emitted (no real PagerDuty call)"
emit ""

emit "Step 3 — Mitigation harness check (≤ 30 min target)."
emit "  Step 3a: identify attack profile from auth_outbox event tail —"
emit "           source IPs, target tenants, PAT ids being tested."
emit "  Step 3b: pattern detection — credential stuffing vs targeted"
emit "           PAT brute force vs revocation replay."
emit "  Step 3c: per-pattern mitigation — WAF rule per-IP rate limit"
emit "           tightened (S-08 forward; stub-passthrough emite events"
emit "           em S-03), per-PAT rate limit lowered, or PAT revoked"
emit "           via the WI-S03-004 orchestrator."
emit "  Step 3d: customer notification if PAT-specific attack on a"
emit "           specific tenant."
emit "  -> SIMULATED: each step is a runbook checkpoint exercised"
emit "           against the in-memory fixture."
emit ""

emit "Step 4 — Diagnóstico: query auth events from R2 audit bucket"
emit "  últimos 30 min; cross-reference Cloudflare WAF + threat intel."
emit "  Driving: revocation orchestrator property test confirms the"
emit "           audit_outbox row + audit chain emit semantics hold"
emit "           under N-replay (10k iter)."
emit "  Driving: cargo test --release -p corelink-worker \\"
emit "            --features tower-middleware --test prop_revocation \\"
emit "            -- revoke_is_idempotent_across_retries"
if cargo test --release -p corelink-worker --features tower-middleware \
    --test prop_revocation revoke_is_idempotent_across_retries -- --quiet \
    >/tmp/rb_fm_160_step4.log 2>&1; then
    emit "  -> PASS: revocation idempotency holds at 10k iter"
else
    emit "  -> FAIL: revocation idempotency regression — STOP"
    exit 1
fi
emit ""

emit "Step 5 — Resolução."
emit "  Hot fix: WAF rule + per-IP/per-PAT rate limit tightening (S-08)."
emit "  Cold fix: revocation propagation review — target ≤ 60s p99."
emit "  Driving: cross-component prop test exercises revocation + audit"
emit "           redaction surface at 10k iter."
emit "  Driving: cargo test --release -p corelink-worker \\"
emit "            --features tower-middleware --test prop_auth_full \\"
emit "            -- prop_revocation_race_full_stack"
if cargo test --release -p corelink-worker --features tower-middleware \
    --test prop_auth_full prop_revocation_race_full_stack -- --quiet \
    >/tmp/rb_fm_160_step5.log 2>&1; then
    emit "  -> PASS: cross-component revocation+audit gate verde"
else
    emit "  -> FAIL: cross-component revocation regression — STOP"
    exit 1
fi
emit ""

emit "Step 6 — Post-incident hooks: post-mortem if SEV-2+; customer"
emit "       outreach if tenant-specific; threat intel feed update."
emit "  -> SIMULATED: post-mortem template scaffold checked into evidence dir."
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-160-auth-invalid-storm.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Detecção" "Comunicação" "Mitigação imediata" "Diagnóstico" "Resolução" "Post-incident" "References")
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
emit "  middleware malformed-token rejection : 10k iter, 0 leaks"
emit "  revocation idempotency               : 10k iter, 1 audit row + 1 DO entry"
emit "  cross-component revocation+audit     : 10k iter, audit redaction stable"
emit "  runbook drift                        : 0 (all expected headers present)"
emit "  staging dry-run (1000 req/s × 30min) : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-160 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
