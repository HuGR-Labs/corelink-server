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
most recent PREVIOUS run's file; save publishes this run's. A missing file is
an UNKNOWN input and REDs the check: a first run cannot prove a regression
comparison. An unreadable or malformed file is likewise an instrument error
and is never silently replaced by a fresh baseline.

Ratchet policy: the baseline is rewritten only when the comparison PASSES. A
regressing run leaves the old baseline in place, so a regression cannot become
the new normal by simply being measured twice.

EXIT CODES
----------
  0 — every expected scenario is present and within threshold
  1 — usage / IO / schema error, including invalid or ambiguous measurements
  2 — at least one scenario regressed beyond REGRESSION_THRESHOLD, or the run
      produced no parseable k6 summary at all

An empty or partial result set is an UNKNOWN input, not evidence of health. The
k6 matrix legs carry `continue-on-error: true`, so a suite that failed to
produce a complete set of summary/status files would otherwise land here as a
silent green — exactly the defect this script exists to remove.

`--expected-scenarios` closes the artifact population: the current summaries and
matrix-leg status records must match it exactly. A focused manual dispatch must
therefore pass its selected list explicitly.

stdlib-only on purpose: the self-hosted fleets have no guaranteed third-party
python packages.
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
import math
import os
import pathlib
import re
import sys
from dataclasses import dataclass

BASELINE_SCHEMA = 2
BASELINE_VERSION = "k6-baseline-v2"
SUMMARY_SCHEMA = 1
SUITE_VERSION = "r3-prep-v2"
CANONICAL_TARGET = "https://staging.corelink.humangr.com"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
TENANT_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
)
REQUIRED_SCENARIOS = frozenset({"signup", "webhook", "dsr", "cas", "byok"})

# Default regression threshold: a scenario fails when its current median is
# more than 20% above the stored baseline median. Overridable by
# --threshold / $REGRESSION_THRESHOLD so the workflow owns the number.
DEFAULT_REGRESSION_THRESHOLD = 1.20

EXIT_OK = 0
EXIT_USAGE = 1
EXIT_REGRESSION = 2


class InputError(ValueError):
    """The run produced ambiguous or invalid measurement input."""


class BaselineError(ValueError):
    """A stored baseline exists but cannot be trusted."""


@dataclass(frozen=True)
class RunIdentity:
    """Public identity that makes a measurement portable only to its target."""

    target: str
    tenant_id: str
    deployment_sha: str
    suite_version: str


@dataclass(frozen=True)
class BaselineRecord:
    identity: RunIdentity
    scenarios: dict[str, dict[str, float]]
    threshold: float


def _finite_positive(value: object, *, label: str) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError) as exc:
        raise InputError(f"{label} is not numeric: {value!r}") from exc
    if not math.isfinite(number) or number <= 0:
        raise InputError(f"{label} must be finite and > 0: {value!r}")
    return number


def _finite_nonnegative(value: object, *, label: str) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError) as exc:
        raise InputError(f"{label} is not numeric: {value!r}") from exc
    if not math.isfinite(number) or number < 0:
        raise InputError(f"{label} must be finite and >= 0: {value!r}")
    return number


