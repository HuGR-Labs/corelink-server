#!/usr/bin/env python3
"""Verify B-257's manifest/Cargo.lock direct-dependency parity.

This is deliberately a structural TOML check.  It reads the manifest and lock
file as data, so comments or strings containing ``uuid`` cannot satisfy the
contract.  It is stdlib-only and never invokes Cargo, the network, or a build.
"""

from __future__ import annotations

import argparse
import stat
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = Path("crates/corelink-handler-customer/Cargo.toml")
LOCK = Path("Cargo.lock")
PACKAGE = "corelink-handler-customer"


def _read_toml(root: Path, relative: Path) -> dict:
    path = root / relative
    try:
        mode = path.stat().st_mode
        if not stat.S_ISREG(mode):
            raise ValueError(f"{relative} is not a regular file")
        raw = path.read_bytes()
    except (OSError, ValueError) as exc:
        raise ValueError(f"cannot read {relative}: {exc}") from exc
    try:
        value = tomllib.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as exc:
        raise ValueError(f"{relative} is not valid TOML: {exc}") from exc
    if not isinstance(value, dict):
        raise ValueError(f"{relative} root is not a TOML table")
    return value


def _manifest_dependency_names(manifest: dict) -> set[str]:
    package = manifest.get("package")
    if not isinstance(package, dict) or package.get("name") != PACKAGE:
        raise ValueError(f"manifest package name is not {PACKAGE!r}")

    names: set[str] = set()
    for table_name in ("dependencies", "dev-dependencies", "build-dependencies"):
        table = manifest.get(table_name, {})
        if not isinstance(table, dict):
            raise ValueError(f"manifest [{table_name}] is not a table")
        for declared_name, requirement in table.items():
            if not isinstance(declared_name, str):
                raise ValueError(f"manifest [{table_name}] has a non-string dependency name")
            if isinstance(requirement, str):
                names.add(declared_name)
            elif isinstance(requirement, dict):
                # Cargo permits an alias whose lockfile entry uses package=.
                package_name = requirement.get("package", declared_name)
                if not isinstance(package_name, str) or not package_name:
                    raise ValueError(
                        f"manifest dependency {declared_name!r} has an invalid package alias"
                    )
                names.add(package_name)
            else:
                raise ValueError(
                    f"manifest dependency {declared_name!r} has an invalid TOML value"
                )
    return names


def _lock_package(lock: dict) -> dict:
    packages = lock.get("package")
    if not isinstance(packages, list):
        raise ValueError("Cargo.lock has no package array")
    matches = [item for item in packages if isinstance(item, dict) and item.get("name") == PACKAGE]
    if len(matches) != 1:
        raise ValueError(f"Cargo.lock must contain exactly one {PACKAGE!r} package")
    return matches[0]


def _lock_dependency_names(package: dict) -> set[str]:
    dependencies = package.get("dependencies", [])
    if not isinstance(dependencies, list) or not all(isinstance(item, str) for item in dependencies):
        raise ValueError(f"Cargo.lock dependencies for {PACKAGE!r} are malformed")
    names: set[str] = set()
    for item in dependencies:
        name = item.split(maxsplit=1)[0]
        if not name:
            raise ValueError(f"Cargo.lock has an empty dependency for {PACKAGE!r}")
        names.add(name)
    return names


def verify(root: Path) -> list[str]:
    try:
        manifest = _read_toml(root, MANIFEST)
        lock = _read_toml(root, LOCK)
        declared = _manifest_dependency_names(manifest)
        locked_package = _lock_package(lock)
        locked = _lock_dependency_names(locked_package)
    except ValueError as exc:
        return [str(exc)]

    errors: list[str] = []
    missing = sorted(declared - locked)
    extra = sorted(locked - declared)
    if missing:
        errors.append(f"Cargo.lock is missing manifest dependencies for {PACKAGE}: {', '.join(missing)}")
    if extra:
        errors.append(f"Cargo.lock has undeclared direct dependencies for {PACKAGE}: {', '.join(extra)}")
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    errors = verify(args.root.resolve())
    if errors:
        for error in errors:
            print(f"B257 RED: {error}", file=sys.stderr)
        return 1
    print("B257 Cargo manifest/lock parity: PASS (corelink-handler-customer)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
