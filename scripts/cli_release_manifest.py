#!/usr/bin/env python3
"""Create and verify immutable CoreLink CLI release manifests.

The release chain has two manifests: ``staging-manifest.json`` binds the exact
tag/source/artifacts BEFORE any privileged signer runs; ``release-manifest.json``
is reconstructed from the final signed inventory before SLSA and publication.
Both use this strict, path-safe schema and are content-addressed by callers.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import stat
import sys
from pathlib import Path

SHA256 = re.compile(r"^[0-9a-f]{64}$")
TAG = re.compile(r"^cli-v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z][0-9A-Za-z.-]*)?$")
SHA = re.compile(r"^[0-9a-f]{40}$")

# This is the closed initial CLI release inventory. Keep asset names explicit
# so a new target cannot enter through a broad platform prefix check.
LINUX_PAYLOADS = {
    "corelink-linux-x86_64",
    "corelink-linux-x86_64.tar.gz",
    "corelink-linux-aarch64",
    "corelink-linux-arm64.tar.gz",
}
WINDOWS_PAYLOADS = {"corelink-windows-x86_64.exe", "corelink-windows-x86_64.zip"}
BASE_PAYLOADS = LINUX_PAYLOADS | WINDOWS_PAYLOADS
CHECKSUMS = {"checksums.txt", *(f"{name}.sha256" for name in BASE_PAYLOADS)}
STAGING_INVENTORY = BASE_PAYLOADS | CHECKSUMS
LINUX_SIGNATURES = {f"{name}.asc" for name in LINUX_PAYLOADS} | {
    f"{name}.asc.sha256" for name in LINUX_PAYLOADS
}
FINAL_INVENTORY = STAGING_INVENTORY | LINUX_SIGNATURES


def validate_inventory(names: set[str], *, final: bool | None = None) -> None:
    if not names or not names <= FINAL_INVENTORY:
        unexpected = sorted(names - FINAL_INVENTORY)
        raise ValueError(f"release inventory contains an unsupported asset: {unexpected[:3]}")
    if final is True and names != FINAL_INVENTORY:
        raise ValueError("final release inventory is not the complete Linux + Windows set")
    if final is False and names != STAGING_INVENTORY:
        raise ValueError("staging release inventory is not the complete Linux + Windows set")


def digest(path: Path) -> str:
    if path.is_symlink() or not stat.S_ISREG(path.stat(follow_symlinks=False).st_mode):
        raise ValueError(f"artifact must be a regular non-symlink file: {path.name!r}")
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def safe_name(name: object) -> str:
    if not isinstance(name, str) or not name or "/" in name or "\\" in name:
        raise ValueError(f"unsafe artifact name: {name!r}")
    if Path(name).name != name or name in {".", ".."}:
        raise ValueError(f"unsafe artifact name: {name!r}")
    return name


def load(path: Path, tag: str, source_sha: str) -> list[dict[str, str]]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"invalid manifest {path}: {error}") from error
    if not isinstance(value, dict):
        raise ValueError("manifest schema is not canonical")
    expected = {"version", "tag", "source_sha", "artifacts"}
    if value.get("version") == 2:
        expected |= {"staging_manifest_sha256", "allowed_transformations"}
    if set(value) != expected:
        raise ValueError("manifest schema is not canonical")
    if value["version"] not in (1, 2) or value["tag"] != tag or value["source_sha"] != source_sha:
        raise ValueError("manifest tag/source identity does not match the typed caller inputs")
    if value["version"] == 2 and (not isinstance(value["staging_manifest_sha256"], str) or not SHA256.fullmatch(value["staging_manifest_sha256"]) or value["allowed_transformations"] != ["replace-platform-signed-artifact", "add-linux-detached-signature-asc"]):
        raise ValueError("final manifest parent or transformation policy is invalid")
    artifacts = value["artifacts"]
    if not isinstance(artifacts, list) or not artifacts:
        raise ValueError("manifest must contain a non-empty artifact list")
    seen: set[str] = set()
    normalized: list[dict[str, str]] = []
    for artifact in artifacts:
        if not isinstance(artifact, dict) or set(artifact) != {"name", "sha256"}:
            raise ValueError("each artifact must contain only name/sha256")
        name = safe_name(artifact["name"])
        checksum = artifact["sha256"]
        if name in seen or not isinstance(checksum, str) or not SHA256.fullmatch(checksum):
            raise ValueError("manifest has duplicate artifact or invalid digest")
        seen.add(name)
        normalized.append({"name": name, "sha256": checksum})
    validate_inventory(seen, final=value["version"] == 2)
    return normalized


def create(directory: Path, output: Path, tag: str, source_sha: str, staging_manifest: Path | None = None) -> None:
    if not TAG.fullmatch(tag) or not SHA.fullmatch(source_sha):
        raise ValueError("tag/source SHA are not canonical")
    ignored = {output.name}
    parent_names: set[str] | None = None
    parent_sha: str | None = None
    if staging_manifest is not None:
        parent = json.loads(staging_manifest.read_text(encoding="utf-8"))
        if not isinstance(parent, dict) or parent.get("version") != 1 or parent.get("tag") != tag or parent.get("source_sha") != source_sha:
            raise ValueError("final manifest parent is not the canonical staging manifest")
        parent_entries = {item["name"]: item["sha256"] for item in load(staging_manifest, tag, source_sha)}
        validate_inventory(set(parent_entries), final=False)
        if any(not isinstance(checksum, str) or not SHA256.fullmatch(checksum) for checksum in parent_entries.values()):
            raise ValueError("final manifest parent has invalid staged digest")
        parent_names = set(parent_entries)
        if not parent_names:
            raise ValueError("final manifest parent has no staged inventory")
        parent_sha = digest(staging_manifest)
        ignored.add(staging_manifest.name)
    files = []
    for path in sorted(directory.iterdir()):
        if path.name in ignored:
            continue
        if path.is_symlink() or not stat.S_ISREG(path.stat(follow_symlinks=False).st_mode):
            raise ValueError(f"release inventory contains non-regular or symlink entry: {path.name!r}")
        files.append(path)
    artifacts = [{"name": safe_name(path.name), "sha256": digest(path)} for path in files]
    if not artifacts:
        raise ValueError("refusing to create an empty release manifest")
    names = {item["name"] for item in artifacts}
    validate_inventory(names, final=staging_manifest is not None)
    if parent_names is not None:
        signed_linux = {name for name in parent_names if name.startswith("corelink-linux-") and not name.endswith(".sha256")}
        signatures = {f"{name}.asc" for name in signed_linux}
        signatures |= {f"{name}.asc.sha256" for name in signed_linux}
        if names - parent_names - signatures or not parent_names <= names:
            raise ValueError("final inventory exceeds allowlisted signed-artifact transformations")
        changed = {item["name"] for item in artifacts if item["name"] in parent_entries and item["sha256"] != parent_entries[item["name"]]}
        allowed_changed = {"corelink-windows-x86_64.exe", "corelink-windows-x86_64.zip"}
        # Digest sidecars and the aggregate checksum necessarily change when an
        # allowlisted signed payload changes; they remain exact manifest subjects.
        allowed_changed |= {name for name in parent_names if name.endswith(".sha256") or name == "checksums.txt"}
        if changed - allowed_changed:
            raise ValueError("final inventory alters a staged artifact outside the signing allowlist")
        payload = {"version": 2, "tag": tag, "source_sha": source_sha, "staging_manifest_sha256": parent_sha, "allowed_transformations": ["replace-platform-signed-artifact", "add-linux-detached-signature-asc"], "artifacts": artifacts}
    else:
        payload = {"version": 1, "tag": tag, "source_sha": source_sha, "artifacts": artifacts}
    output.write_text(json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")


def verify(directory: Path, manifest: Path, tag: str, source_sha: str, asset: str | None) -> None:
    if not TAG.fullmatch(tag) or not SHA.fullmatch(source_sha):
        raise ValueError("tag/source SHA are not canonical")
    artifacts = load(manifest, tag, source_sha)
    requested = safe_name(asset) if asset else None
    matched = False
    for entry in artifacts:
        if requested and entry["name"] != requested:
            continue
        candidate = directory / entry["name"]
        if candidate.is_symlink() or not stat.S_ISREG(candidate.stat(follow_symlinks=False).st_mode) or digest(candidate) != entry["sha256"]:
            raise ValueError(f"artifact does not match immutable manifest: {entry['name']}")
        matched = True
    if requested and not matched:
        raise ValueError(f"requested artifact absent from immutable manifest: {requested}")
    if not requested:
        actual = {path.name for path in directory.iterdir() if path.name != manifest.name}
        expected = {entry["name"] for entry in artifacts}
        if actual != expected:
            raise ValueError("directory inventory differs from immutable manifest")


def main() -> int:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("create", "verify"):
        command = commands.add_parser(name)
        command.add_argument("--directory", type=Path, required=True)
        command.add_argument("--tag", required=True)
        command.add_argument("--source-sha", required=True)
    create_parser = commands.choices["create"]
    create_parser.add_argument("--output", type=Path, required=True)
    create_parser.add_argument("--staging-manifest", type=Path)
    verify_parser = commands.choices["verify"]
    verify_parser.add_argument("--manifest", type=Path, required=True)
    verify_parser.add_argument("--asset")
    args = parser.parse_args()
    try:
        if args.command == "create":
            create(args.directory, args.output, args.tag, args.source_sha, args.staging_manifest)
        else:
            verify(args.directory, args.manifest, args.tag, args.source_sha, args.asset)
    except ValueError as error:
        print(f"cli-release-manifest: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
