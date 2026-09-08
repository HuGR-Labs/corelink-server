#!/usr/bin/env python3
"""Fail-closed static contracts for the fourth D03 Rust residual bundle.

The bundle is deliberately checked without invoking Cargo.  Each contract is
anchored to the source shape that produced the diagnostic in the frozen
``cargo-clippy.log``/``cargo-test.log`` pair; broad project-wide grep is not a
substitute for checking the affected declaration and its surrounding syntax.
"""

from __future__ import annotations

import re
from types import MappingProxyType
from pathlib import Path
from typing import Callable

from rust_source_lexer import include_paths, mask as _code

ROOT = Path(__file__).resolve().parents[1]

ADVERSARIAL = "tests/e2e-tenant-isolation/tests/adversarial.rs"
MAIN = "crates/corelink-container/src/main.rs"
STORAGE_1 = "crates/corelink-container/src/storage/r2_s3_parts/tests_1.rs"
STORAGE_3 = "crates/corelink-container/src/storage/r2_s3_parts/tests_3.rs"
FAILOVER = "crates/corelink-container/src/routes/failover.rs"
OCI = "crates/corelink-container/src/routes/oci/b126_m2_impl_01.rs"
SLI = "crates/corelink-container/src/sli_aggregate.rs"
REVOCATION = "crates/corelink-container/src/byok_revocation_runtime.rs"
REVOCATION_TESTS = "crates/corelink-container/src/byok_revocation_runtime/part-01.rs"
ACCOUNTING = "crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs"
AUDIT_DRAIN = "crates/corelink-container/src/routes/audit_drain/b126_m2_impl_01.rs"
# The CAS route is compiled from include fragments.  Keep each handler family
# anchored to its real source unit so deleting or moving one declaration cannot
# be hidden by a marker that happens to survive in another fragment.
CAS_SINGLE = "crates/corelink-container/src/routes/cas/single_handlers.rs"
CAS_BATCH_WRITE = "crates/corelink-container/src/routes/cas/batch_write.rs"
CAS_BATCH_READ = "crates/corelink-container/src/routes/cas/batch_read.rs"
CAS_ERASE = "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs"
ADAPTER_CACHE = "crates/corelink-container/src/adapter_cache.rs"
CAPACITY = "crates/corelink-container/src/container_capacity.rs"
ORIGIN = "crates/corelink-container/src/origin_timing.rs"
OCI_TEST = "crates/corelink-container/src/routes/oci/b126_m2_test_1_2_part2.rs"
BILLING = "crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs"


class VerificationError(RuntimeError):
    """Raised when a residual target is missing, ambiguous, or regressed."""


# Exactly sixteen logical families, B-297 through B-312.  Paths are kept
# explicit so a missing target cannot silently shrink the contract population.
_FAMILY_PATHS_DATA = {
    "B-297 adversarial imports": (ADVERSARIAL,),
    "B-298 failover test-only accessors": (FAILOVER,),
    "B-299 OCI test-only constructor": (OCI,),
    "B-300 SLI lints": (SLI,),
    "B-301 BYOK revocation lints": (REVOCATION, REVOCATION_TESTS),
    "B-302 byte-accounting match scrutinee": (ACCOUNTING,),
    "B-303 audit-drain arity": (AUDIT_DRAIN,),
    "B-304 CAS handler arity": (CAS_SINGLE, CAS_BATCH_WRITE, CAS_BATCH_READ),
    "B-305 clamp idiom": (CAS_ERASE,),
    "B-306 named recording type": (ADAPTER_CACHE,),
    "B-307 R2/S3 test registration": (STORAGE_1, STORAGE_3),
    "B-308 capacity assertions": (CAPACITY,),
    "B-309 origin test lint policy": (ORIGIN,),
    "B-310 OCI runtime assertion": (OCI_TEST,),
    "B-311 billing property assertions": (BILLING,),
    "B-312 container test imports": (MAIN,),
}

