#!/usr/bin/env python3
"""Verify the closed B-152 Check Runs annotation evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path
from typing import Any


SCHEMA = "b152-actions-check-annotations/v1"
RUNNER_DIGEST = "bddbee89083419f76cff9f653d44c424b86403a4d59179cbfc141c22bf52549d"
TIMEOUT_DIGEST = "aa6cde1b5f3330052ff6d498ec600f6f14981f4575c8120323d7bfc3eb227350"
SHA256 = re.compile(r"^[0-9a-f]{64}$")
BASE_SHA = "64e57a2ccb218f475b44a260b64af95e0bc7df2c"
SNAPSHOT_PATH = "reports/perf/b152-actions-2026-09-06.json"
SNAPSHOT_DIGEST = "6eab5821a3f010aff008cb99ebd4bcbe528b4479a01ad5e649d1a26c11aae38f"
GLOBAL_IDS = {
    99377079175: 33355619309,
    99378740877: 33356199472,
    99388658702: 33359740663,
}
PR_1492_HEAD = "72a7908f5abca61f6ff0bc236f7769a83b087a9e"
PR_1492_JOBS = {
    99388657822: {
        "run_id": 33359740581,
        "workflow": "dco-check",
        "lane": "dco",
        "conclusion": "cancelled",
        "declared_timeout_minutes_at_head_sha": 5,
        "annotation_message_sha256": TIMEOUT_DIGEST,
    },
    99388658702: {
        "run_id": 33359740663,
        "workflow": "admin-ui e2e",
        "lane": "playwright critical-flows (chromium)",
        "conclusion": "failure",
        "declared_timeout_minutes_at_head_sha": 30,
        "annotation_message_sha256": RUNNER_DIGEST,
    },
    99388658533: {
        "run_id": 33359740716,
        "workflow": "proptest-density-gate",
        "lane": "proptest density gate",
        "conclusion": "cancelled",
        "declared_timeout_minutes_at_head_sha": 5,
        "annotation_message_sha256": TIMEOUT_DIGEST,
    },
}
ROOT = Path(__file__).resolve().parents[1]


class EvidenceError(RuntimeError):
    pass


def _obj(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise EvidenceError(f"{label} must be an object")
    return value


def _jobs(value: Any, label: str, count: int) -> list[dict[str, Any]]:
    if not isinstance(value, list) or len(value) != count or any(not isinstance(v, dict) for v in value):
        raise EvidenceError(f"{label} must contain exactly {count} job objects")
    ids = [v.get("job_id") for v in value]
    if any(isinstance(v, bool) or not isinstance(v, int) or v <= 0 for v in ids) or len(ids) != len(set(ids)):
        raise EvidenceError(f"{label} job identities are malformed or duplicated")
    return value


def _digest(message: Any) -> str:
    if not isinstance(message, str) or not message:
        raise EvidenceError("raw annotation message is missing")
    return hashlib.sha256(message.encode("utf-8")).hexdigest()


def _annotation(job: dict[str, Any], digest: str, *, raw_message: bool) -> None:
    if job.get("annotation_count") != 1 or job.get("annotation_level") != "failure":
        raise EvidenceError("job annotation count/level is not exact")
    actual = job.get("annotation_message_sha256")
    if not isinstance(actual, str) or not SHA256.fullmatch(actual) or actual != digest:
        raise EvidenceError("job annotation digest is missing or unexpected")
    if raw_message and _digest(job.get("github_annotation")) != actual:
        raise EvidenceError("job annotation digest does not match its raw message")
    if job.get("annotations_http_status") != 200 or job.get("job_log_http_status") != 404:
        raise EvidenceError("job endpoint status is missing or unexpected")
    if job.get("duration_seconds", job.get("run_duration_seconds")) != 600:
        raise EvidenceError("job duration is not the retained 600-second value")


def verify(data: dict[str, Any]) -> None:
    if (
        data.get("schema") != SCHEMA
        or data.get("repository") != "HuGR-Labs/corelink-server"
        or data.get("base_sha") != BASE_SHA
        or data.get("read_only") is not True
    ):
        raise EvidenceError("schema/provenance mismatch")
    window = _obj(data.get("historical_window"), "historical_window")
    if window.get("start") != "2026-08-31T03:50:00Z" or window.get("end_exclusive") != "2026-08-31T07:20:00Z":
        raise EvidenceError("closed historical window mismatch")
    if (window.get("run_count"), window.get("failed_run_count"), window.get("failed_job_count")) != (1621, 33, 33):
        raise EvidenceError("closed historical population mismatch")
    duration = _obj(window.get("duration_window_seconds"), "duration_window_seconds")
    if duration != {"low": 594, "high": 615, "matching_job_count": 3}:
        raise EvidenceError("historical duration-window population mismatch")
    if (
        window.get("snapshot") != SNAPSHOT_PATH
        or window.get("snapshot_sha256") != SNAPSHOT_DIGEST
        or window.get("refresh_was_byte_identical") is not True
    ):
        raise EvidenceError("snapshot identity/refresh mismatch")
    try:
        actual_snapshot_digest = hashlib.sha256((ROOT / SNAPSHOT_PATH).read_bytes()).hexdigest()
    except OSError as exc:
        raise EvidenceError("bound snapshot cannot be read") from exc
    if actual_snapshot_digest != SNAPSHOT_DIGEST:
        raise EvidenceError("bound snapshot content digest mismatch")

    header = _obj(data.get("github_annotation"), "github_annotation")
    if header.get("annotation_count_per_job") != 1 or header.get("annotation_level") != "failure":
        raise EvidenceError("global annotation header count/level mismatch")
    if _digest(header.get("message")) != RUNNER_DIGEST or header.get("message_sha256") != RUNNER_DIGEST:
        raise EvidenceError("global annotation header digest mismatch")
    newline_digest = hashlib.sha256((header["message"] + "\n").encode("utf-8")).hexdigest()
    if header.get("message_sha256_with_trailing_newline") != newline_digest:
        raise EvidenceError("global annotation newline digest mismatch")
    if header.get("identical_across_matching_jobs") is not True:
        raise EvidenceError("global annotation identity assertion missing")

    global_jobs = _jobs(data.get("jobs"), "jobs", 3)
    if {job["job_id"]: job.get("run_id") for job in global_jobs} != GLOBAL_IDS:
        raise EvidenceError("global run/job identity set mismatch")
    for job in global_jobs:
        _annotation(job, RUNNER_DIGEST, raw_message=False)
        if job.get("steps_retained") != 0 or job.get("artifacts_retained") != 0:
            raise EvidenceError("global job retention boundary mismatch")
        if job.get("check_run_http_status") != 200:
            raise EvidenceError("global Check Run endpoint status mismatch")
    if len({job["annotation_message_sha256"] for job in global_jobs}) != 1:
        raise EvidenceError("global runner-loss annotations are not identical")

    pr = _obj(data.get("pr_1492_reconciliation"), "pr_1492_reconciliation")
    if (
        pr.get("pull_request") != 1492
        or pr.get("branch") != "claude/wp-a-b080-scopes"
        or pr.get("same_push_head_sha") != PR_1492_HEAD
    ):
        raise EvidenceError("PR #1492 identity mismatch")
    query = _obj(pr.get("run_query"), "run_query")
    if query != {
        "branch": "claude/wp-a-b080-scopes",
        "event": "pull_request",
        "created": "2026-08-31T03:50:00Z..2026-08-31T07:20:00Z",
        "per_page": 100,
        "paginated": True,
    }:
        raise EvidenceError("PR #1492 run query is incomplete or ambiguous")
    if pr.get("run_count") != 207 or pr.get("jobs_with_594_to_615_second_duration_in_non_success_runs") != 3:
        raise EvidenceError("PR #1492 population mismatch")
    pr_jobs = _jobs(pr.get("jobs"), "pr_1492_reconciliation.jobs", 3)
    if set(job["job_id"] for job in pr_jobs) != set(PR_1492_JOBS):
        raise EvidenceError("PR #1492 job identity set mismatch")
    for job in pr_jobs:
        expected = PR_1492_JOBS[job["job_id"]]
        for field, value in expected.items():
            if job.get(field) != value:
                raise EvidenceError(f"PR #1492 job {job['job_id']} {field} mismatch")
        _annotation(job, expected["annotation_message_sha256"], raw_message=True)

    classification = _obj(data.get("classification"), "classification")
    if (
        classification.get("common_cause") is not False
        or classification.get("global_failed_duration_window_common_immediate_failure")
        != "self_hosted_runner_lost_communication"
        or classification.get("suite_or_declared_timeout_is_common_cause") is not False
        or classification.get("pr_1492_has_one_common_cause") is not False
        or classification.get("pr_1492_causes")
        != {"declared_five_minute_job_timeout": 2, "self_hosted_runner_lost_communication": 1}
        or classification.get("underlying_runner_loss_mechanism") != "indeterminate"
        or classification.get("underlying_runner_loss_removed") is not False
        or classification.get("status") != "open"
    ):
        raise EvidenceError("causal/status boundary was weakened")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("evidence", type=Path)
    args = parser.parse_args()
    try:
        verify(_obj(json.loads(args.evidence.read_text(encoding="utf-8")), "evidence"))
    except (OSError, json.JSONDecodeError, EvidenceError) as exc:
        print(f"B152 annotations: FAIL ({exc})")
        return 1
    print("B152 annotations: PASS (3 runner-loss jobs; PR #1492 split 2 timeout / 1 runner-loss; status open)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
