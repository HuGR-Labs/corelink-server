"""Behavioral fixtures for the B-112 release inventory and Rekor gates."""

from __future__ import annotations

import base64
import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REKOR = ROOT / "scripts/verify_cli_rekor_bundle.py"
INVENTORY = ROOT / "scripts/verify_cli_release_inventory.py"
TAG = "cli-v1.2.3"
SOURCE = "0123456789abcdef0123456789abcdef01234567"


def _run(script: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(script), *args], cwd=ROOT,
        text=True, capture_output=True, check=False,
    )


def _rekor_fixture(tmp_path: Path, *, root: str | None = None,
                   entries: list[dict] | None = None) -> tuple[Path, Path]:
    tmp_path.mkdir(parents=True, exist_ok=True)
    payload = tmp_path / "provenance.intoto.jsonl"
    payload.write_bytes(b'{"statement":"fixture"}\n')
    expected = hashlib.sha256(payload.read_bytes()).hexdigest()
    body = json.dumps(
        {"spec": {"data": {"hash": {"algorithm": "sha256", "value": expected}}}},
        separators=(",", ":"),
    ).encode()
    leaf = hashlib.sha256(b"\x00" + body).hexdigest()
    proof = {
        "logIndex": 0,
        "treeSize": 1,
        "rootHash": root or leaf,
        "hashes": [],
    }
    entry = {
        "logIndex": 0,
        "logId": {"keyId": "fixture-log"},
        "integratedTime": 0,
        "canonicalizedBody": base64.b64encode(body).decode(),
        "inclusionProof": proof,
    }
    bundle = tmp_path / "provenance.intoto.jsonl.bundle"
    bundle.write_text(json.dumps({"verificationMaterial": {"tlogEntries":
                         entries if entries is not None else [entry]}}), encoding="utf-8")
    return payload, bundle


def _rekor_non_power_of_two_fixture(tmp_path: Path) -> tuple[Path, Path]:
    """A valid RFC 6962 proof for the right edge of a three-leaf tree."""
    tmp_path.mkdir(parents=True, exist_ok=True)
    payload = tmp_path / "provenance.intoto.jsonl"
    payload.write_bytes(b'{"statement":"fixture"}\n')
    expected = hashlib.sha256(payload.read_bytes()).hexdigest()
    body = json.dumps(
        {"spec": {"data": {"hash": {"algorithm": "sha256", "value": expected}}}},
        separators=(",", ":"),
    ).encode()

    def leaf(value: bytes) -> bytes:
        return hashlib.sha256(b"\x00" + value).digest()

    def node(left: bytes, right: bytes) -> bytes:
        return hashlib.sha256(b"\x01" + left + right).digest()

    left_subtree = node(leaf(b"left-0"), leaf(b"left-1"))
    target_leaf = leaf(body)
    root = node(left_subtree, target_leaf)
    entry = {
        "logIndex": 2,
        "logId": {"keyId": "fixture-log"},
        "integratedTime": 0,
        "canonicalizedBody": base64.b64encode(body).decode(),
        "inclusionProof": {
            "logIndex": 2,
            "treeSize": 3,
            # Exercise both accepted hash encodings in one real proof.
            "rootHash": base64.b64encode(root).decode(),
            "hashes": [left_subtree.hex()],
        },
    }
    bundle = tmp_path / "provenance.intoto.jsonl.bundle"
    bundle.write_text(json.dumps({"verificationMaterial": {"tlogEntries": [entry]}}),
                      encoding="utf-8")
    return payload, bundle


def test_rekor_verifier_accepts_zero_fields_and_valid_merkle_proof(tmp_path: Path):
    payload, bundle = _rekor_fixture(tmp_path)
    result = _run(REKOR, "--payload", str(payload), "--bundle", str(bundle))
    assert result.returncode == 0, result.stdout + result.stderr