# Keep the population immutable and retain a private, ordered snapshot.  The
# verifier must not derive its expected population from a structure that a
# caller can replace or mutate before invoking ``verify``.
FAMILY_PATHS = MappingProxyType(_FAMILY_PATHS_DATA)
CANONICAL_FAMILY_PATHS = tuple(FAMILY_PATHS.items())
CONTRACTS = CANONICAL_FAMILY_PATHS

EXPECTED_PATHS = frozenset(path for _, paths in CANONICAL_FAMILY_PATHS for path in paths)


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


def _one_attached_raw(source: str, pattern: str, name: str, label: str) -> re.Match[str]:
    """Match an exact raw attribute only when it owns the real declaration."""
    code_fn = _one_name(source, name, label + ":function")
    matches = [m for m in re.finditer(pattern, source, re.MULTILINE | re.DOTALL) if source.find(f"fn {name}", m.start(), m.end()) == code_fn.start()]
    if len(matches) != 1:
        raise VerificationError(f"{label}: expected one attached match, found {len(matches)}")
    return matches[0]


def _one_name(source: str, name: str, label: str) -> re.Match[str]:
    return _one(_code(source), rf"\bfn\s+{re.escape(name)}\s*\(", label)


def _attached_attrs(source: str, token_start: int) -> tuple[str, ...]:
    """Return the complete contiguous attribute block attached to a token."""
    code = _code(source)
    prefix = code[:token_start]
    match = re.search(
        r"(?P<attrs>(?:#\[[^\]]*\]\s*)+)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?$",
        prefix,
    )
    if not match:
        return ()
    # ``_code`` blanks literals, which is useful for matching but would erase
    # the canonical `multi_thread` value in tokio::test.  Slice the same
    # offsets from the original source when recovering attribute text.
    raw_attrs = source[match.start() : match.end()]
    return tuple(re.findall(r"#\[([^\]]*)\]", raw_attrs, re.DOTALL))


def _attr_key(attr: str) -> str:
    return re.sub(r"\s+", "", attr)


def _function_attrs(source: str, name: str, label: str) -> tuple[str, ...]:
    code = _code(source)
    fn = _one_name(code, name, label + ":function")
    return _attached_attrs(code, fn.start())


def _matching_brace(source: str, opening: int, label: str) -> int:
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    raise VerificationError(f"{label}: unclosed brace")


def _direct_assert(function_block: str, pattern: str, label: str) -> re.Match[str]:
    """Require an assertion as a direct executable statement in a function."""
    code = _code(function_block)
    matches = list(re.finditer(pattern, code, re.MULTILINE | re.DOTALL))
    if len(matches) != 1:
        raise VerificationError(f"{label}: expected one assertion, found {len(matches)}")
    match = matches[0]
    fn_start = code.find("fn ")
    body_open = code.find("{", fn_start)
    if fn_start < 0 or body_open < 0:
        raise VerificationError(f"{label}: missing function body")
    depth = code.count("{", body_open, match.start()) - code.count("}", body_open, match.start())
    if depth != 1:
        raise VerificationError(f"{label}: assertion must be direct in the test body")
    line_start = code.rfind("\n", 0, match.start()) + 1
    if code[line_start:match.start()].strip():
        raise VerificationError(f"{label}: assertion is not an executable statement")
    if re.search(r"#\[[^\]]*\]\s*$", code[:line_start]):
        raise VerificationError(f"{label}: assertion has a conditional/attribute wrapper")
    return match


def _imports(s: str, crate: str, label: str) -> str:
    candidates = list(re.finditer(rf"use\s+{re.escape(crate)}\s*::\s*\{{(?P<body>.*?)\}}\s*;", s, re.MULTILINE | re.DOTALL))
    matches = [m for m in candidates if s.count("{", 0, m.start()) == s.count("}", 0, m.start())]
    if len(matches) != 1:
        raise VerificationError(f"{label}: expected one depth-0 import, found {len(matches)}")
    m = matches[0]
    return m.group("body")


def _include_paths(source: str, label: str) -> tuple[str, ...]:
    """Return real include! paths, excluding comment/string bait."""
    paths = include_paths(source)
    if not paths:
        raise VerificationError(f"{label}: include! census is empty")
    return tuple(paths)


