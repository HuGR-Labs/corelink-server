#!/usr/bin/env python3
"""Validate the sanitized B-012 GitHub App PR and hosted-job receipt."""

from __future__ import annotations

import json
import re
import sys
from datetime import datetime
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_RECORD = ROOT / "evidence/owner-actions/B-012/bot-pr-checks.json"
APP_PERMISSIONS = {"contents": "write", "metadata": "read", "pull_requests": "write"}
SECRET_NAMES = {
    "CORELINK_BOT_APP_ID": "present",
    "CORELINK_BOT_APP_PRIVATE_KEY": "present",
    "BOT_PR_TOKEN": "absent",
}
EXPECTED_RUNS = {
    "dco-check": {
        "job_name": "dco",
        "run_id": 35793072637,
        "job_id": 106965922571,
        "runner_name": "GitHub Actions 1000003581",
        "started_at": "2026-09-22T22:34:13Z",
        "completed_at": "2026-09-22T22:34:29Z",
    },
    "rustfmt": {
        "job_name": "cargo fmt --all --check",
        "run_id": 35793072550,
        "job_id": 106965922375,
        "runner_name": "GitHub Actions 1000003580",
        "started_at": "2026-09-22T22:34:13Z",
        "completed_at": "2026-09-22T22:34:37Z",
    },
}
EXPECTED_PR_HEAD_SHA = "c17e5210d424bcc80848e808bd9c586ab3b7a1f9"
EXPECTED_PR_BASE_SHA = "0389714d9f5408f744e17227b82d795fff245a32"
TOP_LEVEL = {
    "schema_version", "captured_at", "repository", "bot_identity", "credential_kind",
    "pr_number", "app_permissions", "app_events", "installation", "actions_secret_names", "pr",
    "workflow_runs", "all_required_checks_observed", "approval_state", "runner_names",
    "reviewer", "evidence_comment",
}
RUN_FIELDS = {
    "run_id", "workflow", "url", "approval_state", "job_count", "job_urls",
    "runner_names", "runner_group", "runner_labels", "conclusion", "started_at",
    "completed_at",
}
FORBIDDEN_KEYS = {
    "installation_id", "app_installation_id", "private_key", "jwt", "access_token",
    "installation_token", "pem",
}
SECRET_VALUE = re.compile(
    r"-----BEGIN [A-Z ]+ PRIVATE KEY-----|\b(?:gh[pousr]_|github_pat_)\S+",
    re.IGNORECASE,
)
RUN_OR_JOB_URL = re.compile(
    r"https://github\.com/HuGR-dev/corelink-server/actions/runs/[0-9]+(?:/job/[0-9]+)?"
)


class EvidenceError(ValueError):
    pass


def _fail(message: str) -> None:
    raise EvidenceError(message)


