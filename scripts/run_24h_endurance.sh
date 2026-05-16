#!/usr/bin/env bash
# run_24h_endurance.sh — orchestrates the wave-22 24h endurance campaign.
#
# Usage:
#   scripts/run_24h_endurance.sh smoke            # 30s local smoke (default)
#   scripts/run_24h_endurance.sh dressrun         # 10min wave-25 dress-run
#   scripts/run_24h_endurance.sh nightly          # 2h CI variant
#   scripts/run_24h_endurance.sh full             # 24h manual drill (PD-paged)
#
# Required env (full / nightly):
#   K6_TARGET_HOST      e.g. https://staging.corelink.dev
#   K6_AUTH_BEARER      staging PAT scoped to load-test tenant
#
# For smoke runs the script defaults to http://127.0.0.1:8787 with a stub PAT.
# If the target endpoint is unreachable the smoke run is reported as
# `smoke=red[unreachable]` (exit code 0) so the harness wiring itself is
# verifiable on a dev box without a running corelink-server.
#
# Output layout:
#   tests/load/results/<UTC-stamp>/
#     - k6-summary.json
#     - k6-stdout.log
#     - run-meta.json
#     - analysis.md   (after scripts/analyze_endurance_run.py)
#
# This script is the canonical entrypoint cited by RB-24H-ENDURANCE-LOAD.md
# and specs/_audits/2026-05-16-24h-endurance-harness.md.

set -euo pipefail

MODE="${1:-smoke}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_DIR="$REPO_ROOT/scripts"
K6_SCRIPT="$REPO_ROOT/tests/load/k6/scenarios/endurance-24h-w22.js"
TS="$(date -u +%Y%m%dT%H%M%SZ)"
RESULTS_DIR="$REPO_ROOT/tests/load/results/$TS-$MODE"
mkdir -p "$RESULTS_DIR"

case "$MODE" in
  smoke)
    export DURATION="${DURATION:-30s}"
    export K6_TARGET_HOST="${K6_TARGET_HOST:-http://127.0.0.1:8787}"
    export K6_AUTH_BEARER="${K6_AUTH_BEARER:-stub-staging-pat}"
    export K6_ENDURANCE_CONFIRM="${K6_ENDURANCE_CONFIRM:-no}"
    ;;
  dressrun)
    # Wave-25 dress-run: exercises the 10-min profile (2 + 6 + 2) at
    # 100 RPS against a local in-memory mock or cargo-run binary.
    # NOT a substitute for the 24h drill — purpose is to validate the
    # harness + analyzer plumbing end-to-end.
    export DURATION="${DURATION:-10min}"
    export K6_PROFILE="${K6_PROFILE:-10min}"
    export K6_TARGET_HOST="${K6_TARGET_HOST:-http://127.0.0.1:8787}"
    export K6_AUTH_BEARER="${K6_AUTH_BEARER:-stub-staging-pat}"
    export K6_ENDURANCE_CONFIRM="${K6_ENDURANCE_CONFIRM:-no}"
    ;;
  nightly)
    export DURATION="2h"
    export K6_ENDURANCE_CONFIRM="yes"
    : "${K6_TARGET_HOST:?K6_TARGET_HOST required for nightly}"
    : "${K6_AUTH_BEARER:?K6_AUTH_BEARER required for nightly}"
    ;;
  full)
    export DURATION="24h"
    export K6_ENDURANCE_CONFIRM="yes"
    : "${K6_TARGET_HOST:?K6_TARGET_HOST required for full 24h run}"
    : "${K6_AUTH_BEARER:?K6_AUTH_BEARER required for full 24h run}"
    if [[ "${K6_TARGET_HOST}" != *"staging."* ]]; then
      echo "[fatal] full 24h drill must target staging.*; refusing." >&2
      exit 2
    fi
    ;;
  *)
    echo "usage: $0 {smoke|dressrun|nightly|full}" >&2
    exit 64
    ;;
esac

