#!/usr/bin/env python3
"""Fail-closed guards for the final B-319/B-320 bundled Rust residues."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BILLING = "crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs"
REGION = "crates/corelink-region/src/region.rs"
TARGETS = (BILLING, REGION)


class VerificationError(RuntimeError):
    """A reviewed final-residual invariant is absent or ambiguous."""


def _read(path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        text = overrides[path]
    else:
        candidate = ROOT / path
        if candidate.is_symlink() or not candidate.is_file():
            raise VerificationError(f"missing/non-regular target: {path}")
        text = candidate.read_text(encoding="utf-8")
    if not isinstance(text, str) or not text or len(text.encode()) > 300_000:
        raise VerificationError(f"invalid bounded target: {path}")
    return text


def verify(*, overrides: dict[str, str] | None = None) -> None:
    supplied = overrides or {}
    billing = _read(BILLING, supplied)
    region = _read(REGION, supplied)

    start = billing.find("fn real_latency_probe_under_5ms_p99() {")
    end = billing.find("\n}\n", start)
    if start < 0 or end < 0:
        raise VerificationError("B-319 latency probe is absent or malformed")
    probe = billing[start:end]
    if "eprintln!" in probe or "eprint!" in probe:
        raise VerificationError("B-319 print-stderr macro regression")
    if billing.count("use std::io::Write as _;") != 1:
        raise VerificationError("B-319 Write trait binding is not unique")
    if probe.count("std::io::stderr().lock()") != 1 or "p99_us={p99}" not in probe:
        raise VerificationError("B-319 successful probe output was not preserved")
    if "assert!(p99 <= 5_000" not in probe:
        raise VerificationError("B-319 latency ceiling assertion was weakened")
    if re.search(r"#\s*!?\[\s*allow\s*\([^]]*print_stderr", billing):
        raise VerificationError("B-319 targeted lint suppression is forbidden")

    test_start = region.find("fn test_region_from_str_unknown() {")
    test_end = region.find("\n    }", test_start)
    if test_start < 0 or test_end < 0:
        raise VerificationError("B-320 alias test is absent or malformed")
    alias_test = region[test_start:test_end]
    for alias, expected in (("apac", "Apac"), ("afr", "Afr")):
        shape = f'assert!(matches!(Region::from_str("{alias}"), Ok(Region::{expected})));'
        if alias_test.count(shape) != 1:
            raise VerificationError(f"B-320 {alias} success mapping is not exact")
    if "assert_eq!(Region::from_str" in alias_test:
        raise VerificationError("B-320 Result equality regression")
    if 'assert!(Region::from_str("").is_err());' not in alias_test:
        raise VerificationError("B-320 unknown-region negative assertion was lost")


def self_test() -> None:
    source = {path: _read(path, {}) for path in TARGETS}
    mutations = (
        {BILLING: source[BILLING].replace("writeln!(", "eprintln!(", 1)},
        {BILLING: source[BILLING].replace("p99_us={p99}", "p99_missing", 1)},
        {BILLING: source[BILLING].replace("assert!(p99 <= 5_000", "assert!(p99 <= u128::MAX", 1)},
        {REGION: source[REGION].replace('assert!(matches!(Region::from_str("apac"), Ok(Region::Apac)));', 'assert_eq!(Region::from_str("apac"), Ok(Region::Apac));', 1)},
        {REGION: source[REGION].replace('assert!(Region::from_str("").is_err());', "", 1)},
    )
    for index, mutation in enumerate(mutations, 1):
        try:
            verify(overrides=mutation)
        except VerificationError:
            continue
        raise VerificationError(f"self-test mutation {index} was accepted")


if __name__ == "__main__":
    try:
        verify()
        if "--self-test" in sys.argv[1:]:
            self_test()
    except (OSError, VerificationError) as exc:
        print(f"B-319/B-320 final Rust residuals: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    print("B-319/B-320 final Rust residuals: PASS")
