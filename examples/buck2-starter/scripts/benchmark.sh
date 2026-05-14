#!/usr/bin/env bash
# benchmark.sh — Buck2 remote-cache benchmark (parity with bazel-starter)
#
# Usage:
#   export CORELINK_PAT=<your-token>
#   ./scripts/benchmark.sh [--iterations N] [--output-md PATH]
#
# Output:
#   STDOUT: summary table (median + p95 + cache-hit ratio)
#   FILE:   BENCHMARK.md updated with new run results (default: ../BENCHMARK.md)
#
# Requirements: buck2, jq, bc, date
#
# Exit codes:
#   0 — benchmark completed; cache hit ratio >= 80 %
#   1 — argument / dependency error
#   2 — cache hit ratio < 80 % (threshold miss)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STARTER_DIR="$(dirname "${SCRIPT_DIR}")"

# ── defaults ────────────────────────────────────────────────────────────────
ITERATIONS=10
OUTPUT_MD="${STARTER_DIR}/BENCHMARK.md"
TARGET=":hello"
HIT_THRESHOLD=80  # percent; matches WI-S15-003 AC §8

# ── argument parsing ─────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --iterations) ITERATIONS="$2"; shift 2 ;;
        --output-md)  OUTPUT_MD="$2";  shift 2 ;;
        *) echo "Unknown flag: $1" >&2; exit 1 ;;
    esac
done

# ── dependency check ─────────────────────────────────────────────────────────
for cmd in buck2 jq bc; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "ERROR: '$cmd' not found in PATH." >&2
        exit 1
    fi
done

if [[ -z "${CORELINK_PAT:-}" ]]; then
    echo "ERROR: CORELINK_PAT environment variable is not set." >&2
    echo "  Set it with: export CORELINK_PAT=<your-token>" >&2
    exit 1
fi

cd "${STARTER_DIR}"

# ── helper: run one build and return elapsed seconds ─────────────────────────
run_build() {
    local log_file
    log_file="$(mktemp /tmp/buck2-build-XXXXXX.log)"
    local start end elapsed
    start=$(date +%s%3N)  # milliseconds
    buck2 build "${TARGET}" --build-report "${log_file}.report.json" \
        2>>"${log_file}" || true
    end=$(date +%s%3N)
    elapsed=$(( end - start ))
    echo "${elapsed} ${log_file}.report.json"
}

# ── helper: extract cache hits from buck2 build report ───────────────────────
cache_hits_from_report() {
    local report="$1"
    if [[ -f "${report}" ]]; then
        # Buck2 build report JSON: .cache_hits and .action_count fields
        jq -r '(.cache_hits // 0) | tostring' "${report}" 2>/dev/null || echo "0"
    else
        echo "0"
    fi
}

total_from_report() {
    local report="$1"
    if [[ -f "${report}" ]]; then
        jq -r '(.total_actions // 1) | tostring' "${report}" 2>/dev/null || echo "1"
    else
        echo "1"
    fi
}

# ── cold-cache runs ──────────────────────────────────────────────────────────
echo "=== Buck2 Remote Cache Benchmark ==="
echo "Target: ${TARGET}"
echo "Iterations: ${ITERATIONS} cold + ${ITERATIONS} warm"
echo ""

cold_times=()
echo "--- Cold-cache runs (buck2 clean before each) ---"
for i in $(seq 1 "${ITERATIONS}"); do
    buck2 clean 2>/dev/null || true
    read -r ms report_file <<<"$(run_build)"
    cold_times+=("${ms}")
    rm -f "${report_file}" 2>/dev/null || true
    printf "  cold run %2d: %s ms\n" "${i}" "${ms}"
done

# ── warm-cache runs ───────────────────────────────────────────────────────────
warm_times=()
total_hits=0
total_actions=0

echo ""
echo "--- Warm-cache runs (buck2 clean; remote cache populated) ---"
for i in $(seq 1 "${ITERATIONS}"); do
    buck2 clean 2>/dev/null || true
    read -r ms report_file <<<"$(run_build)"
    warm_times+=("${ms}")
    hits=$(cache_hits_from_report "${report_file}")
    actions=$(total_from_report "${report_file}")
    total_hits=$(( total_hits + hits ))
    total_actions=$(( total_actions + actions ))
    rm -f "${report_file}" 2>/dev/null || true
    printf "  warm run %2d: %s ms  (hits=%s / total=%s)\n" "${i}" "${ms}" "${hits}" "${actions}"
