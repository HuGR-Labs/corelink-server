#!/usr/bin/env python3
"""Mutation tests for the persisted B-152 snapshot contract."""

from __future__ import annotations

import copy
import importlib.util


SPEC = importlib.util.spec_from_file_location("verify_b152_snapshot", "scripts/verify_b152_snapshot.py")
assert SPEC and SPEC.loader
mod = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mod)


def fixture() -> dict:
    timestamp = "2026-08-31T04:00:00Z"
    step = {
        "step_id": 1,
        "name": "Set up Node.js",
        "status": "in_progress",
        "conclusion": None,
        "started_at": timestamp,
        "completed_at": None,
    }
    job = {
        "run_id": 10,
        "job_id": 20,
        "workflow": "docs CI",
        "lane": "typecheck",
        "conclusion": "failure",
        "runner_name": "runner-1",
        "created_at": timestamp,
        "started_at": timestamp,
        "completed_at": "2026-08-31T04:10:00Z",
        "duration_seconds": 600,
        "queue_duration_seconds": 0,
        "step_in_progress": ["Set up Node.js"],
        "step_not_completed": ["Set up Node.js"],
        "steps": [step],
        "in_window": True,
    }
    return {
        "schema": mod.SCHEMA,
        "collection": {
            "source": "GitHub Actions REST API via gh api",
            "read_only": True,
            "logs_persisted": False,
            "zero_window_jobs_is_not_closure": True,
        },
        "repo": "o/r",
        "window": {"start": timestamp, "end_exclusive": "2026-09-01T00:00:00Z"},
        "duration_window_seconds": {"low": 594, "high": 615},
        "run_count": 1,
        "run_ids": [10],
        "run_conclusion_counts": {"failure": 1},
        "run_outcomes": [{
            "run_id": 10,
            "conclusion": "failure",
            "classification": "job_or_test_failure",
            "created_at": timestamp,
            "updated_at": "2026-08-31T04:10:00Z",
        }],
        "failed_run_count": 1,
        "failed_job_count": 1,
        "failed_jobs": [job],
        "window_jobs": [copy.deepcopy(job)],
        "logs": [{
            "job_id": 20,
            "available": False,
            "status": "indeterminate",
            "causal": "indeterminate",
            "http_status": 404,
            "log_signature": "indeterminate",
            "log_sha256": None,
            "error": "log retrieval failed",
        }],
        "causal_classification": {
            "status": "indeterminate",
            "causal": "indeterminate",
            "known_log_categories": ["indeterminate"],
        },
    }


def expect_reject(mutator, label: str) -> None:
    candidate = fixture()
    mutator(candidate)
    try:
        mod.verify(candidate)
    except mod.SnapshotError:
        return
    raise AssertionError(f"mutation was accepted: {label}")


def mutate_available_digest(candidate: dict, digest: str) -> None:
    candidate["logs"][0].update(
        available=True,
        status="available",
        causal=False,
        http_status=200,
        log_sha256=digest,
    )


def mutate_available_status_mismatch(candidate: dict) -> None:
    candidate["logs"][0].update(available=True, http_status=200, log_sha256="sha256:" + "a" * 64)


def mutate_available_http_failure(candidate: dict) -> None:
    candidate["logs"][0].update(
        available=True,
        status="available",
        http_status=404,
        causal=False,
        log_sha256="sha256:" + "a" * 64,
    )


def mutate_available_missing_digest(candidate: dict) -> None:
    candidate["logs"][0].update(available=True, status="available", http_status=200, causal=False)


def mutate_unavailable_contradiction(candidate: dict) -> None:
    candidate["logs"][0].update(log_sha256="sha256:" + "a" * 64, causal=False)


def main() -> int:
    result = mod.verify(fixture())
    assert result["status"] == "open/indeterminate"
    expect_reject(lambda p: p["run_ids"].append(11), "run population mismatch")
    expect_reject(lambda p: p["failed_jobs"][0].update(created_at="not-a-timeTZ"), "unparseable timestamp")
    expect_reject(lambda p: p["failed_jobs"][0].pop("runner_name"), "missing runner name")
    expect_reject(lambda p: p["failed_jobs"][0].update(runner_name={}), "runner name object")
    expect_reject(lambda p: p["failed_jobs"][0].update(runner_name=""), "empty runner name")
    expect_reject(lambda p: p["failed_jobs"][0].update(runner_name=" runner-1"), "unnormalized runner name")
    expect_reject(lambda p: p["failed_jobs"][0]["steps"][0].update(step_id=None), "missing step ID")
    expect_reject(lambda p: p["logs"].clear(), "missing log signature")
    expect_reject(mutate_available_status_mismatch, "available/status mismatch")
    expect_reject(mutate_available_http_failure, "available HTTP failure")
    expect_reject(mutate_available_missing_digest, "available log missing digest")
    expect_reject(mutate_unavailable_contradiction, "unavailable log has digest/causal evidence")
    expect_reject(lambda p: p["logs"].append(copy.deepcopy(p["logs"][0])), "duplicate log ID")
    expect_reject(lambda p: p["logs"][0].update(log_signature="runner_cancellation"), "unavailable log causal")
    expect_reject(lambda p: p["logs"][0].update(http_status=500), "missing log digest outside 404")
    expect_reject(lambda p: mutate_available_digest(p, "sha256:" + "A" * 64), "uppercase log digest")
    expect_reject(lambda p: mutate_available_digest(p, "sha256:short"), "malformed log digest")
    expect_reject(lambda p: p["causal_classification"].update(status="closed", causal=True), "false closure")
    expect_reject(lambda p: p["collection"].update(zero_window_jobs_is_not_closure=False), "zero population closure")
    print("B152 snapshot verifier mutations: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
