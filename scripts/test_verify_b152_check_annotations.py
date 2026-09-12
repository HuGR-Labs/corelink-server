#!/usr/bin/env python3
"""Mutation checks for the B-152 annotation evidence verifier."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "reports/perf/b152-actions-check-annotations-2026-09-09.json"
SPEC = importlib.util.spec_from_file_location("verify_b152_check_annotations", ROOT / "scripts/verify_b152_check_annotations.py")
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def rejected(data: dict) -> None:
    try:
        MODULE.verify(data)
    except MODULE.EvidenceError:
        return
    raise AssertionError("mutation unexpectedly passed")


def main() -> int:
    original = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    MODULE.verify(original)

    mutations = []
    for path, value in [
        (("schema",), "b152-actions-check-annotations/v0"),
        (("read_only",), False),
        (("repository",), "other/repo"),
        (("base_sha",), "0" * 40),
        (("historical_window", "start"), "2026-08-31T03:50:01Z"),
        (("historical_window", "end_exclusive"), "2026-08-31T07:19:59Z"),
        (("historical_window", "run_count"), 1620),
        (("historical_window", "failed_run_count"), 32),
        (("historical_window", "failed_job_count"), 32),
        (("historical_window", "duration_window_seconds", "low"), 595),
        (("historical_window", "duration_window_seconds", "high"), 614),
        (("historical_window", "duration_window_seconds", "matching_job_count"), 2),
        (("historical_window", "snapshot"), "other.json"),
        (("historical_window", "snapshot_sha256"), "0" * 64),
        (("historical_window", "refresh_was_byte_identical"), False),
        (("github_annotation", "message"), "mutated"),
        (("github_annotation", "message_sha256"), "0" * 64),
        (("github_annotation", "message_sha256_with_trailing_newline"), "0" * 64),
        (("github_annotation", "identical_across_matching_jobs"), False),
        (("pr_1492_reconciliation", "pull_request"), 1491),
        (("pr_1492_reconciliation", "branch"), "other"),
        (("pr_1492_reconciliation", "same_push_head_sha"), "0" * 40),
        (("pr_1492_reconciliation", "run_query", "branch"), "other"),
        (("pr_1492_reconciliation", "run_query", "event"), "push"),
        (("pr_1492_reconciliation", "run_query", "created"), "other"),
        (("pr_1492_reconciliation", "run_query", "per_page"), 50),
        (("pr_1492_reconciliation", "run_query", "paginated"), False),
        (("pr_1492_reconciliation", "run_count"), 206),
        (("pr_1492_reconciliation", "jobs_with_594_to_615_second_duration_in_non_success_runs"), 2),
        (("jobs", 0, "run_id"), 1),
        (("jobs", 0, "job_id"), 1),
        (("jobs", 0, "run_duration_seconds"), 599),
        (("jobs", 0, "steps_retained"), 1),
        (("jobs", 0, "artifacts_retained"), 1),
        (("jobs", 0, "check_run_http_status"), 404),
        (("jobs", 0, "annotations_http_status"), 404),
        (("jobs", 0, "job_log_http_status"), 200),
        (("jobs", 0, "annotation_count"), 0),
        (("jobs", 1, "annotation_level"), "warning"),
        (("jobs", 2, "annotation_message_sha256"), "0" * 64),
        (("pr_1492_reconciliation", "jobs", 0, "github_annotation"), "mutated"),
        (("pr_1492_reconciliation", "jobs", 0, "duration_seconds"), 599),
        (("pr_1492_reconciliation", "jobs", 0, "annotation_message_sha256"), "0" * 64),
        (("pr_1492_reconciliation", "jobs", 0, "run_id"), 1),
        (("pr_1492_reconciliation", "jobs", 0, "workflow"), "other"),
        (("pr_1492_reconciliation", "jobs", 0, "lane"), "other"),
        (("pr_1492_reconciliation", "jobs", 0, "conclusion"), "failure"),
        (("pr_1492_reconciliation", "jobs", 0, "declared_timeout_minutes_at_head_sha"), 30),
        (("pr_1492_reconciliation", "jobs", 1, "run_id"), 1),
        (("pr_1492_reconciliation", "jobs", 1, "workflow"), "other"),
        (("pr_1492_reconciliation", "jobs", 1, "lane"), "other"),
        (("pr_1492_reconciliation", "jobs", 1, "conclusion"), "cancelled"),
        (("pr_1492_reconciliation", "jobs", 1, "declared_timeout_minutes_at_head_sha"), 5),
        (("pr_1492_reconciliation", "jobs", 1, "annotation_message_sha256"), "0" * 64),
        (("pr_1492_reconciliation", "jobs", 2, "run_id"), 1),
        (("pr_1492_reconciliation", "jobs", 2, "workflow"), "other"),
        (("pr_1492_reconciliation", "jobs", 2, "lane"), "other"),
        (("pr_1492_reconciliation", "jobs", 2, "conclusion"), "failure"),
        (("pr_1492_reconciliation", "jobs", 2, "declared_timeout_minutes_at_head_sha"), 30),
        (("pr_1492_reconciliation", "jobs", 2, "annotation_message_sha256"), "0" * 64),
        (("classification", "common_cause"), True),
        (("classification", "global_failed_duration_window_common_immediate_failure"), "test_failure"),
        (("classification", "suite_or_declared_timeout_is_common_cause"), True),
        (("classification", "pr_1492_has_one_common_cause"), True),
        (("classification", "pr_1492_causes", "declared_five_minute_job_timeout"), 3),
        (("classification", "underlying_runner_loss_mechanism"), "network"),
        (("classification", "underlying_runner_loss_removed"), True),
        (("classification", "status"), "closed"),
    ]:
        item = copy.deepcopy(original)
        cursor = item
        for key in path[:-1]:
            cursor = cursor[key]
        cursor[path[-1]] = value
        mutations.append(item)

    for mutation in mutations:
        rejected(mutation)
    print(f"B152 annotation verifier mutations: PASS ({len(mutations)} rejected)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
