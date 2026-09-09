#!/usr/bin/env python3
"""Build and check the endurance rolling seven-run p99 baseline.

The endurance workflow is dispatch-only because staging is not available. When
it is dispatched, this command is the complete input boundary for the rolling
gate: it validates the run-history population, resolves one named artifact for
each prior run, downloads each artifact with a timeout, validates every summary
and operation, writes a deterministic JSON baseline, and compares the current
run against the aggregate. Missing history, missing/download-failed artifacts,
malformed summaries, and partial operation populations are UNKNOWN (exit 1),
never a first-run pass.

Only the standard library is used because this runs on the ``corelink`` CI
runner and in clean checkouts.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import os
import pathlib
import subprocess
import tempfile
from typing import Any

EXPECTED_OPS = (
    "cas_read",
    "cas_write",
    "audit_query",
    "byok_op",
    "admin_op",
    "webhook",
)
WINDOW_RUNS = 7
REGRESSION_THRESHOLD = 1.20
COMMAND_TIMEOUT_SECONDS = 30
ARTIFACT_PREFIX = "endurance-2h-results-"
SCHEMA = 1


class InputError(ValueError):
    """The gate cannot prove a complete rolling population."""


def _positive_int(value: object, label: str) -> int:
    if isinstance(value, bool):
        raise InputError(f"{label} must be a positive integer")
    try:
        parsed = int(str(value))
    except (TypeError, ValueError) as exc:
        raise InputError(f"{label} must be a positive integer") from exc
    if parsed <= 0 or str(value).strip() != str(parsed):
        raise InputError(f"{label} must be a positive integer")
    return parsed


def _parse_timestamp(value: object, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise InputError(f"{label} is missing createdAt")
    normalized = value.strip().replace("Z", "+00:00")
    try:
        parsed = dt.datetime.fromisoformat(normalized)
    except ValueError as exc:
        raise InputError(f"{label} has malformed createdAt: {value!r}") from exc
    if parsed.tzinfo is None:
        raise InputError(f"{label} createdAt must include a timezone")
    return parsed.astimezone(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def load_history(path: pathlib.Path, current_run_id: str | None = None) -> list[dict[str, str]]:
    """Validate gh's ``[{databaseId, createdAt}]`` population and select seven runs."""
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise InputError(f"run history is unreadable or malformed: {path}") from exc
    if not isinstance(raw, list):
        raise InputError("run history must be a JSON array")

    current = None
    if current_run_id:
        current = str(_positive_int(current_run_id, "current run ID"))
    seen: set[str] = set()
    parsed: list[dict[str, str]] = []
    for index, entry in enumerate(raw):
        if not isinstance(entry, dict):
            raise InputError(f"run history entry {index} is not an object")
        run_id = str(_positive_int(entry.get("databaseId"), f"run history entry {index}"))
        if run_id in seen:
            raise InputError(f"run history contains duplicate run ID {run_id}")
        seen.add(run_id)
        created_at = _parse_timestamp(entry.get("createdAt"), f"run {run_id}")
        if run_id != current:
            parsed.append({"run_id": run_id, "created_at": created_at})

    if len(parsed) < WINDOW_RUNS:
        raise InputError(
            f"run history has {len(parsed)} prior runs; exactly {WINDOW_RUNS} are required"
        )
    # gh normally returns newest first. Sort explicitly so output does not
    # depend on API ordering, then retain the newest seven prior observations.
    parsed.sort(key=lambda row: (row["created_at"], int(row["run_id"])), reverse=True)
    selected = parsed[:WINDOW_RUNS]
    selected.sort(key=lambda row: (row["created_at"], int(row["run_id"])))
    return selected


def _run(command: list[str]) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            command,
            check=True,
            text=True,
            capture_output=True,
            timeout=COMMAND_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired as exc:
        raise InputError(f"bounded command timed out after {COMMAND_TIMEOUT_SECONDS}s: {command[0]}") from exc
    except (OSError, subprocess.CalledProcessError) as exc:
        detail = getattr(exc, "stderr", "") or getattr(exc, "stdout", "") or str(exc)
        raise InputError(f"bounded command failed: {' '.join(command)}: {detail.strip()}") from exc


def find_artifact(repo: str, run_id: str) -> str:
    """Return the sole non-expired endurance artifact for a run."""
    if not repo.strip():
        raise InputError("GITHUB_REPOSITORY is required to resolve historical artifacts")
    result = _run([
        "gh",
        "api",
        f"repos/{repo}/actions/runs/{run_id}/artifacts?per_page=100",
    ])
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise InputError(f"artifact listing for run {run_id} is malformed") from exc
    artifacts = payload.get("artifacts") if isinstance(payload, dict) else None
    if not isinstance(artifacts, list):
        raise InputError(f"artifact listing for run {run_id} has no artifacts array")
    candidates = []
    for artifact in artifacts:
        if not isinstance(artifact, dict):
            raise InputError(f"artifact listing for run {run_id} contains a malformed entry")
        name = artifact.get("name")
        if not isinstance(name, str) or not name.startswith(ARTIFACT_PREFIX):
            continue
        if artifact.get("expired") is True:
            continue
        size = artifact.get("size_in_bytes")
        if isinstance(size, bool) or not isinstance(size, int) or size <= 0:
            raise InputError(f"artifact {name!r} for run {run_id} has no positive size")
        candidates.append(name)
    if len(candidates) != 1:
        raise InputError(
            f"run {run_id} has {len(candidates)} usable {ARTIFACT_PREFIX!r} artifacts; exactly one required"
        )
    return candidates[0]


