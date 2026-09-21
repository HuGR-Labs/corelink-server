"""Behavioral and adversarial tests for the B-113 lane population verifier."""
from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b113_lane_population", ROOT / "scripts" / "verify_b113_lane_population.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def corpus() -> dict[str, str]:
    return {
        lane.workflow: (ROOT / lane.workflow).read_text(encoding="utf-8")
        for lane in MODULE.LANES
    }


def documents() -> dict[str, str]:
    return MODULE.read_documentation()


def test_live_population_and_mutations_pass() -> None:
    MODULE.verify(corpus())
    MODULE.mutation_checks(corpus())


def test_billing_health_rejects_unavailable_self_hosted_runner() -> None:
    lane = next(item for item in MODULE.LANES if item.name == "billing-health")
    sources = corpus()
    sources[lane.workflow] = sources[lane.workflow].replace(
        "runs-on: ubuntu-latest", "runs-on: corelink", 1
    )
    with pytest.raises(MODULE.VerificationError, match="billing-health"):
        MODULE.verify(sources)


@pytest.mark.parametrize("lane_index", range(len(MODULE.LANES)))
def test_removing_each_named_job_fails_closed(lane_index: int) -> None:
    lane = MODULE.LANES[lane_index]
    sources = corpus()
    workflow = sources[lane.workflow]
    start = workflow.index(f"  {lane.job}:\n")
    next_job = workflow.find("\n  ", start + 3)
    end = len(workflow) if next_job < 0 else next_job + 1
    sources[lane.workflow] = workflow[:start] + workflow[end:]
    with pytest.raises(MODULE.VerificationError, match=lane.name):
        MODULE.verify(sources)


def test_shared_buck2_marker_is_scoped_to_the_named_job() -> None:
    """Removing a sibling's timeout must not satisfy the build lane by accident."""
    lane = next(item for item in MODULE.LANES if item.name == "buck2-build")
    sources = corpus()
    sources[lane.workflow] = sources[lane.workflow].replace(
        '"${BUCK2_INSTALL_DIR}/buck2" --version', "", 1
    )
    with pytest.raises(MODULE.VerificationError, match="buck2-build"):
        MODULE.verify(sources)


def test_empty_workflow_cannot_be_a_green_population() -> None:
    sources = corpus()
    sources[".github/workflows/nightly.yml"] = ""
    with pytest.raises(MODULE.VerificationError, match="empty workflow population"):
        MODULE.verify(sources)


def test_missing_workflow_is_a_population_drift() -> None:
    sources = corpus()
    del sources[".github/workflows/sbom.yml"]
    with pytest.raises(MODULE.VerificationError, match="missing B-113 lane"):
        MODULE.verify(sources)


def test_extra_workflow_is_a_population_drift() -> None:
    sources = corpus()
    sources[".github/workflows/unexpected.yml"] = "name: unexpected\n"
    with pytest.raises(MODULE.VerificationError, match="unexpected lane"):
        MODULE.verify(sources)


def test_terraform_drift_is_explicitly_excluded_as_b111_owned() -> None:
    sources = corpus()
    sources[".github/workflows/terraform-drift.yml"] = "name: terraform-drift\n"
    with pytest.raises(MODULE.VerificationError, match="B-111-owned"):
        MODULE.verify(sources)


def test_count_and_ownership_prose_mutations_fail_closed() -> None:
    cases = (
        ("BACKLOG.md", "Sobram sete workflows", "Sobram seis workflows"),
        ("BACKLOG.md", "ownership de B-111", "ownership de B-063"),
        (
            "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json",
            "terraform-drift.yml is excluded because it is B-111-owned",
            "terraform-drift.yml is excluded because it is B-063-owned",
        ),
    )
    for path, marker, replacement in cases:
        mutated = documents()
        assert mutated[path].count(marker) == 1
        mutated[path] = mutated[path].replace(marker, replacement, 1)
        with pytest.raises(MODULE.VerificationError, match="B-113"):
            MODULE.verify(corpus(), mutated)
