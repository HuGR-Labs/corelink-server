#!/usr/bin/env python3
"""Fail-closed contract for the B-263 audit split and workspace lockfile."""

from __future__ import annotations

from pathlib import Path
import re
import tomllib


ROOT = Path(__file__).resolve().parents[1]
EVENTS = "crates/corelink-audit/src/events.rs"
SYNTHETIC = "crates/corelink-audit/src/synthetic_data.rs"
MANIFEST = "Cargo.toml"
LOCKFILE = "Cargo.lock"


class VerificationError(RuntimeError):
    """The audit module graph or locked workspace dependency drifted."""


def _read(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        return overrides[path]
    try:
        return (root / path).read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"missing B-263 input: {path}") from exc


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    overrides = overrides or {}
    events = _read(root, EVENTS, overrides)
    synthetic = _read(root, SYNTHETIC, overrides)
    manifest_text = _read(root, MANIFEST, overrides)
    lock_text = _read(root, LOCKFILE, overrides)

    binding = re.compile(
        r'(?m)^#\[path = "synthetic_data\.rs"\]\nmod synthetic_data;$'
    )
    if len(binding.findall(events)) != 1:
        raise VerificationError("events.rs must bind synthetic_data.rs exactly once")
    if len(re.findall(r"(?m)^pub use synthetic_data::synthetic_data_for;$", events)) != 1:
        raise VerificationError("events.rs must export synthetic_data_for exactly once")
    if len(re.findall(r"(?m)^pub fn synthetic_data_for\(", synthetic)) != 1:
        raise VerificationError("synthetic_data.rs must define synthetic_data_for exactly once")

    try:
        manifest = tomllib.loads(manifest_text)
        lockfile = tomllib.loads(lock_text)
    except (tomllib.TOMLDecodeError, ValueError) as exc:
        raise VerificationError(f"invalid Cargo manifest/lockfile: {exc}") from exc
    workspace_deps = manifest.get("workspace", {}).get("dependencies", {})
    if "corelink-gc" not in workspace_deps:
        raise VerificationError("workspace dependencies omit corelink-gc")
    packages = [p for p in lockfile.get("package", []) if p.get("name") == "corelink-server"]
    if len(packages) != 1:
        raise VerificationError("Cargo.lock must contain exactly one corelink-server package")
    deps = packages[0].get("dependencies", [])
    if deps.count("corelink-gc") != 1:
        raise VerificationError("corelink-server lock entry must contain corelink-gc exactly once")


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-263 BROKEN: {exc}")
    print("B-263 audit module path and lockfile: PASS")
