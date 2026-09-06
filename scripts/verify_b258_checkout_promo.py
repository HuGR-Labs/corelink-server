#!/usr/bin/env python3
"""Fail-closed, stdlib-only contract for B-258 checkout promo coverage.

The verifier checks the executable customer-then-checkout runtime seam and the
wiremock test's closed request population.  It never invokes Cargo, a build, or
Stripe.  Source blob pins make the reviewed runtime and test evidence explicit;
mutation callers can bypass the pins to prove each semantic tooth independently.
"""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RUNTIME = Path("crates/corelink-stripe-real/src/client.rs")
TEST = Path("crates/corelink-stripe-real/tests/checkout_promo.rs")
RUNTIME_BLOB = "7b8651450b2f9eadaab2b352639cde881517a0ac"
TEST_BLOB = "0380c3c78d5dfeaa5e8aa842a5bc9c5ad7dac85f"


class ContractError(ValueError):
    pass


@dataclass(frozen=True)
class _RustLexed:
    """Rust code with comments and string literal bodies safely masked."""

    code: str
    literals: tuple[str, ...]

    def literal(self, token: str) -> str:
        match = re.fullmatch(r"STR(\d+)", token)
        if not match:
            raise ContractError(f"invalid Rust string token: {token}")
        index = int(match.group(1))
        try:
            return self.literals[index]
        except IndexError as exc:
            raise ContractError(f"unknown Rust string token: {token}") from exc


def _mask_region(out: list[str], source: str, start: int, end: int) -> None:
    """Mask a non-code region while retaining newlines for line-sensitive scans."""
    out.extend("\n" if char == "\n" else " " for char in source[start:end])


def _lex_rust(source: str) -> _RustLexed:
    """Lex enough Rust to ignore comments and normal/raw strings fail-closed.

    String literals become ``STR<n>`` tokens so structural checks can inspect a
    literal only when it is the argument of the expected call.  This prevents
    bait in comments or unrelated normal/raw strings from satisfying a grep.
    """
    out: list[str] = []
    literals: list[str] = []
    length = len(source)
    index = 0

    def add_literal(value: str, end: int) -> None:
        token = f"STR{len(literals)}"
        literals.append(value)
        out.append(token)
        # Keep multiline layout stable without exposing literal contents.
        _mask_region(out, source, index, end)

    while index < length:
        if source.startswith("//", index):
            end = source.find("\n", index)
            if end < 0:
                end = length
            _mask_region(out, source, index, end)
            index = end
            continue
        if source.startswith("/*", index):
            end = index + 2
            depth = 1
            while end < length and depth:
                if source.startswith("/*", end):
                    depth += 1
                    end += 2
                elif source.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            if depth:
                raise ContractError("unterminated Rust block comment")
            _mask_region(out, source, index, end)
            index = end
            continue

        # Raw strings: r"...", r#"..."#, and byte forms br#"..."#.
        if source.startswith("br", index):
            hash_start = index + 2
        elif source.startswith("r", index):
            hash_start = index + 1
        else:
            hash_start = -1
        if hash_start >= 0:
            hash_count = 0
            while hash_start + hash_count < length and source[hash_start + hash_count] == "#":
                hash_count += 1
            quote = hash_start + hash_count
            if quote < length and source[quote] == '"':
                terminator = '"' + ("#" * hash_count)
                body_start = quote + 1
                close = source.find(terminator, body_start)
                if close < 0:
                    raise ContractError("unterminated Rust raw string")
                add_literal(source[body_start:close], close + len(terminator))
                index = close + len(terminator)
                continue
        if source[index] == '"':
            end = index + 1
            escaped = False
            while end < length:
                char = source[end]
                if char == '"' and not escaped:
                    break
                if char == "\n" and not escaped:
                    raise ContractError("unterminated Rust normal string")
                if char == "\\" and not escaped:
                    escaped = True
                else:
                    escaped = False
                end += 1
            if end >= length:
                raise ContractError("unterminated Rust normal string")
            add_literal(source[index + 1:end], end + 1)
            index = end + 1
            continue

        out.append(source[index])
        index += 1
    return _RustLexed("".join(out), tuple(literals))