def _now() -> str:
    return _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def collect_current(
    results_dir: pathlib.Path,
    expected: set[str] | None = None,
    identity: RunIdentity | None = None,
) -> dict[str, dict[str, float | None]]:
    """Read only sanitized summaries and bind every row to one run identity."""
    out: dict[str, dict[str, float | None]] = {}
    observed: RunIdentity | None = None
    if not results_dir.exists():
        return out
    for summary in sorted(results_dir.rglob("summary.json")):
        try:
            data = json.loads(summary.read_text())
        except (OSError, ValueError) as exc:
            raise InputError(f"failed to parse {summary}: {exc}") from exc
        if not isinstance(data, dict):
            raise InputError(f"{summary} is not a JSON object")
        if data.get("schema") != SUMMARY_SCHEMA or data.get("suite_version") != SUITE_VERSION:
            raise InputError(f"{summary} is not a sanitized {SUITE_VERSION} summary")
        scenario = summary.parent.name
        if data.get("scenario") != scenario:
            raise InputError(f"{summary} scenario identity does not match its artifact path")
        target = data.get("target")
        tenant_id = data.get("tenant_id")
        deployment_sha = data.get("deployment_sha")
        if not all(isinstance(value, str) and value.strip() for value in (target, tenant_id, deployment_sha)):
            raise InputError(f"{summary} is missing target identity")
        if target != CANONICAL_TARGET:
            raise InputError(f"{summary} is bound to an unexpected target")
        if not TENANT_RE.fullmatch(tenant_id):
            raise InputError(f"{summary} has an invalid tenant identity")
        if not SHA_RE.fullmatch(deployment_sha):
            raise InputError(f"{summary} has an invalid deployment identity")
        current_identity = RunIdentity(target, tenant_id, deployment_sha, data["suite_version"])
        if observed is None:
            observed = current_identity
        elif current_identity != observed:
            raise InputError(f"{summary} target, tenant, deployment, or suite identity differs")
        if identity is not None and current_identity != identity:
            raise InputError(f"{summary} identity does not match the run identity")
        metrics = data.get("metrics")
        if not isinstance(metrics, dict):
            raise InputError(f"{summary} has no metrics object")
        dur = metrics.get("http_req_duration")
        if not isinstance(dur, dict):
            raise InputError(f"{summary} has no http_req_duration object")
        median = dur.get("med")
        p99 = dur.get("p(99)")
        if median is None:
            raise InputError(f"{summary} has no http_req_duration.med")
        if scenario in out:
            raise InputError(
                f"duplicate k6 summary for scenario {scenario!r}; refusing to choose one"
            )
        median_ms = _finite_positive(median, label=f"{summary} http_req_duration.med")
        p99_ms = _finite_positive(p99, label=f"{summary} http_req_duration.p(99)")
        out[scenario] = {
            "median_ms": median_ms,
            "p99_ms": p99_ms,
        }
    if expected is not None and set(out) != expected:
        missing = sorted(expected - set(out))
        unexpected = sorted(set(out) - expected)
        raise InputError(
            "current scenario population mismatch: "
            f"missing={missing or '-'} unexpected={unexpected or '-'}"
        )
    return out


def collect_identity(results_dir: pathlib.Path, expected: set[str]) -> RunIdentity:
    """Collect the identity from the same complete sanitized population."""
    if not results_dir.exists():
        raise InputError(f"results directory {results_dir} is missing")
    identities: set[RunIdentity] = set()
    summaries = sorted(results_dir.rglob("summary.json"))
    if not summaries:
        raise InputError("current run produced no sanitized k6 summaries")
    for summary in summaries:
        try:
            data = json.loads(summary.read_text())
        except (OSError, ValueError) as exc:
            raise InputError(f"failed to parse {summary}: {exc}") from exc
        if not isinstance(data, dict):
            raise InputError(f"{summary} is not a JSON object")
        if data.get("schema") != SUMMARY_SCHEMA or data.get("suite_version") != SUITE_VERSION:
            raise InputError(f"{summary} is not a sanitized {SUITE_VERSION} summary")
        scenario = summary.parent.name
        if data.get("scenario") != scenario or scenario not in expected:
            raise InputError(f"{summary} has an unexpected scenario identity")
        identity = RunIdentity(
            data.get("target"), data.get("tenant_id"), data.get("deployment_sha"), data["suite_version"]
        )
        if not all(
            isinstance(value, str) and value.strip()
            for value in (
                identity.target,
                identity.tenant_id,
                identity.deployment_sha,
                identity.suite_version,
            )
        ):
            raise InputError(f"{summary} is missing target identity")
        if identity.target != CANONICAL_TARGET:
            raise InputError(f"{summary} is bound to an unexpected target")
        if not TENANT_RE.fullmatch(identity.tenant_id) or not SHA_RE.fullmatch(identity.deployment_sha):
            raise InputError(f"{summary} has an invalid tenant or deployment identity")
        identities.add(identity)
    if len(identities) != 1:
        raise InputError("current summaries are not bound to one target, tenant, and deployment")
    return next(iter(identities))


