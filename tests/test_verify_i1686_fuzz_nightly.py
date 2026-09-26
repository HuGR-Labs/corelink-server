"""Adversarial contract tests for the #1686 fuzz nightly workflow."""
from __future__ import annotations

import copy
import importlib.util
from pathlib import Path
import subprocess

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


@pytest.mark.parametrize(
    ("old", "new"),
    [
        ("--version =0.13.2", "--version =0.13.1"),
        ("--version =0.13.2", "--version =9.99.0"),
        ("--version =0.13.2", ""),
        ("--version =0.13.2", "--version 0.13.2"),
    ],
    ids=["old-version", "arbitrary-version", "missing-pin", "malformed-pin"],
)
def test_cargo_fuzz_install_pin_fails_closed(
    workflow: tuple[dict, str], old: str, new: str
) -> None:
    document, source = workflow
    job = document["jobs"]["fuzz-matrix-expansion"]
    install = MODULE._step(
        job, "Install cargo-fuzz (isolated CARGO_HOME — never the shared one)"
    )
    install["run"] = install["run"].replace(old, new)
    with pytest.raises(MODULE.ContractError, match="cargo-fuzz"):
        MODULE.verify(document, source)


@pytest.mark.parametrize(
    ("old", "new"),
    [
        ("grep -Fx 'cargo-fuzz 0.13.2'", "grep -Fx 'cargo-fuzz 0.13.1'"),
        ("grep -Fx 'cargo-fuzz 0.13.2'", "grep -Fx 'cargo-fuzz 0.13.20'"),
        ('if "$TOOLS/bin/cargo-fuzz" --version', "if false"),
    ],
    ids=["version-mismatch", "ambiguous-version-match", "missing-version-check"],
)
def test_cargo_fuzz_version_check_fails_closed(
    workflow: tuple[dict, str], old: str, new: str
) -> None:
    document, source = workflow
    job = document["jobs"]["fuzz-matrix-expansion"]
    install = MODULE._step(
        job, "Install cargo-fuzz (isolated CARGO_HOME — never the shared one)"
    )
    install["run"] = install["run"].replace(old, new)
    with pytest.raises(MODULE.ContractError, match="cargo-fuzz"):
        MODULE.verify(document, source)


def test_cargo_fuzz_runtime_guard_requires_the_exact_version_line() -> None:
    expected_line = f"cargo-fuzz {MODULE.EXPECTED_CARGO_FUZZ_VERSION}"

    def matches(output: str) -> bool:
        result = subprocess.run(
            ["grep", "-Fx", expected_line],
            input=output,
            text=True,
            capture_output=True,
            check=False,
        )
        return result.returncode == 0

    assert matches("cargo-fuzz 0.13.2\n")
    assert not matches("cargo-fuzz 0.13.20\n")
