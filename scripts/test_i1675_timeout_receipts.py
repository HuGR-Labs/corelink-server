#!/usr/bin/env python3
"""Focused contract and mutation tests for issue #1675."""

from __future__ import annotations

import copy
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from classify_timeout_receipt import ReceiptError, classify  # noqa: E402


def observation(**changes):
    base = {
        "schema": "corelink.actions-job-observation.v1",
        "run_id": 1675,
        "job_id": 600,
        "started_at": "2026-09-21T12:00:00Z",
        "completed_at": "2026-09-21T12:10:00Z",
        "declared_timeout_seconds": 600,
        "conclusion": "failure",
        "post_step": {"present": True, "command_exit_code": 1, "signal": None},
        "runner": {"heartbeat_lost": False, "communication_lost": False},
    }
    base.update(changes)
    return base


def assert_classification(expected, **changes):
    receipt = classify(observation(**changes))
    assert receipt["classification"] == expected, receipt
    assert receipt["schema"] == "corelink.actions-timeout-receipt.v1"
    return receipt


def test_complete_matrix():
    assert_classification("declared_timeout", post_step={"present": True, "command_exit_code": 124, "signal": None, "timeout_reason": "declared_timeout"}, conclusion="timed_out")
    assert_classification("cancelled", conclusion="cancelled", post_step=None)
    assert_classification("runner_communication_loss", runner={"heartbeat_lost": True, "communication_lost": False}, post_step=None)
    assert_classification("command_failure", post_step={"present": True, "command_exit_code": 124, "signal": None})
    assert_classification("command_failure", post_step={"present": True, "command_exit_code": None, "signal": "SIGTERM"})
    assert_classification("command_failure", post_step={"present": True, "command_exit_code": 7, "signal": None})
    assert_classification("success", conclusion="success", post_step={"present": True, "command_exit_code": 0, "signal": None})


def test_missing_post_step_is_indeterminate():
    assert_classification("indeterminate", post_step=None)


def test_duration_alone_never_means_timeout():
    receipt = assert_classification("command_failure", post_step={"present": True, "command_exit_code": 1, "signal": None})
    assert "600" not in receipt["classification_reason"]


def test_conflicting_runner_and_cancelled_evidence_is_indeterminate():
    assert_classification("indeterminate", cancelled=True, runner={"heartbeat_lost": True, "communication_lost": False})


def test_mutations_fail_closed():
    base = observation()
    mutants = []
    deleted_marker = copy.deepcopy(base)
    deleted_marker["post_step"] = {"present": True, "command_exit_code": 124, "signal": None}
    deleted_marker["conclusion"] = "timed_out"
    mutants.append(deleted_marker)
    mutants.append({**base, "schema": "corelink.actions-job-observation.v0"})
    mutants.append({**base, "declared_timeout_seconds": 0})
    mutants.append({**base, "runner": {"heartbeat_lost": "true", "communication_lost": False}})
    for mutant in mutants:
        try:
            receipt = classify(mutant)
        except ReceiptError:
            continue
        assert receipt["classification"] in {"indeterminate", "command_failure"}, receipt
        assert receipt["classification"] != "declared_timeout", receipt


if __name__ == "__main__":
    for name, function in sorted(globals().items()):
        if name.startswith("test_"):
            function()
    print("i1675 timeout receipt contract: PASS")
