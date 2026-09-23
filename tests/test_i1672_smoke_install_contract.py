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
PROBE_STEP = "Observe process, readiness, endpoint, death, timeout, and cleanup"
RUNNER_ENV = {
    "I1672_FLEET_LABEL": "corelink",
    "I1672_RUNNER_NAME": "${{ runner.name }}",
    "I1672_RUNNER_OS": "${{ runner.os }}",
    "I1672_RUNNER_ARCH": "${{ runner.arch }}",
}


def _yaml_strings(value: object):
    if isinstance(value, dict):
        for key, nested in value.items():
            yield from _yaml_strings(key)
            yield from _yaml_strings(nested)
    elif isinstance(value, list):
        for nested in value:
            yield from _yaml_strings(nested)
    elif isinstance(value, str):
        yield value


def _contract(workflow: str, helper: str, manifest: dict[str, object]) -> None:
    try:
        # BaseLoader preserves GitHub's `on` key and expressions as strings.
        # Parsing also excludes comments from the contract and secret checks.
        parsed_workflow = yaml.load(workflow, Loader=yaml.BaseLoader)
    except yaml.YAMLError as exc:
        raise AssertionError(f"workflow must be valid YAML: {exc}") from exc
    assert isinstance(parsed_workflow, dict)
    dispatch = parsed_workflow.get("on")
    assert isinstance(dispatch, dict)
    assert isinstance(dispatch.get("workflow_dispatch"), dict)
    assert parsed_workflow.get("permissions") == {"contents": "read"}

    jobs = parsed_workflow.get("jobs")
    assert isinstance(jobs, dict)
    assert set(jobs) == {"observe"}
    observe_job = jobs["observe"]
    assert isinstance(observe_job, dict)
    assert observe_job.get("runs-on") == "corelink"
    assert "env" not in observe_job

    steps = observe_job.get("steps")
    assert isinstance(steps, list)
    probe_step = next(
        (
            step
            for step in steps
            if isinstance(step, dict) and step.get("name") == PROBE_STEP
        ),
        None,
    )
    assert isinstance(probe_step, dict)
    probe_env = probe_step.get("env")
    assert probe_env == {"I1672_TIMEOUT_SECONDS": "${{ inputs.timeout_seconds }}", **RUNNER_ENV}

    values = list(_yaml_strings(parsed_workflow))
    runner_expressions = [
        expression
        for value in values
        for expression in re.findall(r"\$\{\{\s*runner\.[^}]+\}\}", value)
    ]
    assert sorted(runner_expressions) == sorted(
        ["${{ runner.name }}", "${{ runner.os }}", "${{ runner.arch }}"]
    )
    assert not any(
        re.search(r"(?i)(?:secrets\.|CORELINK_CANARY_PAT|CORELINK_TEST_TOKEN)", value)
        for value in values
    )
    assert probe_step.get("if") == "${{ always() }}"
    backend_step = next(
        step
        for step in steps
        if isinstance(step, dict) and step.get("name") == "Observe Docker boundary"
    )
    assert "if docker info > artifacts/i1672/docker-info.txt 2>&1; then" in backend_step.get(
        "run", ""
    )
    assert "smoke_install_observe.py" in probe_step.get("run", "")
    assert isinstance(steps[-1], dict)
    assert "upload-artifact@" in steps[-1].get("uses", "")
    assert "campaign-ci.yml" not in "\n".join(values)
    assert "self-hosted" not in "\n".join(values)

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
        (
            "hosted_runner",
            lambda w, h, m: (w.replace("runs-on: corelink", "runs-on: ubuntu-latest", 1), h, m),
        ),
        (
            "fleet_provenance",
            lambda w, h, m: (
                w.replace("I1672_FLEET_LABEL: corelink", "I1672_FLEET_LABEL: hosted", 1),
                h,
                m,
            ),
        ),
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
            lambda w, h, m: (
                w.replace(
                    "I1672_RUNNER_NAME: ${{ runner.name }}",
                    "I1672_RUNNER_NAME: ${{ github.job }}",
                    1,
                ),
                h,
                m,
            ),
        ),
        (
            "manual_trigger",
            lambda w, h, m: (w.replace("workflow_dispatch:", "workflow_dispatch_removed:", 1), h, m),
        ),
        (
            "credential",
            lambda w, h, m: (
                w.replace(
                    "          I1672_TIMEOUT_SECONDS: ${{ inputs.timeout_seconds }}\n",
                    "          I1672_TIMEOUT_SECONDS: ${{ inputs.timeout_seconds }}\n"
                    "          I1672_TEST_SECRET: ${{ secrets.CORELINK_CANARY_PAT }}\n",
                    1,
                ),
                h,
                m,
            ),
        ),
        (
            "comment_cannot_satisfy_fleet_route",
            lambda w, h, m: (
                w.replace("    runs-on: corelink", "    runs-on: ubuntu-latest", 1)
                + "\n# runs-on: corelink\n",
                h,
                m,
            ),
        ),
        ("backend_seam", lambda w, h, m: (w.replace("if docker info >", "if docker status >", 1), h, m)),
        (
            "process_death",
            lambda w, h, m: (w, h.replace("process.poll() is not None", "process.status() is not None"), m),
        ),
        ("suite_wiring", lambda w, h, m: (w, h, {**m, "kind": "campaign-ci"})),
        ("malformed_yaml", lambda w, h, m: (w + "\nsecrets.CORELINK_CANARY_PAT\n", h, m)),
    ],
)
def test_i1672_mutations_are_rejected(name: str, mutate) -> None:
    workflow, helper, manifest = mutate(*_load())
    with pytest.raises(AssertionError):
        _contract(workflow, helper, manifest)
