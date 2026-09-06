#!/usr/bin/env python3
"""Verify that the scheduled CodeQL matrix produced retained evidence.

The CodeQL workflow runs on the product-owned ``corelink`` fabric.  A step in
that workflow cannot observe a job that never receives a runner, so this
watchdog asks GitHub's Actions API for the latest scheduled run, its complete
matrix population, and one retained SARIF artifact per language.

Exit codes:

* 0 -- a recent successful run has the exact matrix and artifacts;
* 1 -- an evidence gap was observed;
* 2 -- the Actions API could not be inspected.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Callable
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


API_ROOT = "https://api.github.com"
WORKFLOW_FILE = "codeql.yml"
EXPECTED_LANGUAGES = ("rust", "javascript-typescript", "python")
EXPECTED_JOBS = frozenset(f"CodeQL {language}" for language in EXPECTED_LANGUAGES)
EXPECTED_ARTIFACTS = frozenset(f"codeql-sarif-{language}" for language in EXPECTED_LANGUAGES)
DEFAULT_MAX_AGE_HOURS = 26


class ApiInspectionError(RuntimeError):
    """The platform could not be queried, so health is unknown."""


def _parse_timestamp(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def _api_get(path: str, token: str, opener: Callable[..., Any] = urlopen) -> dict[str, Any]:
    request = Request(
        f"{API_ROOT}/{path.lstrip('/')}",
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    try:
        with opener(request, timeout=20) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except (HTTPError, URLError, TimeoutError, OSError, json.JSONDecodeError) as exc:
        raise ApiInspectionError(f"GitHub Actions API request failed: {exc}") from exc
    if not isinstance(payload, dict):
        raise ApiInspectionError("GitHub Actions API returned a non-object response")
    return payload


def inspect_evidence(
    runs_payload: dict[str, Any],
    jobs_payload: dict[str, Any] | None,
    artifacts_payload: dict[str, Any] | None,
    *,
    now: datetime,
    max_age: timedelta,
) -> tuple[bool, str, dict[str, Any]]:
    """Return ``(healthy, reason, redacted metadata)`` for API payloads."""

    runs = runs_payload.get("workflow_runs")
    if not isinstance(runs, list):
        return False, "workflow-runs response omitted workflow_runs", {}

    scheduled = [
        run
        for run in runs
        if isinstance(run, dict)
        and run.get("event") == "schedule"
        and run.get("head_branch") == "main"
    ]
    if not scheduled:
        return False, "no scheduled CodeQL run was observed", {}

    latest = max(scheduled, key=lambda run: run.get("created_at", ""))
    metadata: dict[str, Any] = {
        "run_id": latest.get("id"),
        "run_number": latest.get("run_number"),
        "status": latest.get("status"),
        "conclusion": latest.get("conclusion"),
        "created_at": latest.get("created_at"),
        "html_url": latest.get("html_url"),
    }
    created_at = latest.get("created_at")
    if not isinstance(created_at, str):
        return False, "latest scheduled run has no created_at", metadata
    try:
        age = now - _parse_timestamp(created_at)
    except ValueError:
        return False, "latest scheduled run has an invalid created_at", metadata
    if age > max_age:
        return False, f"latest scheduled run is {age.total_seconds() / 3600:.1f}h old", metadata
    if age < timedelta(0):
        return False, "latest scheduled run is dated in the future", metadata
    if latest.get("status") != "completed":
        return False, f"latest scheduled run is {latest.get('status', 'unknown')}", metadata
    if latest.get("conclusion") != "success":
        return False, f"latest scheduled run concluded {latest.get('conclusion', 'unknown')}", metadata

    if jobs_payload is None:
        return False, "scheduled run job list was not inspected", metadata
    jobs = jobs_payload.get("jobs")
    if not isinstance(jobs, list):
        return False, "job response omitted jobs", metadata
    observed_jobs = {
        job.get("name")
        for job in jobs
        if isinstance(job, dict) and isinstance(job.get("name"), str)
    }
    if len(jobs) != len(EXPECTED_JOBS) or observed_jobs != EXPECTED_JOBS:
        missing = sorted(EXPECTED_JOBS - observed_jobs)
        unexpected = sorted(observed_jobs - EXPECTED_JOBS)
        return False, f"CodeQL job population drifted (missing={missing}, unexpected={unexpected})", metadata
    failed_jobs = sorted(
        job.get("name", "unknown")
        for job in jobs
        if isinstance(job, dict)
        and job.get("name") in EXPECTED_JOBS
        and (job.get("status") != "completed" or job.get("conclusion") != "success")
    )
    if failed_jobs:
        return False, f"CodeQL matrix job(s) unhealthy: {failed_jobs}", metadata
    metadata["jobs"] = sorted(observed_jobs)

    if artifacts_payload is None:
        return False, "scheduled run artifact list was not inspected", metadata
    artifacts = artifacts_payload.get("artifacts")
    if not isinstance(artifacts, list):
        return False, "artifact response omitted artifacts", metadata
    observed_artifacts = {
        artifact.get("name")
        for artifact in artifacts
        if isinstance(artifact, dict) and isinstance(artifact.get("name"), str)
    }
    missing_artifacts = sorted(EXPECTED_ARTIFACTS - observed_artifacts)
    if missing_artifacts or len(observed_artifacts & EXPECTED_ARTIFACTS) != len(EXPECTED_ARTIFACTS):
        return False, f"successful run is missing SARIF artifact(s): {missing_artifacts}", metadata
    expected = [
        artifact
        for artifact in artifacts
        if isinstance(artifact, dict) and artifact.get("name") in EXPECTED_ARTIFACTS
    ]
    counts = {name: sum(artifact.get("name") == name for artifact in expected) for name in EXPECTED_ARTIFACTS}
    duplicates = sorted(name for name, count in counts.items() if count != 1)
    if duplicates:
        return False, f"SARIF artifact population is not exactly one per language: {duplicates}", metadata
    expired = sorted(
        artifact.get("name", "unknown")
        for artifact in expected
        if artifact.get("expired") is not False
    )
    if expired:
        return False, f"SARIF artifact(s) expired or have unknown retention: {expired}", metadata
    metadata["artifacts"] = sorted(EXPECTED_ARTIFACTS)
    return True, "recent successful CodeQL matrix retained every SARIF artifact", metadata


def _write_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, help="owner/name repository slug")
    parser.add_argument("--workflow", default=WORKFLOW_FILE)
    parser.add_argument("--max-age-hours", type=float, default=DEFAULT_MAX_AGE_HOURS)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args(argv)

    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    observed_at = datetime.now(timezone.utc)
    report: dict[str, Any] = {
        "schema_version": "1",
        "workflow": args.workflow,
        "expected_event": "schedule",
        "expected_branch": "main",
        "expected_jobs": sorted(EXPECTED_JOBS),
        "expected_artifacts": sorted(EXPECTED_ARTIFACTS),
        "observed_at": observed_at.isoformat().replace("+00:00", "Z"),
    }
    try:
        if not token:
            raise ApiInspectionError("GH_TOKEN/GITHUB_TOKEN is not configured")
        prefix = f"repos/{args.repo}/actions/workflows/{args.workflow}/runs"
        runs = _api_get(f"{prefix}?event=schedule&branch=main&per_page=20", token)
        candidates = runs.get("workflow_runs", [])
        latest = max(
            (
                run
                for run in candidates
                if isinstance(run, dict)
                and run.get("event") == "schedule"
                and run.get("head_branch") == "main"
            ),
            key=lambda run: run.get("created_at", ""),
            default=None,
        )
        jobs = artifacts = None
        if latest and latest.get("id"):
            run_id = latest["id"]
            jobs = _api_get(f"repos/{args.repo}/actions/runs/{run_id}/jobs?per_page=100", token)
            artifacts = _api_get(f"repos/{args.repo}/actions/runs/{run_id}/artifacts?per_page=100", token)
        healthy, reason, metadata = inspect_evidence(
            runs,
            jobs,
            artifacts,
            now=observed_at,
            max_age=timedelta(hours=args.max_age_hours),
        )
        report.update({"status": "ok" if healthy else "evidence_gap", "reason": reason, **metadata})
        _write_report(args.report, report)
        print(f"CodeQL evidence: {'OK' if healthy else 'GAP'} — {reason}")
        return 0 if healthy else 1
    except ApiInspectionError as exc:
        report.update({"status": "watchdog_error", "reason": str(exc)})
        _write_report(args.report, report)
        print(f"CodeQL evidence: UNKNOWN — {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