def _has_call_literal(lexed: _RustLexed, prefix: str, expected: str) -> bool:
    pattern = re.escape(prefix) + r"\(\s*(STR\d+)"
    return any(lexed.literal(match.group(1)) == expected for match in re.finditer(pattern, lexed.code))


def _count_call_literals(lexed: _RustLexed, prefix: str, expected: str) -> int:
    pattern = re.escape(prefix) + r"\(\s*(STR\d+)"
    return sum(lexed.literal(match.group(1)) == expected for match in re.finditer(pattern, lexed.code))


def _has_literal_after(lexed: _RustLexed, prefix: str, expected: str) -> bool:
    pattern = re.escape(prefix) + r"\s*,\s*(STR\d+)"
    return any(lexed.literal(match.group(1)) == expected for match in re.finditer(pattern, lexed.code))


def _has_body_contains_literal(lexed: _RustLexed, expected: str, *, negated: bool = False) -> bool:
    prefix = r"!\s*body\.contains" if negated else r"body\.contains"
    pattern = prefix + r"\(\s*(STR\d+)\s*\)"
    return any(lexed.literal(match.group(1)) == expected for match in re.finditer(pattern, lexed.code))


def _read(root: Path, relative: Path) -> str:
    path = root / relative
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise ContractError(f"required source is unreadable: {relative}") from exc


def _blob(root: Path, relative: Path) -> str:
    try:
        raw = (root / relative).read_bytes()
        # Match `git hash-object`: the blob header is part of the identity.
        return hashlib.sha1(f"blob {len(raw)}\0".encode() + raw).hexdigest()
    except OSError as exc:
        raise ContractError(f"required source blob is unreadable: {relative}") from exc


def _runtime_body(source: str) -> str:
    match = re.search(
        r"impl\s+StripeClient\s+for\s+StripeRealClient\s*\{(?P<body>.*?)\n\}\n(?:[ \t]*\n)*[ \t]*(?:fn|pub\s+fn)\s+parse_retry_after",
        source,
        re.DOTALL,
    )
    if not match:
        raise ContractError("StripeClient runtime implementation disappeared")
    return match.group("body")


