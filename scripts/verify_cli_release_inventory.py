#!/usr/bin/env python3
"""Verify the complete public CLI release inventory before publication."""

from __future__ import annotations

import argparse
import base64
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
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_checksums(directory: Path, artifact_digests: dict[str, str]) -> None:
    if "checksums.txt" not in artifact_digests:
        raise ValueError("final manifest omits checksums.txt")
    sidecars = sorted(name for name in artifact_digests if name.startswith("corelink-") and name.endswith(".sha256"))
    if not sidecars:
        raise ValueError("final manifest has no artifact checksum sidecars")
    expected: list[str] = []
    for sidecar in sidecars:
        lines = (directory / sidecar).read_text(encoding="utf-8").splitlines()
        if len(lines) != 1 or (match := CHECKSUM_LINE.fullmatch(lines[0])) is None:
            raise ValueError(f"checksum sidecar is not a canonical line: {sidecar}")
        digest, target = match.groups()
        if target != sidecar.removesuffix(".sha256") or target not in artifact_digests:
            raise ValueError(f"checksum sidecar target does not match its name: {sidecar}")
        if sha256(directory / target) != digest.lower():
            raise ValueError(f"checksum sidecar digest is not bound to the final asset: {sidecar}")
        expected.append(f"{digest.lower()}  {target}")
    if (directory / "checksums.txt").read_text(encoding="utf-8") != "\n".join(expected) + "\n":
        raise ValueError("checksums.txt is not the canonical closed-world asset index")


def signed_statement(provenance: Path, bundle_path: Path) -> dict[str, object]:
    """Require the release statement to be the payload in its DSSE bundle."""
    try:
        bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
        payload = base64.b64decode(bundle["dsseEnvelope"]["payload"], validate=True)
        statement = json.loads(payload)
        published = json.loads(provenance.read_text(encoding="utf-8"))
    except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        raise ValueError("provenance is not a GitHub DSSE bundle and statement") from error
    if statement != published or not isinstance(statement, dict):
        raise ValueError("published provenance differs from the signed DSSE payload")
    if statement.get("predicateType") != "https://slsa.dev/provenance/v1":
        raise ValueError("provenance is not SLSA v1")
    return statement


def subject_digests(statement: dict[str, object]) -> dict[str, str]:
    subjects = statement.get("subject")
    if not isinstance(subjects, list) or not subjects:
        raise ValueError("provenance subject list is invalid")
    result: dict[str, str] = {}
    for index, item in enumerate(subjects):
        if not isinstance(item, dict) or set(item) != {"name", "digest"}:
            raise ValueError(f"provenance subject[{index}] has a non-canonical shape")
        name, digest = item.get("name"), item.get("digest")
        if not isinstance(name, str) or not name or name in result:
            raise ValueError(f"provenance subject[{index}] has an invalid name")
        if (not isinstance(digest, dict) or set(digest) != {"sha256"}
                or not isinstance(digest.get("sha256"), str)
                or not re.fullmatch(r"[0-9a-fA-F]{64}", digest["sha256"])):
            raise ValueError(f"provenance subject[{index}] has an invalid SHA-256 digest")
        result[name] = digest["sha256"].lower()
    return result


def verify(api_path: Path, directory: Path, manifest: Path, provenance: Path, bundle: Path,
           tag: str, source_sha: str, manifest_sha256: str) -> None:
    api_names = [asset.get("name") for asset in json.loads(api_path.read_text(encoding="utf-8")).get("assets", [])]
    if any(not isinstance(name, str) or not name for name in api_names) or len(api_names) != len(set(api_names)):
        raise ValueError("release API contains an invalid or duplicate asset name")
    if "staging-manifest.json" in api_names:
        raise ValueError("private staging manifest is still attached to the release")
    entries = load(manifest, tag, source_sha)
    if sha256(manifest) != manifest_sha256:
        raise ValueError("release manifest digest differs from final-manifest output")
    artifacts = {entry["name"]: entry["sha256"] for entry in entries}
    expected = set(artifacts) | METADATA
    if set(api_names) != expected or {path.name for path in directory.iterdir()} != expected:
        raise ValueError("release inventory is not closed-world")
    for name, digest in artifacts.items():
        if sha256(directory / name) != digest:
            raise ValueError(f"release asset digest differs from final manifest: {name}")
    verify_checksums(directory, artifacts)
    if subject_digests(signed_statement(provenance, bundle)) != artifacts:
        raise ValueError("SLSA subjects are not exactly bound to the final manifest")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--api-json", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--provenance", type=Path, required=True)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--manifest-sha256", required=True)
    args = parser.parse_args()
    try:
        verify(args.api_json, args.directory, args.manifest, args.provenance, args.bundle,
               args.tag, args.source_sha, args.manifest_sha256)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"cli-release-inventory: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
