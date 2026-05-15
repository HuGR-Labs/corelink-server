#!/usr/bin/env bash
# refresh-perf-baseline.sh — operator helper to (re)record the
# perf-regression baseline manifest under reports/perf/.
#
# Flow:
#   1. Run the criterion suite for every tracked (crate,bench) pair.
#   2. Read target/criterion/<bench>/new/{estimates,sample}.json.
#   3. Write reports/perf/baseline-<crate>-<bench>.json with median_ns,
#      mean_ns, p99_ns, captured_at, commit.
#   4. Print a summary; the operator is expected to `git add reports/perf/*.json`
#      and commit with a message that names the WI / PR / rationale.
#
# This script DOES NOT auto-commit. It only writes files.
#
# Usage:
#   scripts/refresh-perf-baseline.sh            # run benches + write baselines
#   scripts/refresh-perf-baseline.sh --dry-run  # print planned writes, no benches
#   scripts/refresh-perf-baseline.sh --help
#
# Env:
#   PERF_BENCH_MEASUREMENT_TIME (default: 5)  — criterion measurement seconds
#   PERF_BENCH_WARMUP_TIME      (default: 1)  — criterion warmup seconds
#   PERF_BASELINE_NOTES         (default: "")  — appended to every baseline file
#
# Companion: scripts/perf-regression-check.py
#            specs/_runbooks/RB-PERF-REGRESSION.md
#            docs/internal/PERFORMANCE-PLAYBOOK.md §"How regression gates work"

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

DRY_RUN=0
for arg in "$@"; do
  case "${arg}" in
    --dry-run)
      DRY_RUN=1
      ;;
    -h|--help)
      sed -n '2,28p' "${BASH_SOURCE[0]}"
      exit 0
      ;;
    *)
      echo "unknown arg: ${arg}" >&2
      exit 1
      ;;
  esac
done

MEASUREMENT_TIME="${PERF_BENCH_MEASUREMENT_TIME:-5}"
WARMUP_TIME="${PERF_BENCH_WARMUP_TIME:-1}"
NOTES="${PERF_BASELINE_NOTES:-}"

BASELINE_DIR="${REPO_ROOT}/reports/perf"
mkdir -p "${BASELINE_DIR}"

# Tracked benches — keep in lockstep with scripts/run-perf-baseline.sh.
# Format: <crate>:<bench-target>
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

COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
CAPTURED_AT="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"

if [[ "${DRY_RUN}" -eq 1 ]]; then
  echo "==> DRY-RUN: would refresh ${#BENCHES[@]} baseline(s) into ${BASELINE_DIR}"
  for entry in "${BENCHES[@]}"; do
    crate="${entry%%:*}"
    bench="${entry##*:}"
    echo "   - ${BASELINE_DIR}/baseline-${crate}-${bench}.json"
  done
  exit 0
fi

echo "==> Refreshing ${#BENCHES[@]} bench baseline(s)"
echo "    warmup=${WARMUP_TIME}s  measure=${MEASUREMENT_TIME}s  commit=${COMMIT}"

for entry in "${BENCHES[@]}"; do
  crate="${entry%%:*}"
  bench="${entry##*:}"
  echo ""
  echo "---- ${crate} :: ${bench} ----"
  cargo bench -p "${crate}" --bench "${bench}" -- \
    --warm-up-time "${WARMUP_TIME}" \
    --measurement-time "${MEASUREMENT_TIME}"
done

echo ""
echo "==> Writing baselines under ${BASELINE_DIR}"

CRIT_ROOT="${REPO_ROOT}/target/criterion"
COMMIT="${COMMIT}" CAPTURED_AT="${CAPTURED_AT}" NOTES="${NOTES}" \
  python3 - "${CRIT_ROOT}" "${BASELINE_DIR}" "${BENCHES[@]}" <<'PY'
import json
import os
import sys
from pathlib import Path

crit_root = Path(sys.argv[1])
baseline_dir = Path(sys.argv[2])
benches = sys.argv[3:]
commit = os.environ.get("COMMIT", "unknown")
captured_at = os.environ.get("CAPTURED_AT", "")
notes = os.environ.get("NOTES", "")


def percentile(sorted_vals, pct):
    if not sorted_vals:
        return None
    if len(sorted_vals) == 1:
        return sorted_vals[0]
    k = (len(sorted_vals) - 1) * (pct / 100.0)
    lo = int(k)
    hi = min(lo + 1, len(sorted_vals) - 1)
    frac = k - lo
    return sorted_vals[lo] * (1.0 - frac) + sorted_vals[hi] * frac


def best_match(crate, bench):
    """Find the deepest 'new' directory whose relative path mentions
    either the bench name or the crate. Criterion bench groups can
    nest (e.g. blake3/hash_64/new), so we prefer the shortest match
    rooted at the bench/crate slug."""
    if not crit_root.is_dir():
        return None
    candidates = []
    for dirpath, _dirs, files in os.walk(crit_root):
        if os.path.basename(dirpath) != "new":
            continue
        if "estimates.json" not in files:
            continue
        bench_dir = os.path.dirname(dirpath)
        rel = os.path.relpath(bench_dir, crit_root).replace(os.sep, "/")
        first = rel.split("/", 1)[0]
        if first == bench or first == crate or bench in rel:
            candidates.append((len(rel), rel, dirpath))
    if not candidates:
        return None
    candidates.sort()
    return candidates[0][2]


written = 0
pending = 0
for entry in benches:
    crate, _, bench = entry.partition(":")
    new_dir = best_match(crate, bench)
    out_path = baseline_dir / f"baseline-{crate}-{bench}.json"

    payload = {
        "bench_id": f"{bench}",
        "crate": crate,
        "bench": bench,
        "captured_at": captured_at,
        "commit": commit,
        "median_ns": None,
        "mean_ns": None,
        "p99_ns": None,
        "sample_count": None,
        "notes": notes,
    }

    if new_dir is None:
        payload["notes"] = (notes + " | no criterion output found — baseline pending").strip(" |")
        pending += 1
    else:
        rel = os.path.relpath(os.path.dirname(new_dir), crit_root).replace(os.sep, "/")
        payload["bench_id"] = rel
        est_path = Path(new_dir) / "estimates.json"
        sample_path = Path(new_dir) / "sample.json"
        with est_path.open() as f:
            est = json.load(f)
        payload["median_ns"] = est.get("median", {}).get("point_estimate")
        payload["mean_ns"] = est.get("mean", {}).get("point_estimate")
        if sample_path.is_file():
            with sample_path.open() as f:
                sample = json.load(f)
            per_iter = []
            for t, n in zip(sample.get("times", []), sample.get("iters", [])):
                if n and n > 0:
                    per_iter.append(float(t) / float(n))
            if per_iter:
                per_iter.sort()
                payload["p99_ns"] = percentile(per_iter, 99.0)
                payload["sample_count"] = len(per_iter)

    out_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    written += 1
    status = "pending" if payload["median_ns"] is None else "ok"
    print(f"   {status:>8}  {out_path.relative_to(baseline_dir.parent.parent)}")

print()
print(f"==> Wrote {written} baseline(s); {pending} pending (no criterion data).")
print("    Review with: git diff reports/perf/")
print("    Commit with: git add reports/perf && git commit -m 'chore(perf): refresh baselines'")
PY
