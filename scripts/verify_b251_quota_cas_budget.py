#!/usr/bin/env python3
"""Fail-closed verifier for B-251's deterministic quota-CAS test budget.

The source proof deliberately separates algorithmic correctness/complexity
from the ignored wall-clock probe. Mutation checks prove that neither the
bounded-work assertions nor the real measurement can be made vacuous.
"""

from __future__ import annotations

import ast
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "crates/corelink-billing/src/quota/cas/config.rs"
TEST = ROOT / "crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs"
RUNNER = ROOT / "scripts/run_b251_latency_probe.py"


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


def _python_function(source: str, name: str) -> str:
    try:
        tree = ast.parse(source)
    except SyntaxError as exc:
        raise VerificationError("B-251 runner is not valid Python") from exc
    for node in tree.body:
        if isinstance(node, ast.FunctionDef) and node.name == name:
            segment = ast.get_source_segment(source, node)
            if segment is None:
                break
            return segment
    raise VerificationError(f"missing B-251 runner function: {name}")


def assess(config: str, test: str, runner: str) -> None:
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
        # Keep each decision matcher and its assertion coupled.  The matcher
        # is bound to a named boolean in the cumulative source; requiring
        # both forms makes removal of the assertion fail closed as well.
        "let is_allow = matches!(outcome.decision, QuotaCasDecision::Allow { .. });",
        "prop_assert!(is_allow);",
        "let is_deny = matches!(outcome.decision, QuotaCasDecision::Deny429 { .. });",
        "prop_assert!(is_deny);",
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
        "let p99_rank = (samples.len() * 99 + 99) / 100;",
        "let p99 = samples[p99_rank - 1];",
        "B251_LATENCY_PROBE_JSON=",
        '\\"production_latency_measured\\":false',
        "assert!(p99 <= 5_000",
    ):
        if required not in latency_prefix + latency:
            raise VerificationError(f"isolated latency probe is incomplete: {required}")

    if "prop_check_duration_under_5ms_p99" in test:
        raise VerificationError("old wall-clock property remains in general test surface")

    compare = _python_function(runner, "compare_identity")
    parse = _python_function(runner, "parse_probe_output")
    run = _python_function(runner, "run_probe")
    write = _python_function(runner, "_atomic_write")
    invalidate = _python_function(runner, "_invalidate_output")
    for required in (
        'IDENTITY_FIELDS = ("seed", "failure", "blob")',
        'DEFAULT_D02_IDENTITY = ROOT / "reports/owner-actions/b251-d02-identity.json"',
        '"--ignored"',
        '"--exact"',
        '"--nocapture"',
        "SAMPLE_COUNT = 1_000",
        "LIMIT_US = 5_000",
    ):
        if required not in runner:
            raise VerificationError(f"B-251 runner contract is incomplete: {required}")
    for required in (
        "d02[field] != observed[field]",
        'raise ProbeError("D02 identity mismatch:',
    ):
        if required not in compare:
            raise VerificationError(f"B-251 identity comparison is incomplete: {required}")
    for required in (
        "len(records) != 1",
        'record["sample_count"] != SAMPLE_COUNT',
        'record["limit_us"] != LIMIT_US',
        'record["fixture"] != "InMemoryAtomicQuotaChecker"',
        'record["production_latency_measured"] is not False',
        "p99_us > LIMIT_US",
    ):
        if required not in parse:
            raise VerificationError(f"B-251 probe parsing is incomplete: {required}")
    identity_check = run.find("compare_identity(d02, observed)")
    revision_binding = run.find('committed_test_blob = _git_value("rev-parse", f"HEAD:{TEST_PATH}")')
    clean_source_check = run.find("working_test_blob != committed_test_blob")
    invalidate_call = run.find("_invalidate_output(output_path)")
    cargo_call = run.find("result = runner(")
    parse_call = run.find("probe = parse_probe_output(combined)")
    evidence_write = run.find("_atomic_write(output_path, evidence)")
    if not (
        0
        <= invalidate_call
        < identity_check
        < revision_binding
        < clean_source_check
        < cargo_call
        < parse_call
        < evidence_write
    ):
        raise VerificationError(
            "B-251 runner does not bind revision, identity, and measurement before evidence"
        )
    for required in ("tempfile.mkstemp", "os.fsync", "os.replace"):
        if required not in write:
            raise VerificationError(f"B-251 evidence write is not atomic: {required}")
    for required in ("path.is_symlink()", "not path.is_file()", "path.unlink(missing_ok=True)"):
        if required not in invalidate:
            raise VerificationError(f"B-251 stale evidence invalidation is incomplete: {required}")


def mutation_checks(config: str, test: str, runner: str) -> None:
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
                _function_body(test, "prop_cas_decision_budget_and_correctness"),
                _function_body(test, "prop_cas_decision_budget_and_correctness").replace(
                    "prop_assert!(is_allow);",
                    "prop_assert!(true);",
                    1,
                ),
                1,
            ),
        ),
        (
            "deny correctness removed",
            test.replace(
                _function_body(test, "prop_cas_decision_budget_and_correctness"),
                _function_body(test, "prop_cas_decision_budget_and_correctness").replace(
                    "prop_assert!(is_deny);",
                    "prop_assert!(true);",
                    1,
                ),
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
            "nearest-rank p99 weakened",
            test.replace(
                "let p99_rank = (samples.len() * 99 + 99) / 100;\n    let p99 = samples[p99_rank - 1];",
                "let p99 = samples[(samples.len() * 99 / 100).min(samples.len() - 1)];",
                1,
            ),
        ),
        (
            "latency inclusive boundary weakened",
            test.replace("assert!(p99 <= 5_000", "assert!(p99 < 5_000", 1),
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
            assess(mutated_config, mutated_test, runner)
        except VerificationError:
            continue
        raise VerificationError(f"mutation unexpectedly passed: {name}")

    runner_mutations = (
        (
            "identity comparison removed",
            runner.replace("compare_identity(d02, observed)", "pass  # comparison removed", 1),
        ),
        (
            "inclusive p99 runner boundary weakened",
            runner.replace("p99_us > LIMIT_US", "p99_us >= LIMIT_US", 1),
        ),
        (
            "production boundary removed",
            runner.replace('record["production_latency_measured"] is not False', "False", 1),
        ),
        (
            "atomic replace removed",
            runner.replace("os.replace(temporary, path)", "pass  # replace removed", 1),
        ),
        (
            "revision binding removed",
            runner.replace(
                "working_test_blob != committed_test_blob",
                "False  # revision binding removed",
                1,
            ),
        ),
        (
            "stale evidence invalidation removed",
            runner.replace("_invalidate_output(output_path)", "pass  # stale evidence retained", 1),
        ),
    )
    for name, mutated_runner in runner_mutations:
        try:
            assess(config, test, mutated_runner)
        except VerificationError:
            continue
        raise VerificationError(f"runner mutation unexpectedly passed: {name}")


def main() -> int:
    config = _read(CONFIG)
    test = _read(TEST)
    runner = _read(RUNNER)
    assess(config, test, runner)
    mutation_checks(config, test, runner)
    print(
        "B-251 quota-CAS budget: PASS "
        "(deterministic invariants; seed/failure/blob comparison; isolated 1000-sample p99<=5ms probe; mutations red)"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except VerificationError as error:
        raise SystemExit(f"FAIL: {error}")