done

# ── compute statistics ────────────────────────────────────────────────────────
median() {
    local arr=("$@")
    local sorted
    IFS=$'\n' sorted=($(sort -n <<< "${arr[*]}")); unset IFS
    local count=${#sorted[@]}
    local mid=$(( count / 2 ))
    if (( count % 2 == 0 )); then
        echo $(( ( sorted[mid-1] + sorted[mid] ) / 2 ))
    else
        echo "${sorted[mid]}"
    fi
}

p95() {
    local arr=("$@")
    local sorted
    IFS=$'\n' sorted=($(sort -n <<< "${arr[*]}")); unset IFS
    local count=${#sorted[@]}
    local idx=$(( (count * 95) / 100 ))
    echo "${sorted[idx]}"
}

cold_median=$(median "${cold_times[@]}")
cold_p95=$(p95    "${cold_times[@]}")
warm_median=$(median "${warm_times[@]}")
warm_p95=$(p95    "${warm_times[@]}")

cache_ratio=0
if (( total_actions > 0 )); then
    cache_ratio=$(( (total_hits * 100) / total_actions ))
fi

# ── display summary ───────────────────────────────────────────────────────────
echo ""
echo "=== Summary ==="
printf "  Cold  — median: %s ms  p95: %s ms\n" "${cold_median}" "${cold_p95}"
printf "  Warm  — median: %s ms  p95: %s ms\n" "${warm_median}" "${warm_p95}"
printf "  Cache hit ratio: %s%% (threshold: %s%%)\n" "${cache_ratio}" "${HIT_THRESHOLD}"
echo ""

# ── write BENCHMARK.md ────────────────────────────────────────────────────────
RUN_DATE=$(date -u +"%Y-%m-%d %H:%M UTC")
BUCK2_VERSION=$(buck2 --version 2>/dev/null | head -1 || echo "unknown")

cat > "${OUTPUT_MD}" <<EOF
# Buck2 Remote Cache Benchmark — CoreLink Starter

> Last updated: ${RUN_DATE}

## Configuration

| Field | Value |
|---|---|
| Target | \`${TARGET}\` |
| Buck2 version | \`${BUCK2_VERSION}\` |
| Iterations | ${ITERATIONS} cold + ${ITERATIONS} warm |
| Hit threshold | ≥ ${HIT_THRESHOLD}% |

## Results

| Metric | Cold (no cache) | Warm (remote cache) |
|---|---|---|
| Median | ${cold_median} ms | ${warm_median} ms |
| p95    | ${cold_p95} ms   | ${warm_p95} ms    |
| Cache hit ratio | — | **${cache_ratio}%** |

## Speedup

$(echo "scale=1; ${cold_median} / (${warm_median} + 1)" | bc)× median speedup (cold → warm).

## Notes

- Cold runs: \`buck2 clean\` before each build; remote cache populated during warm phase.
- Cache hit ratio derived from Buck2 build report JSON (\`cache_hits / total_actions\`).
- Threshold ≥ 80% maps to WI-S15-003 AC §8 + sprint contract R-S15-8.
- Run this script weekly to track regression: \`./scripts/benchmark.sh\`.

## Parity vs Bazel starter (WI-S15-002)

See \`docs/integrations/bazel-vs-buck2.md\` for apples-to-apples comparison methodology.
EOF

echo "BENCHMARK.md written to: ${OUTPUT_MD}"

# ── exit code based on threshold ─────────────────────────────────────────────
if (( cache_ratio < HIT_THRESHOLD )); then
    echo "WARN: cache hit ratio ${cache_ratio}% is below threshold ${HIT_THRESHOLD}%." >&2
    exit 2
fi

echo "OK: cache hit ratio ${cache_ratio}% >= threshold ${HIT_THRESHOLD}%."
exit 0
