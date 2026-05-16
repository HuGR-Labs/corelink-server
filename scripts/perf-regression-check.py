#!/usr/bin/env python3
"""
perf-regression-check.py — Compare criterion bench output to a committed
baseline and fail (exit 2) if any tracked bench regresses beyond a
configurable p99 threshold.

Reads:
  - target/criterion/<bench_id>/new/estimates.json   (criterion summary)
  - target/criterion/<bench_id>/new/sample.json      (raw sample timings,
    used to compute p99 when present)
  - reports/perf/baseline-<bench>.json               (committed baseline,
    one per crate:bench; see schema below)

Baseline schema (per bench), JSON object:
  {
    "bench_id": "<crate>::<bench>::<group>",   # criterion path under target/criterion
    "crate": "<crate>",
    "bench": "<bench>",
    "captured_at": "YYYY-MM-DDTHH:MM:SSZ",
    "commit": "<git sha or 'pending'>",
    "median_ns": <float>,
    "mean_ns":   <float>,
    "p99_ns":    <float|null>,        # null when sample.json not present
    "sample_count": <int|null>,
    "criticality": "CRITICAL"|"NON_CRITICAL",  # gate tolerance class
    "tolerance_pct": <float|null>,             # optional per-bench override
    "notes": "<str>"
  }

A baseline with `median_ns == null` is tolerated as
"no baseline yet — record one" and emits a warning (not a failure).

Exit codes:
  0 — all tracked benches within threshold (or no comparable baselines)
  1 — usage / IO / schema error
  2 — at least one regression beyond threshold

Thresholds (wave-22 tightening — pre-GA pilot tenant workloads):
  --critical-threshold-pct N      (default: 5)   — CRITICAL benches
  --default-threshold-pct N       (default: 15)  — non-critical benches
  --threshold-pct N               (legacy/back-compat; overrides BOTH
                                   defaults with a single value when set)
  env PERF_REGRESS_CRITICAL_PCT, PERF_REGRESS_DEFAULT_PCT, and
  PERF_REGRESS_THRESHOLD_PCT (legacy) override the defaults when the
  matching CLI flag is not passed.

  Per-bench override: `tolerance_pct` in the baseline JSON wins over
  the criticality-based class default.

Usage:
  scripts/perf-regression-check.py
  scripts/perf-regression-check.py --threshold-pct 15
  PERF_REGRESS_THRESHOLD_PCT=5 scripts/perf-regression-check.py
  scripts/perf-regression-check.py --criterion-root target/criterion \\
                                   --baseline-dir reports/perf

Companion docs:
  specs/_runbooks/RB-PERF-REGRESSION.md
  docs/internal/PERFORMANCE-PLAYBOOK.md (§"How regression gates work")
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent


def _load_json(path: Path) -> dict | list | None:
    try:
        with path.open("r", encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        return None
    except json.JSONDecodeError as e:
        print(f"ERROR: invalid JSON at {path}: {e}", file=sys.stderr)
        sys.exit(1)


def _percentile(sorted_values: list[float], pct: float) -> float:
    if not sorted_values:
        return float("nan")
    if len(sorted_values) == 1:
        return sorted_values[0]
    k = (len(sorted_values) - 1) * (pct / 100.0)
    lo = int(k)
    hi = min(lo + 1, len(sorted_values) - 1)
    frac = k - lo
    return sorted_values[lo] * (1.0 - frac) + sorted_values[hi] * frac


def _collect_criterion_results(criterion_root: Path) -> dict[str, dict]:
    """Walk target/criterion and gather (bench_id -> metrics) for each
    `new/` directory. bench_id is the relative path to the bench group
    under criterion_root (e.g. "blake3/hash_64").
    """
    results: dict[str, dict] = {}
    if not criterion_root.is_dir():
        return results
    for dirpath, _dirnames, filenames in os.walk(criterion_root):
        if os.path.basename(dirpath) != "new":
            continue
        if "estimates.json" not in filenames:
            continue
        bench_dir = os.path.dirname(dirpath)
        bench_id = os.path.relpath(bench_dir, criterion_root).replace(os.sep, "/")
        estimates = _load_json(Path(dirpath) / "estimates.json") or {}
        median_ns = estimates.get("median", {}).get("point_estimate")
        mean_ns = estimates.get("mean", {}).get("point_estimate")
        p99_ns: float | None = None
        sample_count: int | None = None
        sample_path = Path(dirpath) / "sample.json"
        if sample_path.is_file():
            sample = _load_json(sample_path) or {}
            times = sample.get("times") or []
            iters = sample.get("iters") or []
            per_iter: list[float] = []
            for t, n in zip(times, iters):
                if n and n > 0:
                    per_iter.append(float(t) / float(n))
            if per_iter:
                per_iter.sort()
                p99_ns = _percentile(per_iter, 99.0)
                sample_count = len(per_iter)
        results[bench_id] = {
            "median_ns": median_ns,
            "mean_ns": mean_ns,
            "p99_ns": p99_ns,
            "sample_count": sample_count,
        }
    return results


def _load_baselines(baseline_dir: Path) -> dict[str, dict]:
    """Each baseline file is `baseline-<crate>-<bench>.json` OR
    `baseline-<bench>.json`. We key the in-memory map by the
    `bench_id` field inside each file, falling back to the filename
    stem when missing — this keeps the format human-editable.
    """
    out: dict[str, dict] = {}
    if not baseline_dir.is_dir():
        return out
    for p in sorted(baseline_dir.glob("baseline-*.json")):
        data = _load_json(p)
        if not isinstance(data, dict):
            print(f"WARN: skipping malformed baseline {p}", file=sys.stderr)
            continue
        bench_id = data.get("bench_id") or p.stem.replace("baseline-", "", 1)
        out[bench_id] = data
    return out


def _format_ns(v: float | None) -> str:
    if v is None:
        return "       -"
    return f"{v:>10.2f}"


def main(argv: list[str] | None = None) -> int:
    # Wave-22 tightening: split single threshold into criticality classes.
    # Back-compat: PERF_REGRESS_THRESHOLD_PCT (if set) still applies to BOTH
    # classes uniformly (the legacy single-knob behaviour).
    legacy_threshold_env = os.environ.get("PERF_REGRESS_THRESHOLD_PCT")
    default_critical = float(
        os.environ.get("PERF_REGRESS_CRITICAL_PCT")
        or legacy_threshold_env
        or "5"
    )
    default_noncritical = float(
        os.environ.get("PERF_REGRESS_DEFAULT_PCT")
        or legacy_threshold_env
        or "15"
    )
    parser = argparse.ArgumentParser(
        prog="perf-regression-check.py",
        description="Compare criterion bench output to committed baselines and fail on p99 regression.",
    )
    parser.add_argument(
        "--critical-threshold-pct",
        type=float,
        default=default_critical,
        help="Regression threshold for CRITICAL benches (default: 5; env PERF_REGRESS_CRITICAL_PCT)",
    )
    parser.add_argument(
        "--default-threshold-pct",
        type=float,
        default=default_noncritical,
        help="Regression threshold for non-critical benches (default: 15; env PERF_REGRESS_DEFAULT_PCT)",
    )
    parser.add_argument(
        "--threshold-pct",
        type=float,
        default=None,
        help="Legacy single threshold — when set, applies to both critical and non-critical (back-compat with pre-wave-22)",
    )
    parser.add_argument(
        "--criterion-root",
        type=Path,
        default=REPO_ROOT / "target" / "criterion",
        help="Path to criterion output root (default: target/criterion)",
    )
    parser.add_argument(
        "--baseline-dir",
        type=Path,
        default=REPO_ROOT / "reports" / "perf",
        help="Path to baseline JSON directory (default: reports/perf)",
    )
    parser.add_argument(
        "--metric",
        choices=["p99", "median", "mean"],
        default="p99",
        help="Which metric to gate on (default: p99 if available, else median)",
    )
    parser.add_argument(
        "--allow-missing-baseline",
        action="store_true",
        help="Treat unknown bench_ids as warnings, not failures",
    )
    parser.add_argument(
        "--json-out",
        type=Path,
        default=None,
        help="Optional JSON report path for CI artifact upload",
    )
    args = parser.parse_args(argv)

    # Resolve effective thresholds (legacy single-knob wins when provided).
    if args.threshold_pct is not None:
        critical_pct = args.threshold_pct
        noncritical_pct = args.threshold_pct
        threshold_mode = f"legacy single threshold {args.threshold_pct:.1f}%"
    else:
        critical_pct = args.critical_threshold_pct
        noncritical_pct = args.default_threshold_pct
        threshold_mode = (
            f"CRITICAL {critical_pct:.1f}% / non-critical {noncritical_pct:.1f}%"
        )

    results = _collect_criterion_results(args.criterion_root)
    baselines = _load_baselines(args.baseline_dir)

    if not results:
        print(
            f"WARN: no criterion results under {args.criterion_root}. "
            "Did `cargo bench` run? (no-op exit 0)",
            file=sys.stderr,
        )
        return 0

    if not baselines:
        print(
            f"WARN: no baselines under {args.baseline_dir}. "
            "Run scripts/refresh-perf-baseline.sh and commit. (no-op exit 0)",
            file=sys.stderr,
        )
        return 0

    print(f"==> perf regression check ({threshold_mode} on {args.metric})")
    print(f"{'bench_id':<60} {'class':>14} {'baseline':>12} {'current':>12} {'delta':>10}")

    regressions: list[tuple[str, float, float, float, float, str]] = []
    compared = 0
    missing_results: list[str] = []
    missing_baselines: list[str] = []
    pending_baselines: list[str] = []
    report_entries: list[dict] = []

    for bench_id in sorted(set(baselines) | set(results)):
        base = baselines.get(bench_id)
        cur = results.get(bench_id)
        if base is None:
            missing_baselines.append(bench_id)
            continue
        if cur is None:
            missing_results.append(bench_id)
            continue

        def _pick(d: dict) -> float | None:
            key = f"{args.metric}_ns"
            v = d.get(key)
            if v is not None:
                return float(v)
            # fall back to median when p99 unavailable
            v = d.get("median_ns")
            return float(v) if v is not None else None

        base_v = _pick(base)
        cur_v = _pick(cur)

        # Resolve per-bench tolerance class & threshold.
        # Priority: explicit `tolerance_pct` override > class-default
        # (CRITICAL | NON_CRITICAL) > non-critical fallback.
        criticality_raw = str(base.get("criticality") or "NON_CRITICAL").upper()
        if criticality_raw not in ("CRITICAL", "NON_CRITICAL"):
            print(
                f"WARN: bench {bench_id} has unknown criticality "
                f"{criticality_raw!r}; treating as NON_CRITICAL",
                file=sys.stderr,
            )
            criticality_raw = "NON_CRITICAL"
        class_default = critical_pct if criticality_raw == "CRITICAL" else noncritical_pct
        # Per-bench tolerance_pct override applies only in split-threshold
        # mode. Legacy single-threshold mode (--threshold-pct) is meant as
        # a global escape hatch and ignores per-bench overrides.
        tol_override = base.get("tolerance_pct")
        if args.threshold_pct is not None:
            effective_threshold = args.threshold_pct
        else:
            effective_threshold = (
                float(tol_override) if tol_override is not None else class_default
            )

        if base_v is None:
            pending_baselines.append(bench_id)
            print(
                f"{bench_id:<60} {criticality_raw:>14} {'pending':>12} "
                f"{_format_ns(cur_v):>12} {'(record)':>10}"
            )
            report_entries.append({
                "bench_id": bench_id,
                "criticality": criticality_raw,
                "threshold_pct": effective_threshold,
                "status": "pending_baseline",
                "current_ns": cur_v,
            })
            continue
        if cur_v is None:
            print(
                f"WARN: bench {bench_id} has baseline but no current metric; skipping",
                file=sys.stderr,
            )
            continue

        delta_pct = ((cur_v - base_v) / base_v) * 100.0 if base_v != 0 else 0.0
        compared += 1
        marker = ""
        if delta_pct > effective_threshold:
            regressions.append(
                (bench_id, base_v, cur_v, delta_pct, effective_threshold, criticality_raw)
            )
            marker = "  REGRESS"
        report_entries.append({
            "bench_id": bench_id,
            "criticality": criticality_raw,
            "threshold_pct": effective_threshold,
            "status": "regression" if marker else "ok",
            "baseline_ns": base_v,
            "current_ns": cur_v,
            "delta_pct": delta_pct,
        })
        print(
            f"{bench_id:<60} {criticality_raw:>14} {_format_ns(base_v):>12} "
            f"{_format_ns(cur_v):>12} {delta_pct:>+9.1f}%{marker}"
        )

    for b in pending_baselines:
        print(f"NOTE: baseline for '{b}' is pending — record via scripts/refresh-perf-baseline.sh", file=sys.stderr)
    for b in missing_baselines:
        msg = f"no baseline for '{b}'"
        if args.allow_missing_baseline:
            print(f"WARN: {msg}", file=sys.stderr)
        else:
            print(f"NOTE: {msg} (use --allow-missing-baseline to silence)", file=sys.stderr)
    for b in missing_results:
        print(f"WARN: baseline '{b}' has no current criterion result", file=sys.stderr)

    if args.json_out is not None:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        with args.json_out.open("w", encoding="utf-8") as f:
            json.dump(
                {
                    "critical_threshold_pct": critical_pct,
                    "default_threshold_pct": noncritical_pct,
                    "legacy_threshold_pct": args.threshold_pct,
                    "threshold_mode": threshold_mode,
                    "metric": args.metric,
                    "compared": compared,
                    "regressions": [
                        {
                            "bench_id": r[0],
                            "baseline_ns": r[1],
                            "current_ns": r[2],
                            "delta_pct": r[3],
                            "effective_threshold_pct": r[4],
                            "criticality": r[5],
                        }
                        for r in regressions
                    ],
                    "entries": report_entries,
                },
                f,
                indent=2,
                sort_keys=True,
            )

    if regressions:
        print()
        print(f"==> FAIL: {len(regressions)} bench(es) regressed on {args.metric} ({threshold_mode})")
        for bench_id, base_v, cur_v, delta, eff, klass in regressions:
            print(
                f"   - [{klass}] {bench_id}: {delta:+.1f}% > {eff:.1f}%  "
                f"(baseline={base_v:.2f}ns  current={cur_v:.2f}ns)"
            )
        print()
        print("See specs/_runbooks/RB-PERF-REGRESSION.md for triage.")
        return 2

    print()
    print(f"==> OK: {compared} bench(es) within threshold ({threshold_mode}); "
          f"{len(pending_baselines)} pending baseline(s); "
          f"{len(missing_baselines)} unknown bench_id(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
