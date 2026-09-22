"""Fail-closed population and schema tests for the k6 baseline gate."""
from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ("signup", "webhook", "dsr", "cas", "byok")
TARGET = "https://staging.corelink.humangr.com"
TENANT = "019e7109-e514-72b2-ac5b-607d97ea64a1"
DEPLOYMENT = "a" * 40
SPEC = importlib.util.spec_from_file_location(
    "load_test_baseline_check", ROOT / "scripts" / "load-test-baseline-check.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def write_summary(root: Path, scenario: str, median: object = 10, p99: object = 20) -> None:
    target = root / scenario / "summary.json"
    target.parent.mkdir(parents=True)
    target.write_text(json.dumps({
        "schema": 1,
        "suite_version": "r3-prep-v2",
        "scenario": scenario,
        "target": TARGET,
        "tenant_id": TENANT,
        "deployment_sha": DEPLOYMENT,
        "metrics": {"http_req_duration": {"med": median, "p(99)": p99}},
    }))
    (target.parent / "status.json").write_text(
        json.dumps({"scenario": scenario, "outcome": "success"})
    )


def write_baseline(path: Path, scenarios: dict[str, dict[str, object]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(
            {
                "schema": 2,
                "baseline_version": "k6-baseline-v2",
                "suite_version": "r3-prep-v2",
                "captured_at": "2026-09-05T00:00:00Z",
                "commit": "test",
                "metric": "http_req_duration.med (ms)",
                "threshold_multiplier": 1.20,
                "identity": {
                    "target": TARGET,
                    "tenant_id": TENANT,
                    "deployment_sha": DEPLOYMENT,
                },
                "scenarios": scenarios,
            }
        )
    )


def baseline_for(*scenarios: str) -> dict[str, dict[str, object]]:
    return {scenario: {"median_ms": 10, "p99_ms": 20} for scenario in scenarios}


def run_gate(tmp_path: Path, expected: str = ",".join(SCENARIOS)) -> int:
    return MODULE.main(
        [
            "--results-dir",
            str(tmp_path / "current"),
            "--baseline",
            str(tmp_path / "baseline.json"),
            "--expected-scenarios",
            expected,
        ]
    )


def test_complete_population_compares_and_updates(tmp_path: Path) -> None:
    current = tmp_path / "current"
    for scenario in SCENARIOS:
        write_summary(current, scenario)
    write_baseline(tmp_path / "baseline.json", baseline_for(*SCENARIOS))

    assert run_gate(tmp_path) == MODULE.EXIT_OK


@pytest.mark.parametrize("mutation", ["missing-baseline", "malformed-baseline", "partial-baseline"])
def test_baseline_mutations_fail_closed(tmp_path: Path, mutation: str) -> None:
    current = tmp_path / "current"
    for scenario in SCENARIOS:
        write_summary(current, scenario)
    baseline = tmp_path / "baseline.json"
    if mutation == "malformed-baseline":
        baseline.write_text("not-json")
    elif mutation == "partial-baseline":
        write_baseline(baseline, baseline_for(*SCENARIOS[:-1]))

    assert run_gate(tmp_path) == MODULE.EXIT_USAGE

@pytest.mark.parametrize(
    "median,p99",
    [(None, 20), ("not-a-number", 20), (10, 0), (10, "nan")],
)
def test_malformed_current_summary_fails_closed(
    tmp_path: Path, median: object, p99: object
) -> None:
    current = tmp_path / "current"
    write_summary(current, "signup", median, p99)
    for scenario in SCENARIOS[1:]:
        write_summary(current, scenario)
    write_baseline(tmp_path / "baseline.json", baseline_for(*SCENARIOS))

    assert run_gate(tmp_path) == MODULE.EXIT_USAGE


def test_current_population_mutation_fails_closed(tmp_path: Path) -> None:
    current = tmp_path / "current"
    write_summary(current, "signup")
    for scenario in SCENARIOS[1:]:
        write_summary(current, scenario)
    write_baseline(tmp_path / "baseline.json", baseline_for(*SCENARIOS))
    (current / "byok" / "summary.json").unlink()

    assert run_gate(tmp_path) == MODULE.EXIT_USAGE


def test_continue_on_error_leg_cannot_be_green(tmp_path: Path) -> None:
    current = tmp_path / "current"
    for scenario in SCENARIOS:
        write_summary(current, scenario)
    (current / "webhook" / "status.json").write_text(
        json.dumps({"scenario": "webhook", "outcome": "failure"})
    )
    write_baseline(tmp_path / "baseline.json", baseline_for(*SCENARIOS))

    assert run_gate(tmp_path) == MODULE.EXIT_USAGE


def test_identity_mutations_fail_closed(tmp_path: Path) -> None:
    current = tmp_path / "current"
    for scenario in SCENARIOS:
        write_summary(current, scenario)
    write_baseline(tmp_path / "baseline.json", baseline_for(*SCENARIOS))
    mutated = json.loads((current / "cas" / "summary.json").read_text())
    mutated["tenant_id"] = "119e7109-e514-72b2-ac5b-607d97ea64a1"
    (current / "cas" / "summary.json").write_text(json.dumps(mutated))
    assert run_gate(tmp_path) == MODULE.EXIT_USAGE


def test_raw_k6_summary_mutation_is_rejected(tmp_path: Path) -> None:
    current = tmp_path / "current"
    for scenario in SCENARIOS:
        write_summary(current, scenario)
    write_baseline(tmp_path / "baseline.json", baseline_for(*SCENARIOS))
    (current / "cas" / "summary.json").write_text(
        json.dumps({"metrics": {"http_req_duration": {"med": 10, "p(99)": 20}}})
    )
    assert run_gate(tmp_path) == MODULE.EXIT_USAGE


def test_baseline_version_threshold_and_identity_are_required(tmp_path: Path) -> None:
    current = tmp_path / "current"
    for scenario in SCENARIOS:
        write_summary(current, scenario)
    write_baseline(tmp_path / "baseline.json", baseline_for(*SCENARIOS))
    baseline = json.loads((tmp_path / "baseline.json").read_text())
    for key, value in (
        ("baseline_version", "k6-baseline-v1"),
        ("suite_version", "r3-prep-v1"),
        ("threshold_multiplier", 1.25),
    ):
        mutated = dict(baseline)
        mutated[key] = value
        (tmp_path / "baseline.json").write_text(json.dumps(mutated))
        assert run_gate(tmp_path) == MODULE.EXIT_USAGE


def test_subset_population_cannot_publish_a_baseline(tmp_path: Path) -> None:
    assert run_gate(tmp_path, "signup,webhook") == MODULE.EXIT_USAGE
