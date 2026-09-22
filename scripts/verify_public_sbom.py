#!/usr/bin/env python3
"""Fail-closed verifier for the public-clone CycloneDX artifact."""
from __future__ import annotations

import argparse
import json
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = ROOT / "Cargo.lock"
CHECKSUM_PROPERTY = "corelink:cargo:checksum"
SOURCE_PROPERTY = "corelink:cargo:source"


def lock_packages() -> list[dict[str, object]]:
    with LOCK_PATH.open("rb") as handle:
        packages = tomllib.load(handle).get("package", [])
    if not isinstance(packages, list) or not packages:
        raise SystemExit("Cargo.lock has no package population")
    return packages


def properties(component: dict[str, object], name: str) -> list[object]:
    values = []
    for item in component.get("properties", []):
        if isinstance(item, dict) and item.get("name") == name:
            values.append(item.get("value"))
    return values


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("artifact", type=Path)
    args = parser.parse_args()
    try:
        document = json.loads(args.artifact.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"invalid SBOM JSON: {error}") from error
    if document.get("bomFormat") != "CycloneDX" or document.get("specVersion") != "1.5":
        raise SystemExit("SBOM must declare CycloneDX 1.5")
    components = document.get("components")
    if not isinstance(components, list) or not components:
        raise SystemExit("SBOM components must be a nonempty list")
    packages = lock_packages()
    expected = {(str(p["name"]), str(p["version"]), str(p.get("source") or "path")): p for p in packages}
    if len(components) != len(expected):
        raise SystemExit(f"component count mismatch: expected {len(expected)}, got {len(components)}")
    seen: set[tuple[str, str, str]] = set()
    hashed = 0
    for component in components:
        if not isinstance(component, dict):
            raise SystemExit("component is not an object")
        identity = (str(component.get("name")), str(component.get("version")), str((properties(component, SOURCE_PROPERTY) or [""])[0]))
        package = expected.get(identity)
        if package is None or identity in seen:
            raise SystemExit(f"unexpected or duplicate component: {identity}")
        seen.add(identity)
        checksum = package.get("checksum")
        checksums = properties(component, CHECKSUM_PROPERTY)
        hashes = component.get("hashes", [])
        if checksum is not None:
            expected_checksum = str(checksum)
            if checksums != [expected_checksum] or not isinstance(hashes, list) or len(hashes) != 1:
                raise SystemExit(f"missing checksum/hash for {identity}")
            digest = hashes[0]
            if not isinstance(digest, dict) or digest.get("alg") != "SHA-256" or digest.get("content") != expected_checksum:
                raise SystemExit(f"nonmatching SHA-256 hash for {identity}")
            if not re.fullmatch(r"[0-9a-f]{64}", expected_checksum):
                raise SystemExit(f"nonempty hash is not a SHA-256 digest for {identity}")
            hashed += 1
        elif checksums or hashes:
            raise SystemExit(f"path component has an unexpected checksum/hash: {identity}")
    if seen != set(expected):
        raise SystemExit("SBOM does not cover exactly Cargo.lock")
    print(f"verified CycloneDX 1.5 schema; components={len(components)}; nonempty_sha256_hashes={hashed}; path_components={len(components)-hashed}")


if __name__ == "__main__":
    main()
