"""Closed-world Linux + Windows release inventory contract for issue #2609."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest
import yaml

from scripts.cli_release_manifest import FINAL_INVENTORY, LINUX_PAYLOADS, STAGING_INVENTORY, WINDOWS_PAYLOADS, create, load


ROOT = Path(__file__).resolve().parents[1]
TAG = "cli-v1.2.3"
SOURCE = "0123456789abcdef0123456789abcdef01234567"


def test_release_workflow_is_closed_to_linux_and_windows():
    workflow_text = (ROOT / ".github/workflows/release-cli.yml").read_text(encoding="utf-8")
    workflow = yaml.safe_load(workflow_text)
    targets = workflow["jobs"]["build"]["strategy"]["matrix"]["target"]

    assert {(target["triple"], target["name"], target["runner"]) for target in targets} == {
        ("x86_64-unknown-linux-gnu", "corelink-linux-x86_64", "ubuntu-24.04"),
        ("aarch64-unknown-linux-gnu", "corelink-linux-aarch64", "ubuntu-24.04"),
        ("x86_64-pc-windows-gnu", "corelink-windows-x86_64", "windows-2022"),
    }
    assert "notarize-macos" not in workflow["jobs"]
    assert "APPLE_" not in workflow_text
    assert "corelink-darwin-" not in workflow_text
    assert "| macOS" not in workflow_text
    assert "--emit-initial-release-preflight-ready" in workflow_text
    assert "--emit-preflight-ready" not in workflow_text


def test_linux_signer_uses_canonical_arm64_installer_and_keeps_archive_name():
    workflow = yaml.safe_load((ROOT / ".github/workflows/sign-linux.yml").read_text(encoding="utf-8"))
    targets = workflow["jobs"]["sign"]["strategy"]["matrix"]["include"]
    assert {(target["asset_suffix"], target["raw_asset_suffix"]) for target in targets} == {
        ("linux-x86_64", "linux-x86_64"),
        ("linux-arm64", "linux-aarch64"),
    }
    raw_asset_env = [
        step["env"]["RAW_ASSET"]
        for step in workflow["jobs"]["sign"]["steps"]
        if isinstance(step, dict) and "RAW_ASSET" in step.get("env", {})
    ]
    assert len(raw_asset_env) >= 4
    assert all(value == "corelink-${{ matrix.raw_asset_suffix }}" for value in raw_asset_env)


def test_manifest_builder_accepts_complete_staging_inventory_and_rejects_extra_asset(tmp_path: Path):
    assets = tmp_path / "assets"
    assets.mkdir()
    for name in STAGING_INVENTORY:
        (assets / name).write_bytes(name.encode())
    output = assets / "staging-manifest.json"

    create(assets, output, TAG, SOURCE)

    manifest = json.loads(output.read_text(encoding="utf-8"))
    assert {entry["name"] for entry in manifest["artifacts"]} == STAGING_INVENTORY
    (assets / "corelink-darwin-x86_64").write_bytes(b"unexpected")
    with pytest.raises(ValueError, match="unsupported asset"):
        create(assets, output, TAG, SOURCE)


@pytest.mark.parametrize("name", ["corelink-darwin-x86_64", "corelink-macos-aarch64"])
def test_manifest_loader_rejects_unapproved_platform_assets(tmp_path: Path, name: str):
    manifest = tmp_path / "manifest.json"
    manifest.write_text(json.dumps({
        "version": 1,
        "tag": TAG,
        "source_sha": SOURCE,
        "artifacts": [{"name": name, "sha256": "a" * 64}],
    }), encoding="utf-8")

    with pytest.raises(ValueError, match="unsupported asset"):
        load(manifest, TAG, SOURCE)


def test_final_manifest_preserves_linux_signatures_and_windows_signed_bytes(tmp_path: Path):
    assets = tmp_path / "assets"
    assets.mkdir()
    payloads = STAGING_INVENTORY - {name for name in STAGING_INVENTORY if name.endswith(".sha256")} - {"checksums.txt"}
    for name in payloads:
        (assets / name).write_bytes(f"staged:{name}".encode())
    for name in payloads:
        target_digest = hashlib.sha256((assets / name).read_bytes()).hexdigest()
        (assets / f"{name}.sha256").write_text(f"{target_digest}  {name}\n", encoding="utf-8")
    (assets / "checksums.txt").write_text("".join(
        (assets / name).read_text(encoding="utf-8")
        for name in sorted(f"{payload}.sha256" for payload in payloads)
    ), encoding="utf-8")
    staging = assets / "staging-manifest.json"
    create(assets, staging, TAG, SOURCE)

    for name in LINUX_PAYLOADS:
        (assets / f"{name}.asc").write_bytes(f"signature:{name}".encode())
    for name in WINDOWS_PAYLOADS:
        (assets / name).write_bytes(f"signed:{name}".encode())
    final_payloads = payloads | {f"{name}.asc" for name in LINUX_PAYLOADS}
    for name in final_payloads:
        target_digest = hashlib.sha256((assets / name).read_bytes()).hexdigest()
        (assets / f"{name}.sha256").write_text(f"{target_digest}  {name}\n", encoding="utf-8")
    (assets / "checksums.txt").write_text("".join(
        (assets / name).read_text(encoding="utf-8")
        for name in sorted(f"{payload}.sha256" for payload in final_payloads)
    ), encoding="utf-8")

    final = assets / "release-manifest.json"
    create(assets, final, TAG, SOURCE, staging)
    assert {entry["name"] for entry in load(final, TAG, SOURCE)} == FINAL_INVENTORY
