#!/usr/bin/env python3
"""Generate and verify the committed Cargo.lock CycloneDX inventory.

The Rust SBOM is deliberately a lock-file inventory, rather than an aggregate
of binaries selected by cargo-cyclonedx.  A binary aggregate can omit a valid
but currently-unreferenced lock entry and it can be non-deterministic (UUIDs,
timestamps, and host paths).  This harness is the sole generator for
`.sbom/cyclonedx-rust.json`: it obtains license metadata from `cargo metadata
--locked`, explicitly controls the seven Cargo.lock entries which Cargo does
not resolve, and compares canonical bytes with the committed artifact.

Usage:
  python3 tests/verify_rust_sbom.py --check
  python3 tests/verify_rust_sbom.py --write
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tomllib
from pathlib import Path
from urllib.parse import quote

try:
    from license_expression import ExpressionError, get_spdx_licensing
except ImportError as error:
    raise SystemExit(
        "missing SPDX expression validator; install requirements-ci.txt before running this harness"
    ) from error


ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = ROOT / "Cargo.lock"
SBOM_PATH = ROOT / ".sbom" / "cyclonedx-rust.json"
SOURCE_PROPERTY = "corelink:cargo:source"
CHECKSUM_PROPERTY = "corelink:cargo:checksum"
LICENSE_REF = "LicenseRef-CoreLink-Proprietary"
SPDX_LICENSING = get_spdx_licensing()

# These are present in Cargo.lock but absent from `cargo metadata --locked`
# because no current workspace member resolves them.  Keeping this exact map is
# intentional: a newly stale package must be reviewed rather than silently
# receiving NOASSERTION or a host-cache-derived license.
INACTIVE_LOCK_LICENSES = {
    ("deadpool-postgres", "0.14.1", "registry+https://github.com/rust-lang/crates.io-index"): "MIT OR Apache-2.0",
    ("der_derive", "0.7.3", "registry+https://github.com/rust-lang/crates.io-index"): "Apache-2.0 OR MIT",
    ("flagset", "0.4.7", "registry+https://github.com/rust-lang/crates.io-index"): "Apache-2.0",
    ("tls_codec", "0.4.2", "registry+https://github.com/rust-lang/crates.io-index"): "Apache-2.0 OR MIT",
    ("tls_codec_derive", "0.4.2", "registry+https://github.com/rust-lang/crates.io-index"): "Apache-2.0 OR MIT",
    ("tokio-postgres-rustls", "0.14.0", "registry+https://github.com/rust-lang/crates.io-index"): "MIT",
    ("x509-cert", "0.2.5", "registry+https://github.com/rust-lang/crates.io-index"): "Apache-2.0 OR MIT",
}

def identity(package: dict[str, object]) -> tuple[str, str, str]:
    return (
        str(package["name"]),
        str(package["version"]),
        str(package.get("source") or "path"),
    )


def cargo_metadata() -> dict[str, object]:
    """Return metadata or fail; a generator error must never yield an SBOM."""
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise SystemExit(f"cargo metadata returned invalid JSON: {error}") from error


def is_local_path_package(package: dict[str, object]) -> bool:
    """Return whether cargo identified this package as a path under this checkout."""
    if package.get("source") is not None:
        return False
    manifest = package.get("manifest_path")
    if not isinstance(manifest, str):
        return False
    try:
        Path(manifest).resolve().relative_to(ROOT.resolve())
    except ValueError:
        return False
    return True


def normalized_license(package: dict[str, object]) -> str:
    """Validate SPDX metadata, with one local-only proprietary exception."""
    value = package.get("license")
    license_expression = " ".join(str(value).strip().replace("/", " OR ").split())
    if license_expression == "UNLICENSED":
        if not is_local_path_package(package):
            raise SystemExit(f"external package declares UNLICENSED: {identity(package)}")
        return LICENSE_REF
    if license_expression == LICENSE_REF:
        if not is_local_path_package(package):
            raise SystemExit(f"external package uses proprietary license reference: {identity(package)}")
        return LICENSE_REF
    try:
        SPDX_LICENSING.parse(license_expression, validate=True, strict=True)
    except ExpressionError as error:
        raise SystemExit(f"invalid SPDX license expression: {license_expression!r}") from error
    return license_expression


def valid_spdx_expression(expression: str) -> bool:
    """Use the SPDX identifier list and expression parser; never accept syntax alone."""
    try:
        SPDX_LICENSING.parse(expression, validate=True, strict=True)
    except ExpressionError:
        return False
    return True


def lock_packages() -> list[dict[str, object]]:
    with LOCK_PATH.open("rb") as lock_file:
        packages = tomllib.load(lock_file).get("package", [])
    if not packages:
        raise SystemExit("Cargo.lock has no package population")
    identities = [identity(package) for package in packages]
    if len(identities) != len(set(identities)):
        raise SystemExit("Cargo.lock has duplicate (name, version, source) identities")
    return sorted(packages, key=identity)


def component_for(package: dict[str, object], licenses: dict[tuple[str, str, str], str]) -> dict[str, object]:
    package_id = identity(package)
    name, version, source = package_id
    license_expression = licenses.get(package_id)
    if license_expression is None:
        raise SystemExit(f"missing license metadata for locked package: {package_id}")
    purl = f"pkg:cargo/{quote(name, safe='.-_')}@{quote(version, safe='.-_')}"
    properties = [{"name": SOURCE_PROPERTY, "value": source}]
    if checksum := package.get("checksum"):
        properties.append({"name": CHECKSUM_PROPERTY, "value": str(checksum)})
    return {
        "bom-ref": f"{purl}?source={quote(source, safe='')}",
        "type": "library",
        "name": name,
        "version": version,
        "purl": purl,
        "licenses": [{"expression": license_expression}],
        "properties": properties,
    }


def build_sbom() -> dict[str, object]:
    packages = lock_packages()
    metadata = cargo_metadata()
    licenses = {
        identity(package): normalized_license(package)
        for package in metadata["packages"]
        if package.get("license")
    }
    overlap = set(licenses) & set(INACTIVE_LOCK_LICENSES)
    if overlap:
        raise SystemExit(f"inactive lock license map is stale: {sorted(overlap)}")
    licenses.update(INACTIVE_LOCK_LICENSES)
    components = [component_for(package, licenses) for package in packages]
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": "urn:uuid:00000000-0000-4000-8000-000000000091",
        "version": 1,
        "metadata": {
            "timestamp": "1970-01-01T00:00:00Z",
            "tools": [{"vendor": "HumanGuardrail", "name": "corelink-cargo-lock-sbom", "version": "1"}],
            "component": {
                "bom-ref": "pkg:cargo/corelink-server-workspace@0.1.0",
                "type": "application",
                "name": "corelink-server-workspace",
                "version": "0.1.0",
                "purl": "pkg:cargo/corelink-server-workspace@0.1.0",
                "licenses": [{"expression": LICENSE_REF}],
            },
        },
        "components": components,
    }


def rendered(sbom: dict[str, object]) -> str:
    return json.dumps(sbom, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def component_identities(sbom: dict[str, object]) -> set[tuple[str, str, str]]:
    components = sbom.get("components")
    if not isinstance(components, list):
        raise SystemExit("SBOM components is missing or not a list")
    found = set()
    for component in components:
        if not isinstance(component, dict):
            raise SystemExit("SBOM contains a non-object component")
        source = next(
            (
                property_["value"]
                for property_ in component.get("properties", [])
                if property_.get("name") == SOURCE_PROPERTY
            ),
            None,
        )
        if not isinstance(source, str):
            raise SystemExit(f"component has no {SOURCE_PROPERTY}: {component.get('name')}")
        expression = component.get("licenses", [{}])[0].get("expression")
        if not isinstance(expression, str) or expression == "UNLICENSED":
            raise SystemExit(f"component has invalid SPDX license: {component.get('name')}")
        if expression == LICENSE_REF:
            if source != "path":
                raise SystemExit(f"external component uses proprietary license reference: {component.get('name')}")
        elif not valid_spdx_expression(expression):
            raise SystemExit(f"component has invalid SPDX license: {component.get('name')}")
        found.add((str(component.get("name")), str(component.get("version")), source))
    if len(found) != len(components):
        raise SystemExit("SBOM components are not one-to-one with Cargo.lock identities")
    return found


def verify(sbom: dict[str, object]) -> None:
    expected = {identity(package) for package in lock_packages()}
    actual = component_identities(sbom)
    if expected != actual:
        missing, extra = sorted(expected - actual), sorted(actual - expected)
        raise SystemExit(f"SBOM/Cargo.lock population mismatch; missing={missing[:3]}, extra={extra[:3]}")
    known = next(package for package in expected if package[0] == "serde")
    if known not in actual:
        raise SystemExit("known lock component serde is absent from SBOM")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true", help="write the canonical artifact")
    parser.add_argument("--check", action="store_true", help="compare canonical output with the committed artifact")
    args = parser.parse_args()
    if args.write == args.check:
        parser.error("choose exactly one of --write or --check")
    sbom = build_sbom()
    verify(sbom)
    expected = rendered(sbom)
    if args.write:
        SBOM_PATH.write_text(expected, encoding="utf-8")
        return
    try:
        committed = json.loads(SBOM_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"missing or invalid committed SBOM: {error}") from error
    verify(committed)
    if rendered(committed) != expected:
        raise SystemExit("committed SBOM is stale; run: python3 tests/verify_rust_sbom.py --write")


if __name__ == "__main__":
    main()
