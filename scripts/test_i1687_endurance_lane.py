#!/usr/bin/env python3
"""Contract and mutation tests for issue #1687's bounded lane."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_i1687_endurance_lane import verify_path, verify_workflow  # noqa: E402


def workflow_text() -> str:
    return (Path(__file__).resolve().parents[1] / ".github/workflows/endurance-2h-nightly.yml").read_text()


def test_contract_is_green() -> None:
    verify_path(Path(__file__).resolve().parents[1])


def test_mutations_fail_closed() -> None:
    source = workflow_text()
    mutations = (
        ("workflow_dispatch:", "schedule:"),
        ("CANONICAL_TARGET='https://staging.corelink.humangr.com'", "CANONICAL_TARGET='https://evil.example'"),
        ('K6_TARGET_HOST:             ${{ steps.target_host.outputs.target_host }}', 'K6_TARGET_HOST: ${{ secrets.K6_TARGET_HOST }}'),
        ("timeout --signal=TERM --kill-after=60s 130m k6 run", "k6 run"),
        ("VUS:                        '50'", "VUS:                        '500'"),
        ('"artifact_sha256": hashes', '"artifact_sha256": {}'),
        ("corelink.endurance-heartbeat.v1", "corelink.heartbeat.v0"),
        ("checkpoint-teardown", "teardown"),
    )
    for old, new in mutations:
        mutated = source.replace(old, new, 1)
        assert old != new and mutated != source
        assert verify_workflow(mutated), f"mutation escaped: {old}"


def test_arbitrary_target_is_rejected_even_with_a_trailing_slash() -> None:
    source = workflow_text()
    mutated = source.replace(
        "CANONICAL_TARGET='https://staging.corelink.humangr.com'",
        "CANONICAL_TARGET='https://staging.corelink.humangr.com/evil'",
        1,
    )
    errors = verify_workflow(mutated)
    assert any("canonical target" in error for error in errors)


def test_schedule_is_rejected() -> None:
    source = workflow_text()
    errors = verify_workflow(source.replace("on:\n  # No unattended", "on:\n  schedule:\n    - cron: '0 0 * * *'\n  # No unattended", 1))
    assert any("unattended" in error for error in errors)


def test_measurement_and_baseline_reject_self_hosted_runners() -> None:
    source = workflow_text()
    runner_positions = []
    offset = 0
    while True:
        offset = source.find("runs-on: ubuntu-24.04", offset)
        if offset < 0:
            break
        runner_positions.append(offset)
        offset += 1
    assert len(runner_positions) == 2
    marker = "runs-on: ubuntu-24.04"
    for occurrence in range(2):
        pieces = source.split(marker)
        mutated = marker.join(pieces[: occurrence + 1]) + "runs-on: corelink" + marker.join(pieces[occurrence + 1 :])
        assert verify_workflow(mutated), f"self-hosted mutation escaped at job index {occurrence}"


def test_unprotected_dispatch_is_rejected() -> None:
    source = workflow_text()
    guard = (
        "github.repository == 'HuGR-dev/corelink-server' && "
        "github.event_name == 'workflow_dispatch' && "
        "github.ref == 'refs/heads/main' && github.ref_protected"
    )
    mutated = source.replace(guard, "github.event_name == 'workflow_dispatch'", 1)
    errors = verify_workflow(mutated)
    assert any("protected canonical dispatch" in error for error in errors)


if __name__ == "__main__":
    for name, function in sorted(globals().items()):
        if name.startswith("test_"):
            function()
    print("i1687 endurance lane contract and mutations: PASS")
