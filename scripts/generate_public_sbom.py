#!/usr/bin/env python3
"""Generate the bounded public-clone CycloneDX 1.5 SBOM from Cargo.lock."""
from __future__ import annotations

import argparse
import json
import re
import tomllib
from pathlib import Path
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = ROOT / "Cargo.lock"


def package_identity(package: dict[str, object]) -> tuple[str, str, str]:
    return (str(package["name"]), str(package["version"]), str(package.get("source") or "path"))


def load_packages() -> list[dict[str, object]]:
    with LOCK_PATH.open("rb") as handle:
        packages = tomllib.load(handle).get("package", [])
    if not isinstance(packages, list) or not packages:
        raise SystemExit("Cargo.lock has no package population")
    identities = [package_identity(package) for package in packages]
    if len(identities) != len(set(identities)):
        raise SystemExit("Cargo.lock has duplicate package identities")
    return sorted(packages, key=package_identity)


def component(package: dict[str, object]) -> dict[str, object]:
    name, version, source = package_identity(package)
    purl = f"pkg:cargo/{quote(name, safe='.-_')}@{quote(version, safe='.-_')}"
    properties = [{"name": "corelink:cargo:source", "value": source}]
    checksum = package.get("checksum")
    entry: dict[str, object] = {
        "bom-ref": f"{purl}?source={quote(source, safe='')}",
        "type": "library",
        "name": name,
        "version": version,
        "purl": purl,
        "properties": properties,
    }
    if checksum is not None:
        value = str(checksum)
        if not re.fullmatch(r"[0-9a-f]{64}", value):
            raise SystemExit(f"invalid Cargo checksum for {name}@{version}")
        properties.append({"name": "corelink:cargo:checksum", "value": value})
        # Cargo's registry checksum is the crate archive SHA-256.
        entry["hashes"] = [{"alg": "SHA-256", "content": value}]
    return entry


def build() -> dict[str, object]:
    packages = load_packages()
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": "urn:uuid:00000000-0000-4000-8000-000000000168",
        "version": 1,
        "metadata": {
            "timestamp": "1970-01-01T00:00:00Z",
            "tools": [{"vendor": "HuGR-dev", "name": "public-clone-cargo-lock-sbom", "version": "1"}],
            "component": {
                "bom-ref": "pkg:cargo/corelink-server-workspace@0.1.0",
                "type": "application",
                "name": "corelink-server-workspace",
                "version": "0.1.0",
                "purl": "pkg:cargo/corelink-server-workspace@0.1.0",
            },
        },
        "components": [component(package) for package in packages],
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    payload = json.dumps(build(), indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    args.output.write_text(payload, encoding="utf-8")
    print(f"generated {args.output} from {len(json.loads(payload)['components'])} Cargo.lock components")


if __name__ == "__main__":
    main()