def download_history(
    repo: str,
    runs: list[dict[str, str]],
    destination: pathlib.Path,
) -> dict[str, str]:
    """Resolve and download one artifact per historical run into isolated dirs."""
    if destination.exists() and any(destination.iterdir()):
        raise InputError(f"historical artifact directory is not empty: {destination}")
    destination.mkdir(parents=True, exist_ok=True)
    names: dict[str, str] = {}
    for run in runs:
        run_id = run["run_id"]
        artifact = find_artifact(repo, run_id)
        target = destination / run_id
        target.mkdir()
        _run([
            "gh",
            "run",
            "download",
            run_id,
            "--repo",
            repo,
            "--name",
            artifact,
            "--dir",
            str(target),
        ])
        if not any(target.iterdir()):
            raise InputError(f"artifact download for run {run_id} produced no files")
        names[run_id] = artifact
    return names


def _p99(metrics: object, op: str, label: str) -> float:
    key = f"endurance_op_latency_ms{{op:{op}}}"
    if not isinstance(metrics, dict) or not isinstance(metrics.get(key), dict):
        raise InputError(f"{label} is missing metric {key}")
    metric = metrics[key]
    value = metric.get("p(99)", metric.get("p99"))
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise InputError(f"{label} has invalid p99 for {op}")
    number = float(value)
    if not math.isfinite(number) or number <= 0:
        raise InputError(f"{label} has non-finite or non-positive p99 for {op}")
    return number


def read_summary(directory: pathlib.Path, label: str) -> dict[str, float]:
    summaries = sorted(directory.rglob("endurance-2h-summary-*.json")) if directory.exists() else []
    if len(summaries) != 1:
        raise InputError(f"{label} has {len(summaries)} endurance summaries; exactly one required")
    try:
        data: Any = json.loads(summaries[0].read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise InputError(f"{label} summary is unreadable or malformed") from exc
    metrics = data.get("metrics") if isinstance(data, dict) else None
    return {op: _p99(metrics, op, label) for op in EXPECTED_OPS}


def write_baseline(
    path: pathlib.Path,
    runs: list[dict[str, str]],
    artifacts: dict[str, str],
    observations: dict[str, dict[str, float]],
) -> dict[str, Any]:
    aggregate = {
        op: round(math.fsum(observations[run["run_id"]][op] for run in runs) / WINDOW_RUNS, 6)
        for op in EXPECTED_OPS
    }
    output: dict[str, Any] = {
        "schema": SCHEMA,
        "window_runs": WINDOW_RUNS,
        "metric": "endurance_op_latency_ms p(99) (ms)",
        "runs": [
            {
                "run_id": run["run_id"],
                "created_at": run["created_at"],
                "artifact": artifacts[run["run_id"]],
                "p99_ms": {op: round(observations[run["run_id"]][op], 6) for op in EXPECTED_OPS},
            }
            for run in runs
        ],
        "p99_ms": aggregate,
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, delete=False) as handle:
        json.dump(output, handle, indent=2, sort_keys=True)
        handle.write("\n")
        temporary = pathlib.Path(handle.name)
    os.replace(temporary, path)
    return output


def compare(current: dict[str, float], baseline: dict[str, Any]) -> list[str]:
    values = baseline.get("p99_ms") if isinstance(baseline, dict) else None
    if not isinstance(values, dict) or baseline.get("schema") != SCHEMA or baseline.get("window_runs") != WINDOW_RUNS:
        raise InputError("generated rolling baseline has unsupported schema")
    failed: list[str] = []
    for op in EXPECTED_OPS:
        base = values.get(op)
        if isinstance(base, bool) or not isinstance(base, (int, float)) or not math.isfinite(float(base)) or float(base) <= 0:
            raise InputError(f"generated rolling baseline has invalid p99 for {op}")
        ratio = current[op] / float(base)
        print(f"op={op} current_p99={current[op]:.2f}ms rolling_7d_p99={float(base):.2f}ms ratio={ratio:.3f}")
        if ratio > REGRESSION_THRESHOLD:
            failed.append(op)
    return failed


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history", required=True, type=pathlib.Path)
    parser.add_argument("--current-dir", required=True, type=pathlib.Path)
    parser.add_argument("--downloads-dir", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""))
    parser.add_argument("--current-run-id", default=os.environ.get("GITHUB_RUN_ID"))
    args = parser.parse_args(argv)
    try:
        runs = load_history(args.history, args.current_run_id)
        artifacts = download_history(args.repo, runs, args.downloads_dir)
        observations = {
            run["run_id"]: read_summary(args.downloads_dir / run["run_id"], f"run {run['run_id']}")
            for run in runs
        }
        current = read_summary(args.current_dir, "current run")
        baseline = write_baseline(args.output, runs, artifacts, observations)
        failed = compare(current, baseline)
        if failed:
            print("::error::rolling 7-day p99 drift >20%: " + ", ".join(failed))
            return 1
        print(f"rolling 7-day p99 baseline generated from {WINDOW_RUNS} complete prior runs")
        return 0
    except InputError as exc:
        print(f"::error::rolling 7-day p99 input is UNKNOWN: {exc}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main(__import__("sys").argv[1:]))