def verify_sources(
    runtime: str,
    test: str,
    *,
    runtime_blob: str | None = None,
    test_blob: str | None = None,
) -> None:
    """Verify executable semantics; optional blob args are used by ``verify``."""
    runtime_lex = _lex_rust(runtime)
    runtime_code = runtime_lex.code
    runtime_body = _runtime_body(runtime_code)
    required_runtime = (
        'pub fn create_customer',
        'create_checkout_session_raw(req, &idem, &price_id, &promo, &created_customer.id)',
        'post_form::<CheckoutSessionObject>',
        'CheckoutPromo::AllowCodes',
    )
    for marker in required_runtime:
        if marker not in runtime_code:
            raise ContractError(f"runtime contract marker missing: {marker}")
    if not _has_call_literal(runtime_lex, 'post_form::<CustomerObject>', '/v1/customers'):
        raise ContractError("runtime customer endpoint literal is missing from its call")
    if not _has_call_literal(runtime_lex, 'post_form::<CheckoutSessionObject>', '/v1/checkout/sessions'):
        raise ContractError("runtime checkout endpoint literal is missing from its call")
    if not re.search(r"create_customer\(\s*(STR\d+)\s*,\s*req\.tenant_id\.as_str\(\)", runtime_code):
        raise ContractError("runtime customer creation is not tenant-scoped")
    if not any(
        runtime_lex.literal(match.group(1)) == ""
        for match in re.finditer(r"create_customer\(\s*(STR\d+)\s*,", runtime_code)
    ):
        raise ContractError("runtime customer creation must pass an empty email")
    if not re.search(r"form\.push\(\(\s*(STR\d+)\s*,\s*coupon_id", runtime_code):
        raise ContractError("runtime coupon form pair is missing")
    if not any(
        runtime_lex.literal(match.group(1)) == "discounts[0][coupon]"
        for match in re.finditer(r"form\.push\(\(\s*(STR\d+)\s*,", runtime_code)
    ):
        raise ContractError("runtime coupon form key drifted")
    customer_call = 'create_customer(STR'
    checkout_call = "create_checkout_session_raw(req, &idem, &price_id, &promo, &created_customer.id)"
    customer_pos = runtime_body.find(customer_call)
    checkout_pos = runtime_body.find(checkout_call)
    if customer_pos < 0 or checkout_pos < 0 or customer_pos >= checkout_pos:
        raise ContractError("runtime must create the customer before checkout")
    if _count_call_literals(runtime_lex, 'post_form::<CustomerObject>', '/v1/customers') != 1:
        raise ContractError("runtime customer endpoint population drifted")
    if _count_call_literals(runtime_lex, 'post_form::<CheckoutSessionObject>', '/v1/checkout/sessions') != 1:
        raise ContractError("runtime checkout endpoint population drifted")

    test_lex = _lex_rust(test)
    test_code = test_lex.code
    for name in (
        "default_checkout_sends_allow_promotion_codes",
        "configured_coupon_preapplies_discount",
        "mount_checkout_mocks",
        "assert_checkout_request_sequence",
    ):
        if test_code.count(name) < 1:
            raise ContractError(f"checkout test marker missing: {name}")
    if test_code.count("mount_checkout_mocks(&server).await") != 2:
        raise ContractError("both promo cases must install the customer and checkout mocks")
    if _count_call_literals(test_lex, '.and(path', '/v1/customers') != 1:
        raise ContractError("customer mock endpoint is missing or duplicated")
    if _count_call_literals(test_lex, '.and(path', '/v1/checkout/sessions') != 1:
        raise ContractError("checkout mock endpoint is missing or duplicated")
    if not re.search(r"assert_eq!\(\s*received\.len\(\),\s*2", test_code):
        raise ContractError("request population assertion missing")
    if not _has_literal_after(test_lex, 'assert_eq!(received[0].url.path()', '/v1/customers'):
        raise ContractError("customer request order assertion missing")
    if not _has_literal_after(test_lex, 'assert_eq!(received[1].url.path()', '/v1/checkout/sessions'):
        raise ContractError("checkout request order assertion missing")
    required_sequence = (
        'String::from_utf8(received[1].body.clone())',
    )
    for marker in required_sequence:
        if marker not in test_code:
            raise ContractError(f"request/promo assertion missing: {marker}")
    if not _has_body_contains_literal(test_lex, 'allow_promotion_codes=true'):
        raise ContractError("default promotion assertion missing")
    if not _has_body_contains_literal(test_lex, 'discounts') or not _has_body_contains_literal(test_lex, 'discounts', negated=True):
        raise ContractError("default/coupon discount mutual exclusion assertion missing")
    if not _has_body_contains_literal(test_lex, 'coupon_LAUNCH100'):
        raise ContractError("coupon assertion missing")
    if not _has_body_contains_literal(test_lex, 'allow_promotion_codes', negated=True):
        raise ContractError("coupon promotion mutual exclusion assertion missing")
    if runtime_blob is not None and runtime_blob != RUNTIME_BLOB:
        raise ContractError(f"runtime source blob drifted: {runtime_blob}")
    if test_blob is not None and test_blob != TEST_BLOB:
        raise ContractError(f"checkout test source blob drifted: {test_blob}")


def verify(root: Path = ROOT) -> None:
    runtime = _read(root, RUNTIME)
    test = _read(root, TEST)
    verify_sources(
        runtime,
        test,
        runtime_blob=_blob(root, RUNTIME),
        test_blob=_blob(root, TEST),
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    try:
        verify(args.root.resolve())
    except ContractError as exc:
        print(f"B258 RED: {exc}", file=sys.stderr)
        return 1
    print("B258 checkout promo contract: PASS (customer then checkout; 2/2 wire cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
