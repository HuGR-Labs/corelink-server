#!/usr/bin/env python3
"""Contract and mutation tests for issue #1687's bounded lane."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_i1687_endurance_lane import verify_path, verify_scenario, verify_workflow  # noqa: E402


def workflow_text() -> str:
    return (Path(__file__).resolve().parents[1] / ".github/workflows/endurance-2h-nightly.yml").read_text()


def test_contract_is_green() -> None:
    verify_path(Path(__file__).resolve().parents[1])


def test_mutations_cannot_escape_run_scoped_state() -> None:
    scenario = (Path(__file__).resolve().parents[1] / "tests/load/k6/scenarios/endurance-24h.js").read_text()
    mutations = (
        ("'x-corelink-load-test-run-id': RUN_ID", "'x-corelink-load-test-run-id': 'shared'"),
        ("_run_${RUN_ID}_endurance_", "_endurance_"),
        ("evt_load_endurance_${RUN_ID}_${idx}", "evt_load_endurance_${idx}"),
        ("if (!/^\\d{1,20}$/.test(RUN_ID))", "if (false)"),
    )
    for old, new in mutations:
        mutated = scenario.replace(old, new, 1)
        assert mutated != scenario
        assert verify_scenario(mutated), f"run-scope mutation escaped: {old}"


def test_mutations_fail_closed() -> None:
    source = workflow_text()
    mutations = (
        ("workflow_dispatch:", "schedule:"),
        ("persist-credentials: false", "persist-credentials: true"),
        ("    environment: staging", "    environment: production"),
        ("CANONICAL_TARGET='https://staging.corelink.humangr.com'", "CANONICAL_TARGET='https://evil.example'"),
        ("scripts/validate_load_target_receipt.py", "scripts/skip_load_target_receipt.py"),
        ('K6_TARGET_HOST:             ${{ steps.target_host.outputs.target_host }}', 'K6_TARGET_HOST: ${{ secrets.K6_TARGET_HOST }}'),
        ("timeout --signal=TERM --kill-after=60s 130m k6 run", "k6 run"),
        ("VUS:                        '50'", "VUS:                        '500'"),
        ('"artifact_sha256": hashes', '"artifact_sha256": {}'),
        ("corelink.endurance-heartbeat.v1", "corelink.heartbeat.v0"),
        ("checkpoint-teardown", "teardown"),
        ("if: always() && steps.target_host.outcome == 'success'", "if: always()"),
        ("K6_STAGING_TEARDOWN_TOKEN is required; refusing unmanaged synthetic state", "cleanup token optional"),
        ("timeout 30s curl --fail --silent --show-error --location", "curl --fail --silent --show-error --location"),
        ('"run_id":"%s","scenario":"endurance-2h"', '"run_id":"*","scenario":"endurance-2h"'),
        ('"teardown_status": os.environ["TEARDOWN_STATUS"]', '"teardown_status": "passed"'),
        ('"teardown_deletion_proven": False', '"teardown_deletion_proven": True'),
        ('test "${CONFIRM}" = "run-bounded-endurance"', 'test "${CONFIRM}" = "yes"'),
        ('test "${DURATION}" = \'2h\'', 'test "${DURATION}" = \'30s\''),
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
