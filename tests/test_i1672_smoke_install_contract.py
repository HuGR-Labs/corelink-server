from __future__ import annotations

import json
from pathlib import Path

import pytest


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
