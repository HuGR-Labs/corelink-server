"""Focused mutation and trust-boundary tests for B-115."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys

import pytest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_prod_deployability as deploy  # noqa: E402


def test_closed_inventory_rederives_all_production_surfaces() -> None:
    report = deploy.verify(ROOT)

    assert report["surfaces"] == 6
    assert report["excluded_configs"] == ["crates/corelink-clerk-cf/wrangler.toml"]
    assert report["status"] == "production_deployability_verified"


def test_missing_artifact_is_fail_closed(tmp_path: Path) -> None:
    manifest = json.loads((ROOT / deploy.MANIFEST).read_text(encoding="utf-8"))
    surface = next(item for item in manifest["surfaces"] if item["id"] == "signup-worker")
    surface["artifacts"][0]["path"] = "apps/signup-worker/src/missing-entrypoint.ts"
    altered = tmp_path / deploy.MANIFEST
    altered.parent.mkdir(parents=True)
    altered.write_text(json.dumps(manifest), encoding="utf-8")

    with pytest.raises(deploy.DeployabilityError, match="source artifact is absent"):
        deploy.verify(ROOT, manifest_path=altered)


def test_invalid_base_sha_is_fail_closed() -> None:
    with pytest.raises(deploy.DeployabilityError, match="base_sha"):
        deploy.verify(ROOT, base_sha="not-a-commit", head_sha="HEAD")


def _base_sha() -> str:
    return subprocess.check_output(
        ("git", "rev-parse", "HEAD^"), cwd=ROOT, text=True
    ).strip()


def test_excluded_allowlist_rejects_a_production_surface_mutation(monkeypatch: pytest.MonkeyPatch) -> None:
    base = _base_sha()
    original = deploy._git_file

    def moved_production(repo_root: Path, commit: str, relative: str, label: str) -> bytes:
        if relative == "crates/corelink-clerk-cf/wrangler.toml":
            return original(repo_root, commit, "wrangler.toml", label)
        return original(repo_root, commit, relative, label)

    monkeypatch.setattr(deploy, "_git_file", moved_production)
    with pytest.raises(deploy.DeployabilityError, match="identity"):
        deploy.verify(ROOT, base_sha=base, head_sha="HEAD")


def test_base_workflow_marker_drift_is_fail_closed(monkeypatch: pytest.MonkeyPatch) -> None:
    base = _base_sha()
    original = deploy._git_file

    def stale_base_workflow(repo_root: Path, commit: str, relative: str, label: str) -> bytes:
        raw = original(repo_root, commit, relative, label)
        if commit == base and relative == ".github/workflows/cf-deploy-prod.yml":
            return raw.replace(b"wrangler deploy", b"wrangler publish")
        return raw

    monkeypatch.setattr(deploy, "_git_file", stale_base_workflow)
    with pytest.raises(deploy.DeployabilityError, match="stale or missing marker"):
        deploy.verify(ROOT, base_sha=base, head_sha="HEAD")


def test_base_package_deploy_script_drift_is_fail_closed(monkeypatch: pytest.MonkeyPatch) -> None:
    base = _base_sha()
    original = deploy._git_file

    def stale_base_package(repo_root: Path, commit: str, relative: str, label: str) -> bytes:
        raw = original(repo_root, commit, relative, label)
        if commit == base and relative == "apps/analytics-worker/package.json":
            package = json.loads(raw.decode("utf-8"))
            package["scripts"].pop("deploy:prod")
            return json.dumps(package).encode("utf-8")
        return raw

    monkeypatch.setattr(deploy, "_git_file", stale_base_package)
    with pytest.raises(deploy.DeployabilityError, match="production deploy script"):
        deploy.verify(ROOT, base_sha=base, head_sha="HEAD")


def test_fork_safe_gate_has_no_secret_or_target_trigger() -> None:
    workflow = (ROOT / ".github/workflows/production-deployability.yml").read_text(encoding="utf-8")
    assert "pull_request:" in workflow
    assert "pull_request_target" not in workflow
    assert "secrets." not in workflow
    assert "wrangler deploy" not in workflow
