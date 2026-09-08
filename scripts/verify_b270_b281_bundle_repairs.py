#!/usr/bin/env python3
"""Fail-closed structural contract for the final D03 Rust repair bundle."""

from __future__ import annotations

import re
from pathlib import Path

from rust_source_lexer import include_paths, marker_count, marker_present, mask, normal_string_literals


ROOT = Path(__file__).resolve().parents[1]
GC_PURGE_PART = "crates/corelink-container/src/gc_sweep/part-03.rs"
GC_PURGE_QUERY = "SELECT epoch, state, updated_at FROM gc_purge_intent"

INCLUDE_CENSUSES: tuple[tuple[str, tuple[str, ...]], ...] = (
    (
        "crates/corelink-container/src/routes/cas.rs",
        (
            "cas/foundation_core.rs", "cas/foundation_state.rs", "cas/single_setup.rs",
            "cas/single_handlers.rs", "cas/batch_write.rs", "cas/batch_read.rs",
            "cas/list_delete.rs", "cas/tests_core_part1.rs", "cas/tests_core_part2.rs",
            "cas/tests_batch_part1.rs", "cas/tests_batch_part2.rs",
            "cas/tests_batch_write_part2.rs", "cas/tests_edges.rs", "cas/tests_read_ceiling.rs",
        ),
    ),
    ("crates/corelink-container/src/gc_sweep.rs", (
        "gc_sweep/part-02.rs", "gc_sweep/part-03.rs", "gc_sweep/part-04.rs", "gc_sweep/part-01.rs",
    )),
    ("crates/corelink-container/src/routes/dsr/portal.rs", (
        "portal/part-00.rs", "portal/part-00-01.rs", "portal/part-01.rs", "portal/part-01-01.rs", "portal/part-02.rs",
    )),
    ("crates/corelink-container/src/routes/tier_select.rs", (
        "tier_select/part-00.rs", "tier_select/part-01.rs", "tier_select/part-02.rs",
    )),
    ("crates/corelink-container/src/routes/tier_select/part-00.rs", (
        "part-00-00.rs", "part-00-01.rs", "part-00-02.rs",
    )),
    ("crates/corelink-container/src/routes/tier_select/part-02.rs", (
        "part-02-00.rs", "part-02-01.rs",
    )),
    ("crates/corelink-container/src/byte_accounting.rs", (
        "byte_accounting/b126_m2_impl_01.rs", "byte_accounting/b126_m2_impl_02.rs",
    )),
    ("crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs", (
        "b126_m2_impl_01_part_02.rs",
    )),
    ("crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs", (
        "b126_m2_test_1_1.rs", "b126_m2_test_2_1.rs", "b126_m2_test_3_1.rs", "b126_m2_test_4_1.rs",
    )),
    ("crates/corelink-container/src/byte_accounting/b126_m2_test_3_1.rs", (
        "b126_m2_test_3_1_part_02.rs",
    )),
    ("crates/corelink-container/src/routes/oci.rs", (
        "oci/b126_m2_impl_01.rs", "oci/b126_m2_impl_01_part2.rs", "oci/b126_m2_impl_02.rs",
    )),
    ("crates/corelink-container/src/routes/oci/b126_m2_impl_02.rs", (
        "b126_m2_test_1_1.rs", "b126_m2_test_1_1_part2.rs", "b126_m2_test_1_2.rs",
        "b126_m2_test_1_2_part2.rs", "b126_m2_test_1_3.rs",
    )),
)

