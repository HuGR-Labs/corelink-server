#!/usr/bin/env python3
"""Fail-closed structural contract for Neon shadow module paths (B-262)."""

from __future__ import annotations

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
REAL = "crates/corelink-audit-chain/src/neon_shadow/real.rs"
PARENT = "crates/corelink-audit-chain/src/neon_shadow.rs"
NATIVE = "crates/corelink-audit-chain/src/neon_shadow/native.rs"


class VerificationError(RuntimeError):
    """The Neon shadow native module graph is not reachable."""


def _source(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        return overrides[path]
    try:
        return (root / path).read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"missing B-262 source: {path}") from exc


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    """Verify the production module binding and its sibling implementation."""
    overrides = overrides or {}
    real = _source(root, REAL, overrides)
    parent = _source(root, PARENT, overrides)
    native = _source(root, NATIVE, overrides)

    native_decl = re.compile(
        r'(?m)^[ \t]*#\[cfg\(not\(target_arch = "wasm32"\)\)\][ \t]*\n'
        r'[ \t]*#\[path = "native\.rs"\][ \t]*\n'
        r'[ \t]*mod native;[ \t]*$'
    )
    declarations = list(native_decl.finditer(real))
    if len(declarations) != 1:
        raise VerificationError(
            "real.rs must bind exactly one non-wasm native.rs sibling with #[path]"
        )

    # The re-export must remain native-only and must consume that module;
    # checking executable syntax prevents comments or strings from satisfying
    # the contract.
    export_decl = re.compile(
        r'(?m)^[ \t]*#\[cfg\(not\(target_arch = "wasm32"\)\)\][ \t]*\n'
        r'[ \t]*pub use native::RealNeonShadowSink;[ \t]*$'
    )
    if len(list(export_decl.finditer(real))) != 1:
        raise VerificationError(
            "real.rs must expose exactly one native-only RealNeonShadowSink re-export"
        )

    parent_exports = re.findall(r"(?m)^[ \t]*pub mod real;[ \t]*$", parent)
    if len(parent_exports) != 1:
        raise VerificationError("neon_shadow.rs must expose real.rs exactly once")
    if "pub struct RealNeonShadowSink" not in native:
        raise VerificationError("native.rs no longer defines RealNeonShadowSink")
    if "impl NeonShadowSink for RealNeonShadowSink" not in native:
        raise VerificationError("native.rs no longer implements NeonShadowSink")
    if not native.startswith("use super::*;"):
        raise VerificationError("native.rs must remain a child module of real.rs")


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-262 BROKEN: {exc}")
    print("B-262 Neon shadow module path: PASS")
