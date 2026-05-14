#!/usr/bin/env bash
# WI-S14-001 — Verify DO jurisdictional restriction per region.
# Critical for WEUR: DO MUST have jurisdiction = "eu" (Schrems II + GDPR Art. 46).
#
# Usage: ./scripts/verify_do_jurisdiction.sh [--region REGION]
#        REGION: wnam | enam | weur | sam (default: all)
#
# Requires: CF API token with Workers:Read permission.
# Exit codes: 0 = all jurisdictions correct; 1 = mismatch (CRITICAL for WEUR).
#
# WEUR mismatch = CRITICAL (Schrems II + GDPR Art. 46 violation risk).
# Non-WEUR mismatch = SEV-2 (unexpected jurisdiction config).
set -euo pipefail

REGION_FILTER="${1:-}"
if [[ "$REGION_FILTER" == "--region" ]]; then
    REGION_FILTER="${2:-}"
fi

# Expected DO jurisdiction per region (per WI-S14-001 §1 + spec contract §5.1 R-S14-1)
declare -A EXPECTED_JURISDICTIONS=(
    ["wnam"]="us"
    ["enam"]="us"
    ["weur"]="eu"    # MANDATORY: Schrems II + GDPR Art. 46
    ["sam"]="none"
)

REGIONS=("wnam" "enam" "weur" "sam")
if [[ -n "$REGION_FILTER" ]]; then
    REGIONS=("$REGION_FILTER")
fi

FAIL=0
CRITICAL=0

for region in "${REGIONS[@]}"; do
    worker="corelink-do-${region}"
    expected="${EXPECTED_JURISDICTIONS[$region]}"

    echo "[verify_do_jurisdiction] Checking DO Worker: ${worker} (expected jurisdiction: ${expected})"

    # Query Cloudflare API for Worker metadata
    # CF Workers API: GET /accounts/{account_id}/workers/scripts/{script_name}
    # Response includes `jurisdictional_restriction` field.
    if [[ -n "${CF_API_TOKEN:-}" ]] && [[ -n "${CF_ACCOUNT_ID:-}" ]]; then
        actual=$(curl -s -X GET \
            "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT_ID}/workers/scripts/${worker}" \
            -H "Authorization: Bearer ${CF_API_TOKEN}" \
            -H "Content-Type: application/json" \
            2>/dev/null \
            | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
    result = d.get('result', {})
    j = result.get('jurisdictional_restriction', {}).get('label', 'none')
    print(j.lower())
except Exception:
    print('unknown')
" 2>/dev/null || echo "unknown")
    else
        echo "[verify_do_jurisdiction] WARN: CF_API_TOKEN or CF_ACCOUNT_ID not set — using stub (unknown)"
        actual="unknown"
    fi

    if [[ "$actual" == "unknown" ]]; then
        echo "[verify_do_jurisdiction] WARN: Could not determine jurisdiction for ${worker}"
        if [[ "${PRODUCTION_VERIFY:-false}" == "true" ]] || [[ "$region" == "weur" ]]; then
            echo "[verify_do_jurisdiction] ERROR: Production/WEUR verification requires known jurisdiction"
            FAIL=1
            if [[ "$region" == "weur" ]]; then
                CRITICAL=1
            fi
        fi
    elif [[ "$actual" != "$expected" ]]; then
        if [[ "$region" == "weur" ]]; then
            echo "[verify_do_jurisdiction] CRITICAL: WEUR DO jurisdiction MISMATCH!"
            echo "[verify_do_jurisdiction] Expected: eu | Actual: ${actual}"
            echo "[verify_do_jurisdiction] SCHREMS II / GDPR Art. 46 VIOLATION RISK"
            echo "[verify_do_jurisdiction] ACTION: HALT deployment + escalate to Compliance Officer immediately"
            echo "[verify_do_jurisdiction] RUNBOOK: See RB-region §3 DO jurisdiction setting procedure"
            CRITICAL=1
        else
            echo "[verify_do_jurisdiction] ERROR: Jurisdiction MISMATCH for ${worker}: expected=${expected}, actual=${actual}"
            echo "[verify_do_jurisdiction] ACTION: Emit audit + alert SEV-2"
        fi
        FAIL=1
    else
        echo "[verify_do_jurisdiction] OK: ${worker} jurisdiction=${actual} (matches expected)"
        if [[ "$region" == "weur" ]]; then
            echo "[verify_do_jurisdiction] WEUR EU jurisdiction confirmed — Schrems II compliant"
        fi
    fi
done

if [[ $CRITICAL -ne 0 ]]; then
    echo "[verify_do_jurisdiction] RESULT: CRITICAL FAIL — WEUR jurisdiction violation (exit 2)"
    exit 2
elif [[ $FAIL -ne 0 ]]; then
    echo "[verify_do_jurisdiction] RESULT: FAIL — jurisdiction mismatches detected (exit 1)"
    exit 1
else
    echo "[verify_do_jurisdiction] RESULT: OK — all DO jurisdictions match expected values"
    exit 0
fi