# Each marker is intentionally load-bearing. The mutation suite removes every
# one in turn, so comment-only or partial repairs cannot satisfy the guard.
CONTRACTS: tuple[tuple[str, tuple[str, ...], tuple[str, ...]], ...] = (
    ("tests/e2e-tenant-isolation/src/fakes/foundation.rs", ("use uuid::Uuid;",), ()),
    ("crates/corelink-container/src/adapter_pat.rs", (
        '#[path = "adapter_pat_crypto.rs"]\nmod adapter_pat_crypto;',
        '#[path = "adapter_pat_gate.rs"]\nmod adapter_pat_gate;',
        '#[path = "adapter_pat_lookup.rs"]\nmod adapter_pat_lookup;',
        '#[path = "adapter_pat_verifier.rs"]\nmod adapter_pat_verifier;',
    ), ()),
    ("crates/corelink-container/src/storage/r2_s3.rs", ("use super::{byok_cas, StorageEnv};",), ()),
    ("crates/corelink-gc/src/physical_delete.rs", (
        "pub enum PurgeStage {", "fn begin_purge(", "fn claim_retry_epoch(",
        "fn mark_r2_deleted(", "fn mark_r2_retry(", "fn finalize_purge(",
    ), ()),
    ("crates/corelink-container/src/gc_sweep/part-03.rs", (
        "fn begin_purge(",
        "fn claim_retry_epoch(", "fn mark_r2_deleted(", "fn mark_r2_retry(",
        "fn finalize_purge(",
    ), ()),
    ("crates/corelink-container/src/routes/dsr/portal/part-00.rs", (
        "super::super::d1util::d1_query_blocking",
    ), ()),
    ("crates/corelink-container/src/routes/dsr/portal/part-00-01.rs", (
        "super::super::access::run_access", "super::super::access::run_portability",
        "super::super::access::run_rectification", "super::super::build_audit_r2_client",
    ), ()),
    ("crates/corelink-container/src/routes/tier_select/part-00-00.rs", (
        "crate::routes::tier_select_store::D1HttpTierSelectStore",
        "crate::routes::tier_select_checkout::StripeCheckoutCreator",
        "crate::routes::tier_select_audit::TierSelectAuditAdapter",
    ), ()),
    ("crates/corelink-container/src/routes/tier_select/part-00-01.rs", (
        "crate::routes::admin::resolve_internal_auth_key",
    ), ()),
    ("tests/e2e-tenant-isolation/src/fakes/extended.rs", (), ("use corelink_audit::",)),
    ("tests/e2e-tenant-isolation/src/fakes/stores.rs", (), ("use corelink_audit::",)),
    ("crates/corelink-container/src/routes/audit_archive.rs", ("let row = Value::Object(row);",), ()),
    ("crates/corelink-container/src/routes/audit_drain/b126_m2_impl_02.rs", (
        "let row = Value::Object(row);", "parse_sealed_epoch_metadata(&row)?",
    ), ()),
    ("crates/corelink-container/src/origin_timing.rs", ("slot.replace(previous);",), ("previous.flatten()",)),
    ("crates/corelink-container/src/sli_aggregate.rs", (
        "let mut result: BTreeMap<Sli, SliCounters> = BTreeMap::new();",
    ), ()),
    ("crates/corelink-container/src/routes/public_pullthrough.rs", ("pub(crate) use implementation::*;",), ()),
    ("crates/corelink-container/src/storage/cas_write_fence.rs", (), ("use std::time::Duration;",)),
)

MODULE_PATHS: tuple[tuple[str, str, str], ...] = (
    ("crates/corelink-audit/src/events.rs", "tests", "tests.rs"),
    ("crates/corelink-billing-stripe-materializer/src/d1.rs", "tests", "tests.rs"),
    ("crates/corelink-byok/src/byok_aws/real.rs", "tests", "tests.rs"),
    ("crates/corelink-byok/src/byok_gcp/real.rs", "tests", "tests.rs"),
    ("crates/corelink-cf-bindings/src/d1_real.rs", "tests", "tests.rs"),
    ("crates/corelink-clerk-cf/src/clerk_health_do.rs", "tests", "tests.rs"),
    ("crates/corelink-enterprise-inquiry/src/ledger.rs", "tests", "tests.rs"),
    ("crates/corelink-gc/src/run.rs", "run_tests", "run_tests.rs"),
    ("crates/corelink-gc/src/sweep_runner.rs", "sweep_runner_tests", "sweep_runner_tests.rs"),
    ("crates/corelink-handler-cas/src/handler.rs", "tests", "tests.rs"),
    ("crates/corelink-ops/src/oncall/events.rs", "tests", "tests.rs"),
    ("crates/corelink-rate-headers/src/circuit.rs", "tests", "tests.rs"),
    ("crates/corelink-ratelimit/src/limiter.rs", "tests", "tests.rs"),
    ("crates/corelink-stripe-real/src/webhook_dispatch.rs", "tests", "tests.rs"),
    ("crates/corelink-telemetry/src/lighthouse.rs", "tests", "tests.rs"),
    ("crates/corelink-telemetry/src/logpush/redaction.rs", "tests", "tests.rs"),
    ("tests/e2e-user-journeys/src/journeys/adapters.rs", "adapters_auth", "adapters_auth.rs"),
)


