#!/usr/bin/env python3
"""Fail-closed guard for the B-324 main-test lint scope repair."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "crates/corelink-container/src/main_tests.rs"
ALLOWANCE = "#![allow(clippy::unwrap_used, clippy::expect_used)]"


class VerificationError(RuntimeError):
    """The reviewed test-only lint allowance is missing or duplicated."""


def _read(overrides: dict[str, str]) -> str:
    if TARGET in overrides:
        text = overrides[TARGET]
    else:
        path = ROOT / TARGET
        if path.is_symlink() or not path.is_file():
            raise VerificationError(f"missing/non-regular target: {TARGET}")
        text = path.read_text(encoding="utf-8")
    if not isinstance(text, str) or not text or len(text.encode()) > 100_000:
        raise VerificationError("invalid bounded target")
    return text


def verify(*, overrides: dict[str, str] | None = None) -> None:
    source = _read(overrides or {})
    if source.count(ALLOWANCE) != 1:
        raise VerificationError("test-only allowance must occur exactly once")
    if not source.startswith(ALLOWANCE + "\n\nuse super::{"):
        raise VerificationError("allowance must remain a single crate attribute before imports")


def self_test() -> None:
    source = _read({})
    mutations = (
        source.replace(ALLOWANCE + "\n", "", 1),
        ALLOWANCE + "\n" + source,
        source.replace(ALLOWANCE, "#[allow(clippy::unwrap_used, clippy::expect_used)]", 1),
    )
    for index, mutation in enumerate(mutations, 1):
        if mutation == source:
            raise VerificationError(f"self-test mutation {index} changed nothing")
        try:
            verify(overrides={TARGET: mutation})
        except VerificationError:
            continue
        raise VerificationError(f"self-test mutation {index} was accepted")


if __name__ == "__main__":
    try:
        verify()
        if "--self-test" in sys.argv[1:]:
            self_test()
    except (OSError, VerificationError) as exc:
        print(f"B-324 main test lint scope: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    print("B-324 main test lint scope: PASS")