def _check_adversarial_import(sources: dict[str, str]) -> None:
    body = _imports(_code(sources[ADVERSARIAL]), "e2e_tenant_isolation", ADVERSARIAL)
    expected = {
        "AuditCapture", "CasStore", "CmkRotationLedger", "ConstantTimeAuthProbe",
        "DenyKind", "IdempotencyStore", "PatRevokeLedger", "PatStore",
        "QuotaStore", "RateLimiter", "StripeWebhookLedger", "TenantCtx",
    }
    imported = set(re.findall(r"\b[A-Z][A-Za-z0-9_]*\b", body))
    if imported != expected:
        raise VerificationError(f"B-297 imports: expected exact 12-symbol set, got {sorted(imported)}")

def _check_container_import(sources: dict[str, str]) -> None:
    main = _code(sources[MAIN])
    candidates = list(re.finditer(r"use\s+boot\s*::\s*\{(?P<body>.*?)\}\s*;", main, re.MULTILINE | re.DOTALL))
    boot_imports = [m for m in candidates if main.count("{", 0, m.start()) == main.count("}", 0, m.start())]
    if len(boot_imports) != 2:
        raise VerificationError(f"{MAIN}: expected one production and one cfg(test) boot import")
    attrs = [_attached_attrs(main, m.start()) for m in boot_imports]
    production = [m for m, block in zip(boot_imports, attrs) if not block]
    test_only = [m for m, block in zip(boot_imports, attrs) if tuple(map(_attr_key, block)) == ("cfg(test)",)]
    if len(production) != 1 or len(test_only) != 1:
        raise VerificationError(f"{MAIN}: boot imports must have exactly one bare production and one cfg(test) import")
    primary = production[0].group("body")
    for name in ("build_runners_resolver_from", "build_tier_selector_from"):
        if re.search(rf"\b{re.escape(name)}\b", primary):
            raise VerificationError(f"B-312 imports: test-only {name} remains in production import")
    test_body = test_only[0].group("body")
    for name in ("build_runners_resolver_from", "build_tier_selector_from"):
        if not re.search(rf"\b{re.escape(name)}\b", test_body):
            raise VerificationError(f"B-312 imports: test import lacks {name}")


def _check_region_test_registration(sources: dict[str, str]) -> None:
    source = _code(sources[STORAGE_1])
    fn = _one(source, r"\bfn\s+physical_cas_bucket_must_match_serving_region\s*\(", STORAGE_1)
    attrs = tuple(map(_attr_key, _attached_attrs(source, fn.start())))
    if attrs != ("test",):
        raise VerificationError(f"{STORAGE_1}: B-307 region test must have only #[test] directly attached")


def _check_byok_test_registration(sources: dict[str, str]) -> None:
    raw = sources[STORAGE_3]
    source = _code(raw)
    fn = _one(source, r"\bfn\s+byok_mode_b_read_fails_closed_when_kms_down\s*\(", STORAGE_3)
    attrs = tuple(map(_attr_key, _attached_attrs(raw, fn.start())))
    if attrs != ("tokio::test(flavor=\"multi_thread\",worker_threads=2)",):
        raise VerificationError(f"{STORAGE_3}: B-307 BYOK test must have only the canonical tokio::test")


def _check_failover_accessors(sources: dict[str, str]) -> None:
    source = _code(sources[FAILOVER])
    for name in ("stale_after_ms", "heartbeat"):
        _one_name(source, name, f"{FAILOVER}:{name}")
        attrs = tuple(map(_attr_key, _function_attrs(source, name, f"{FAILOVER}:{name}")))
        if attrs != ("cfg(test)", "must_use"):
            raise VerificationError(f"{FAILOVER}:{name}: only cfg(test), must_use may be attached")


def _check_oci_constructor(sources: dict[str, str]) -> None:
    source = _code(sources[OCI])
    _one_name(source, "with_allowlist", f"{OCI}:with_allowlist")
    attrs = tuple(map(_attr_key, _function_attrs(source, "with_allowlist", f"{OCI}:with_allowlist")))
    if attrs != ("cfg(test)",):
        raise VerificationError(f"{OCI}:with_allowlist: only cfg(test) may be attached")


