#!/usr/bin/env python3
"""Fail-closed structural contract for the B-268 customer BYOK type import."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
KEYS = "crates/corelink-handler-customer/src/request/keys.rs"
OVERVIEW = "crates/corelink-handler-customer/src/request/overview.rs"


class VerificationError(RuntimeError):
    """The split customer request module no longer resolves ByokStatus."""


def _read(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        return overrides[path]
    try:
        return (root / path).read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"missing B-268 input: {path}") from exc


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    overrides = overrides or {}
    keys = _read(root, KEYS, overrides)
    overview = _read(root, OVERVIEW, overrides)
    if len(re.findall(r"(?m)^use super::overview::ByokStatus;$", keys)) != 1:
        raise VerificationError("keys.rs must import overview::ByokStatus exactly once")
    if len(re.findall(r"(?m)^pub struct ByokStatus \{$", overview)) != 1:
        raise VerificationError("overview.rs must define ByokStatus exactly once")
    required_uses = (
        "pub byok: ByokStatus,",
        "pub fn new(pats: Vec<PatRow>, byok: ByokStatus) -> Self",
    )
    for marker in required_uses:
        if keys.count(marker) != 1:
            raise VerificationError(f"keys.rs must consume the shared type exactly once: {marker}")


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-268 BROKEN: {exc}")
    print("B-268 customer BYOK import: PASS")
