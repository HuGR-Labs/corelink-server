from __future__ import annotations

import json
from pathlib import Path
from typing import Any

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
RUNNER_ENV = {
    "I1672_RUNNER_NAME": "${{ runner.name }}",
    "I1672_RUNNER_OS": "${{ runner.os }}",
    "I1672_RUNNER_ARCH": "${{ runner.arch }}",
    "I1672_FLEET_LABEL": "corelink",
}


class _NoDuplicateKeysLoader(yaml.SafeLoader):
    """Parse workflow structure without PyYAML's last-key-wins behavior."""


def _construct_mapping(loader: _NoDuplicateKeysLoader, node: yaml.nodes.MappingNode) -> dict[Any, Any]:
    result: dict[Any, Any] = {}
    loader.flatten_mapping(node)
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=True)
        if key in result:
            raise ValueError(f"duplicate YAML key: {key!r}")
        result[key] = loader.construct_object(value_node, deep=True)
    return result


_NoDuplicateKeysLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG,
    _construct_mapping,
)


def _parse_workflow(workflow: str) -> dict[str, Any]:
    parsed = yaml.load(workflow, Loader=_NoDuplicateKeysLoader)
    assert isinstance(parsed, dict)
    return parsed


def _events(parsed: dict[str, Any]) -> dict[str, Any]:
    # PyYAML 1.1 parses the YAML key `on` as boolean True.
    events = parsed.get("on", parsed.get(True))
    assert isinstance(events, dict)
    return events


def _step(job: dict[str, Any], name: str) -> dict[str, Any]:
    steps = job.get("steps")
    assert isinstance(steps, list)
    matches = [item for item in steps if isinstance(item, dict) and item.get("name") == name]
    assert len(matches) == 1, f"expected exactly one {name!r} step"
    return matches[0]


def _contains_secret(value: Any) -> bool:
    if isinstance(value, dict):
        for key, nested in value.items():
            if isinstance(key, str) and any(word in key.lower() for word in ("token", "pat", "secret", "credential")):
                return True
            if _contains_secret(nested):
                return True
    elif isinstance(value, list):
        return any(_contains_secret(nested) for nested in value)
    elif isinstance(value, str):
        return "secrets." in value.lower()
    return False


def _contract(workflow: str, helper: str, manifest: dict[str, Any]) -> None:
    parsed = _parse_workflow(workflow)
    assert set(_events(parsed)) == {"workflow_dispatch"}
    assert parsed.get("permissions") == {"contents": "read"}
    assert not _contains_secret(parsed)

    jobs = parsed.get("jobs")
    assert isinstance(jobs, dict)
    assert set(jobs) == {"observe"}
    job = jobs["observe"]
    assert isinstance(job, dict)
    assert job.get("runs-on") == "corelink"
    assert job.get("timeout-minutes") == 5
    assert job.get("env") == {"I1672_FLEET_LABEL": "corelink"}
    assert "campaign-ci.yml" not in workflow

    probe = _step(job, "Observe process, readiness, endpoint, death, timeout, and cleanup")
    assert probe.get("if") == "${{ always() }}"
    assert probe.get("shell") == "bash"
    assert probe.get("env") == {
        "I1672_TIMEOUT_SECONDS": "${{ inputs.timeout_seconds }}",
        **RUNNER_ENV,
    }
    probe_script = probe.get("run")
    assert isinstance(probe_script, str)
    assert "set -euo pipefail" in probe_script
    assert "smoke_install_observe.py" in probe_script
    assert "--receipt artifacts/i1672/smoke-install-receipt.json" in probe_script

    checkout = _step(job, "Checkout tested tree")
    assert checkout.get("with") == {"persist-credentials": False}
    backend = _step(job, "Observe Docker boundary")
    backend_script = backend.get("run")
    assert isinstance(backend_script, str)
    assert "if docker info > artifacts/i1672/docker-info.txt 2>&1; then" in backend_script
    assert 'exit "${rc}"' in backend_script
    upload = _step(job, "Upload structured observation")
    assert upload.get("if") == "${{ always() }}"
    assert "upload-artifact@" in str(upload.get("uses"))
    assert upload.get("with", {}).get("if-no-files-found") == "error"

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


def _load() -> tuple[str, str, dict[str, Any]]:
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
        ("hosted_runner", lambda w, h, m: (w.replace("runs-on: corelink", "runs-on: ubuntu-latest", 1), h, m)),
        ("fleet_provenance", lambda w, h, m: (w.replace("I1672_FLEET_LABEL: corelink", "I1672_FLEET_LABEL: hosted", 1), h, m)),
        ("manual_trigger", lambda w, h, m: (w.replace("workflow_dispatch:", "workflow_dispatch_removed:", 1), h, m)),
        ("missing_runner_name", lambda w, h, m: (w.replace("          I1672_RUNNER_NAME: ${{ runner.name }}\n", "", 1), h, m)),
        (
            "job_scope_runner_context",
            lambda w, h, m: (
                w.replace("      I1672_FLEET_LABEL: corelink\n", "      I1672_FLEET_LABEL: corelink\n      I1672_RUNNER_NAME: ${{ runner.name }}\n", 1),
                h,
                m,
            ),
        ),
        (
            "secret_environment",
            lambda w, h, m: (
                w.replace("          I1672_FLEET_LABEL: corelink\n", "          I1672_FLEET_LABEL: corelink\n          CORELINK_CANARY_PAT: ${{ secrets.CORELINK_CANARY_PAT }}\n", 1),
                h,
                m,
            ),
        ),
        (
            "comment_decoy",
            lambda w, h, m: (w.replace("    runs-on: corelink", "    runs-on: ubuntu-latest\n    # runs-on: corelink", 1), h, m),
        ),
        (
            "duplicate_yaml_key",
            lambda w, h, m: (w.replace("    runs-on: corelink", "    runs-on: ubuntu-latest\n    runs-on: corelink", 1), h, m),
        ),
        ("invalid_yaml", lambda w, h, m: (w + "\n  broken: [\n", h, m)),
        ("suite_wiring", lambda w, h, m: (w, h, {**m, "kind": "campaign-ci"})),
    ],
)
def test_i1672_mutations_are_rejected(name: str, mutate) -> None:
    workflow, helper, manifest = mutate(*_load())
    with pytest.raises((AssertionError, ValueError, yaml.YAMLError)):
        _contract(workflow, helper, manifest)


def test_duplicate_yaml_key_is_explicitly_rejected() -> None:
    workflow, _, _ = _load()
    duplicate = workflow.replace("    runs-on: corelink", "    runs-on: ubuntu-latest\n    runs-on: corelink", 1)
    with pytest.raises(ValueError, match="duplicate YAML key: 'runs-on'"):
        _parse_workflow(duplicate)
