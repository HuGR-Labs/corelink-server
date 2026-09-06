#!/usr/bin/env python3
"""Fail-closed verifier for B-251's deterministic quota-CAS test budget.

The source proof deliberately separates algorithmic correctness/complexity
from the ignored wall-clock probe. Mutation checks prove that neither the
bounded-work assertions nor the real measurement can be made vacuous.
"""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "crates/corelink-billing/src/quota/cas/config.rs"
TEST = ROOT / "crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs"


class VerificationError(RuntimeError):
    pass


def _read(path: Path) -> str:
    if not path.is_file():
        raise VerificationError(f"missing B-251 proof input: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def _function_body(source: str, name: str) -> str:
    marker = f"fn {name}"
    start = source.find(marker)
    if start < 0:
        raise VerificationError(f"missing B-251 function: {name}")
    opening = source.find("{", start)
    if opening < 0:
        raise VerificationError(f"function has no body: {name}")
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    raise VerificationError(f"unterminated function: {name}")


def assess(config: str, test: str) -> None:
    for required in (
        "pub const MAX_CONFIGURED_CAS_ATTEMPTS: u32 = 16;",
        "pub const MAX_CAS_AUDIT_EVENTS_PER_ATTEMPT: u32 = 2;",
        "pub const MAX_CAS_AUDIT_EVENTS_PER_DECISION: u32 =\n    MAX_CONFIGURED_CAS_ATTEMPTS * MAX_CAS_AUDIT_EVENTS_PER_ATTEMPT;",
        "const _: () = assert!(MAX_CAS_AUDIT_EVENTS_PER_DECISION <= u8::MAX as u32);",
    ):
        if required not in config:
            raise VerificationError(f"missing production budget invariant: {required}")

    deterministic = _function_body(test, "prop_cas_decision_budget_and_correctness")
    required_deterministic = (
        "used in 0u64..2_000",
        "quota in 1_000u64..2_000",
        "request_bytes in 1u64..50",
        "prop_assume!(used < quota);",
        "let would_use = used.checked_add(request_bytes).unwrap();",
        "let should_allow = would_use < quota;",
        "outcome.cas_attempts <= checker.config().max_cas_attempts()",
        "(audit.len() as u32) <= MAX_CAS_AUDIT_EVENTS_PER_DECISION",
        "audit.len() as u32, MAX_CAS_AUDIT_EVENTS_PER_ATTEMPT",
        "metrics.counter_total(QuotaCasMetricKind::CheckTotal), 1",
        "QuotaCasDecision::Allow { .. })",
        "QuotaCasDecision::Deny429 { .. })",
        "row.bytes_used, would_use",
        "row.bytes_used, used",
        "CasCommitSucceeded",
        "CasRetryAfterEmitted",
    )
    for required in required_deterministic:
        if required not in deterministic:
            raise VerificationError(f"deterministic CAS proof is incomplete: {required}")

    latency_start = test.find("fn real_latency_probe_under_5ms_p99")
    if latency_start < 0:
        raise VerificationError("missing isolated real latency probe")
    latency_prefix = test[max(0, latency_start - 240) : latency_start]
    latency = _function_body(test, "real_latency_probe_under_5ms_p99")
    for required in (
        "#[ignore = \"opt-in real latency measurement; excluded from general CI\"]",
        "std::time::Instant::now()",
        "Vec::with_capacity(1_000)",
        "for _ in 0..1_000",
        "samples.sort_unstable();",
        "let p99 = samples[(samples.len() * 99 / 100)",
        "assert!(p99 <= 5_000",
    ):
        if required not in latency_prefix + latency:
            raise VerificationError(f"isolated latency probe is incomplete: {required}")

    if "prop_check_duration_under_5ms_p99" in test:
        raise VerificationError("old wall-clock property remains in general test surface")


def mutation_checks(config: str, test: str) -> None:
    mutations = (
        (
            "budget assertion removed",
            test.replace(
                "prop_assert!((audit.len() as u32) <= MAX_CAS_AUDIT_EVENTS_PER_DECISION);",
                "prop_assert!(true);",
                1,
            ),
        ),
        (
            "allow correctness removed",
            test.replace(
                "prop_assert!(matches!(outcome.decision, QuotaCasDecision::Allow { .. }));",
                "prop_assert!(true);",
                1,
            ),
        ),
        (
            "deny correctness removed",
            test.replace(
                "prop_assert!(matches!(outcome.decision, QuotaCasDecision::Deny429 { .. }));",
                "prop_assert!(true);",
                1,
            ),
        ),
        (
            "ignored probe promoted to general CI",
            test.replace(
                '#[ignore = "opt-in real latency measurement; excluded from general CI"]\n',
                "",
                1,
            ),
        ),
        (
            "latency p99 order-statistic removed",
            test.replace("samples.sort_unstable();", "", 1),
        ),
        (
            "request case removed",
            test.replace(
                _function_body(test, "prop_cas_decision_budget_and_correctness"),
                _function_body(test, "prop_cas_decision_budget_and_correctness").replace(
                    "request_bytes in 1u64..50",
                    "request_bytes in 1u64..1",
                    1,
                ),
                1,
            ),
        ),
        (
            "production attempt budget weakened",
            config.replace(
                "MAX_CONFIGURED_CAS_ATTEMPTS: u32 = 16;",
                "MAX_CONFIGURED_CAS_ATTEMPTS: u32 = 1;",
                1,
            ),
        ),
        (
            "production audit-per-attempt budget weakened",
            config.replace(
                "MAX_CAS_AUDIT_EVENTS_PER_ATTEMPT: u32 = 2;",
                "MAX_CAS_AUDIT_EVENTS_PER_ATTEMPT: u32 = 1;",
                1,
            ),
        ),
    )
    for name, mutated in mutations:
        mutated_config = mutated if name.startswith("production ") else config
        mutated_test = test if name.startswith("production ") else mutated
        try:
            assess(mutated_config, mutated_test)
        except VerificationError:
            continue
        raise VerificationError(f"mutation unexpectedly passed: {name}")


def main() -> int:
    config = _read(CONFIG)
    test = _read(TEST)
    assess(config, test)
    mutation_checks(config, test)
    print("B-251 quota-CAS budget: PASS (deterministic invariants, isolated latency probe, mutations red)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except VerificationError as error:
        raise SystemExit(f"FAIL: {error}")
