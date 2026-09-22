#!/usr/bin/env python3
"""Validate the redacted B-012 GitHub App bot-PR evidence receipt."""

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
    "dco-check": {"job": "dco", "run_id": 35793072637, "job_id": 106965922571,
                  "completed_at": "2026-09-22T22:34:29Z"},
    "rustfmt": {"job": "cargo fmt --all --check", "run_id": 35793072550,
                "job_id": 106965922375, "completed_at": "2026-09-22T22:34:37Z"},
}
TOP_LEVEL = {
    "schema_version", "captured_at", "repository", "bot_identity", "credential_kind",
    "app_permissions", "app_events", "installation", "actions_secret_names", "pr",
    "workflow_runs", "all_required_checks_observed", "approval_state", "runner_names",
    "reviewer", "evidence_comment",
}
FORBIDDEN_KEYS = {"installation_id", "app_installation_id", "private_key", "jwt",
                  "access_token", "installation_token", "pem"}
SECRET_VALUE = re.compile(r"-----BEGIN [A-Z ]+ PRIVATE KEY-----|\b(?:gh[pousr]_|github_pat_)\S+", re.I)


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
            normalized = key.casefold()
            if normalized in FORBIDDEN_KEYS:
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
    if captured_at.utcoffset().total_seconds() != 0:
        _fail("captured_at must use UTC")
    if obj["repository"] != "HuGR-dev/corelink-server":
        _fail("repository does not match the target")
    if obj["bot_identity"] != "app/corelink-bot-ci" or obj["credential_kind"] != "app":
        _fail("App bot identity or credential kind does not match the readback")
    if obj["app_permissions"] != APP_PERMISSIONS or obj["app_events"] != []:
        _fail("App permissions/events do not match the least-privilege manifest")
    installation = _keys(obj["installation"],
                         {"account", "repository_selection", "repositories", "permissions"},
                         "installation")
    if installation != {
        "account": "HuGR-dev", "repository_selection": "selected",
        "repositories": ["HuGR-dev/corelink-server"], "permissions": APP_PERMISSIONS,
    }:
        _fail("installation scope or permissions differ from the verified readback")
    if obj["actions_secret_names"] != SECRET_NAMES:
        _fail("Actions secret-name presence does not match the verified metadata")

    pr = _keys(obj["pr"], {"number", "url", "author", "is_bot", "head_sha", "base_sha",
                            "state", "merged", "temporary_branch_deleted"}, "pr")
    if (pr["number"] != 2093 or pr["author"] != "app/corelink-bot-ci" or pr["is_bot"] is not True
            or pr["state"] != "closed" or pr["merged"] is not False
            or pr["temporary_branch_deleted"] is not True):
        _fail("temporary bot PR state or identity is not verified")
    if pr["url"] != "https://github.com/HuGR-dev/corelink-server/pull/2093":
        _fail("PR URL does not match the observed proof")
    if not re.fullmatch(r"[0-9a-f]{40}", pr["head_sha"]) or not re.fullmatch(r"[0-9a-f]{40}", pr["base_sha"]):
        _fail("PR revisions must be full commit IDs")

    runs = obj["workflow_runs"]
    if not isinstance(runs, list) or len(runs) != len(EXPECTED_RUNS):
        _fail("exactly the DCO and rustfmt workflow runs are required")
    seen: set[str] = set()
    for run in runs:
        run_obj = _keys(run, {"run_id", "workflow", "event", "head_sha", "conclusion", "started_at",
                               "completed_at", "job_count", "job_name", "job_url", "runner_group",
                               "runner_labels"}, "workflow run")
        workflow = run_obj["workflow"]
        expected = EXPECTED_RUNS.get(workflow)
        if expected is None or workflow in seen:
            _fail("unexpected or duplicate required workflow")
        seen.add(workflow)
        if (run_obj["run_id"] != expected["run_id"] or run_obj["job_name"] != expected["job"]
                or run_obj["event"] != "pull_request" or run_obj["head_sha"] != pr["head_sha"]
                or run_obj["conclusion"] != "success" or run_obj["completed_at"] != expected["completed_at"]
                or run_obj["job_count"] != 1 or run_obj["runner_group"] != "GitHub Actions"
                or run_obj["runner_labels"] != ["ubuntu-latest"]):
            _fail(f"{workflow} did not meet the hosted completed-job contract")
        expected_url = (f"https://github.com/HuGR-dev/corelink-server/actions/runs/{expected['run_id']}"
                        f"/job/{expected['job_id']}")
        if run_obj["job_url"] != expected_url:
            _fail(f"{workflow} job URL does not match observed metadata")
    if seen != set(EXPECTED_RUNS):
        _fail("DCO and rustfmt are both required")
    if obj["all_required_checks_observed"] is not True or obj["approval_state"] != "not_required":
        _fail("required checks or approval-state evidence is incomplete")
    if obj["runner_names"] != ["ubuntu-latest"]:
        _fail("runner_names must identify the hosted runner")
    if not isinstance(obj["reviewer"], str) or not obj["reviewer"].strip():
        _fail("reviewer metadata is required")
    if obj["evidence_comment"] != "https://github.com/HuGR-dev/corelink-server/issues/1642#issuecomment-5785376771":
        _fail("evidence comment URL does not match the issue record")


def main() -> int:
    try:
        record = json.loads(DEFAULT_RECORD.read_text(encoding="utf-8"))
        validate_record(record)
    except (OSError, json.JSONDecodeError, EvidenceError) as error:
        print(f"B-012 evidence: FAIL: {error}", file=sys.stderr)
        return 1
    print("B-012 sanitized App bot-PR evidence: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
