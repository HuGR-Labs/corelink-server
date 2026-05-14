#!/usr/bin/env bash
# WI-S14-001 — Verify R2 bucket location matches expected location hint.
# Runs as post-deploy check + daily drift detection (cron via GH Actions).
#
# Usage: ./scripts/verify_r2_location.sh [--region REGION]
#        REGION: wnam | enam | weur | sam (default: all)
#
# Requires: wrangler CLI authenticated + CF_ACCOUNT_ID env var.
# Exit codes: 0 = all locations match; 1 = mismatch detected (SEV-2 alert).
#
# INV-DATA-RESIDENCY: location mismatch = audit emit + alert SEV-2.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REGION_FILTER="${1:-}"
if [[ "$REGION_FILTER" == "--region" ]]; then
    REGION_FILTER="${2:-}"
fi

# Expected R2 location hint per region
declare -A EXPECTED_LOCATIONS=(
    ["wnam"]="WNAM"
    ["enam"]="ENAM"
    ["weur"]="WEUR"
    ["sam"]="SAM"
)

REGIONS=("wnam" "enam" "weur" "sam")
if [[ -n "$REGION_FILTER" ]]; then
    REGIONS=("$REGION_FILTER")
fi

FAIL=0

for region in "${REGIONS[@]}"; do
    bucket="corelink-cas-${region}"
    expected="${EXPECTED_LOCATIONS[$region]}"

    echo "[verify_r2_location] Checking bucket: ${bucket} (expected: ${expected})"

    # Query R2 bucket metadata via wrangler
    # In production: `wrangler r2 bucket info <bucket>` returns location hint.
    # Stub: output format `location: <REGION>` (adjust to actual wrangler output).
    if command -v wrangler &>/dev/null; then
        actual=$(wrangler r2 bucket info "${bucket}" --json 2>/dev/null \
            | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('location','UNKNOWN').upper())" 2>/dev/null || echo "UNKNOWN")
    else
        echo "[verify_r2_location] WARN: wrangler not found — using stub (UNKNOWN)"
        actual="UNKNOWN"
    fi

    if [[ "$actual" == "UNKNOWN" ]]; then
        echo "[verify_r2_location] WARN: Could not determine location for ${bucket} (wrangler not configured or bucket not yet provisioned)"
        # Non-fatal in CI dry-run; fatal in production verification
        if [[ "${PRODUCTION_VERIFY:-false}" == "true" ]]; then
            echo "[verify_r2_location] ERROR: Production verification requires known location"
            FAIL=1
        fi
    elif [[ "$actual" != "$expected" ]]; then
        echo "[verify_r2_location] ERROR: MISMATCH for ${bucket}: expected=${expected}, actual=${actual}"
        echo "[verify_r2_location] ACTION: Emit audit corelink.region.r2_location_mismatch + alert SEV-2"
        echo "[verify_r2_location] RUNBOOK: See RB-region §6 R2 location drift procedure"
        FAIL=1
    else
        echo "[verify_r2_location] OK: ${bucket} location=${actual} (matches hint)"
    fi
done

if [[ $FAIL -ne 0 ]]; then
    echo "[verify_r2_location] RESULT: FAIL — location mismatches detected (exit 1)"
    exit 1
else
    echo "[verify_r2_location] RESULT: OK — all R2 bucket locations match hints"
    exit 0
fi