def test_rekor_verifier_accepts_rfc6962_right_edge_and_rejects_bad_paths(tmp_path: Path):
    payload, bundle = _rekor_non_power_of_two_fixture(tmp_path)
    result = _run(REKOR, "--payload", str(payload), "--bundle", str(bundle))
    assert result.returncode == 0, result.stdout + result.stderr

    value = json.loads(bundle.read_text(encoding="utf-8"))
    proof = value["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]
    proof["hashes"] = []
    bundle.write_text(json.dumps(value), encoding="utf-8")
    result = _run(REKOR, "--payload", str(payload), "--bundle", str(bundle))
    assert result.returncode != 0

    payload, bundle = _rekor_non_power_of_two_fixture(tmp_path / "fake")
    value = json.loads(bundle.read_text(encoding="utf-8"))
    value["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["rootHash"] = "00" * 32
    bundle.write_text(json.dumps(value), encoding="utf-8")
    result = _run(REKOR, "--payload", str(payload), "--bundle", str(bundle))
    assert result.returncode != 0


def test_rekor_verifier_rejects_empty_and_fake_proofs(tmp_path: Path):
    payload, bundle = _rekor_fixture(tmp_path, entries=[])
    result = _run(REKOR, "--payload", str(payload), "--bundle", str(bundle))
    assert result.returncode != 0

    payload, bundle = _rekor_fixture(tmp_path, root="00" * 32)
    result = _run(REKOR, "--payload", str(payload), "--bundle", str(bundle))
    assert result.returncode != 0

    payload, bundle = _rekor_fixture(tmp_path)
    value = json.loads(bundle.read_text(encoding="utf-8"))
    value["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["treeSize"] = 2
    bundle.write_text(json.dumps(value), encoding="utf-8")
    result = _run(REKOR, "--payload", str(payload), "--bundle", str(bundle))
    assert result.returncode != 0


def _inventory_fixture(tmp_path: Path, *, replacement: bool = False,
                       duplicate_subject: bool = False) -> tuple[Path, Path, Path, Path, str]:
    directory = tmp_path / "assets"
    directory.mkdir()
    asset = directory / "corelink-linux-x86_64"
    asset.write_bytes(b"replacement" if replacement else b"signed-bytes")
    digest = hashlib.sha256(b"signed-bytes").hexdigest()
    manifest = directory / "release-manifest.json"
    manifest.write_text(json.dumps({"version": 1, "tag": TAG, "source_sha": SOURCE,
                                    "artifacts": [{"name": asset.name, "sha256": digest}]}),
                        encoding="utf-8")
    actual_manifest_sha = hashlib.sha256(manifest.read_bytes()).hexdigest()
    statement = {"subject": [{"name": asset.name, "digest": {"sha256": digest}}]}
    if duplicate_subject:
        statement["subject"].append({"name": asset.name, "digest": {"sha256": digest}})
    provenance = directory / "provenance.intoto.jsonl"
    provenance.write_text(json.dumps(statement) + "\n", encoding="utf-8")
    (directory / "provenance.intoto.jsonl.bundle").write_text("{}", encoding="utf-8")
    api = tmp_path / "api.json"
    api.write_text(json.dumps({"assets": [{"name": name} for name in (
        asset.name, "release-manifest.json", "provenance.intoto.jsonl",
        "provenance.intoto.jsonl.bundle")]}), encoding="utf-8")
    return api, directory, manifest, provenance, actual_manifest_sha


def _run_inventory(tmp_path: Path, **kwargs: bool) -> subprocess.CompletedProcess[str]:
    api, directory, manifest, provenance, manifest_sha = _inventory_fixture(tmp_path, **kwargs)
    return _run(
        INVENTORY, "--api-json", str(api), "--directory", str(directory),
        "--manifest", str(manifest), "--provenance", str(provenance),
        "--tag", TAG, "--source-sha", SOURCE, "--manifest-sha256", manifest_sha,
    )


def test_inventory_verifier_rejects_duplicate_subjects(tmp_path: Path):
    result = _run_inventory(tmp_path, duplicate_subject=True)
    assert result.returncode != 0
    assert "duplicate" in result.stdout.lower()


def test_inventory_verifier_rejects_replaced_bytes(tmp_path: Path):
    result = _run_inventory(tmp_path, replacement=True)
    assert result.returncode != 0
    assert "digest" in result.stdout.lower()
