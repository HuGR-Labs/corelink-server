"""Static runner/runtime contract and negative mutations for hosted Python CI."""

from __future__ import annotations

import copy
from pathlib import Path
from typing import Any

import pytest
import yaml


ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = {
    ".github/workflows/okf_wiki.yml": ("okf-wiki-validation", 20),
    ".github/workflows/python-tests.yml": ("pytest", 10),
}
SETUP_PYTHON = "actions/setup-python@5fda3b95a4ea91299a34e894583c3862153e4b97"


def _workflow(relative: str) -> dict[str, Any]:
    parsed = yaml.safe_load((ROOT / relative).read_text(encoding="utf-8"))
    assert isinstance(parsed, dict)
    return parsed


def _assert_hosted_python_contract(
    document: dict[str, Any], job_name: str, max_timeout: int
) -> None:
    jobs = document.get("jobs")
    assert isinstance(jobs, dict)
    job = jobs.get(job_name)
    assert isinstance(job, dict)
    assert job.get("runs-on") == "ubuntu-24.04"
    timeout = job.get("timeout-minutes")
    assert isinstance(timeout, int) and 1 <= timeout <= max_timeout

    steps = job.get("steps")
    assert isinstance(steps, list)
    python_setup = [
        step
        for step in steps
        if isinstance(step, dict) and step.get("uses", "").startswith("actions/setup-python@")
    ]
    assert len(python_setup) == 1
    setup = python_setup[0]
    assert setup["uses"] == SETUP_PYTHON
    assert setup.get("with", {}).get("python-version") == "3.12"
    assert any(
        isinstance(step, dict) and "requirements-ci.txt" in step.get("run", "")
        for step in steps
    )


@pytest.mark.parametrize("relative,contract", WORKFLOWS.items())
def test_target_workflow_uses_pinned_hosted_python(relative: str, contract: tuple[str, int]) -> None:
    _assert_hosted_python_contract(_workflow(relative), *contract)


def _mutations() -> list[tuple[str, Any]]:
    return [
        ("corelink-runner", lambda job: job.update({"runs-on": "corelink"})),
        ("self-hosted-runner", lambda job: job.update({"runs-on": ["self-hosted", "linux"]})),
        ("missing-timeout", lambda job: job.pop("timeout-minutes")),
        ("zero-timeout", lambda job: job.update({"timeout-minutes": 0})),
        ("over-budget-timeout", lambda job: job.update({"timeout-minutes": 60})),
        (
            "missing-python-provisioning",
            lambda job: job["steps"].pop(
                next(
                    index
                    for index, step in enumerate(job["steps"])
                    if isinstance(step, dict)
                    and step.get("uses", "").startswith("actions/setup-python@")
                )
            ),
        ),
        (
            "unpinned-python-action",
            lambda job: next(
                step
                for step in job["steps"]
                if isinstance(step, dict) and step.get("uses", "").startswith("actions/setup-python@")
            ).update({"uses": "actions/setup-python@v7"}),
        ),
        (
            "ambient-python-version",
            lambda job: next(
                step
                for step in job["steps"]
                if isinstance(step, dict) and step.get("uses", "").startswith("actions/setup-python@")
            ).setdefault("with", {}).update({"python-version": ""}),
        ),
    ]


@pytest.mark.parametrize("relative,contract", WORKFLOWS.items())
@pytest.mark.parametrize("mutation_name,mutation", _mutations(), ids=lambda value: value if isinstance(value, str) else None)
def test_runner_runtime_mutations_are_rejected(
    relative: str,
    contract: tuple[str, int],
    mutation_name: str,
    mutation: Any,
) -> None:
    del mutation_name
    document = copy.deepcopy(_workflow(relative))
    job_name, max_timeout = contract
    mutation(document["jobs"][job_name])
    with pytest.raises(AssertionError):
        _assert_hosted_python_contract(document, job_name, max_timeout)


def test_python_lane_lints_both_workflows_with_a_checksum_pinned_actionlint() -> None:
    workflow = _workflow(".github/workflows/python-tests.yml")
    steps = workflow["jobs"]["pytest"]["steps"]
    lint_step = next(step for step in steps if step.get("name") == "Lint hosted OKF and Python test workflows")
    command = lint_step["run"]
    assert '"${RUNNER_TEMP}/actionlint/actionlint" -color -config-file .actionlint.yaml' in command
    assert ".github/workflows/okf_wiki.yml" in command
    assert ".github/workflows/python-tests.yml" in command
    source = (ROOT / ".github/workflows/python-tests.yml").read_text(encoding="utf-8")
    assert "8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8" in source
