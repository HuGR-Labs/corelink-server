#!/usr/bin/env python3
"""Verify that the daily secrets-drift evidence actually exists.

The secrets scanner is a self-hosted job.  A scheduled job can still be absent
when the runner pool is unavailable, so this watchdog asks GitHub's Actions API
for the latest scheduled run and its artifact.  It fails closed when there is
no recent completed-success run or when the run did not retain the report.

Only run metadata is returned by this script; artifact contents and credentials
are never printed.  Exit codes:

* 0 -- a recent successful run has the expected artifact;
* 1 -- an evidence gap was observed;
* 2 -- the watchdog could not inspect GitHub's API.
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
WORKFLOW_FILE = "secrets-drift.yml"
EXPECTED_ARTIFACT = "secrets-drift-report"
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
        return False, "no scheduled secrets-drift run was observed", {}

    latest = max(scheduled, key=lambda run: run.get("created_at", ""))
    metadata = {
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

    if artifacts_payload is None:
        return False, "scheduled run artifact list was not inspected", metadata
    artifacts = artifacts_payload.get("artifacts")
    if not isinstance(artifacts, list):
        return False, "artifact response omitted artifacts", metadata
    expected = [
        artifact
        for artifact in artifacts
        if isinstance(artifact, dict) and artifact.get("name") == EXPECTED_ARTIFACT
    ]
    if not expected:
        return False, f"successful run has no {EXPECTED_ARTIFACT} artifact", metadata
    if any(artifact.get("expired") is not False for artifact in expected):
        return False, f"{EXPECTED_ARTIFACT} artifact expiry is unknown or true", metadata

    metadata["artifact_name"] = EXPECTED_ARTIFACT
    metadata["artifact_count"] = len(expected)
    return True, "recent successful run retained the secrets-drift report", metadata


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
        "expected_artifact": EXPECTED_ARTIFACT,
        "observed_at": observed_at.isoformat().replace("+00:00", "Z"),
    }
    try:
        if not token:
            raise ApiInspectionError("GH_TOKEN/GITHUB_TOKEN is not configured")
        runs = _api_get(
            f"repos/{args.repo}/actions/workflows/{args.workflow}/runs"
            "?event=schedule&branch=main&per_page=20",
            token,
        )
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
        artifacts = None
        if latest and latest.get("id"):
            artifacts = _api_get(
                f"repos/{args.repo}/actions/runs/{latest['id']}/artifacts?per_page=100",
                token,
            )
        healthy, reason, metadata = inspect_evidence(
            runs,
            artifacts,
            now=observed_at,
            max_age=timedelta(hours=args.max_age_hours),
        )
        report.update({"status": "ok" if healthy else "evidence_gap", "reason": reason, **metadata})
        _write_report(args.report, report)
        print(f"secrets-drift evidence: {'OK' if healthy else 'GAP'} — {reason}")
        return 0 if healthy else 1
    except ApiInspectionError as exc:
        report.update({"status": "watchdog_error", "reason": str(exc)})
        _write_report(args.report, report)
        print(f"secrets-drift evidence: UNKNOWN — {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
