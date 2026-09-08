#!/usr/bin/env python3
"""Fail-closed structural contract for Azure real-module paths (B-261)."""

from __future__ import annotations

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
REAL = "crates/corelink-byok/src/byok_azure/real.rs"
PARENT = "crates/corelink-byok/src/byok_azure.rs"
NATIVE = "crates/corelink-byok/src/byok_azure/native.rs"
TESTS = "crates/corelink-byok/src/byok_azure/tests.rs"


class VerificationError(RuntimeError):
    """The Azure native/test module graph is not reachable."""


def _source(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        return overrides[path]
    try:
        return (root / path).read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"missing B-261 source: {path}") from exc


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    overrides = overrides or {}
    real = _source(root, REAL, overrides)
    parent = _source(root, PARENT, overrides)
    native = _source(root, NATIVE, overrides)
    tests = _source(root, TESTS, overrides)
    native_decl = re.compile(r'(?m)^[ \t]*#\[path = "native\.rs"\][ \t]*\n[ \t]*mod native;[ \t]*$')
    tests_decl = re.compile(r'(?m)^[ \t]*#\[path = "tests\.rs"\][ \t]*\n[ \t]*mod tests;[ \t]*$')
    native_matches = list(native_decl.finditer(real))
    tests_matches = list(tests_decl.finditer(real))
    if len(native_matches) != 1:
        raise VerificationError("real.rs must path-bind exactly one native.rs sibling")
    if len(tests_matches) != 1:
        raise VerificationError("real.rs must path-bind exactly one tests.rs sibling")
    native_cfgs = re.findall(r"#\[cfg\([^]]+\)\]", real[max(0, native_matches[0].start() - 200) : native_matches[0].start()])
    tests_cfgs = re.findall(r"#\[cfg\([^]]+\)\]", real[max(0, tests_matches[0].start() - 300) : tests_matches[0].start()])
    if native_cfgs != ['#[cfg(not(target_arch = "wasm32"))]']:
        raise VerificationError("native.rs must be active for every non-wasm production build")
    if tests_cfgs != ['#[cfg(test)]']:
        raise VerificationError("tests.rs must be active only under cfg(test)")
    if '#[cfg(any(\n    all(feature = "production-azure", not(target_arch = "wasm32")),\n    target_arch = "wasm32"\n))]\npub mod real;' not in parent:
        raise VerificationError("byok_azure.rs must expose real.rs for non-wasm production")
    if "pub struct AzureKeyVaultRealProvider" not in native:
        raise VerificationError("native.rs no longer defines AzureKeyVaultRealProvider")
    for marker in (
        "aad_canonicalize_is_key_order_independent",
        "resolve_fips_host_rejects_non_https",
        "resolve_fips_host_rejects_non_fips_tld",
    ):
        if marker not in tests:
            raise VerificationError(f"B-261 behavioral test marker missing: {marker}")


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-261 BROKEN: {exc}")
    print("B-261 Azure module paths: PASS")
