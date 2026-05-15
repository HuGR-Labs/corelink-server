#!/usr/bin/env bash
# run-perf-baseline.sh — run the full criterion bench suite and compare
# the median estimates against the persisted JSON baseline.
#
# Exit codes:
#   0  — all benches within tolerance.
#   1  — usage / environment error.
#   2  — at least one bench regressed beyond REGRESS_PCT (default 20%).
#
# Outputs:
#   target/criterion/<bench>/...  (criterion HTML + JSON reports)
#   reports/perf/baseline.json    (rolled forward on `--update`)
#
# Usage:
#   scripts/run-perf-baseline.sh               # run, compare, fail on regression.
#   scripts/run-perf-baseline.sh --update      # write the latest run as baseline.
#   REGRESS_PCT=10 scripts/run-perf-baseline.sh  # tighter regression tolerance.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

REGRESS_PCT="${REGRESS_PCT:-20}"
BASELINE_FILE="${BASELINE_FILE:-reports/perf/baseline.json}"
UPDATE_BASELINE=0

for arg in "$@"; do
  case "${arg}" in
    --update) UPDATE_BASELINE=1 ;;
    -h|--help)
      sed -n '2,18p' "${BASH_SOURCE[0]}"
      exit 0
      ;;
    *)
      echo "unknown arg: ${arg}" >&2
      exit 1
      ;;
  esac
done

mkdir -p "$(dirname "${BASELINE_FILE}")"

# The benches we own. Format: <crate>:<bench-name>
BENCHES=(
  "corelink-tenant-path:derive"
  "corelink-tenant-path:derive_prefix_v2"
  "corelink-hash:blake3_bench"
  "corelink-hash:blake3"
  "corelink-byok:envelope_roundtrip"
  "corelink-audit-chain:merkle_append"
  "corelink-audit-chain:jcs_canonicalize"
  "corelink-signup:orchestrator"
  "corelink-tier-selection:select"
  "corelink-dpa-acceptance:accept_and_verify_jwt"
  "corelink-stripe-real:webhook_verify"
)

echo "==> Running ${#BENCHES[@]} bench targets (regression tolerance: ${REGRESS_PCT}%)"

for entry in "${BENCHES[@]}"; do
  crate="${entry%%:*}"
  bench="${entry##*:}"
  echo ""
  echo "---- ${crate} :: ${bench} ----"
  cargo bench -p "${crate}" --bench "${bench}" -- --warm-up-time 1 --measurement-time 5
done

# Collect new estimates into a flat JSON.
NEW_FILE="$(mktemp)"
trap 'rm -f "${NEW_FILE}"' EXIT

python3 - "${NEW_FILE}" "${REPO_ROOT}/target/criterion" <<'PY'
import json
import os
import sys
out_path = sys.argv[1]
root = sys.argv[2]
results = {}
if os.path.isdir(root):
    for dirpath, _dirnames, filenames in os.walk(root):
        if "estimates.json" in filenames and os.path.basename(dirpath) == "new":
            est = json.load(open(os.path.join(dirpath, "estimates.json")))
            # Bench id is the parent of "new".
            bench_id = os.path.relpath(os.path.dirname(dirpath), root)
            median = est.get("median", {}).get("point_estimate")
            if median is not None:
                results[bench_id] = median
json.dump(results, open(out_path, "w"), indent=2, sort_keys=True)
PY

if [[ "${UPDATE_BASELINE}" -eq 1 ]]; then
  cp "${NEW_FILE}" "${BASELINE_FILE}"
  echo ""
  echo "==> Baseline updated at ${BASELINE_FILE}"
  exit 0
fi

if [[ ! -f "${BASELINE_FILE}" ]]; then
  echo ""
  echo "==> No baseline at ${BASELINE_FILE}; writing initial baseline (first run)."
  cp "${NEW_FILE}" "${BASELINE_FILE}"
  exit 0
fi

# Compare.
python3 - "${BASELINE_FILE}" "${NEW_FILE}" "${REGRESS_PCT}" <<'PY'
import json
import sys
baseline = json.load(open(sys.argv[1]))
current = json.load(open(sys.argv[2]))
threshold = float(sys.argv[3])
regressions = []
print()
print("==> Comparison report (median estimate, lower is better)")
print(f"{'bench':<70} {'baseline':>14} {'current':>14} {'delta':>10}")
for bench in sorted(set(baseline) | set(current)):
    base = baseline.get(bench)
    cur = current.get(bench)
    if base is None or cur is None:
        delta_s = "  (n/a)"
    else:
        delta_pct = ((cur - base) / base) * 100.0 if base != 0 else 0.0
        delta_s = f"{delta_pct:+.1f}%"
        if delta_pct > threshold:
            regressions.append((bench, base, cur, delta_pct))
    base_s = f"{base:.2f}" if base is not None else "      -"
    cur_s = f"{cur:.2f}" if cur is not None else "      -"
    print(f"{bench:<70} {base_s:>14} {cur_s:>14} {delta_s:>10}")
if regressions:
    print()
    print(f"==> FAIL: {len(regressions)} bench(es) regressed > {threshold}%")
    for r in regressions:
        print(f"    - {r[0]}: {r[3]:+.1f}% (baseline={r[1]:.2f} → current={r[2]:.2f})")
    sys.exit(2)
print()
print("==> OK: no regression beyond threshold.")
PY
