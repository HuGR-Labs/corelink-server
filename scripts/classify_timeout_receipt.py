#!/usr/bin/env python3
"""Fail-closed classifier for instrumented Actions job termination receipts.

The classifier never derives a cause from elapsed time.  A 600 second job can
only be called a declared timeout when the workflow's timeout marker is
present; exit 124 remains a command failure unless that marker is explicit.
"""

from __future__ import annotations

import json
import sys
from datetime import datetime
from typing import Any


OBSERVATION_SCHEMA = "corelink.actions-job-observation.v1"
RECEIPT_SCHEMA = "corelink.actions-timeout-receipt.v1"
CONCLUSIONS = frozenset({"success", "failure", "cancelled", "timed_out"})
CLASSES = frozenset({
    "success",
    "declared_timeout",
    "cancelled",
    "runner_communication_loss",
    "command_failure",
    "indeterminate",
})


class ReceiptError(ValueError):
    """Malformed or contradictory evidence; never silently classify it."""


def _bool(value: Any, field: str) -> bool:
    if not isinstance(value, bool):
        raise ReceiptError(f"{field} must be boolean")
    return value


def _positive_int(value: Any, field: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ReceiptError(f"{field} must be a positive integer")
    return value


def _id(value: Any, field: str) -> int:
    return _positive_int(value, field)


def _timestamp(value: Any, field: str) -> str:
    if not isinstance(value, str):
        raise ReceiptError(f"{field} must be an ISO-8601 string")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ReceiptError(f"{field} is not ISO-8601") from exc
    if parsed.tzinfo is None:
        raise ReceiptError(f"{field} must include a UTC offset")
    return value


def _post_step(observation: dict[str, Any]) -> dict[str, Any] | None:
    value = observation.get("post_step")
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ReceiptError("post_step must be an object")
    if not _bool(value.get("present"), "post_step.present"):
        return None
    code = value.get("command_exit_code")
    if code is not None and (isinstance(code, bool) or not isinstance(code, int)):
        raise ReceiptError("post_step.command_exit_code must be an integer or null")
    signal = value.get("signal")
    if signal is not None and (not isinstance(signal, str) or not signal.startswith("SIG")):
        raise ReceiptError("post_step.signal must be a SIG* string or null")
    timeout_reason = value.get("timeout_reason")
    if timeout_reason is not None and timeout_reason != "declared_timeout":
        raise ReceiptError("post_step.timeout_reason is not recognized")
    return value


def _runner(observation: dict[str, Any]) -> tuple[bool, bool]:
    value = observation.get("runner", {})
    if not isinstance(value, dict):
        raise ReceiptError("runner must be an object")
    heartbeat = _bool(value.get("heartbeat_lost", False), "runner.heartbeat_lost")
    communication = _bool(value.get("communication_lost", False), "runner.communication_lost")
    return heartbeat, communication


def classify(observation: dict[str, Any]) -> dict[str, Any]:
    """Validate one observation and return a versioned, auditable receipt."""
    if not isinstance(observation, dict) or observation.get("schema") != OBSERVATION_SCHEMA:
        raise ReceiptError(f"schema must be {OBSERVATION_SCHEMA}")
    run_id = _id(observation.get("run_id"), "run_id")
    job_id = _id(observation.get("job_id"), "job_id")
    conclusion = observation.get("conclusion")
    if conclusion not in CONCLUSIONS:
        raise ReceiptError("conclusion is unknown")
    declared = observation.get("declared_timeout_seconds")
    if declared is not None:
        declared = _positive_int(declared, "declared_timeout_seconds")
    started = _timestamp(observation.get("started_at"), "started_at")
    completed = _timestamp(observation.get("completed_at"), "completed_at")
    post = _post_step(observation)
    heartbeat, communication = _runner(observation)
    runner_loss = heartbeat or communication
    cancelled = observation.get("cancelled", False)
    if not isinstance(cancelled, bool):
        raise ReceiptError("cancelled must be boolean")
    cancelled = cancelled or conclusion == "cancelled"

    reasons: list[str] = []
    if runner_loss:
        reasons.append("runner heartbeat or communication evidence")
    if cancelled:
        reasons.append("explicit cancellation evidence")
    if runner_loss and cancelled:
        category, reason = "indeterminate", "conflicting runner-loss and cancellation evidence"
    elif runner_loss:
        category, reason = "runner_communication_loss", reasons[0]
    elif cancelled:
        category, reason = "cancelled", reasons[0]
    elif post is None:
        category, reason = "indeterminate", "post-step evidence is missing"
    elif post.get("timeout_reason") == "declared_timeout":
        if declared is None:
            category, reason = "indeterminate", "declared timeout marker lacks a declared limit"
        else:
            category, reason = "declared_timeout", "workflow emitted the declared-timeout marker"
    elif post.get("command_exit_code") is not None and post["command_exit_code"] != 0:
        code = post["command_exit_code"]
        category, reason = "command_failure", f"command exit code {code}"
    elif post.get("signal") is not None:
        category, reason = "command_failure", f"command terminated by {post['signal']}"
    elif post.get("command_exit_code") == 0 and conclusion == "success":
        category, reason = "success", "command and job completed successfully"
    else:
        category, reason = "indeterminate", "no typed cause establishes the outcome"

    receipt = {
        "schema": RECEIPT_SCHEMA,
        "contract_version": 1,
        "run_id": run_id,
        "job_id": job_id,
        "started_at": started,
        "completed_at": completed,
        "declared_timeout_seconds": declared,
        "conclusion": conclusion,
        "classification": category,
        "classification_reason": reason,
        "evidence": {
            "post_step_present": post is not None,
            "heartbeat_lost": heartbeat,
            "communication_lost": communication,
            "cancelled": cancelled,
            "command_exit_code": None if post is None else post.get("command_exit_code"),
            "signal": None if post is None else post.get("signal"),
        },
    }
    if category not in CLASSES:
        raise ReceiptError("internal classification contract drift")
    return receipt


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: classify_timeout_receipt.py OBSERVATION.json RECEIPT.json", file=sys.stderr)
        return 2
    try:
        with open(argv[0], encoding="utf-8") as stream:
            observation = json.load(stream)
        receipt = classify(observation)
        with open(argv[1], "w", encoding="utf-8") as stream:
            json.dump(receipt, stream, indent=2, sort_keys=True)
            stream.write("\n")
    except (OSError, json.JSONDecodeError, ReceiptError) as exc:
        print(f"indeterminate receipt: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
