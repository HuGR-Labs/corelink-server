#!/usr/bin/env python3
"""Fail-closed guard for the B-321 alert-delivery test repair."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "crates/corelink-ops/src/alerts/alerter.rs"
TESTS = (
    "configured_transport_receives_all_four_closed_channels",
    "configured_transport_receives_recovery_and_returns_receipts",
)


class VerificationError(RuntimeError):
    """The two reviewed success-path tests no longer have a strict shape."""


def _read(overrides: dict[str, str]) -> str:
    if TARGET in overrides:
        text = overrides[TARGET]
    else:
        path = ROOT / TARGET
        if path.is_symlink() or not path.is_file():
            raise VerificationError(f"missing/non-regular target: {TARGET}")
        text = path.read_text(encoding="utf-8")
    if not isinstance(text, str) or not text or len(text.encode()) > 300_000:
        raise VerificationError("invalid bounded target")
    return text


def _function(source: str, name: str) -> str:
    marker = f"async fn {name}() {{"
    start = source.find(marker)
    if start < 0 or source.count(marker) != 1:
        raise VerificationError(f"{name}: missing or duplicate test")
    brace = source.find("{", start)
    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[brace + 1 : index]
    raise VerificationError(f"{name}: unterminated body")


def verify(*, overrides: dict[str, str] | None = None) -> None:
    source = _read(overrides or {})
    bodies = [_function(source, name) for name in TESTS]
    for name, body in zip(TESTS, bodies, strict=True):
        if body.count("let result = alerter") != 1:
            raise VerificationError(f"{name}: fallible result binding is not unique")
        if body.count("result.is_ok()") != 1:
            raise VerificationError(f"{name}: success assertion is not unique")
        if ".await.unwrap()" in body or re.search(r"\.await\s*\.unwrap\s*\(", body):
            raise VerificationError(f"{name}: unwrap regression")
        if "configured recording transport must accept" not in body:
            raise VerificationError(f"{name}: diagnostic assertion message was lost")
    if re.search(r"#\s*!?\[\s*allow\s*\([^]]*unwrap_used", source):
        raise VerificationError("targeted unwrap lint suppression is forbidden")


def self_test() -> None:
    source = _read({})
    mutations = (
        source.replace("let result = alerter.alert(payload()).await;", "alerter.alert(payload()).await.unwrap();", 1),
        source.replace("result.is_ok()", "true", 1),
        source.replace("configured recording transport must accept recovery", "recovery", 1),
        "#[allow(clippy::unwrap_used)]\n" + source,
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
        print(f"B-321 alert delivery tests: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    print("B-321 alert delivery tests: PASS")
