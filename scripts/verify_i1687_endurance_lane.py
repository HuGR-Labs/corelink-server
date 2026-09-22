#!/usr/bin/env python3
"""Fail-closed static contract for the bounded #1687 endurance lane."""

from __future__ import annotations

import re
from pathlib import Path


CANONICAL_TARGET = "https://staging.corelink.humangr.com"
WORKFLOW = Path(".github/workflows/endurance-2h-nightly.yml")
PROTECTED_DISPATCH = (
    "github.repository == 'HuGR-dev/corelink-server' && "
    "github.event_name == 'workflow_dispatch' && "
    "github.ref == 'refs/heads/main' && github.ref_protected"
)


class LaneContractError(ValueError):
    """The workflow cannot prove the bounded operator lane contract."""


def verify_workflow(text: str) -> list[str]:
    errors: list[str] = []
    if "workflow_dispatch:" not in text:
        errors.append("workflow_dispatch trigger is missing")
    if text.count(PROTECTED_DISPATCH) != 2:
        errors.append("measurement and baseline jobs must require a protected canonical dispatch")
    if re.search(r"^\s+schedule:", text, re.MULTILINE):
        errors.append("schedule trigger would make the lane unattended")
    if "CANONICAL_TARGET='https://staging.corelink.humangr.com'" not in text:
        errors.append("owner approved canonical target is missing")
    if 'TARGET_HOST="${K6_TARGET_HOST%/}"' not in text:
        errors.append("target normalization is missing")
    if 'K6_TARGET_HOST:             ${{ steps.target_host.outputs.target_host }}' not in text:
        errors.append("runtime does not consume the checked target output")
    if "K6_TARGET_HOST: ${{ secrets.K6_TARGET_HOST }}" not in text:
        errors.append("preflight does not read the environment secret")
    if "VUS:                        '50'" not in text:
        errors.append("runtime population is not pinned to 50 VUs")
    if '"vus": int(os.environ["VUS"])' not in text:
        errors.append("receipt does not bind the effective VU population")
    timeout = re.search(r"timeout-minutes:\s*(\d+)", text)
    if timeout is None or int(timeout.group(1)) > 145:
        errors.append("job timeout is absent or exceeds the 145 minute bound")
    if "timeout --signal=TERM --kill-after=60s 130m k6 run" not in text:
        errors.append("k6 command has no bounded graceful timeout")
    for marker in (
        '"schema": "corelink.endurance-lane-receipt.v1"',
        '"run_id": "${GITHUB_RUN_ID}"',
        '"run_attempt": "${GITHUB_RUN_ATTEMPT}"',
        '"sha": "${GITHUB_SHA}"',
        '"runner_name": "${RUNNER_NAME}"',
        '"artifact_sha256": hashes',
    ):
        if marker not in text:
            errors.append(f"receipt marker missing: {marker}")
    for marker in (
        "corelink.endurance-heartbeat.v1",
        "heartbeat &",
        "checkpoint-prepared",
        "checkpoint-running",
        "checkpoint-completed",
        "checkpoint-teardown",
        "if: always()",
    ):
        if marker not in text:
            errors.append(f"lifecycle marker missing: {marker}")
    if "sha256sum" not in text:
        errors.append("artifact digest is missing")
    if "K6_TARGET_HOST: ${{ secrets.K6_TARGET_HOST }}" in text and "k6 run" in text:
        # The secret is allowed only as preflight input. The command must use
        # the step output, otherwise an arbitrary environment URL can bypass it.
        run_block = text.split("- name: run k6 endurance 2h", 1)[-1]
        if "secrets.K6_TARGET_HOST" in run_block:
            errors.append("k6 run receives the unchecked target secret")
    if "campaign-ci" in text or "manifesto" in text or "manifest" in text.lower():
        errors.append("campaign or remote manifest wiring leaked into the lane")
    return errors


def verify_path(root: Path = Path(".")) -> None:
    try:
        text = (root / WORKFLOW).read_text(encoding="utf-8")
    except OSError as exc:
        raise LaneContractError(f"workflow is unreadable: {exc}") from exc
    errors = verify_workflow(text)
    if errors:
        raise LaneContractError("; ".join(errors))


if __name__ == "__main__":
    verify_path()
    print("i1687 endurance lane contract: PASS")
