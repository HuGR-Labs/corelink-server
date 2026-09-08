#!/usr/bin/env python3
"""Fail-closed structural guard for the four B-318 Clippy repairs."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HANDLER = "crates/corelink-handler-customer/tests/handler_customer.rs"
NPM = "crates/corelink-adapter-host/src/npm/upstream.rs"
OCI = "crates/corelink-adapter-host/src/oci/server/core.rs"
TARGETS = (HANDLER, NPM, OCI)
MAX_BYTES = 300_000


class VerificationError(RuntimeError):
    """A reviewed B-318 invariant is missing or ambiguous."""


def _read(path: str, overrides: dict[str, str]) -> str:
    if path in overrides:
        text = overrides[path]
    else:
        candidate = ROOT / path
        if candidate.is_symlink() or not candidate.is_file():
            raise VerificationError(f"missing/non-regular target: {path}")
        text = candidate.read_text(encoding="utf-8")
    if not isinstance(text, str) or not text or len(text.encode()) > MAX_BYTES:
        raise VerificationError(f"invalid bounded target: {path}")
    return text


def verify(*, overrides: dict[str, str] | None = None) -> None:
    supplied = overrides or {}
    handler = _read(HANDLER, supplied)
    npm = _read(NPM, supplied)
    oci = _read(OCI, supplied)

    closure_start = handler.find("let assert_denied = |before: usize, expected: AuditEventKind| {")
    closure_end = handler.find("\n    };", closure_start)
    if closure_start < 0 or closure_end < 0:
        raise VerificationError("handler denial assertion closure is missing")
    denial_closure = handler[closure_start:closure_end]
    if denial_closure.count("assert!(sli.snapshot().unwrap().last().unwrap().is_error);") != 1:
        raise VerificationError("handler denial path lost its boolean assertion")
    if re.search(r"assert_eq!\s*\([^;]*is_error\s*,\s*true\s*\)", denial_closure):
        raise VerificationError("handler boolean-comparison lint regression")

    positive = "const _: () = assert!(NPM_METADATA_MAX_RESPONSE_BYTES > 0);"
    admission = (
        "assert!(NPM_METADATA_MAX_RESPONSE_BYTES > "
        "crate::npm::config::DEFAULT_METADATA_CACHE_MAX_BYTES);"
    )
    compact = re.sub(r"\s+", " ", npm)
    if npm.count(positive) != 1:
        raise VerificationError("NPM positive response-cap invariant is not unique")
    if compact.count(admission) != 1 or "const _: () = assert!(NPM_METADATA_MAX_RESPONSE_BYTES > crate::npm::config::DEFAULT_METADATA_CACHE_MAX_BYTES);" not in compact:
        raise VerificationError("NPM cache-admission invariant is not compile-time load-bearing")
    if re.search(r"#\s*!?\[\s*allow\s*\([^]]*(?:assertions_on_constants|bool_assert_comparison|items_after_test_module)", handler + npm + oci):
        raise VerificationError("targeted Clippy lint suppression is forbidden")

    function = oci.find("pub fn err_response(")
    module = oci.find("#[cfg(test)]\nmod tests {")
    if function < 0 or module < 0 or oci.count("#[cfg(test)]\nmod tests {") != 1:
        raise VerificationError("OCI function/test module population is ambiguous")
    if module < function:
        raise VerificationError("OCI test module precedes a production item")
    tail = oci[module:]
    if "live_storage_failures_are_retryable_503s" not in tail:
        raise VerificationError("OCI regression test was not preserved")


def self_test() -> None:
    source = {path: _read(path, {}) for path in TARGETS}
    mutations = (
        {HANDLER: source[HANDLER].replace("assert!(sli.snapshot().unwrap().last().unwrap().is_error);", "assert_eq!(sli.snapshot().unwrap().last().unwrap().is_error, true);", 1)},
        {NPM: source[NPM].replace("const _: () = assert!(NPM_METADATA_MAX_RESPONSE_BYTES > 0);", "assert!(NPM_METADATA_MAX_RESPONSE_BYTES > 0);", 1)},
        {NPM: source[NPM].replace("const _: () =\n    assert!(NPM_METADATA_MAX_RESPONSE_BYTES > crate::npm::config::DEFAULT_METADATA_CACHE_MAX_BYTES);", "", 1)},
        {OCI: source[OCI].replace("#[cfg(test)]\nmod tests {", "#[cfg(test)]\nmod tests_removed {", 1)},
        {NPM: "#[allow(clippy::assertions_on_constants)]\n" + source[NPM]},
    )
    for index, mutation in enumerate(mutations, 1):
        if all(mutation.get(path, source[path]) == source[path] for path in TARGETS):
            raise VerificationError(f"self-test mutation {index} changed nothing")
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
        print(f"B-318 Clippy residuals: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    print("B-318 Clippy residuals: PASS")
