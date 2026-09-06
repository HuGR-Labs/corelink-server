#!/usr/bin/env python3
"""Fail-closed structural contract for B-269's pilot harness module path."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
HARNESS = "tests/e2e-pilot-onboarding/src/harness.rs"
LIFECYCLE = "tests/e2e-pilot-onboarding/src/harness_lifecycle.rs"


class VerificationError(RuntimeError):
    """The pilot harness lifecycle module no longer resolves as a sibling."""


def _read(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        return overrides[path]
    try:
        return (root / path).read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"missing B-269 input: {path}") from exc


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    overrides = overrides or {}
    harness = _read(root, HARNESS, overrides)
    lifecycle = _read(root, LIFECYCLE, overrides)
    binding = re.compile(
        r'(?m)^#\[path = "harness_lifecycle\.rs"\]\nmod harness_lifecycle;$'
    )
    if len(binding.findall(harness)) != 1:
        raise VerificationError("harness.rs must bind sibling harness_lifecycle.rs exactly once")
    required_methods = (
        "pub fn request_dsr_erasure(",
        "pub fn finalise_dsr_erasure(",
        "pub fn cancel_subscription(",
    )
    for marker in required_methods:
        if lifecycle.count(marker) != 1:
            raise VerificationError(f"lifecycle module must define exactly one {marker}")


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-269 BROKEN: {exc}")
    print("B-269 pilot harness module path: PASS")
