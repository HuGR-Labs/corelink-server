#!/usr/bin/env python3
"""Verify the complete public CLI release inventory before publication."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import stat
from pathlib import Path

from cli_release_manifest import load


METADATA = {"release-manifest.json", "provenance.intoto.jsonl", "provenance.intoto.jsonl.bundle"}
CHECKSUM_LINE = re.compile(r"^([0-9a-fA-F]{64})  ([^\r\n]+)$")


def sha256(path: Path) -> str:
    if path.is_symlink() or not stat.S_ISREG(path.stat(follow_symlinks=False).st_mode):
        raise ValueError(f"non-regular release asset: {path.name!r}")
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def verify_checksums(directory: Path, artifact_digests: dict[str, str]) -> None:
    """Require checksums.txt to be the canonical closed-world sidecar index."""
    checksum_file = directory / "checksums.txt"
    if "checksums.txt" not in artifact_digests:
        raise ValueError("final manifest omits checksums.txt")
    sidecars = sorted(
        name for name in artifact_digests
        if name.startswith("corelink-") and name.endswith(".sha256")
    )
    if not sidecars:
        raise ValueError("final manifest has no artifact checksum sidecars")
    expected_lines: list[str] = []
    for sidecar in sidecars:
        text = (directory / sidecar).read_text(encoding="utf-8")
        lines = text.splitlines()
        if len(lines) != 1:
            raise ValueError(f"checksum sidecar is not a single canonical line: {sidecar}")
        match = CHECKSUM_LINE.fullmatch(lines[0])
        if match is None:
            raise ValueError(f"checksum sidecar has invalid syntax: {sidecar}")
        digest, target = match.groups()
        if target != sidecar[:-len(".sha256")]:
            raise ValueError(f"checksum sidecar target does not match its name: {sidecar}")
        if target not in artifact_digests or sha256(directory / target) != digest.lower():
            raise ValueError(f"checksum sidecar digest is not bound to the final asset: {sidecar}")
        expected_lines.append(f"{digest.lower()}  {target}")
    expected = "\n".join(sorted(expected_lines)) + "\n"
    actual = checksum_file.read_text(encoding="utf-8")
    if actual != expected:
        raise ValueError("checksums.txt is not the canonical closed-world asset index")


def verify(api_path: Path, directory: Path, manifest: Path, provenance: Path,
           tag: str, source_sha: str, manifest_sha256: str, repository: str) -> None:
    api = json.loads(api_path.read_text(encoding="utf-8"))
    api_names = [asset.get("name") for asset in api.get("assets", [])]
    if any(not isinstance(name, str) or not name for name in api_names):
        raise ValueError("release API contains an invalid asset name")
    if len(api_names) != len(set(api_names)):
        raise ValueError("release API contains duplicate asset names")
    if "staging-manifest.json" in api_names:
        raise ValueError("private staging manifest is still attached to the release")

    entries = load(manifest, tag, source_sha)
    if sha256(manifest) != manifest_sha256:
        raise ValueError("release manifest digest differs from final-manifest output")
    artifact_digests = {entry["name"]: entry["sha256"] for entry in entries}
    expected = set(artifact_digests) | METADATA
    if set(api_names) != expected:
        raise ValueError(
            f"release API inventory is not closed-world: expected {sorted(expected)}, "
            f"got {sorted(api_names)}"
        )
    actual_names = {path.name for path in directory.iterdir()}
    if actual_names != expected:
        raise ValueError("downloaded release inventory differs from authenticated API inventory")
    for name, expected_digest in artifact_digests.items():
        if sha256(directory / name) != expected_digest:
            raise ValueError(f"release asset digest differs from final manifest: {name}")
    verify_checksums(directory, artifact_digests)

    statement = json.loads(provenance.read_text(encoding="utf-8"))
    expected_builder = f"https://github.com/{repository}/.github/workflows/release-slsa3.yml@refs/tags/{tag}"
    try:
        builder_id = statement["predicate"]["buildDefinition"]["runDetails"]["builder"]["id"]
    except (KeyError, TypeError):
        raise ValueError("provenance builder identity is missing")
    if builder_id != expected_builder:
        raise ValueError("provenance builder is not the tagged reusable release workflow")
    subjects = statement.get("subject")
    if not isinstance(subjects, list):
        raise ValueError("provenance subject list is invalid")
    subject_digests: dict[str, str] = {}
    for index, item in enumerate(subjects):
        if not isinstance(item, dict) or set(item) != {"name", "digest"}:
            raise ValueError(f"provenance subject[{index}] has a non-canonical shape")
        name = item.get("name")
        digest = item.get("digest")
        if not isinstance(name, str) or not name:
            raise ValueError(f"provenance subject[{index}] has an invalid name")
        if name in subject_digests:
            raise ValueError(f"provenance subject list contains duplicate name: {name}")
        if (not isinstance(digest, dict) or set(digest) != {"sha256"}
                or not isinstance(digest.get("sha256"), str)
                or len(digest["sha256"]) != 64
                or any(char not in "0123456789abcdefABCDEF" for char in digest["sha256"])):
            raise ValueError(f"provenance subject[{index}] has an invalid SHA-256 digest")
        subject_digests[name] = digest["sha256"].lower()
    if subject_digests != artifact_digests:
        raise ValueError("SLSA subjects are not exactly bound to the final manifest")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--api-json", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--provenance", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--manifest-sha256", required=True)
    parser.add_argument("--repository", required=True)
    args = parser.parse_args()
    try:
        verify(args.api_json, args.directory, args.manifest, args.provenance,
               args.tag, args.source_sha, args.manifest_sha256, args.repository)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"cli-release-inventory: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