class VerificationError(RuntimeError):
    """A D03 repair contract is absent or ambiguous."""


def _matching_delimiter(code: str, opening: int, opener: str, closer: str) -> int:
    depth = 0
    for index in range(opening, len(code)):
        if code[index] == opener:
            depth += 1
        elif code[index] == closer:
            depth -= 1
            if depth == 0:
                return index
    raise VerificationError(f"unterminated delimiter in {GC_PURGE_PART}: {opener}")


def _verify_gc_query(root: Path, overrides: dict[str, str]) -> None:
    """Require the SQL marker in begin_purge's real query argument."""
    source = _read(root, GC_PURGE_PART, overrides)
    code = mask(source)
    functions = list(re.finditer(r"\bfn\s+begin_purge\s*\(", code))
    if len(functions) != 1:
        raise VerificationError(f"{GC_PURGE_PART}: begin_purge declaration is missing or ambiguous")
    function = functions[0]
    opening = code.find("{", function.end())
    if opening < 0:
        raise VerificationError(f"{GC_PURGE_PART}: begin_purge body is missing")
    closing = _matching_delimiter(code, opening, "{", "}")
    literals = normal_string_literals(source)
    body = code[function.start() : closing]
    hits = 0
    for call in re.finditer(r"\bquery_sync\s*\(", body):
        call_opening = function.start() + call.end() - 1
        call_closing = _matching_delimiter(code, call_opening, "(", ")")
        for start, end, text in literals:
            if not (call_opening < start < end < call_closing and GC_PURGE_QUERY in text):
                continue
            prefix = code[call_opening + 1 : start]
            suffix = code[end:call_closing]
            if re.search(r"&self\.d1\s*,\s*$", prefix) and re.match(r"\s*,\s*&\[", suffix):
                hits += 1
    if hits != 1:
        raise VerificationError(
            f"{GC_PURGE_PART}: gc purge SQL marker is not the unique begin_purge query argument"
        )


def _read(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        return overrides[path]
    try:
        return (root / path).read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"missing input: {path}") from exc


def _verify_include_censuses(root: Path, overrides: dict[str, str]) -> None:
    for parent, expected in INCLUDE_CENSUSES:
        source = _read(root, parent, overrides)
        actual = include_paths(source)
        if actual != expected:
            raise VerificationError(
                f"{parent}: include census mismatch; expected {expected!r}, got {actual!r}"
            )
        parent_dir = Path(parent).parent
        for child in expected:
            child_path = str(parent_dir / child)
            _read(root, child_path, overrides)


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    overrides = overrides or {}
    _verify_include_censuses(root, overrides)
    _verify_gc_query(root, overrides)
    for path, required, forbidden in CONTRACTS:
        source = _read(root, path, overrides)
        for marker in required:
            if marker_count(source, marker) == 0:
                raise VerificationError(f"{path}: missing {marker!r}")
        for marker in forbidden:
            if marker_present(source, marker):
                raise VerificationError(f"{path}: forbidden {marker!r}")

    manifest_path = "crates/corelink-container/Cargo.toml"
    manifest = _read(root, manifest_path, overrides)
    rand_line = 'rand = "0.8"'
    if len(re.findall(r'(?m)^\s*rand = "0\.8"\s*$', manifest)) != 1:
        raise VerificationError("container rand dependency must occur exactly once")
    dev = manifest.find("[dev-dependencies]")
    if dev < 0 or manifest.find(rand_line) > dev:
        raise VerificationError("container rand must be a production dependency")

    for parent, module, sibling in MODULE_PATHS:
        source = _read(root, parent, overrides)
        binding = f'#[path = "{sibling}"]\nmod {module};'
        if marker_count(source, binding) != 1:
            raise VerificationError(f"{parent}: must bind sibling {sibling} exactly once")
        _read(root, str(Path(parent).parent / sibling), overrides)


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-270..B-281 BROKEN: {exc}")
    print("B-270..B-281 D03 bundle repairs: PASS")
