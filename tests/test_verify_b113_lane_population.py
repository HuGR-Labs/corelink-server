"""Behavioral and adversarial tests for the B-113 lane population verifier."""
from __future__ import annotations

import importlib.util
import re
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


@pytest.mark.parametrize(
    ("job", "before", "after"),
    (
        (None, "        default: all\n", "        default: mutants\n"),
        (None, "          - mutants\n", "          - other\n"),
        (
            "tlc-extended",
            "if: github.event_name != 'workflow_dispatch' || inputs.lane == 'all'\n",
            "if: always()\n",
        ),
        (
            "proptest-extended",
            "if: github.event_name != 'workflow_dispatch' || inputs.lane == 'all'\n",
            "if: always()\n",
        ),
        (
            "fuzz-matrix",
            "if: github.event_name != 'workflow_dispatch' || inputs.lane == 'all'\n",
            "if: always()\n",
        ),
        (
            "mutants-workspace",
            "if: github.event_name != 'workflow_dispatch' || inputs.lane == 'all' || inputs.lane == 'mutants'\n",
            "if: github.event_name != 'workflow_dispatch' || inputs.lane == 'all'\n",
        ),
    ),
)
def test_nightly_dispatch_cannot_run_the_wrong_lane(job: str | None, before: str, after: str) -> None:
    sources = corpus()
    nightly = sources[".github/workflows/nightly.yml"]
    if job is None:
        assert nightly.count(before) == 1
        mutated = nightly.replace(before, after, 1)
    else:
        boundary = re.compile(rf"(?ms)^  {re.escape(job)}:\n.*?(?=^  [A-Za-z0-9_-]+:\n|\Z)")
        match = boundary.search(nightly)
        assert match is not None and match.group(0).count(before) == 1
        mutated = (
            nightly[: match.start()]
            + match.group(0).replace(before, after, 1)
            + nightly[match.end() :]
        )
    sources[".github/workflows/nightly.yml"] = mutated
    with pytest.raises(MODULE.VerificationError, match="nightly-mutants"):
        MODULE.verify(sources)


@pytest.mark.parametrize("job", ("build", "negative-scenarios", "benchmark"))
def test_buck2_jobs_require_hosted_linux_runner(job: str) -> None:
    sources = corpus()
    workflow = sources[".github/workflows/buck2-starter-ci.yml"]
    start = workflow.index(f"  {job}:\n")
    runner = workflow.index("runs-on: ubuntu-latest", start)
    sources[".github/workflows/buck2-starter-ci.yml"] = (
        workflow[:runner] + "runs-on: corelink" + workflow[runner + len("runs-on: ubuntu-latest"):]
    )
    with pytest.raises(MODULE.VerificationError, match="hosted ubuntu-latest"):
        MODULE.verify(sources)


@pytest.mark.parametrize(
    ("old", "new", "error"),
    (
        ("contents: read", "contents: write", "permissions"),
        ("if-no-files-found: error", "if-no-files-found: warn", "missing report"),
        ("retention-days: 14", "retention-days: 0", "retention"),
        ("actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a", "actions/upload-artifact@v7", "SHA-pinned"),
        ("git push", "git fetch", "repository write"),
    ),
)
def test_buck2_benchmark_write_and_artifact_regressions_fail(
    old: str, new: str, error: str
) -> None:
    sources = corpus()
    workflow = sources[".github/workflows/buck2-starter-ci.yml"]
    if old == "git push":
        marker = "      - name: Upload benchmark report\n"
        assert workflow.count(marker) == 1
        workflow = workflow.replace(marker, "      - name: Forbidden mutation probe\n        run: git push\n\n" + marker, 1)
    else:
        assert workflow.count(old) >= 1
        prefix, separator, suffix = workflow.rpartition(old)
        assert separator
        workflow = prefix + new + suffix
    sources[".github/workflows/buck2-starter-ci.yml"] = workflow
    with pytest.raises(MODULE.VerificationError, match=error):
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
