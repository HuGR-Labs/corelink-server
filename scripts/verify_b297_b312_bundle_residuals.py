#!/usr/bin/env python3
"""Fail-closed static contracts for the fourth D03 Rust residual bundle.

The bundle is deliberately checked without invoking Cargo.  Each contract is
anchored to the source shape that produced the diagnostic in the frozen
``cargo-clippy.log``/``cargo-test.log`` pair; broad project-wide grep is not a
substitute for checking the affected declaration and its surrounding syntax.
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[1]

ADVERSARIAL = "tests/e2e-tenant-isolation/tests/adversarial.rs"
MAIN = "crates/corelink-container/src/main.rs"
STORAGE_1 = "crates/corelink-container/src/storage/r2_s3_parts/tests_1.rs"
STORAGE_3 = "crates/corelink-container/src/storage/r2_s3_parts/tests_3.rs"
FAILOVER = "crates/corelink-container/src/routes/failover.rs"
OCI = "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs"
SLI = "crates/corelink-container/src/sli_aggregate.rs"
REVOCATION = "crates/corelink-container/src/byok_revocation_runtime.rs"
ACCOUNTING = "crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs"
AUDIT_DRAIN = "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs"
CAS_SINGLE = "crates/corelink-container/src/routes/cas/single.rs"
CAS_BATCH = "crates/corelink-container/src/routes/cas/batch.rs"
CAS_ERASE = "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs"
ADAPTER_CACHE = "crates/corelink-container/src/adapter_cache.rs"
CAPACITY = "crates/corelink-container/src/container_capacity.rs"
ORIGIN = "crates/corelink-container/src/origin_timing.rs"
OCI_TEST = "crates/corelink-container/src/routes/oci/b126_m2_test_1_2.rs"
BILLING = "crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs"


class VerificationError(RuntimeError):
    """Raised when a residual target is missing, ambiguous, or regressed."""


# Exactly sixteen logical families, B-297 through B-312.  Paths are kept
# explicit so a missing target cannot silently shrink the contract population.
FAMILY_PATHS = {
    "B-297 adversarial imports": (ADVERSARIAL,),
    "B-298 failover test-only accessors": (FAILOVER,),
    "B-299 OCI test-only constructor": (OCI,),
    "B-300 SLI lints": (SLI,),
    "B-301 BYOK revocation lints": (REVOCATION,),
    "B-302 byte-accounting match scrutinee": (ACCOUNTING,),
    "B-303 audit-drain arity": (AUDIT_DRAIN,),
    "B-304 CAS handler arity": (CAS_SINGLE, CAS_BATCH),
    "B-305 clamp idiom": (CAS_ERASE,),
    "B-306 named recording type": (ADAPTER_CACHE,),
    "B-307 R2/S3 test registration": (STORAGE_1, STORAGE_3),
    "B-308 capacity assertions": (CAPACITY,),
    "B-309 origin test lint policy": (ORIGIN,),
    "B-310 OCI runtime assertion": (OCI_TEST,),
    "B-311 billing property assertions": (BILLING,),
    "B-312 container test imports": (MAIN,),
}

CONTRACTS = tuple(FAMILY_PATHS.items())

EXPECTED_PATHS = frozenset(path for paths in FAMILY_PATHS.values() for path in paths)


def _read(root: Path, path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        value = overrides[path]
        if not isinstance(value, str):
            raise VerificationError(f"override for {path} is not text")
        return value
    target = root / path
    try:
        if target.is_symlink() or not target.is_file():
            raise VerificationError(f"missing/non-regular target: {path}")
        return target.read_text(encoding="utf-8")
    except OSError as exc:
        raise VerificationError(f"cannot read target: {path}") from exc


def _one(source: str, pattern: str, label: str) -> re.Match[str]:
    matches = list(re.finditer(pattern, source, re.MULTILINE | re.DOTALL))
    if len(matches) != 1:
        raise VerificationError(f"{label}: expected one match, found {len(matches)}")
    return matches[0]


def _one_name(source: str, name: str, label: str) -> re.Match[str]:
    return _one(source, rf"\bfn\s+{re.escape(name)}\s*\(", label)


def _imports(s: str, crate: str, label: str) -> str:
    m = _one(s, rf"use\s+{re.escape(crate)}\s*::\s*\{{(?P<body>.*?)\}}\s*;", label)
    return m.group("body")


def _check_adversarial_import(sources: dict[str, str]) -> None:
    body = _imports(sources[ADVERSARIAL], "e2e_tenant_isolation", ADVERSARIAL)
    forbidden = {
        "AuditChain", "AuditQueryEngine", "DsrIntake", "HierarchicalQuotaStore",
        "KvReplicatedPatStore", "MultipartBroker", "RegionRouter",
    }
    if forbidden & set(re.findall(r"\b[A-Z][A-Za-z0-9_]*\b", body)):
        raise VerificationError("B-297 imports: stale adversarial imports remain")
    for name in ("AuditCapture", "CasStore", "TenantCtx", "StripeWebhookLedger"):
        if not re.search(rf"\b{re.escape(name)}\b", body):
            raise VerificationError(f"B-297 imports: required {name} disappeared")

def _check_container_import(sources: dict[str, str]) -> None:
    main = sources[MAIN]
    boot_imports = list(re.finditer(r"(?P<cfg>#\[cfg\(test\)\]\s*)?use\s+boot\s*::\s*\{(?P<body>.*?)\}\s*;", main, re.MULTILINE | re.DOTALL))
    if len(boot_imports) != 2 or sum(bool(m.group("cfg")) for m in boot_imports) != 1:
        raise VerificationError(f"{MAIN}: expected one production and one cfg(test) boot import")
    primary = next(m.group("body") for m in boot_imports if not m.group("cfg"))
    for name in ("build_runners_resolver_from", "build_tier_selector_from"):
        if re.search(rf"\b{re.escape(name)}\b", primary):
            raise VerificationError(f"B-312 imports: test-only {name} remains in production import")
    test_body = _one(
        main,
        r"#\[cfg\(test\)\]\s*use\s+boot\s*::\s*\{(?P<body>.*?)\}\s*;",
        MAIN + ":test boot import",
    ).group("body")
    for name in ("build_runners_resolver_from", "build_tier_selector_from"):
        if not re.search(rf"\b{re.escape(name)}\b", test_body):
            raise VerificationError(f"B-312 imports: test import lacks {name}")


def _check_region_test_registration(sources: dict[str, str]) -> None:
    _one_name(sources[STORAGE_1], "physical_cas_bucket_must_match_serving_region", STORAGE_1)
    _one(sources[STORAGE_1], r"#\[test\]\s*fn\s+physical_cas_bucket_must_match_serving_region\s*\(", STORAGE_1)


def _check_byok_test_registration(sources: dict[str, str]) -> None:
    _one_name(sources[STORAGE_3], "byok_mode_b_read_fails_closed_when_kms_down", STORAGE_3)
    _one(sources[STORAGE_3], r"#\[tokio::test\([^\]]+\)\]\s*async\s+fn\s+byok_mode_b_read_fails_closed_when_kms_down\s*\(", STORAGE_3)


def _check_failover_accessors(sources: dict[str, str]) -> None:
    for name in ("stale_after_ms", "heartbeat"):
        _one_name(sources[FAILOVER], name, f"{FAILOVER}:{name}")
        _one(sources[FAILOVER], rf"#\[cfg\(test\)\]\s*#\[must_use\]\s*fn\s+{name}\s*\(", f"{FAILOVER}:{name}")


def _check_oci_constructor(sources: dict[str, str]) -> None:
    _one_name(sources[OCI], "with_allowlist", f"{OCI}:with_allowlist")
    _one(sources[OCI], r"#\[cfg\(test\)\]\s*fn\s+with_allowlist\s*\(", f"{OCI}:with_allowlist")


def _check_sli_accessors(sources: dict[str, str]) -> None:
    for name in ("counters_at", "window_counters_at"):
        _one(sources[SLI], rf"#\[cfg\(test\)\]\s*fn\s+{name}\s*\(", f"{SLI}:{name}")


def _check_must_use(sources: dict[str, str]) -> None:
    _one_name(sources[REVOCATION], "detector_for_client", REVOCATION + ":detector_for_client")
    _one(
        sources[REVOCATION],
        r"#\[must_use\s*=\s*\"[^\"]+\"\]\s*pub\s+fn\s+detector_for_client\s*\(",
        REVOCATION + ":detector_for_client",
    )


def _check_blocks(sources: dict[str, str]) -> None:
    source = sources[ACCOUNTING]
    _one(source, r"let\s+committed_len\s*=\s*\{.*?byok_committed_len\(.*?\)\s*\};\s*let\s+byte_len\s*=\s*match\s+committed_len\s*\{", ACCOUNTING)
    if re.search(r"let\s+byte_len\s*=\s*match\s*\{", source):
        raise VerificationError("B-302 blocks-in-conditions: match still has a block scrutinee")


def _check_argument_attrs(source: str, path: str, names: tuple[str, ...]) -> None:
    for name in names:
        _one_name(source, name, f"{path}:{name}")
        # The allow is attached to this declaration, with a non-empty reason.
        _one(
            source,
            rf"#\[allow\((?=[^\]]*clippy::too_many_arguments)(?=[^\]]*reason\s*=\s*\"[^\"]+\")[^\]]*\)\]\s*(?:async\s+)?fn\s+{name}\s*\(",
            f"{path}:{name}",
        )


def _check_audit_arguments(sources: dict[str, str]) -> None:
    _check_argument_attrs(sources[AUDIT_DRAIN], AUDIT_DRAIN, ("canonical_head_v2_bytes", "sign_head_v2", "verify_head_v2"))


def _check_cas_arguments(sources: dict[str, str]) -> None:
    _check_argument_attrs(sources[CAS_SINGLE], CAS_SINGLE, ("handle_write",))
    _check_argument_attrs(sources[CAS_BATCH], CAS_BATCH, ("handle_batch_write", "handle_batch_read", "handle_batch_exists"))


def _check_clamp(sources: dict[str, str]) -> None:
    source = sources[CAS_ERASE]
    _one(source, r"max_tenants\s*:\s*max_tenants\.clamp\(\s*1\s*,\s*DEFAULT_MAX_TENANT_BLOOMS\s*\)", CAS_ERASE)
    if ".max(1).min(DEFAULT_MAX_TENANT_BLOOMS)" in source:
        raise VerificationError("B-305 manual-clamp: old max/min chain remains")


def _check_map_or(sources: dict[str, str]) -> None:
    source = sources[SLI]
    _one(source, r"\.front\(\)\s*\.is_some_and\(\s*\|bucket\|\s*now_ms\.saturating_sub\(bucket\.start_ms\)\s*>=\s*MAX_WINDOW_MS\s*\)", SLI + ":evict")
    if re.search(r"\.front\(\)\.map_or\(\s*false\s*,", source):
        raise VerificationError("B-300 unnecessary-map-or: map_or remains in evict")


def _check_type_complexity(sources: dict[str, str]) -> None:
    source = sources[ADAPTER_CACHE]
    _one(source, r"type\s+WriteRecord\s*=\s*\(String,\s*String,\s*String,\s*bool\)\s*;", ADAPTER_CACHE)
    _one(source, r"writes\s*:\s*Arc<\s*Mutex<\s*Vec<\s*WriteRecord\s*>\s*>\s*>", ADAPTER_CACHE + ":writes")
    if "Vec<(String, String, String, bool)>" in source:
        raise VerificationError("B-306 type-complexity: inline tuple remains")


def _check_revocation_test_lints(sources: dict[str, str]) -> None:
    _one(sources[REVOCATION], r"#\[cfg\(test\)\]\s*#\[allow\(\s*clippy::expect_used\s*,\s*clippy::indexing_slicing\s*\)\]\s*mod\s+tests\s*\{", REVOCATION + ":tests lint scope")


def _check_origin_test_lints(sources: dict[str, str]) -> None:
    _one(sources[ORIGIN], r"#\[cfg\(test\)\]\s*#\[allow\(\s*clippy::expect_used\s*,.*?reason\s*=\s*\"[^\"]+\"\s*\)\]\s*mod\s+tests_b279_bridge\s*\{", ORIGIN + ":bridge lint scope")


def _function_block(source: str, name: str) -> str:
    start = source.find(f"fn {name}")
    if start < 0:
        raise VerificationError(f"missing function block: {name}")
    next_fn = re.search(r"\n\s*(?:pub\s+)?(?:async\s+)?fn\s+\w+", source[start + 3 :])
    end = start + 3 + next_fn.start() if next_fn else len(source)
    return source[start:end]


def _check_capacity_assertions(sources: dict[str, str]) -> None:
    capacity = sources[CAPACITY]
    deployed = _function_block(capacity, "deployed_budget_fits_with_runtime_reserve")
    peaks = _function_block(capacity, "batch_request_parse_peaks_are_inside_the_read_and_write_slices")
    for block, label in ((deployed, "deployed budget"), (peaks, "batch peaks")):
        if "budget_fits(" not in block:
            raise VerificationError(f"B-308 assertions-on-constants: {label} lacks budget_fits")
    if re.search(r"assert!\(\s*DECLARED_MEMORY_BUDGET_BYTES\s*\+", deployed):
        raise VerificationError("B-308 assertions-on-constants: raw deployed constant assert remains")
    if re.search(r"assert!\(\s*CAS_READ_SINGLE_PEAK_BYTES\s*\+", peaks):
        raise VerificationError("B-308 assertions-on-constants: raw peak constant assert remains")


def _check_oci_assertions(sources: dict[str, str]) -> None:
    oci = _function_block(sources[OCI_TEST], "append_chunk_respects_per_tenant_inflight_budget")
    _one(oci, r"let\s+seeded_hog_budget\s*=\s*\{.*?tenant_inflight\.insert\(hog_key\.clone\(\),\s*OCI_MAX_INFLIGHT_BYTES_PER_TENANT\).*?\};", OCI_TEST)
    _one(oci, r"assert_eq!\(\s*seeded_hog_budget\s*,\s*Some\(OCI_MAX_INFLIGHT_BYTES_PER_TENANT\)", OCI_TEST)
    _one(oci, r"seeded_hog_budget\.is_some_and\(\s*\|bytes\|\s*bytes\s*<\s*OCI_MAX_INFLIGHT_BYTES\s*\)", OCI_TEST)
    if ".expect(" in oci:
        raise VerificationError("B-310 OCI assertion: expect_used regression remains")
    if re.search(r"assert!\(\s*OCI_MAX_INFLIGHT_BYTES_PER_TENANT\s*<\s*OCI_MAX_INFLIGHT_BYTES\s*\)", oci):
        raise VerificationError("B-310 assertions-on-constants: direct OCI constant assert remains")


def _check_billing(sources: dict[str, str]) -> None:
    source = sources[BILLING]
    _one(source, r"let\s+is_allow\s*=\s*matches!\(\s*outcome\.decision,\s*QuotaCasDecision::Allow\s*\{\s*\.\.\s*\}\s*\)\s*;\s*prop_assert!\(is_allow\)", BILLING + ":Allow")
    _one(source, r"let\s+is_deny\s*=\s*matches!\(\s*outcome\.decision,\s*QuotaCasDecision::Deny429\s*\{\s*\.\.\s*\}\s*\)\s*;\s*prop_assert!\(is_deny\)", BILLING + ":Deny429")
    if re.search(r"prop_assert!\(\s*matches!\([^\n]*\{\s*\.\.\s*\}\s*\)\s*\)", source):
        raise VerificationError("B-311 billing diagnostics: format-bearing matches! remains inside prop_assert!")


def _check_b300(sources: dict[str, str]) -> None:
    _check_sli_accessors(sources)
    _check_map_or(sources)


def _check_b301(sources: dict[str, str]) -> None:
    _check_must_use(sources)
    _check_revocation_test_lints(sources)


def _check_b307(sources: dict[str, str]) -> None:
    _check_region_test_registration(sources)
    _check_byok_test_registration(sources)


CHECKS: tuple[Callable[[dict[str, str]], None], ...] = (
    _check_adversarial_import,
    _check_failover_accessors,
    _check_oci_constructor,
    _check_b300,
    _check_b301,
    _check_blocks,
    _check_audit_arguments,
    _check_cas_arguments,
    _check_clamp,
    _check_type_complexity,
    _check_b307,
    _check_capacity_assertions,
    _check_origin_test_lints,
    _check_oci_assertions,
    _check_billing,
    _check_container_import,
)

if len(CHECKS) != 16 or len(CONTRACTS) != 16:
    raise RuntimeError("B-297..B-312 contract population must contain exactly 16 families")


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    """Verify every target; overrides are hermetic mutation-test inputs."""
    overrides = {} if overrides is None else dict(overrides)
    unknown = set(overrides) - EXPECTED_PATHS
    if unknown:
        raise VerificationError(f"unknown/ambiguous override target(s): {sorted(unknown)}")
    sources = {path: _read(root, path, overrides) for path in EXPECTED_PATHS}
    for check in CHECKS:
        check(sources)


if __name__ == "__main__":
    try:
        verify()
    except VerificationError as exc:
        raise SystemExit(f"B-297..B-312 BROKEN: {exc}")
    print("B-297..B-312 D03 bundle residuals: PASS")