def _keys(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        _fail(f"{label} has the wrong keys")
    return value


def _check_no_sensitive_material(value: Any, path: str = "receipt") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if key.casefold() in FORBIDDEN_KEYS:
                _fail(f"{path} contains forbidden field {key}")
            _check_no_sensitive_material(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _check_no_sensitive_material(child, f"{path}[{index}]")
    elif isinstance(value, str) and SECRET_VALUE.search(value):
        _fail(f"{path} contains credential-shaped text")


def validate_record(record: Any) -> None:
    obj = _keys(record, TOP_LEVEL, "B-012 evidence")
    _check_no_sensitive_material(obj)
    if obj["schema_version"] != 1:
        _fail("schema_version must be 1")
    try:
        captured_at = datetime.fromisoformat(obj["captured_at"].replace("Z", "+00:00"))
    except (AttributeError, ValueError):
        _fail("captured_at must be an ISO-8601 UTC timestamp")
    if captured_at.utcoffset() is None or captured_at.utcoffset().total_seconds() != 0:
        _fail("captured_at must use UTC")
    if obj["repository"] != "HuGR-dev/corelink-server":
        _fail("repository does not match the target")
    if obj["bot_identity"] != "app/corelink-bot-ci" or obj["credential_kind"] != "app":
        _fail("App bot identity or credential kind does not match the readback")
    if obj["pr_number"] != 2093:
        _fail("pr_number does not match the observed proof")
    if obj["app_permissions"] != APP_PERMISSIONS or obj["app_events"] != []:
        _fail("App permissions/events do not match the least-privilege manifest")
    installation = _keys(
        obj["installation"],
        {"account", "repository_selection", "repositories", "permissions"},
        "installation",
    )
    if installation != {
        "account": "HuGR-dev",
        "repository_selection": "selected",
        "repositories": ["HuGR-dev/corelink-server"],
        "permissions": APP_PERMISSIONS,
    }:
        _fail("installation scope or permissions differ from the verified readback")
    if obj["actions_secret_names"] != SECRET_NAMES:
        _fail("Actions secret-name presence does not match the verified metadata")

    pr = _keys(
        obj["pr"],
        {"number", "url", "author", "is_bot", "head_sha", "base_sha", "state", "merged",
         "temporary_branch_deleted"},
        "pr",
    )
    if (
        pr["number"] != 2093
        or pr["number"] != obj["pr_number"]
        or pr["author"] != "app/corelink-bot-ci"
        or pr["is_bot"] is not True
        or pr["state"] != "closed"
        or pr["merged"] is not False
        or pr["temporary_branch_deleted"] is not True
        or pr["url"] != "https://github.com/HuGR-dev/corelink-server/pull/2093"
    ):
        _fail("temporary bot PR state or identity is not verified")
    if not re.fullmatch(r"[0-9a-f]{40}", pr["head_sha"]) or not re.fullmatch(
        r"[0-9a-f]{40}", pr["base_sha"]
    ):
        _fail("PR revisions must be full commit IDs")
    if pr["head_sha"] != EXPECTED_PR_HEAD_SHA:
        _fail("PR head SHA does not match the observed proof PR")
    if pr["base_sha"] != EXPECTED_PR_BASE_SHA:
        _fail("PR base SHA does not match the observed proof PR")

    runs = obj["workflow_runs"]
    if not isinstance(runs, list) or len(runs) != len(EXPECTED_RUNS):
        _fail("exactly the DCO and rustfmt workflow runs are required")
    seen: set[str] = set()
    runner_names: list[str] = []
    for value in runs:
        run = _keys(value, RUN_FIELDS, "workflow run")
        workflow = run["workflow"]
        expected = EXPECTED_RUNS.get(workflow)
        if expected is None or workflow in seen:
            _fail("unexpected or duplicate required workflow")
        seen.add(workflow)
        expected_run_url = f"https://github.com/HuGR-dev/corelink-server/actions/runs/{expected['run_id']}"
        expected_job_url = f"{expected_run_url}/job/{expected['job_id']}"
        if (
            run["run_id"] != expected["run_id"]
            or run["url"] != expected_run_url
            or not RUN_OR_JOB_URL.fullmatch(run["url"])
            or run["approval_state"] != "not_required"
            or run["job_count"] != 1
            or run["job_urls"] != [expected_job_url]
            or run["runner_names"] != [expected["runner_name"]]
            or run["runner_group"] != "GitHub Actions"
            or run["runner_labels"] != ["ubuntu-latest"]
            or run["conclusion"] != "success"
            or run["started_at"] != expected["started_at"]
            or run["completed_at"] != expected["completed_at"]
        ):
            _fail(f"{workflow} did not meet the hosted completed-job contract")
        runner_names.extend(run["runner_names"])
    if seen != set(EXPECTED_RUNS):
        _fail("DCO and rustfmt are both required")
    if obj["all_required_checks_observed"] is not True or obj["approval_state"] != "not_required":
        _fail("required checks or approval-state evidence is incomplete")
    if obj["runner_names"] != runner_names:
        _fail("runner_names must match the actual hosted runner names")
    if not isinstance(obj["reviewer"], str) or not obj["reviewer"].strip():
        _fail("reviewer metadata is required")
    if obj["evidence_comment"] != (
        "https://github.com/HuGR-dev/corelink-server/issues/1642#issuecomment-5785376771"
    ):
        _fail("evidence comment URL does not match the issue record")


def main() -> int:
    try:
        validate_record(json.loads(DEFAULT_RECORD.read_text(encoding="utf-8")))
    except (OSError, json.JSONDecodeError, EvidenceError) as error:
        print(f"B-012 evidence: FAIL: {error}", file=sys.stderr)
        return 1
    print("B-012 sanitized App bot-PR evidence: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
