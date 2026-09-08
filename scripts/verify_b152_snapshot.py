#!/usr/bin/env python3
"""Fail-closed verifier for a persisted B-152 Actions snapshot.

This checks completeness and identity of the metadata population offline.  It
does not infer a root cause: an unavailable log, a zero match count, or a
uniform metadata signature is explicitly insufficient for closure.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
from pathlib import Path
from typing import Any


SCHEMA = "b152-actions-diagnostic/v2"
SIGNATURES = frozenset({
    "billing_or_startup",
    "enospc_or_linker_failure",
    "indeterminate",
    "job_timeout",
    "playwright_webserver",
    "runner_cancellation",
    "test_failure",
})
RFC3339_UTC = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|\+00:00)$")
SHA256_DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")


class SnapshotError(RuntimeError):
    """The snapshot cannot establish a closed, identity-complete population."""


def _obj(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise SnapshotError(f"{label} must be an object")
    return value


def _positive_id(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise SnapshotError(f"{label} must be a positive integer")
    return value


def _timestamp(value: Any, label: str) -> None:
    if not isinstance(value, str) or not RFC3339_UTC.fullmatch(value):
        raise SnapshotError(f"{label} must be an explicit UTC timestamp")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise SnapshotError(f"{label} must be a parseable RFC3339 UTC timestamp") from exc
    if parsed.utcoffset() != dt.timedelta(0):
        raise SnapshotError(f"{label} must be an explicit UTC timestamp")


def _runner_name(value: Any, label: str) -> None:
    if not isinstance(value, str) or not value or value != value.strip():
        raise SnapshotError(f"{label} must be a non-empty normalized runner name")


def _http_success(value: Any, label: str) -> None:
    """Require an explicit successful HTTP status for an available log."""
    if isinstance(value, bool) or not isinstance(value, int) or not 200 <= value <= 299:
        raise SnapshotError(f"{label} must be an HTTP success status")


def verify(snapshot: dict[str, Any]) -> dict[str, Any]:
    if snapshot.get("schema") != SCHEMA:
        raise SnapshotError("unsupported or missing snapshot schema")
    collection = _obj(snapshot.get("collection"), "collection")
    if collection.get("source") != "GitHub Actions REST API via gh api" or collection.get("read_only") is not True:
        raise SnapshotError("snapshot provenance is not read-only GitHub API evidence")
    if collection.get("logs_persisted") is not False:
        raise SnapshotError("raw log bodies must never be persisted")
    if collection.get("zero_window_jobs_is_not_closure") is not True:
        raise SnapshotError("zero-population non-closure guard is missing")
    _obj(snapshot.get("window"), "window")
    window = snapshot["window"]
    _timestamp(window.get("start"), "window.start")
    _timestamp(window.get("end_exclusive"), "window.end_exclusive")
    bounds = _obj(snapshot.get("duration_window_seconds"), "duration_window_seconds")
    low, high = bounds.get("low"), bounds.get("high")
    if isinstance(low, bool) or not isinstance(low, int) or isinstance(high, bool) or not isinstance(high, int) or low < 0 or high < low:
        raise SnapshotError("duration bounds are invalid")

    run_ids = snapshot.get("run_ids")
    if not isinstance(run_ids, list) or any(isinstance(v, bool) or not isinstance(v, int) or v <= 0 for v in run_ids):
        raise SnapshotError("run_ids is incomplete or malformed")
    if len(run_ids) != len(set(run_ids)):
        raise SnapshotError("run_ids contains duplicates")
    if snapshot.get("run_count") != len(run_ids):
        raise SnapshotError("run_count does not equal the closed run population")
    failed_jobs = snapshot.get("failed_jobs")
    window_jobs = snapshot.get("window_jobs")
    if not isinstance(failed_jobs, list) or not isinstance(window_jobs, list):
        raise SnapshotError("failed_jobs/window_jobs must be arrays")
    if snapshot.get("failed_job_count") != len(failed_jobs):
        raise SnapshotError("failed_job_count does not equal failed_jobs")
    conclusion_counts = _obj(snapshot.get("run_conclusion_counts"), "run_conclusion_counts")
    if any(not isinstance(key, str) or isinstance(value, bool) or not isinstance(value, int) or value < 0 for key, value in conclusion_counts.items()):
        raise SnapshotError("run conclusion counts are malformed")
    if sum(conclusion_counts.values()) != len(run_ids):
        raise SnapshotError("run conclusion counts do not cover the closed run population")
    outcomes = snapshot.get("run_outcomes")
    if not isinstance(outcomes, list):
        raise SnapshotError("run_outcomes is missing")
    outcome_ids = set()
    for index, outcome in enumerate(outcomes):
        row = _obj(outcome, f"run_outcomes[{index}]")
        run_id = _positive_id(row.get("run_id"), f"run_outcomes[{index}].run_id")
        if run_id in outcome_ids or run_id not in set(run_ids):
            raise SnapshotError("run outcome identity is duplicated or outside the population")
        outcome_ids.add(run_id)
        if not isinstance(row.get("conclusion"), str) or row["conclusion"] == "success":
            raise SnapshotError("run outcome conclusion is missing or successful")
        _timestamp(row.get("created_at"), f"run_outcomes[{index}].created_at")
        _timestamp(row.get("updated_at"), f"run_outcomes[{index}].updated_at")
    if snapshot.get("failed_run_count") != conclusion_counts.get("failure", 0):
        raise SnapshotError("failed_run_count does not equal the failure conclusion population")

    job_ids: set[int] = set()
    window_ids: set[int] = set()
    for index, job in enumerate(failed_jobs):
        row = _obj(job, f"failed_jobs[{index}]")
        run_id = _positive_id(row.get("run_id"), f"failed_jobs[{index}].run_id")
        job_id = _positive_id(row.get("job_id"), f"failed_jobs[{index}].job_id")
        if job_id in job_ids:
            raise SnapshotError("job identity is duplicated")
        job_ids.add(job_id)
        if run_id not in set(run_ids):
            raise SnapshotError("failed job references a run outside the closed population")
        _runner_name(row.get("runner_name"), f"failed_jobs[{index}].runner_name")
        for key in ("created_at", "started_at", "completed_at"):
            _timestamp(row.get(key), f"failed_jobs[{index}].{key}")
        steps = row.get("steps")
        if not isinstance(steps, list) or not steps:
            raise SnapshotError("failed job is missing complete step evidence")
        for step_index, step in enumerate(steps):
            step_obj = _obj(step, f"failed_jobs[{index}].steps[{step_index}]")
            if isinstance(step_obj.get("step_id"), bool) or not isinstance(step_obj.get("step_id"), int) or step_obj["step_id"] <= 0:
                raise SnapshotError("failed job has a missing or malformed step ID")
            if not isinstance(step_obj.get("name"), str) or not isinstance(step_obj.get("status"), str):
                raise SnapshotError("failed job has malformed step identity/status")
            for key in ("started_at", "completed_at"):
                value = step_obj.get(key)
                if value is not None:
                    _timestamp(value, f"failed_jobs[{index}].steps[{step_index}].{key}")
        if row.get("in_window") is True:
            window_ids.add(job_id)
    listed_ids = set()
    for index, job in enumerate(window_jobs):
        row = _obj(job, f"window_jobs[{index}]")
        job_id = _positive_id(row.get("job_id"), f"window_jobs[{index}].job_id")
        if job_id in listed_ids or job_id not in job_ids or job_id not in window_ids:
            raise SnapshotError("window_jobs is not an exact subset of failed_jobs")
        listed_ids.add(job_id)
    if listed_ids != window_ids:
        raise SnapshotError("window_jobs omits a selected in-window job")

    logs = snapshot.get("logs")
    if not isinstance(logs, list) or len(logs) != len(window_ids):
        raise SnapshotError("logs must have exactly one record per selected job")
    log_ids: set[int] = set()
    for index, log in enumerate(logs):
        row = _obj(log, f"logs[{index}]")
        job_id = _positive_id(row.get("job_id"), f"logs[{index}].job_id")
        if job_id in log_ids or job_id not in window_ids:
            raise SnapshotError("log identity is duplicated or outside the selected jobs")
        log_ids.add(job_id)
        signature = row.get("log_signature")
        if signature not in SIGNATURES:
            raise SnapshotError("unknown log signature")
        available = row.get("available")
        status = row.get("status")
        digest = row.get("log_sha256")
        causal = row.get("causal")
        error = row.get("error")
        if available is True:
            if status != "available":
                raise SnapshotError("available log must have status=available")
            _http_success(row.get("http_status"), f"logs[{index}].http_status")
            if not isinstance(digest, str) or not SHA256_DIGEST.fullmatch(digest):
                raise SnapshotError("available log lacks an exact lowercase sha256 digest")
            if causal is not False or error is not None:
                raise SnapshotError("available log has contradictory causal/error fields")
        elif available is False:
            if (
                status != "indeterminate"
                or row.get("http_status") != 404
                or signature != "indeterminate"
                or causal != "indeterminate"
                or digest is not None
                or not isinstance(error, str)
                or not error
            ):
                raise SnapshotError("unavailable log must be a non-causal HTTP 404 without a digest")
        else:
            raise SnapshotError("log availability must be a boolean")
    if log_ids != window_ids:
        raise SnapshotError("logs omit a selected in-window job")

    causal = _obj(snapshot.get("causal_classification"), "causal_classification")
    if causal.get("status") == "closed" or causal.get("causal") is True:
        raise SnapshotError("snapshot must not close B-152 or assert a common cause")
    if len(window_jobs) == 0 and collection.get("zero_window_jobs_is_not_closure") is not True:
        raise SnapshotError("zero window population was allowed to close the item")
    return {
        "runs": len(run_ids),
        "failed_jobs": len(failed_jobs),
        "window_jobs": len(window_jobs),
        "status": "open/indeterminate",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("snapshot", type=Path)
    args = parser.parse_args()
    try:
        snapshot = json.loads(args.snapshot.read_text(encoding="utf-8"))
        result = verify(_obj(snapshot, "snapshot"))
    except (OSError, json.JSONDecodeError, SnapshotError) as exc:
        print(f"INDETERMINATE: {exc}")
        return 2
    print(f"B152 snapshot: PASS ({result['runs']} runs, {result['window_jobs']} in-window failures; status open)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
