from __future__ import annotations

import json
import re
from pathlib import Path

import pytest
import yaml


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/smoke-install.yml"
HELPER = ROOT / "scripts/smoke_install_observe.py"
MANIFEST = ROOT / "docs/campaigns/remediation/i1672-smoke-install-suite-manifest.json"
REQUIRED_OBSERVATIONS = {
    "startup",
    "readiness",
    "endpoint_functional",
    "endpoint_wrong",
    "timeout",
    "process_death",
    "cleanup",
    "artifact_provenance",
}


def _contract(workflow: str, helper: str, manifest: dict[str, object]) -> None:
    parsed_workflow = yaml.safe_load(workflow)
    assert isinstance(parsed_workflow, dict)
    jobs = parsed_workflow.get("jobs")
    assert isinstance(jobs, dict)
    observe_job = jobs.get("observe")
    assert isinstance(observe_job, dict)
    job_env = observe_job.get("env") or {}
    assert isinstance(job_env, dict)
    assert not any(re.search(r"\$\{\{\s*runner\.", str(value)) for value in job_env.values())

    steps = observe_job.get("steps")
    assert isinstance(steps, list)
    probe_step = next(
        (
            step
            for step in steps
            if isinstance(step, dict)
            and step.get("name") == "Observe process, readiness, endpoint, death, timeout, and cleanup"
        ),
        None,
    )
    assert isinstance(probe_step, dict)
    probe_env = probe_step.get("env") or {}
    assert isinstance(probe_env, dict)
    assert probe_env.get("I1672_FLEET_LABEL") == "corelink"
    assert probe_env.get("I1672_RUNNER_NAME") == "${{ runner.name }}"
    assert probe_env.get("I1672_RUNNER_OS") == "${{ runner.os }}"
    assert probe_env.get("I1672_RUNNER_ARCH") == "${{ runner.arch }}"

    assert "workflow_dispatch:" in workflow
    assert "runs-on: corelink" in workflow
    assert "runs-on: ubuntu-latest" not in workflow
    assert "self-hosted" not in workflow
    assert "campaign-ci.yml" not in workflow
    assert "secrets." not in workflow
    assert "if docker info >" in workflow
    assert "if: ${{ always() }}" in workflow
    assert "upload-artifact@" in workflow
    assert "smoke_install_observe.py" in workflow

    for observation in REQUIRED_OBSERVATIONS:
        assert f'"{observation}"' in helper or f'"{observation}":' in helper
    for marker in (
        '[docker, "info"]',
        "process.poll() is not None",
        "healthz",
        "wrong-endpoint",
        "subprocess.TimeoutExpired",
        "shutil.rmtree",
        "helper_sha256",
        '"result": "PASS"',
        '"corelink-fleet"',
        '"I1672_FLEET_LABEL"',
        '"runner_provenance_invalid"',
    ):
        assert marker in helper
    assert "GITHUB_SHA" in helper
    assert "CORELINK_CANARY_PAT" not in helper

    assert manifest["suite_id"] == "i1672"
    assert manifest["kind"] == "contract-only"
    assert manifest["runner"] == "corelink"
    assert manifest["trigger"] == "workflow_dispatch"
    assert manifest["workflow"] == ".github/workflows/smoke-install.yml"
    assert set(manifest["observations"]) == REQUIRED_OBSERVATIONS
    assert "campaign-ci.yml" in manifest["forbidden"]
    assert manifest["evidence_remainder"]["status"] == "PENDING_MANUAL_DISPATCH"
    assert "runner_label" in manifest["evidence_remainder"]["required"]
    assert manifest["evidence_remainder"]["claim_allowed_before_dispatch"] is False


def _load() -> tuple[str, str, dict[str, object]]:
    return (
        WORKFLOW.read_text(encoding="utf-8"),
        HELPER.read_text(encoding="utf-8"),
        json.loads(MANIFEST.read_text(encoding="utf-8")),
    )


def test_i1672_contract() -> None:
    _contract(*_load())


@pytest.mark.parametrize(
    ("name", "mutate"),
    [
        ("hosted_runner", lambda w, h, m: (w.replace("runs-on: corelink", "runs-on: ubuntu-latest"), h, m)),
        ("fleet_provenance", lambda w, h, m: (w.replace("I1672_FLEET_LABEL: corelink", "I1672_FLEET_LABEL: hosted"), h, m)),
        (
            "runner_context_at_job_scope",
            lambda w, h, m: (
                w.replace(
                    "    runs-on: corelink\n",
                    "    runs-on: corelink\n    env:\n      I1672_RUNNER_NAME: ${{ runner.name }}\n",
                    1,
                ),
                h,
                m,
            ),
        ),
        (
            "runner_context_missing_from_probe_step",
            lambda w, h, m: (w.replace("I1672_RUNNER_NAME: ${{ runner.name }}", "I1672_RUNNER_NAME: ${{ github.job }}", 1), h, m),
        ),
        ("manual_trigger", lambda w, h, m: (w.replace("workflow_dispatch:", "workflow_dispatch_removed:", 1), h, m)),
        ("credential", lambda w, h, m: (w + "\nsecrets.CORELINK_CANARY_PAT\n", h, m)),
        ("backend_seam", lambda w, h, m: (w.replace("if docker info >", "if docker status >", 1), h, m)),
        (
            "process_death",
            lambda w, h, m: (w, h.replace("process.poll() is not None", "process.status() is not None"), m),
        ),
        ("suite_wiring", lambda w, h, m: (w, h, {**m, "kind": "campaign-ci"})),
    ],
)
def test_i1672_mutations_are_rejected(name: str, mutate) -> None:
    workflow, helper, manifest = mutate(*_load())
    with pytest.raises(AssertionError):
        _contract(workflow, helper, manifest)
