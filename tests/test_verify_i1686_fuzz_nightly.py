"""Adversarial contract tests for the #1686 fuzz nightly workflow."""
from __future__ import annotations

import copy
import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_i1686_fuzz_nightly", ROOT / "scripts/verify_i1686_fuzz_nightly.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


@pytest.fixture
def workflow() -> tuple[dict, str]:
    return MODULE.load()


def test_live_workflow_and_mutations_pass(workflow: tuple[dict, str]) -> None:
    document, source = workflow
    MODULE.verify(document, source)
    MODULE.mutation_checks()


@pytest.mark.parametrize(
    "mutation",
    [
        lambda d: d["concurrency"].update({"cancel-in-progress": True}),
        lambda d: d["concurrency"].update({"group": "ci-fuzz-${{ github.ref }}"}),
        lambda d: MODULE._events(d).update({"schedule": []}),
        lambda d: d["jobs"]["fuzz-matrix-expansion"].update({"runs-on": "corelink"}),
        lambda d: d["jobs"]["fuzz-matrix-expansion"]["strategy"].update({"max-parallel": 1}),
        lambda d: d["jobs"]["fuzz-matrix-expansion"].update({"timeout-minutes": 0}),
    ],
)
def test_each_control_mutation_fails_closed(
    workflow: tuple[dict, str], mutation
) -> None:
    document, source = workflow
    mutated = copy.deepcopy(document)
    mutation(mutated)
    with pytest.raises(MODULE.ContractError):
        MODULE.verify(mutated, source)


def test_campaign_ci_is_not_part_of_the_contract(workflow: tuple[dict, str]) -> None:
    document, _source = workflow
    with pytest.raises(MODULE.ContractError, match="campaign-ci"):
        MODULE.verify(document, "campaign-ci.yml")