def load_baseline_record(path: pathlib.Path) -> BaselineRecord:
    if not path.exists():
        raise BaselineError(f"baseline {path} is missing")
    try:
        data = json.loads(path.read_text())
    except (OSError, ValueError) as exc:
        raise BaselineError(f"baseline {path} is unreadable: {exc}") from exc
    if not isinstance(data, dict):
        raise BaselineError(f"baseline {path} must be a JSON object")
    if data.get("schema") != BASELINE_SCHEMA:
        raise BaselineError(
            f"baseline {path} has unsupported schema {data.get('schema')!r}"
        )
    if data.get("baseline_version") != BASELINE_VERSION:
        raise BaselineError(f"baseline {path} has an unsupported baseline version")
    if data.get("suite_version") != SUITE_VERSION:
        raise BaselineError(f"baseline {path} has an unsupported suite version")
    if data.get("metric") != "http_req_duration.med (ms)":
        raise BaselineError(f"baseline {path} has an unexpected metric")
    for field in ("captured_at", "commit"):
        if not isinstance(data.get(field), str) or not data[field].strip():
            raise BaselineError(f"baseline {path} is missing metadata field {field!r}")
    threshold = data.get("threshold_multiplier")
    try:
        threshold = _finite_positive(threshold, label=f"baseline {path} threshold_multiplier")
    except InputError as exc:
        raise BaselineError(str(exc)) from exc
    if threshold <= 1.0:
        raise BaselineError(f"baseline {path} threshold_multiplier must be > 1.0")
    identity_data = data.get("identity")
    if not isinstance(identity_data, dict) or set(identity_data) != {
        "target",
        "tenant_id",
        "deployment_sha",
    }:
        raise BaselineError(f"baseline {path} has no exact target identity")
    target = identity_data.get("target")
    tenant_id = identity_data.get("tenant_id")
    deployment_sha = identity_data.get("deployment_sha")
    if target != CANONICAL_TARGET or not isinstance(tenant_id, str) or not TENANT_RE.fullmatch(tenant_id) or not isinstance(deployment_sha, str) or not SHA_RE.fullmatch(deployment_sha):
        raise BaselineError(f"baseline {path} has an invalid target identity")
    scenarios = data.get("scenarios")
    if not isinstance(scenarios, dict):
        raise BaselineError(f"baseline {path} has no `scenarios` map")
    validated: dict[str, dict[str, float | None]] = {}
    for scenario, values in scenarios.items():
        if not isinstance(scenario, str) or not scenario:
            raise BaselineError(f"baseline {path} contains an invalid scenario name")
        if not isinstance(values, dict):
            raise BaselineError(f"baseline {path} scenario {scenario!r} is not an object")
        median = values.get("median_ms")
        if median is None:
            raise BaselineError(f"baseline {path} scenario {scenario!r} has no median_ms")
        try:
            median_ms = _finite_positive(median, label=f"baseline {scenario} median_ms")
            p99 = values.get("p99_ms")
            p99_ms = _finite_positive(p99, label=f"baseline {scenario} p99_ms")
        except InputError as exc:
            raise BaselineError(str(exc)) from exc
        validated[scenario] = {"median_ms": median_ms, "p99_ms": p99_ms}
    return BaselineRecord(
        identity=RunIdentity(target, tenant_id, deployment_sha, data["suite_version"]),
        scenarios=validated,
        threshold=threshold,
    )


def load_baseline(path: pathlib.Path) -> dict[str, dict[str, float]]:
    """Compatibility view for callers that only need scenario measurements."""
    return load_baseline_record(path).scenarios


def collect_statuses(results_dir: pathlib.Path, expected: set[str]) -> None:
    """Require one successful matrix-leg status for every expected scenario."""
    statuses: dict[str, str] = {}
    for status_file in sorted(results_dir.rglob("status.json")):
        try:
            data = json.loads(status_file.read_text())
        except (OSError, ValueError) as exc:
            raise InputError(f"failed to parse {status_file}: {exc}") from exc
        scenario = status_file.parent.name
        if scenario in statuses:
            raise InputError(f"duplicate status population for scenario {scenario!r}")
        if not isinstance(data, dict) or data.get("scenario") != scenario:
            raise InputError(f"{status_file} has a malformed scenario status")
        outcome = data.get("outcome")
        if outcome != "success":
            raise InputError(
                f"scenario {scenario!r} matrix leg outcome is {outcome!r}, not success"
            )
        statuses[scenario] = outcome
    if set(statuses) != expected:
        missing = sorted(expected - set(statuses))
        unexpected = sorted(set(statuses) - expected)
        raise InputError(
            "scenario status population mismatch: "
            f"missing={missing or '-'} unexpected={unexpected or '-'}"
        )


