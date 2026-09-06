#!/usr/bin/env python3
"""Static guard for the DPA acceptance property-test RSA fixture.

This guard intentionally reads source only.  It keeps the expensive 2048-bit
key generation behind the test-only ``OnceLock`` fixture and keeps the
idempotency property at its canonical default of 32 cases.  Rust comments are
removed before checking so commented-out code cannot satisfy the contract.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import NoReturn

ROOT = Path(__file__).resolve().parent.parent
COMMON = ROOT / "crates/corelink-dpa-acceptance/tests/common.rs"
IDEMPOTENCY = ROOT / "crates/corelink-dpa-acceptance/tests/prop_idempotency.rs"


class VerificationError(RuntimeError):
    """A source contract is missing or has been weakened."""


def fail(message: str) -> NoReturn:
    raise VerificationError(message)


def read(path: Path) -> str:
    if not path.is_file():
        fail(f"missing DPA fixture proof object: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def strip_rust_comments(source: str) -> str:
    """Remove Rust comments while preserving literals and line structure."""

    out: list[str] = []
    i = 0
    state = "code"
    raw_hashes = 0

    while i < len(source):
        if state == "line":
            if source[i] == "\n":
                out.append("\n")
                state = "code"
            else:
                out.append(" ")
            i += 1
            continue
        if state == "block":
            if source.startswith("*/", i):
                out.extend((" ", " "))
                i += 2
                state = "code"
            else:
                out.append("\n" if source[i] == "\n" else " ")
                i += 1
            continue
        if state in {"string", "char"}:
            quote = '"' if state == "string" else "'"
            out.append(source[i])
            if source[i] == "\\" and i + 1 < len(source):
                out.append(source[i + 1])
                i += 2
                continue
            if source[i] == quote:
                state = "code"
            i += 1
            continue
        if state == "raw":
            closing = '"' + ("#" * raw_hashes) + ")"
            if source.startswith(closing, i):
                out.extend(closing)
                i += len(closing)
                state = "code"
            else:
                out.append(source[i])
                i += 1
            continue

        if source.startswith("//", i):
            out.extend((" ", " "))
            i += 2
            state = "line"
            continue
        if source.startswith("/*", i):
            out.extend((" ", " "))
            i += 2
            state = "block"
            continue
        if source[i] == '"':
            out.append(source[i])
            i += 1
            state = "string"
            continue
        if source[i] == "'":
            out.append(source[i])
            i += 1
            state = "char"
            continue
        if source[i] == "r":
            match = re.match(r'r(#+)?"', source[i:])
            if match:
                raw_hashes = len(match.group(1) or "")
                token = match.group(0)
                out.append(token)
                i += len(token)
                state = "raw"
                continue
        out.append(source[i])
        i += 1

    return "".join(out)


def validate_source(common: str, idempotency: str) -> None:
    common_code = strip_rust_comments(common)
    idempotency_code = strip_rust_comments(idempotency)

    required_common = (
        "use std::sync::{Mutex, OnceLock};",
        "static PEM: OnceLock<(String, String)> = OnceLock::new();",
        ".get_or_init(||",
        "(keys.private.0, keys.public.0)",
        "fn fixture_keys() -> Keys",
        "let keys = fixture_keys();",
        "fn fixture_is_rsa_2048_and_pem_shaped()",
        "from_pkcs1_pem",
        "from_public_key_pem",
        "private.size() * 8, 2048",
        "public.size() * 8, 2048",
    )
    for marker in required_common:
        if marker not in common_code:
            fail(f"DPA fixture guard missing active common.rs marker: {marker!r}")

    if common_code.count("RsaPrivateKey::new(&mut rng, 2048)") != 1:
        fail("DPA fixture guard requires exactly one 2048-bit keygen expression")

    build_start = common_code.find("pub fn build_service(")
    build_end = common_code.find("#[cfg(test)]", build_start)
    if build_start < 0 or build_end < 0:
        fail("DPA fixture guard cannot locate build_service boundary")
    build_body = common_code[build_start:build_end]
    if "gen_keys(" in build_body:
        fail("DPA fixture guard forbids keygen in build_service")
    if "fixture_keys()" not in build_body:
        fail("DPA fixture guard requires build_service to use fixture_keys")

    required_idempotency = (
        "fn proptest_cases() -> u32",
        "unwrap_or(32)",
        "ProptestConfig::with_cases(proptest_cases())",
        "fn idempotent_retry_returns_same_jti",
        "fn idempotency_conflict_on_diverging_payload",
        "fn distinct_signups_get_distinct_records",
    )
    for marker in required_idempotency:
        if marker not in idempotency_code:
            fail(f"DPA idempotency guard missing active marker: {marker!r}")

    if not re.search(r"unwrap_or\(\s*32\s*\)", idempotency_code):
        fail("DPA idempotency guard requires the canonical 32-case default")
    if re.search(r"with_cases\(\s*(?:[0-9]|[12][0-9]|30|31)\s*\)", idempotency_code):
        fail("DPA idempotency guard detected a reduced literal case count")


def mutation_checks(common: str, idempotency: str) -> int:
    """Prove comments cannot hide keygen or property-case reductions."""

    mutations = (
        (
            "comment out OnceLock",
            common.replace(
                "static PEM: OnceLock<(String, String)> = OnceLock::new();",
                "// static PEM: OnceLock<(String, String)> = OnceLock::new();",
                1,
            ),
            idempotency,
            "OnceLock",
        ),
        (
            "keygen per service",
            common.replace("let keys = fixture_keys();", "let keys = gen_keys();", 1),
            idempotency,
            "keygen in build_service",
        ),
        (
            "reduce default cases",
            common,
            idempotency.replace("unwrap_or(32)", "unwrap_or(16)", 1),
            "unwrap_or(32)",
        ),
        (
            "comment out proptest case wiring",
            common,
            idempotency.replace(
                "ProptestConfig::with_cases(proptest_cases())",
                "// ProptestConfig::with_cases(proptest_cases())",
                1,
            ),
            "with_cases",
        ),
    )
    rejected = 0
    for name, mutant_common, mutant_idempotency, expected in mutations:
        try:
            validate_source(mutant_common, mutant_idempotency)
        except VerificationError as error:
            if expected not in str(error):
                fail(f"DPA mutation {name} failed for the wrong reason: {error}")
            rejected += 1
        else:
            fail(f"DPA mutation unexpectedly passed: {name}")
    return rejected


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true", help="run mutation teeth")
    args = parser.parse_args()
    try:
        common = read(COMMON)
        idempotency = read(IDEMPOTENCY)
        validate_source(common, idempotency)
        if args.self_test:
            rejected = mutation_checks(common, idempotency)
            print(f"DPA fixture guard: {rejected}/4 mutations rejected")
        else:
            print("DPA fixture guard: source contract valid")
        return 0
    except VerificationError as error:
        print(f"DPA fixture guard DRIFTED: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
