#!/usr/bin/env bash
# WI-S14-006: BYOK kill switch chaos drill harness.
#
# Weekly cron Sunday 03:00 UTC (see .github/workflows/byok_kill_switch_drill_weekly.yml).
# Per-provider rotation: 4 providers over 4 weeks = monthly full coverage.
#
# Usage: ./scripts/byok_kill_switch_drill.sh [provider] [tenant_id]
#   provider: aws | gcp | azure | vault (default: weekly rotation)
#   tenant_id: staging BYOK tenant ID
#
# SLA assertion: total kill switch ≤ 360s (detection 60s + DEK TTL 300s).
#
# Output: report committed to specs/_audits/YYYY-MM-DD-byok-kill-switch-drill-{provider}.md

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DATE="$(date -u +%Y-%m-%d)"
REPORT_DIR="${REPO_ROOT}/specs/_audits"

# Determine provider rotation (week of year mod 4).
# macOS date uses BSD date; GNU date uses -u +%V. Both support +%V.
WEEK_OF_YEAR="$(date +%V 2>/dev/null || echo "01")"
PROVIDER_INDEX=$(( (10#${WEEK_OF_YEAR}) % 4 ))
PROVIDERS=("aws" "gcp" "azure" "vault")
DEFAULT_PROVIDER="${PROVIDERS[$PROVIDER_INDEX]}"

PROVIDER="${1:-$DEFAULT_PROVIDER}"
STAGING_TENANT="${2:-staging-byok-drill-${PROVIDER}}"
REPORT_FILE="${REPORT_DIR}/${DATE}-byok-kill-switch-drill-${PROVIDER}.md"

# SLA: ≤ 360s total (60s detection + 300s DEK TTL hard).
SLA_SECONDS=360

log() { echo "[$(date -u +%H:%M:%S)] $*"; }
fail() { echo "[FAIL] $*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# B-084: this harness must not be able to emit a passing attestation for work
# it did not do.
#
# Every phase below whose provider call is still a comment assigns a hardcoded
# constant — `TENANT_STATUS="active"`, `DETECTED=true`,
# `TENANT_STATUS_POST="degraded_read_only"`, `RECOVERY_STATUS="active"` — and
# `SLA_RESULT` was likewise pinned to "PASS". The report written in phase 7
# therefore said PASS regardless of the world, and that file is forwarded to
# customer SRE as SLA evidence by
# marketing/lighthouse-kit/05-sla-attestation-instructions.md.
#
# That is categorically worse than a missing control (B-083, which tracks
# BUILDING this): a missing control leaves a gap, this manufactures proof the
# gap is closed.
#
# The mechanism: each stubbed phase registers itself here. At report time, a
# non-empty ledger means NO report is written and the script exits non-zero.
# There is deliberately no override flag — an env var that re-enables the
# attestation is the hole again, one indirection further away.
#
# TO CLOSE A PHASE: implement its real provider call and DELETE its
# `drill_stub` line. The attestation becomes reachable when the ledger empties,
# and not one phase before.
STUBBED_PHASES=()
drill_stub() { STUBBED_PHASES+=("$1"); log "  [STUB] $1 — no measurement taken"; }

log "=== BYOK Kill Switch Chaos Drill ==="
log "Provider: ${PROVIDER}"
log "Tenant: ${STAGING_TENANT}"
log "SLA: ≤ ${SLA_SECONDS}s"
log "Report: ${REPORT_FILE}"

# ---------------------------------------------------------------------------
# Phase 1: Verify staging BYOK tenant exists and is active.
# ---------------------------------------------------------------------------
log "Phase 1: Verify staging tenant..."
# Production: query D1 staging for tenant byok_status = 'active'.
# Here: NOT IMPLEMENTED — the constant below is not a measurement.
drill_stub "phase 1: verify staging tenant is active (D1 query)"
TENANT_STATUS="active"
log "Tenant ${STAGING_TENANT} status: ${TENANT_STATUS}"

if [[ "${TENANT_STATUS}" != "active" ]]; then
    fail "Staging tenant ${STAGING_TENANT} is not active (status=${TENANT_STATUS})"
fi

# ---------------------------------------------------------------------------
# Phase 2: Revoke CMK in staging provider.
# ---------------------------------------------------------------------------
log "Phase 2: Revoking CMK in ${PROVIDER} staging..."
drill_stub "phase 2: revoke the CMK in the provider"
T_REVOKE_START="$(date +%s)"
# Production: call provider CLI/API to revoke/disable CMK.
# AWS: aws kms disable-key --key-id $KEY_ID
# GCP: gcloud kms keys versions destroy ...
# Azure: az keyvault key disable ...
# Vault: vault write auth/token/revoke ...
log "CMK revoked at $(date -u +%H:%M:%S UTC)"

# ---------------------------------------------------------------------------
# Phase 3: Wait for RevocationDetector to detect (≤ 60s).
# ---------------------------------------------------------------------------
log "Phase 3: Waiting for detection (max 60s)..."
DETECT_TIMEOUT=60
T_DETECT_START="$(date +%s)"
DETECTED=false

for _ in $(seq 1 $DETECT_TIMEOUT); do
    # Production: poll audit_outbox for corelink.byok.cmk_revoked event.
    # CI: NOT IMPLEMENTED — this sleep is not a detection.
    drill_stub "phase 3: poll audit_outbox for corelink.byok.cmk_revoked"
    sleep 2
    DETECTED=true
    break
done

T_DETECT_END="$(date +%s)"
DETECT_LATENCY_S=$(( T_DETECT_END - T_DETECT_START ))

if [[ "${DETECTED}" != "true" ]]; then
    fail "Detection timeout after ${DETECT_TIMEOUT}s (SLA miss)"
fi
log "Detection latency: ${DETECT_LATENCY_S}s (SLA ≤ 60s): PASS"

# ---------------------------------------------------------------------------
# Phase 4: Verify DEK cache evicted and tenant degraded.
# ---------------------------------------------------------------------------
log "Phase 4: Verify cache eviction + tenant degrade..."
# Production: query cache metrics + D1 tenant status.
# CI: NOT IMPLEMENTED — the constants below are not measurements.
drill_stub "phase 4: verify DEK cache evicted + tenant degraded"
DEK_CACHE_EMPTY=true
TENANT_STATUS_POST="degraded_read_only"
log "DEK cache empty: ${DEK_CACHE_EMPTY}"
log "Tenant status: ${TENANT_STATUS_POST}"

if [[ "${TENANT_STATUS_POST}" != "degraded_read_only" ]]; then
    fail "Tenant not degraded after kill switch"
fi

# ---------------------------------------------------------------------------
# Phase 5: Measure total kill switch duration.
# ---------------------------------------------------------------------------
T_REVOKE_END="$(date +%s)"
TOTAL_S=$(( T_REVOKE_END - T_REVOKE_START ))
log "Total kill switch duration: ${TOTAL_S}s (SLA ≤ ${SLA_SECONDS}s)"

# Derived, not pinned. The old `SLA_RESULT="PASS"` here was the literal string
# that reached the customer-facing table in phase 7.
if (( TOTAL_S > SLA_SECONDS )); then
    SLA_RESULT="FAIL (SLA MISS: ${TOTAL_S}s > ${SLA_SECONDS}s)"
else
    SLA_RESULT="PASS"
fi

# ---------------------------------------------------------------------------
# Phase 6: Restore CMK and verify recovery.
# ---------------------------------------------------------------------------
log "Phase 6: Re-enabling CMK in ${PROVIDER} staging..."
# Production: aws kms enable-key ... / gcloud / az / vault
drill_stub "phase 6: re-enable the CMK and verify recovery"
sleep 1 # NOT IMPLEMENTED
log "Recovery check (≤ 60s)..."
sleep 1 # NOT IMPLEMENTED
RECOVERY_STATUS="active"
log "Tenant status after recovery: ${RECOVERY_STATUS}"

# ---------------------------------------------------------------------------
# Phase 7: Write report.
# ---------------------------------------------------------------------------
# ---------------------------------------------------------------------------
# The gate. No attestation is written while any phase is a stub.
#
# Placed BEFORE `mkdir -p`/`cat >` so the file is never created — not created
# and then deleted, and not created with a warning banner. A file under
# specs/_audits/ with a PASS table in it gets forwarded; a banner does not
# survive a copy-paste into a customer form.
# ---------------------------------------------------------------------------
if (( ${#STUBBED_PHASES[@]} > 0 )); then
    echo >&2
    echo "[REFUSED] This drill measured nothing, so it will not write an attestation." >&2
    echo "          ${#STUBBED_PHASES[@]} phase(s) are still stubs:" >&2
    for phase in "${STUBBED_PHASES[@]}"; do echo "            - ${phase}" >&2; done
    echo >&2
    echo "          No report written to ${REPORT_FILE}." >&2
    echo "          Building the real kill switch is tracked as B-083; implementing a" >&2
    echo "          phase means writing its provider call and deleting its drill_stub line." >&2
    echo "          Until the ledger is empty this harness cannot emit PASS. That is the point." >&2
    exit 1
fi

mkdir -p "${REPORT_DIR}"
cat > "${REPORT_FILE}" << EOF
---
type: chaos-drill-report
wi: WI-S14-006
date: ${DATE}
provider: ${PROVIDER}
tenant: ${STAGING_TENANT}
sla_seconds: ${SLA_SECONDS}
result: ${SLA_RESULT}
---

# BYOK Kill Switch Chaos Drill — ${PROVIDER} — ${DATE}

## Summary

| Metric | Value | SLA | Result |
|---|---|---|---|
| Detection latency | ${DETECT_LATENCY_S}s | ≤ 60s | $([ ${DETECT_LATENCY_S} -le 60 ] && echo PASS || echo FAIL) |
| Total kill switch duration | ${TOTAL_S}s | ≤ ${SLA_SECONDS}s | ${SLA_RESULT} |
| DEK cache empty post-evict | ${DEK_CACHE_EMPTY} | true | $([ "${DEK_CACHE_EMPTY}" = "true" ] && echo PASS || echo FAIL) |
| Tenant degraded read-only | ${TENANT_STATUS_POST} = degraded_read_only | true | $([ "${TENANT_STATUS_POST}" = "degraded_read_only" ] && echo PASS || echo FAIL) |
| Recovery after re-enable | ${RECOVERY_STATUS} = active | true | $([ "${RECOVERY_STATUS}" = "active" ] && echo PASS || echo FAIL) |

## Provider

**${PROVIDER}** (week ${WEEK_OF_YEAR} of 4-week rotation: ${PROVIDERS[*]})

## INV-BYOK-CRYPTO-SOVEREIGNTY

Kill switch SLA: **${SLA_RESULT}**

## Drift Findings

- None (automated drill; staging environment).

## Drill Date

${DATE} UTC — cron weekly Sunday 03:00 UTC.
EOF

log "Report written: ${REPORT_FILE}"
log "=== Drill complete: ${SLA_RESULT} ==="

if [[ "${SLA_RESULT}" != "PASS" ]]; then
    exit 1
fi
