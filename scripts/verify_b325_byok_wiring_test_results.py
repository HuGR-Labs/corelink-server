#!/usr/bin/env python3
"""Fail-closed guard for the B-325 BYOK wiring-test error propagation."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "crates/corelink-byok/tests/byok_revocation_wiring.rs"
TESTS = (
    "revoked_active_key_reaches_the_real_kill_switch",
    "population_failure_is_not_coerced_to_no_keys",
    "an_unconfigured_scheduler_does_not_start_a_vacuous_loop",
    "zero_updated_rows_fail_closed_in_the_kill_switch",
    "revocation_alert_failure_reaches_the_cycle_result",
    "recovery_status_restore_and_audit_failures_reach_the_cycle_result",
)


class VerificationError(RuntimeError):
    """The reviewed result-propagation shape is missing or weakened."""


def _read(overrides: dict[str, str]) -> str:
    if TARGET in overrides:
        source = overrides[TARGET]
    else:
        path = ROOT / TARGET
        if path.is_symlink() or not path.is_file():
            raise VerificationError(f"missing/non-regular target: {TARGET}")
        source = path.read_text(encoding="utf-8")
    if not isinstance(source, str) or not source or len(source.encode()) > 100_000:
        raise VerificationError("invalid bounded target")
    return source


def verify(*, overrides: dict[str, str] | None = None) -> None:
    source = _read(overrides or {})
    forbidden = (".expect(", ".unwrap(", "panic!(", "unimplemented!(", "todo!(")
    if any(token in source for token in forbidden):
        raise VerificationError("panic-based result handling remains in the wiring test")
    alias = "type TestResult = Result<(), Box<dyn std::error::Error>>;"
    if source.count(alias) != 1:
        raise VerificationError("bounded test-result alias must occur exactly once")
    for name in TESTS:
        pattern = rf"async\s+fn\s+{re.escape(name)}\s*\(\s*\)\s*->\s*TestResult\s*\{{"
        if len(re.findall(pattern, source)) != 1:
            raise VerificationError(f"{name} must propagate errors through TestResult")
    if source.count("DekCache::new(300)?") != 4:
        raise VerificationError("all four cache constructions must propagate invalid TTL errors")
    if not re.search(r"run_one_cycle\(\)\s*\.await\?;", source):
        raise VerificationError("successful revocation cycle must propagate its error")
    if not re.search(r"current_status\(&key_id\)\.await\?,", source):
        raise VerificationError("status read must propagate its error")
    helper = r"\)\s*->\s*Result<RevocationDetector,\s*BYOKError>\s*\{\s*Ok\(RevocationDetector::new\("
    if len(re.findall(helper, source)) != 1:
        raise VerificationError("detector helper must propagate cache construction failure")


def self_test() -> None:
    source = _read({})
    mutations = (
        source.replace("DekCache::new(300)?", 'DekCache::new(300).expect("valid")', 1),
        source.replace("-> TestResult", "", 1),
        source.replace(".await?;", ".await;", 1),
        source.replace("Result<RevocationDetector, BYOKError>", "RevocationDetector", 1),
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
        print(f"B-325 BYOK wiring test results: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    print("B-325 BYOK wiring test results: PASS")