def _check_sli_accessors(sources: dict[str, str]) -> None:
    source = _code(sources[SLI])
    for name in ("counters_at", "window_counters_at"):
        matches = list(re.finditer(rf"\bfn\s+{re.escape(name)}\s*\(", source))
        if not matches:
            raise VerificationError(f"{SLI}:{name}: function is missing")
        all_attrs = [tuple(map(_attr_key, _attached_attrs(source, match.start()))) for match in matches]
        test_indices = [index for index, attrs in enumerate(all_attrs) if "cfg(test)" in attrs]
        # The test-only forwarding accessor is the final same-named method;
        # requiring that identity prevents cfg(test) from being moved onto a
        # production implementation while leaving the accessor bare.
        if test_indices != [len(matches) - 1] or all_attrs[-1] != ("cfg(test)",):
            raise VerificationError(f"{SLI}:{name}: only cfg(test) may be attached")


def _check_must_use(sources: dict[str, str]) -> None:
    source = sources[REVOCATION]
    _one_name(source, "detector_for_client", REVOCATION + ":detector_for_client")
    _one_attached_raw(
        source,
        r"#\[must_use\s*=\s*\"handle the result to obtain the configured revocation detector\"\]\s*pub\s+fn\s+detector_for_client\s*\(",
        "detector_for_client",
        REVOCATION + ":detector_for_client",
    )
    _one(
        _code(source),
        r"#\[must_use\s*=\s*\s*\]\s*pub\s+fn\s+detector_for_client\s*\(",
        REVOCATION + ":detector_for_client attachment",
    )


def _check_blocks(sources: dict[str, str]) -> None:
    source = _code(sources[ACCOUNTING])
    _one(source, r"let\s+committed_len\s*=\s*\{.*?byok_committed_len\(.*?\)\s*\};\s*let\s+byte_len\s*=\s*match\s+committed_len\s*\{", ACCOUNTING)
    if re.search(r"let\s+byte_len\s*=\s*match\s*\{", source):
        raise VerificationError("B-302 blocks-in-conditions: match still has a block scrutinee")


def _check_argument_attrs(
    source: str,
    path: str,
    expected: dict[str, tuple[tuple[str, ...], str]],
) -> None:
    for name, (allowed_lints, reason) in expected.items():
        _one_name(source, name, f"{path}:{name}")
        lint_pattern = r"\s*,\s*".join(map(re.escape, allowed_lints))
        # The allow is attached to this declaration and is an exact, narrow
        # exception.  In particular, a broad clippy::all must not be smuggled
        # into a targeted arity exception.
        _one_attached_raw(
            source,
            rf"#\[allow\(\s*{lint_pattern}\s*,\s*reason\s*=\s*\"{re.escape(reason)}\"\s*\)\]\s*(?:pub\s+)?(?:async\s+)?fn\s+{name}\s*\(",
            name,
            f"{path}:{name}",
        )
        # Ensure the declaration attachment itself exists in comment/string-
        # stripped code; a matching comment must never satisfy this contract.
        _one(
            _code(source),
            rf"#\[allow\(\s*{lint_pattern}\s*,\s*reason\s*=\s*\s*\)\]\s*(?:pub\s+)?(?:async\s+)?fn\s+{name}\s*\(",
            f"{path}:{name} attachment",
        )
        code = _code(source)
        fn_start = _one(code, rf"\bfn\s+{name}\s*\(", f"{path}:{name} function").start()
        previous_fn = code.rfind("fn ", 0, fn_start)
        declaration_region = code[previous_fn if previous_fn >= 0 else 0 : fn_start]
        if len(re.findall(r"#\[allow\(", declaration_region)) != 1:
            raise VerificationError(f"{path}:{name}: more than one allow attribute is attached")
        if re.search(r"#\[allow\([^\]]*\bclippy::all\b", declaration_region):
            raise VerificationError(f"{path}:{name}: broad clippy::all is not permitted")


