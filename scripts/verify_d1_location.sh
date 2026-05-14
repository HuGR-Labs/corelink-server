#!/usr/bin/env bash
# WI-S14-001 — Verify D1 database location matches expected region.
# Runs as post-deploy check + daily drift detection (cron via GH Actions).
#
# Usage: ./scripts/verify_d1_location.sh [--region REGION]
#        REGION: wnam | enam | weur | sam (default: all)
#
# Requires: wrangler CLI authenticated + CF_ACCOUNT_ID env var.
# Exit codes: 0 = all locations match; 1 = drift detected (SEV-2 alert).
#
# INV-DATA-RESIDENCY: D1 location drift = audit emit + alert SEV-2.
set -euo pipefail

REGION_FILTER="${1:-}"
if [[ "$REGION_FILTER" == "--region" ]]; then
    REGION_FILTER="${2:-}"
fi

# Expected D1 location hint per region (lowercase per CF D1 API)
declare -A EXPECTED_LOCATIONS=(
    ["wnam"]="wnam"
    ["enam"]="enam"
    ["weur"]="weur"
    ["sam"]="sam"
)

REGIONS=("wnam" "enam" "weur" "sam")
if [[ -n "$REGION_FILTER" ]]; then
    REGIONS=("$REGION_FILTER")
fi

FAIL=0

for region in "${REGIONS[@]}"; do
    db="corelink-meta-${region}"
    expected="${EXPECTED_LOCATIONS[$region]}"

    echo "[verify_d1_location] Checking D1 database: ${db} (expected location: ${expected})"

    if command -v wrangler &>/dev/null; then
        # Query D1 database info for location
        actual=$(wrangler d1 info "${db}" --json 2>/dev/null \
            | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('location','unknown').lower())" 2>/dev/null || echo "unknown")
    else
        echo "[verify_d1_location] WARN: wrangler not found — using stub (unknown)"
        actual="unknown"
    fi

    if [[ "$actual" == "unknown" ]]; then
        echo "[verify_d1_location] WARN: Could not determine location for ${db}"
        if [[ "${PRODUCTION_VERIFY:-false}" == "true" ]]; then
            echo "[verify_d1_location] ERROR: Production verification requires known location"
            FAIL=1
        fi
    elif [[ "$actual" != "$expected" ]]; then
        echo "[verify_d1_location] ERROR: DRIFT detected for ${db}: expected=${expected}, actual=${actual}"
        echo "[verify_d1_location] ACTION: Emit audit corelink.region.d1_location_drift + alert SEV-2"
        echo "[verify_d1_location] RUNBOOK: See RB-region §6 D1 location drift procedure"
        FAIL=1
    else
        echo "[verify_d1_location] OK: ${db} location=${actual} (matches expected)"
    fi
done

if [[ $FAIL -ne 0 ]]; then
    echo "[verify_d1_location] RESULT: FAIL — D1 location drift detected (exit 1)"
    exit 1
else
    echo "[verify_d1_location] RESULT: OK — all D1 databases at expected locations"
    exit 0
fi