cat > "$RESULTS_DIR/run-meta.json" <<EOF
{
  "mode": "$MODE",
  "started_at_utc": "$TS",
  "duration": "$DURATION",
  "target_host": "$K6_TARGET_HOST",
  "k6_script": "tests/load/k6/scenarios/endurance-24h-w22.js",
  "fixtures": "tests/load/fixtures/customer-routes.ndjson",
  "operator": "${USER:-unknown}",
  "host": "$(hostname -s 2>/dev/null || echo unknown)"
}
EOF

echo "[run_24h_endurance] mode=$MODE duration=$DURATION target=$K6_TARGET_HOST"
echo "[run_24h_endurance] results -> $RESULTS_DIR"

# Preflight: probe target. For smoke against 127.0.0.1 we ALLOW failure
# so the harness can be exercised on a clean dev box.
PREFLIGHT_OK=1
if command -v curl >/dev/null 2>&1; then
  if ! curl -sS -o /dev/null -m 3 -w "%{http_code}" "$K6_TARGET_HOST/healthz" >/dev/null 2>&1; then
    PREFLIGHT_OK=0
  fi
fi

if [[ "$PREFLIGHT_OK" == "0" ]]; then
  if [[ "$MODE" == "smoke" || "$MODE" == "dressrun" ]]; then
    echo "[run_24h_endurance] target unreachable; $MODE run reports red[unreachable]." >&2
    {
      echo "preflight=unreachable"
      echo "$MODE=red[unreachable]"
    } > "$RESULTS_DIR/smoke-status.txt"
    cat > "$RESULTS_DIR/k6-summary.json" <<EOF
{"preflight":"unreachable","mode":"$MODE","duration":"$DURATION","target":"$K6_TARGET_HOST"}
EOF
    echo "[run_24h_endurance] DONE ($MODE unreachable; harness wiring still verifiable)"
    # Still run the analyzer so the verdict path is exercised.
    if [[ -x "$SCRIPT_DIR/analyze_endurance_run.py" ]]; then
      python3 "$SCRIPT_DIR/analyze_endurance_run.py" \
        --run-dir "$RESULTS_DIR" \
        --out "$RESULTS_DIR/analysis.md" || true
    fi
    exit 0
  fi
  echo "[fatal] target $K6_TARGET_HOST unreachable for non-smoke mode." >&2
  exit 3
fi

# Run k6 if installed; otherwise simulate (smoke only).
if command -v k6 >/dev/null 2>&1; then
  set +e
  k6 run \
    --summary-export "$RESULTS_DIR/k6-summary.json" \
    --tag mode="$MODE" \
    "$K6_SCRIPT" \
    2>&1 | tee "$RESULTS_DIR/k6-stdout.log"
  K6_RC=${PIPESTATUS[0]}
  set -e
  echo "k6_rc=$K6_RC" >> "$RESULTS_DIR/run-meta.json.partial" || true
else
  if [[ "$MODE" == "smoke" ]]; then
    echo "[run_24h_endurance] k6 not installed; smoke run records harness wiring only." >&2
    cat > "$RESULTS_DIR/k6-summary.json" <<EOF
{"preflight":"ok","mode":"smoke","duration":"$DURATION","target":"$K6_TARGET_HOST","k6":"missing"}
EOF
    {
      echo "preflight=ok"
      echo "k6=missing"
      echo "smoke=green[harness-verified]"
    } > "$RESULTS_DIR/smoke-status.txt"
  else
    echo "[fatal] k6 not installed; required for $MODE mode." >&2
    exit 4
  fi
fi

# Run analysis (best-effort — analyze script emits its own verdict).
if [[ -x "$SCRIPT_DIR/analyze_endurance_run.py" ]]; then
  python3 "$SCRIPT_DIR/analyze_endurance_run.py" \
    --run-dir "$RESULTS_DIR" \
    --out "$RESULTS_DIR/analysis.md" || echo "[run_24h_endurance] analysis step non-zero (continuing)"
fi

echo "[run_24h_endurance] DONE -> $RESULTS_DIR"