def parse_expected_scenarios(value: str | None) -> tuple[str, ...] | None:
    if value is None:
        return None
    names = tuple(part.strip() for part in value.split(",") if part.strip())
    if not names:
        raise InputError("--expected-scenarios must name at least one scenario")
    if len(set(names)) != len(names):
        raise InputError("--expected-scenarios contains duplicate scenario names")
    return names


def write_baseline(
    path: pathlib.Path,
    scenarios: dict[str, dict[str, float]],
    commit: str,
    identity: RunIdentity,
    threshold: float,
) -> None:
    if not math.isfinite(threshold) or threshold <= 1.0:
        raise BaselineError("cannot write a baseline with an invalid threshold")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(
            {
                "schema": BASELINE_SCHEMA,
                "baseline_version": BASELINE_VERSION,
                "suite_version": SUITE_VERSION,
                "captured_at": _now(),
                "commit": commit,
                "metric": "http_req_duration.med (ms)",
                "threshold_multiplier": threshold,
                "identity": {
                    "target": identity.target,
                    "tenant_id": identity.tenant_id,
                    "deployment_sha": identity.deployment_sha,
                },
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
    ap.add_argument(
        "--expected-scenarios",
        default=os.environ.get("K6_EXPECTED_SCENARIOS"),
        help=(
            "comma-separated scenarios requested for this run; when supplied, "
            "the current artifact set must match it exactly"
        ),
    )
    ap.add_argument(
        "--bootstrap",
        action="store_true",
        help=(
            "seed a missing baseline from one complete successful five-scenario "
            "population; callers must not treat this as regression proof"
        ),
    )
    args = ap.parse_args(argv)

    threshold = args.threshold
    if not math.isfinite(threshold) or threshold <= 1.0:
        print(f"::error::REGRESSION_THRESHOLD must be > 1.0 (got {threshold})")
        return EXIT_USAGE

    results_dir = pathlib.Path(args.results_dir)
    baseline_path = pathlib.Path(args.baseline)

    try:
        expected = parse_expected_scenarios(args.expected_scenarios)
        if expected is None:
            raise InputError("--expected-scenarios must define a non-empty population")
        expected_set = set(expected)
        collect_statuses(results_dir, expected_set)
        current_identity = collect_identity(results_dir, expected_set)
        current = collect_current(results_dir, expected_set, current_identity)
    except InputError as exc:
        print(f"::error::{exc}")
        return EXIT_USAGE
    if expected_set != REQUIRED_SCENARIOS:
        print(
            "::error::baseline publication requires all five scenarios: "
            + ",".join(sorted(REQUIRED_SCENARIOS))
        )
        return EXIT_USAGE

    if args.bootstrap:
        if baseline_path.exists():
            print(
                f"::error::baseline {baseline_path} already exists; refusing to "
                "overwrite established evidence in bootstrap mode"
            )
            return EXIT_USAGE
        write_baseline(baseline_path, current, args.commit)
        print(
            "BOOTSTRAP ONLY — complete successful five-scenario baseline seeded; "
            "this is not a regression comparison"
        )
        return EXIT_OK

    try:
        baseline_record = load_baseline_record(baseline_path)
    except BaselineError as exc:
        print(f"::error::{exc}")
        return EXIT_USAGE
    if baseline_record.identity != current_identity:
        print("::error::baseline target, tenant, deployment, or suite identity does not match current summaries")
        return EXIT_USAGE
    if baseline_record.threshold != threshold:
        print(
            f"::error::baseline threshold {baseline_record.threshold} does not match requested {threshold}"
        )
        return EXIT_USAGE
    baseline = baseline_record.scenarios
    if set(baseline) != expected_set:
        print("::error::baseline scenario population does not exactly match the current run")
        return EXIT_USAGE
    lines: list[str] = []
    regressions: list[str] = []
    compared = 0

    for scenario in sorted(current):
        cur = current[scenario]
        cur_med = cur["median_ms"]
        p99_note = "" if cur["p99_ms"] is None else f" p99={cur['p99_ms']:.2f}ms"
        prev = baseline.get(scenario)
        if prev is None:
            print(f"::error::baseline scenario population is partial; missing: {scenario}")
            return EXIT_USAGE
        prev_med = prev["median_ms"]
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
        write_baseline(baseline_path, merged, args.commit, current_identity, threshold)
        print(
            f"PASS — {compared} scenario(s) compared against the stored "
            f"baseline, all within {(threshold - 1.0) * 100:.0f}%. Baseline updated."
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