def _check_audit_arguments(sources: dict[str, str]) -> None:
    _check_argument_attrs(
        sources[AUDIT_DRAIN],
        AUDIT_DRAIN,
        {
            "canonical_head_v2_bytes": (("dead_code", "clippy::too_many_arguments"), "B-054 v2 writer is gated until external witness wiring lands"),
            "sign_head_v2": (("dead_code", "clippy::too_many_arguments"), "B-054 v2 writer is gated until external witness wiring lands"),
            "verify_head_v2": (("dead_code", "clippy::too_many_arguments"), "B-054 v2 verifier is gated until external witness wiring lands"),
        },
    )


def _check_cas_arguments(sources: dict[str, str]) -> None:
    # The split route now groups the three auth extractors into the named
    # `CasRequestAuth` tuple.  This is the load-bearing B-304 repair: it keeps
    # each handler's extractor boundary explicit without function-level
    # `too_many_arguments` exemptions.  Check every handler in its real source
    # fragment; a surviving marker in one fragment must not mask a moved or
    # regressed sibling.
    handlers = (
        (CAS_SINGLE, ("handle_write",)),
        (CAS_BATCH_WRITE, ("handle_batch_write",)),
        (CAS_BATCH_READ, ("handle_batch_read", "handle_batch_exists")),
    )
    for path, names in handlers:
        source = _code(sources[path])
        for name in names:
            fn = _one_name(source, name, f"{path}:{name}")
            body_open = source.find("{", fn.end())
            if body_open < 0:
                raise VerificationError(f"{path}:{name}: missing function body")
            signature = source[fn.start() : body_open]
            if "(auth, scope, headers): CasRequestAuth," not in signature:
                raise VerificationError(f"{path}:{name}: auth extractors are not grouped")
            attrs = _function_attrs(source, name, f"{path}:{name}")
            if any("clippy::too_many_arguments" in attr for attr in attrs):
                raise VerificationError(f"{path}:{name}: stale too_many_arguments exemption remains")


def _check_clamp(sources: dict[str, str]) -> None:
    source = _code(sources[CAS_ERASE])
    # Include the field delimiter so a suffix such as `.max(1)`, a cast, or
    # another method cannot preserve a substring while changing the value.
    _one(source, r"max_tenants\s*:\s*max_tenants\.clamp\(\s*1\s*,\s*DEFAULT_MAX_TENANT_BLOOMS\s*\)\s*,", CAS_ERASE)
    if ".max(1).min(DEFAULT_MAX_TENANT_BLOOMS)" in source:
        raise VerificationError("B-305 manual-clamp: old max/min chain remains")


def _check_map_or(sources: dict[str, str]) -> None:
    source = _code(sources[SLI])
    _one(source, r"\.front\(\)\s*\.is_some_and\(\s*\|bucket\|\s*now_ms\.saturating_sub\(bucket\.start_ms\)\s*>=\s*MAX_WINDOW_MS\s*\)", SLI + ":evict")
    if re.search(r"\.front\(\)\.map_or\(\s*false\s*,", source):
        raise VerificationError("B-300 unnecessary-map-or: map_or remains in evict")


def _check_type_complexity(sources: dict[str, str]) -> None:
    source = _code(sources[ADAPTER_CACHE])
    _one(source, r"type\s+WriteRecord\s*=\s*\(String,\s*String,\s*String,\s*bool\)\s*;", ADAPTER_CACHE)
    _one(source, r"writes\s*:\s*Arc<\s*Mutex<\s*Vec<\s*WriteRecord\s*>\s*>\s*>", ADAPTER_CACHE + ":writes")
    if "Vec<(String, String, String, bool)>" in source:
        raise VerificationError("B-306 type-complexity: inline tuple remains")


def _check_revocation_test_lints(sources: dict[str, str]) -> None:
    includes = _include_paths(sources[REVOCATION], REVOCATION)
    if includes != ("byok_revocation_runtime/part-01.rs",):
        raise VerificationError(f"{REVOCATION}: test fragment include census is stale")
    _one(_code(sources[REVOCATION_TESTS]), r"#\[cfg\(test\)\]\s*mod\s+tests\s*\{\s*#!\[allow\(clippy::expect_used,\s*clippy::indexing_slicing\)\]", REVOCATION_TESTS + ":tests lint scope")


