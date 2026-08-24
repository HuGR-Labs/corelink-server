#!/usr/bin/env python3
"""
load-test-baseline-check.py — real regression gate for the k6 load suite.

WHY THIS SCRIPT EXISTS
----------------------
`.github/workflows/load-test-nightly.yml` used to declare
`REGRESSION_THRESHOLD = 1.20` inside an inline heredoc, never reference it,
print `baseline regression check complete (advisory mode)` and exit 0. The
"baseline lookup" was a comment saying the implementation lived in a follow-up
WI, and the only artifacts it downloaded were the CURRENT run's own — so there
was nothing to compare against. A green check that asserted nothing about
performance, in a repo whose product claim is speed (BACKLOG B-029).

This script is the comparison the workflow claimed to do. It is a FILE, not a
heredoc, so the gate can be executed and proven locally.

METRIC: MEDIAN, NOT p99
-----------------------
The job name used to promise "p99 ≤ +20%". It gates on the MEDIAN instead, for
the same measured reason `perf-regression.yml` does (see its `perfcheck` step
comment): on shared/noisy CI hosts p99 jitters ~22% run-to-run on identical
work while the median holds to <4%, so a 20% threshold on p99 is a coin flip
and a 20% threshold on the median is a real signal. p99 is still RECORDED in
the baseline and PRINTED every run for trend review — it just does not decide
the exit code.

Both stats come from k6's `--summary-export` output; every scenario script in
`tests/load/k6/` sets
`summaryTrendStats: ['avg','min','med','p(90)','p(95)','p(99)','max']`, so
`metrics.http_req_duration.med` and `.p(99)` are always present. No metric is
invented here.

BASELINE PERSISTENCE
--------------------
The baseline is a single JSON file carried across runs by the GitHub Actions
cache (`actions/cache/restore` with a prefix restore-key, then
`actions/cache/save` under a run-unique key). Restore therefore returns the
most recent PREVIOUS run's file; save publishes this run's.

Ratchet policy: the baseline is rewritten only when the comparison PASSES. A
regressing run leaves the old baseline in place, so a regression cannot become
the new normal by simply being measured twice.

EXIT CODES
----------
  0 — every compared scenario within threshold, or baseline seeded (first run)
  1 — usage / IO / schema error
  2 — at least one scenario regressed beyond REGRESSION_THRESHOLD, or the run
      produced no parseable k6 summary at all

Note on exit 2 for "no results": an empty result set is not evidence of health.
The k6 matrix legs carry `continue-on-error: true`, so a suite that failed to
produce a single summary.json would otherwise land here as a silent green —
exactly the defect this script exists to remove.

stdlib-only on purpose: the self-hosted fleets have no guaranteed third-party
python packages.
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
import os
import pathlib
import sys

BASELINE_SCHEMA = 1

# Default regression threshold: a scenario fails when its current median is
# more than 20% above the stored baseline median. Overridable by
# --threshold / $REGRESSION_THRESHOLD so the workflow owns the number.
DEFAULT_REGRESSION_THRESHOLD = 1.20

EXIT_OK = 0
EXIT_USAGE = 1
EXIT_REGRESSION = 2


def _now() -> str:
    return _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def collect_current(results_dir: pathlib.Path) -> dict[str, dict[str, float]]:
    """Read every `summary.json` under results_dir into {scenario: stats}."""
    out: dict[str, dict[str, float]] = {}
    if not results_dir.exists():
        return out
    for summary in sorted(results_dir.rglob("summary.json")):
        try:
            data = json.loads(summary.read_text())
        except (OSError, ValueError) as exc:
            print(f"::warning::failed to parse {summary}: {exc}")
            continue
        dur = ((data.get("metrics") or {}).get("http_req_duration") or {})
        median = dur.get("med")
        p99 = dur.get("p(99)")
        if median is None:
            print(f"::warning::{summary} has no http_req_duration.med — skipped")
            continue
        scenario = summary.parent.name
        out[scenario] = {
            "median_ms": float(median),
            "p99_ms": float(p99) if p99 is not None else None,
        }
    return out


def load_baseline(path: pathlib.Path) -> dict[str, dict[str, float]]:
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text())
    except (OSError, ValueError) as exc:
        print(f"::warning::baseline {path} unreadable ({exc}) — treating as absent")
        return {}
    scenarios = data.get("scenarios")
    if not isinstance(scenarios, dict):
        print(f"::warning::baseline {path} has no `scenarios` map — treating as absent")
        return {}
    return scenarios


def write_baseline(
    path: pathlib.Path,
    scenarios: dict[str, dict[str, float]],
    commit: str,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(
            {
                "schema": BASELINE_SCHEMA,
                "captured_at": _now(),
                "commit": commit,
                "metric": "http_req_duration.med (ms)",
                "scenarios": scenarios,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--results-dir",
        default="tests/load/results/current",
        help="directory containing this run's k6 summary.json files",
    )
    ap.add_argument(
        "--baseline",
        default="tests/load/baseline/k6-baseline.json",
        help="path to the restored baseline JSON (may not exist on first run)",
    )
    ap.add_argument(
        "--threshold",
        type=float,
        default=float(
            os.environ.get("REGRESSION_THRESHOLD", DEFAULT_REGRESSION_THRESHOLD)
        ),
        help="fail when current median > baseline median * THRESHOLD",
    )
    ap.add_argument(
        "--commit",
        default=os.environ.get("GITHUB_SHA", "local"),
        help="commit recorded in a newly written baseline",
    )
    ap.add_argument(
        "--summary-out",
        default=os.environ.get("GITHUB_STEP_SUMMARY", ""),
        help="optional markdown summary sink (GitHub step summary)",
    )
    args = ap.parse_args(argv)

    threshold = args.threshold
    if threshold <= 1.0:
        print(f"::error::REGRESSION_THRESHOLD must be > 1.0 (got {threshold})")
        return EXIT_USAGE

    results_dir = pathlib.Path(args.results_dir)
    baseline_path = pathlib.Path(args.baseline)

    current = collect_current(results_dir)
    if not current:
        print(
            "::error::no parseable k6 summary.json under "
            f"{results_dir} — the load suite produced no measurement, so this "
            "run proves nothing about performance"
        )
        return EXIT_REGRESSION

    baseline = load_baseline(baseline_path)
    lines: list[str] = []
    regressions: list[str] = []
    seeded: list[str] = []
    compared = 0

    for scenario in sorted(current):
        cur = current[scenario]
        cur_med = cur["median_ms"]
        p99_note = "" if cur["p99_ms"] is None else f" p99={cur['p99_ms']:.2f}ms"
        prev = baseline.get(scenario) or {}
        prev_med = prev.get("median_ms")
        if prev_med is None:
            seeded.append(scenario)
            print(
                f"SEED    scenario={scenario} median={cur_med:.2f}ms{p99_note} "
                "— no stored baseline for this scenario; recording it as the "
                "baseline (this run cannot regress)"
            )
            lines.append(
                f"| `{scenario}` | {cur_med:.2f} | — | — | 🌱 seeded |"
            )
            continue
        compared += 1
        limit = float(prev_med) * threshold
        delta_pct = ((cur_med / float(prev_med)) - 1.0) * 100.0
        if cur_med > limit:
            regressions.append(scenario)
            verdict = "❌ REGRESSED"
            print(
                f"FAIL    scenario={scenario} median={cur_med:.2f}ms "
                f"baseline={float(prev_med):.2f}ms limit={limit:.2f}ms "
                f"({delta_pct:+.1f}%){p99_note}"
            )
            print(
                f"::error::load-test regression: {scenario} median "
                f"{cur_med:.2f}ms exceeds baseline {float(prev_med):.2f}ms "
                f"* {threshold} = {limit:.2f}ms ({delta_pct:+.1f}%)"
            )
        else:
            verdict = "✅ ok"
            print(
                f"OK      scenario={scenario} median={cur_med:.2f}ms "
                f"baseline={float(prev_med):.2f}ms limit={limit:.2f}ms "
                f"({delta_pct:+.1f}%){p99_note}"
            )
        lines.append(
            f"| `{scenario}` | {cur_med:.2f} | {float(prev_med):.2f} | "
            f"{delta_pct:+.1f}% | {verdict} |"
        )

    if regressions:
        print(
            f"::error::{len(regressions)} scenario(s) regressed beyond "
            f"{(threshold - 1.0) * 100:.0f}%: {', '.join(regressions)}. "
            "Baseline left UNCHANGED so the regression cannot ratchet in."
        )
        outcome = "❌ regressed"
        exit_code = EXIT_REGRESSION
    else:
        merged = dict(baseline)
        merged.update(current)
        write_baseline(baseline_path, merged, args.commit)
        if compared == 0:
            print(
                f"BASELINE SEEDED — {len(seeded)} scenario(s) recorded, nothing "
                "to compare against yet. The NEXT run compares against this "
                "file and can go red."
            )
            outcome = "🌱 baseline seeded (first run)"
        else:
            print(
                f"PASS — {compared} scenario(s) compared against the stored "
                f"baseline, all within {(threshold - 1.0) * 100:.0f}%. "
                "Baseline updated."
            )
            outcome = "✅ within threshold"
        exit_code = EXIT_OK

    if args.summary_out:
        try:
            with open(args.summary_out, "a", encoding="utf-8") as fh:
                fh.write("## load-test baseline regression check\n\n")
                fh.write(
                    f"Metric: `http_req_duration.med` · threshold: "
                    f"`{threshold}`x · outcome: **{outcome}**\n\n"
                )
                fh.write(
                    "| scenario | current (ms) | baseline (ms) | delta | verdict |\n"
                    "| --- | ---: | ---: | ---: | --- |\n"
                )
                fh.write("\n".join(lines) + "\n")
        except OSError as exc:  # pragma: no cover - summary is best-effort
            print(f"::warning::could not write step summary: {exc}")

    return exit_code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