def _check_origin_test_lints(sources: dict[str, str]) -> None:
    source = sources[ORIGIN]
    code = _code(source)
    _one(code, r"#\[cfg\(test\)\]\s*#\[allow\(\s*clippy::expect_used\s*,\s*reason\s*=\s*\s*\)\]\s*mod\s+tests_b279_bridge\s*\{", ORIGIN + ":bridge lint scope")
    if re.search(r"#\[cfg\(not\(test\)\)\].*?mod\s+tests_b279_bridge\s*\{", code, re.DOTALL):
        raise VerificationError(ORIGIN + ": bridge test module must be cfg(test)")


def _function_block(source: str, name: str) -> str:
    source = _code(source)
    start = source.find(f"fn {name}")
    if start < 0:
        raise VerificationError(f"missing function block: {name}")
    open_brace = source.find("{", start)
    if open_brace < 0:
        raise VerificationError(f"missing function body: {name}")
    depth = 0
    for index in range(open_brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    raise VerificationError(f"unclosed function body: {name}")


def _check_capacity_assertions(sources: dict[str, str]) -> None:
    capacity = sources[CAPACITY]
    deployed = _function_block(capacity, "deployed_budget_fits_with_runtime_reserve")
    peaks = _function_block(capacity, "batch_request_parse_peaks_are_inside_the_read_and_write_slices")
    _one(deployed, r"let\s+declared_bytes\s*=\s*DECLARED_MEMORY_BUDGET_BYTES\s*;", "B-308 deployed declared budget")
    _one(deployed, r"let\s+reserve_bytes\s*=\s*RUNTIME_MEMORY_RESERVE_BYTES\s*;", "B-308 deployed runtime reserve")
    _direct_assert(
        deployed,
        r"assert!\(\s*budget_fits\(\s*CONTAINER_MEMORY_BYTES\s*,\s*declared_bytes\s*,\s*reserve_bytes\s*,\s*CONTAINER_VCPU_MILLICORES\s*,?\s*\)\s*\)",
        "B-308 assertions-on-constants: deployed budget must assert its predicate directly",
    )
    _one(
        peaks,
        r"let\s+read_peak_bytes\s*=\s*CAS_READ_SINGLE_PEAK_BYTES\s*\+\s*CAS_READ_BATCH_PEAK_BYTES\s*;",
        "B-308 read peak calculation",
    )
    _direct_assert(
        peaks,
        r"assert!\(\s*budget_fits\(\s*CAS_READ_GLOBAL_BUDGET_BYTES\s*,\s*read_peak_bytes\s*,\s*0\s*,\s*CONTAINER_VCPU_MILLICORES\s*,?\s*\)\s*\)",
        "B-308 assertions-on-constants: batch peaks must assert their predicate directly",
    )
    if re.search(r"assert!\(\s*DECLARED_MEMORY_BUDGET_BYTES\s*\+", deployed):
        raise VerificationError("B-308 assertions-on-constants: raw deployed constant assert remains")
    if re.search(r"assert!\(\s*CAS_READ_SINGLE_PEAK_BYTES\s*\+", peaks):
        raise VerificationError("B-308 assertions-on-constants: raw peak constant assert remains")


def _check_oci_assertions(sources: dict[str, str]) -> None:
    oci = _function_block(sources[OCI_TEST], "append_chunk_respects_per_tenant_inflight_budget")
    setup = _one(oci, r"let\s+seeded_hog_budget\s*=\s*\{.*?tenant_inflight\.insert\(hog_key\.clone\(\),\s*OCI_MAX_INFLIGHT_BYTES_PER_TENANT\s*\);\s*tenant_inflight\.get\(&hog_key\)\.copied\(\).*?\};", OCI_TEST)
    seeded = _one(oci, r"assert_eq!\(\s*seeded_hog_budget\s*,\s*Some\(OCI_MAX_INFLIGHT_BYTES_PER_TENANT\)", OCI_TEST)
    if seeded.start() < setup.end():
        raise VerificationError(OCI_TEST + ": seeded budget assertion must follow seed/get setup")
    predicate = _direct_assert(
        oci,
        r"assert!\(\s*seeded_hog_budget\.is_some_and\(\s*\|bytes\|\s*bytes\s*<\s*OCI_MAX_INFLIGHT_BYTES\s*\)\s*,",
        OCI_TEST + ": predicate consumed directly by assert",
    )
    if predicate.start() < seeded.end():
        raise VerificationError(OCI_TEST + ": ordering predicate assertion must follow seeded budget assertion")
    if ".expect(" in oci:
        raise VerificationError("B-310 OCI assertion: expect_used regression remains")
    if re.search(r"assert!\(\s*OCI_MAX_INFLIGHT_BYTES_PER_TENANT\s*<\s*OCI_MAX_INFLIGHT_BYTES\s*\)", oci):
        raise VerificationError("B-310 assertions-on-constants: direct OCI constant assert remains")


def _check_billing(sources: dict[str, str]) -> None:
    source = _function_block(sources[BILLING], "prop_cas_decision_budget_and_correctness")
    code = _code(source)
    branch = _one(code, r"\bif\s+should_allow\s*\{", BILLING + ":decision branch")
    then_open = code.find("{", branch.start())
    then_close = _matching_brace(code, then_open, BILLING + ":Allow branch")
    else_match = _one(code, r"\belse\s*\{", BILLING + ":deny branch")
    if else_match.start() < then_close:
        raise VerificationError(BILLING + ": deny branch is not paired with should_allow")
    else_open = code.find("{", else_match.start())
    else_close = _matching_brace(code, else_open, BILLING + ":Deny branch")
    allow_body = "fn __allow_branch() {" + code[then_open + 1 : then_close] + "}"
    deny_body = "fn __deny_branch() {" + code[else_open + 1 : else_close] + "}"
    _direct_assert(
        allow_body,
        r"let\s+is_allow\s*=\s*matches!\(\s*outcome\.decision,\s*QuotaCasDecision::Allow\s*\{\s*\.\.\s*\}\s*\)\s*;\s*prop_assert!\(is_allow\)",
        BILLING + ":Allow",
    )
    _direct_assert(
        deny_body,
        r"let\s+is_deny\s*=\s*matches!\(\s*outcome\.decision,\s*QuotaCasDecision::Deny429\s*\{\s*\.\.\s*\}\s*\)\s*;\s*prop_assert!\(is_deny\)",
        BILLING + ":Deny429",
    )
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

# Canonical ordered ID -> paths -> check identity.  Function objects are kept
# here (rather than names or positions alone) so a duplicate/replaced/missing
# check cannot hide behind an unchanged population length.
CANONICAL_REGISTRY = tuple(
    (family_id, paths, check)
    for (family_id, paths), check in zip(CANONICAL_FAMILY_PATHS, CHECKS)
)


def _validate_registry() -> None:
    actual_families = tuple(FAMILY_PATHS.items())
    if actual_families != CANONICAL_FAMILY_PATHS or CONTRACTS != CANONICAL_FAMILY_PATHS:
        raise VerificationError("B-297..B-312 registry: family IDs/paths are not the canonical ordered population")
    actual_checks = tuple(CHECKS)
    if len(actual_checks) != len(CANONICAL_REGISTRY) or any(
        actual is not expected
        for actual, (_, _, expected) in zip(actual_checks, CANONICAL_REGISTRY)
    ):
        raise VerificationError("B-297..B-312 registry: check identities are not the canonical ordered population")
    actual_registry = tuple(
        (family_id, paths, check)
        for (family_id, paths), check in zip(actual_families, actual_checks)
    )
    if actual_registry != CANONICAL_REGISTRY:
        raise VerificationError("B-297..B-312 registry: IDs, paths, and checks are not canonically paired")


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    """Verify every target; overrides are hermetic mutation-test inputs."""
    _validate_registry()
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
