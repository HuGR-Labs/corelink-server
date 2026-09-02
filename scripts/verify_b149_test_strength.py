#!/usr/bin/env python3
"""Structural, dependency-free verifier for the B-149 test-strength backlog item."""
from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


AUDIT = "crates/corelink-container/src/storage/d1_audit_sink/tests_batch_limits.rs"
AUTH = "crates/corelink-container/src/routes/billing_ingest/tests_auth.rs"
VALIDATE = "crates/corelink-container/src/routes/billing_ingest/tests_validate_record.rs"
SKIP = "crates/corelink-container/src/routes/billing_ingest/tests_record_skip.rs"
INGEST = "crates/corelink-container/src/routes/billing_ingest.rs"
REQUIRED = (AUDIT, AUTH, VALIDATE, SKIP, INGEST)


class InstrumentError(RuntimeError):
    """The source shape prevents this verifier from making a trustworthy verdict."""


@dataclass(frozen=True)
class Token:
    kind: str
    value: str
    start: int
    end: int


def lex(source: str, label: str) -> list[Token]:
    """Lex just enough Rust to safely locate code scopes and string literals."""
    out: list[Token] = []
    i, size = 0, len(source)
    while i < size:
        if source[i].isspace():
            i += 1
            continue
        if source.startswith("//", i):
            i = source.find("\n", i)
            if i < 0:
                break
            continue
        if source.startswith("/*", i):
            start, depth = i, 1
            i += 2
            while i < size and depth:
                if source.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif source.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
            if depth:
                raise InstrumentError(f"{label}: unclosed block comment at offset {start}")
            continue
        raw = re.match(r"(?:br|rb|r)(#{0,32})\"", source[i:])
        if raw:
            start, hashes = i, raw.group(1)
            i += len(raw.group(0))
            end_mark = '"' + hashes
            end = source.find(end_mark, i)
            if end < 0:
                raise InstrumentError(f"{label}: unclosed raw string at offset {start}")
            out.append(Token("string", source[i:end], start, end + len(end_mark)))
            i = end + len(end_mark)
            continue
        if source[i] == '"':
            start, i = i, i + 1
            content: list[str] = []
            while i < size:
                if source[i] == "\\":
                    if i + 1 >= size:
                        raise InstrumentError(f"{label}: unclosed string at offset {start}")
                    content.extend(source[i:i + 2])
                    i += 2
                elif source[i] == '"':
                    i += 1
                    break
                else:
                    content.append(source[i])
                    i += 1
            else:
                raise InstrumentError(f"{label}: unclosed string at offset {start}")
            out.append(Token("string", "".join(content), start, i))
            continue
        if source[i] == "'":  # Char literals matter because they may contain braces.
            end = i + 1
            while end < size and source[end] not in "\n\r":
                if source[end] == "\\":
                    end += 2
                elif source[end] == "'":
                    end += 1
                    out.append(Token("char", source[i:end], i, end))
                    i = end
                    break
                else:
                    end += 1
            else:  # A lifetime, not a char literal.
                out.append(Token("symbol", "'", i, i + 1))
                i += 1
            continue
        if source[i].isalpha() or source[i] == "_":
            start, i = i, i + 1
            while i < size and (source[i].isalnum() or source[i] == "_"):
                i += 1
            out.append(Token("ident", source[start:i], start, i))
            continue
        if source[i].isdigit():
            start, i = i, i + 1
            while i < size and (source[i].isalnum() or source[i] in "_."):
                i += 1
            out.append(Token("number", source[start:i], start, i))
            continue
        out.append(Token("symbol", source[i], i, i + 1))
        i += 1
    _check_braces(out, label)
    return out


def _check_braces(tokens: Iterable[Token], label: str) -> None:
    depth = 0
    for token in tokens:
        if token.value == "{":
            depth += 1
        elif token.value == "}":
            depth -= 1
            if depth < 0:
                raise InstrumentError(f"{label}: unmatched closing brace at offset {token.start}")
    if depth:
        raise InstrumentError(f"{label}: unmatched opening brace")


def _body(tokens: list[Token], name: str, label: str) -> list[Token]:
    starts = [i for i in range(len(tokens) - 1) if tokens[i].value == "fn" and tokens[i + 1].value == name]
    if len(starts) != 1:
        raise InstrumentError(f"{label}: expected exactly one function {name}, found {len(starts)}")
    start = starts[0] + 2
    try:
        left = next(i for i in range(start, len(tokens)) if tokens[i].value == "{")
    except StopIteration as error:
        raise InstrumentError(f"{label}: function {name} has no body") from error
    depth = 1
    for i in range(left + 1, len(tokens)):
        if tokens[i].value == "{":
            depth += 1
        elif tokens[i].value == "}":
            depth -= 1
            if not depth:
                return tokens[left + 1:i]
    raise InstrumentError(f"{label}: function {name} has malformed braces")


def _optional_body(tokens: list[Token], name: str, label: str) -> list[Token] | None:
    """An absent proof function is a B-149 gap; ambiguous source is not."""
    starts = [i for i in range(len(tokens) - 1) if tokens[i].value == "fn" and tokens[i + 1].value == name]
    if not starts:
        return None
    return _body(tokens, name, label)


def _has(tokens: list[Token], values: list[str]) -> bool:
    return any([t.value for t in tokens[i:i + len(values)]] == values for i in range(len(tokens) - len(values) + 1))


def _all_count(tokens: list[Token], values: list[str]) -> int:
    return sum([t.value for t in tokens[i:i + len(values)]] == values for i in range(len(tokens) - len(values) + 1))


def _camel_reason(variant: str) -> str:
    return re.sub(r"(?<!^)([A-Z])", r"_\1", variant).lower()


def _enum_variants(tokens: list[Token], label: str) -> list[str]:
    starts = [i for i in range(len(tokens) - 1) if tokens[i].value == "enum" and tokens[i + 1].value == "RecordError"]
    if len(starts) != 1:
        raise InstrumentError(f"{label}: expected exactly one enum RecordError, found {len(starts)}")
    try:
        left = next(i for i in range(starts[0] + 2, len(tokens)) if tokens[i].value == "{")
    except StopIteration as error:
        raise InstrumentError(f"{label}: RecordError has no body") from error
    variants: list[str] = []
    depth, after_comma = 1, True
    for token in tokens[left + 1:]:
        if token.value == "{":
            depth += 1
        elif token.value == "}":
            depth -= 1
            if not depth:
                break
        elif depth == 1 and token.value == ",":
            after_comma = True
        elif depth == 1 and after_comma and token.kind == "ident":
            variants.append(token.value)
            after_comma = False
    else:
        raise InstrumentError(f"{label}: RecordError has malformed braces")
    if len(variants) < 2 or len(set(variants)) != len(variants):
        raise InstrumentError(f"{label}: RecordError population is not a usable enum")
    return variants


def _match_arms_are_exhaustive(tokens: list[Token], variants: list[str]) -> bool:
    """Require a RecordError match whose every top-level arm is a named variant."""
    wanted = set(variants)
    found = False
    for index, token in enumerate(tokens):
        if token.value != "match":
            continue
        try:
            left = next(i for i in range(index + 1, len(tokens)) if tokens[i].value == "{")
        except StopIteration:
            continue
        depth, arms, arm = 1, [], []
        for current in tokens[left + 1:]:
            if current.value == "{":
                depth += 1
            elif current.value == "}":
                depth -= 1
                if not depth:
                    arms.append(arm)
                    break
            if depth == 1 and current.value == ",":
                arms.append(arm)
                arm = []
            else:
                arm.append(current)
        else:
            continue
        seen: list[str] = []
        clean = [a for a in arms if a]
        for arm_tokens in clean:
            arrow = next((i for i, item in enumerate(arm_tokens) if item.value == "=" and i + 1 < len(arm_tokens) and arm_tokens[i + 1].value == ">"), None)
            if arrow is None:
                seen = []
                break
            pattern = [item.value for item in arm_tokens[:arrow]]
            matches = [variant for variant in variants if ["RecordError", ":", ":", variant] == pattern]
            if len(matches) != 1:
                seen = []  # `_`, `other`, or any binding is a non-exhaustive fallback.
                break
            seen.append(matches[0])
        if not seen:
            return False
        if set(seen) == wanted and len(seen) == len(wanted):
            found = True
        else:
            return False
    return found


def _stable_code_mapping(tokens: list[Token], variants: list[str]) -> dict[str, str] | None:
    """Return an exact, wildcard-free ``Self::Variant => \"code\"`` mapping."""
    wanted = set(variants)
    for index, token in enumerate(tokens):
        if token.value != "match":
            continue
        try:
            left = next(i for i in range(index + 1, len(tokens)) if tokens[i].value == "{")
        except StopIteration:
            continue
        depth, arms, arm = 1, [], []
        for current in tokens[left + 1:]:
            if current.value == "{":
                depth += 1
            elif current.value == "}":
                depth -= 1
                if not depth:
                    arms.append(arm)
                    break
            if depth == 1 and current.value == ",":
                arms.append(arm)
                arm = []
            else:
                arm.append(current)
        else:
            continue
        mapping: dict[str, str] = {}
        for arm_tokens in (item for item in arms if item):
            arrow = next(
                (
                    i
                    for i, item in enumerate(arm_tokens)
                    if item.value == "="
                    and i + 1 < len(arm_tokens)
                    and arm_tokens[i + 1].value == ">"
                ),
                None,
            )
            if arrow is None:
                mapping = {}
                break
            pattern = [item.value for item in arm_tokens[:arrow]]
            matches = [variant for variant in variants if ["Self", ":", ":", variant] == pattern]
            values = [item.value for item in arm_tokens[arrow + 2:] if item.kind == "string"]
            if len(matches) != 1 or len(values) != 1 or matches[0] in mapping:
                mapping = {}
                break
            mapping[matches[0]] = values[0]
        if set(mapping) == wanted:
            return mapping
    return None


def assess(root: Path) -> list[str]:
    root = root.resolve()
    texts: dict[str, str] = {}
    token_sets: dict[str, list[Token]] = {}
    for relative in REQUIRED:
        path = root / relative
        if not path.is_file():
            raise InstrumentError(f"required source is missing: {relative}")
        texts[relative] = path.read_text(encoding="utf-8")
        token_sets[relative] = lex(texts[relative], relative)

    gaps: list[str] = []
    audit = _body(token_sets[AUDIT], "empty_batch_issues_no_statement_at_all", AUDIT)
    # Bind the exact dispatch seam's empty-slice result to the collection that
    # is asserted.  Merely calling the builder and separately asserting an
    # unrelated empty Vec is a false proof.
    empty_build_direct = _has(
        audit,
        [
            "let", "statements", "=", "D1AuditOutboxSink", ":", ":",
            "build_batch_statements", "(", "&", "[", "]", ")",
        ],
    )
    empty_build_named = (
        _has(
            audit,
            [
                "let", "rows", ":", "Vec", "<", "AuditRow", ">", "=",
                "Vec", ":", ":", "new", "(", ")",
            ],
        )
        and _has(
            audit,
            [
                "let", "statements", "=", "D1AuditOutboxSink", ":", ":",
                "build_batch_statements", "(", "&", "rows", ")",
            ],
        )
    )
    empty_build = empty_build_direct or empty_build_named
    empty_assert = _has(audit, ["assert", "!", "(", "statements", ".", "is_empty", "(", ")"])
    if not (empty_build and empty_assert):
        gaps.append("empty_batch-no-empty-statement-assertion")

    auth = _body(token_sets[AUTH], "build_state_returns_none_when_secret_absent", AUTH)
    direct_none = (
        _has(auth, ["assert_eq", "!", "(", "configured_ingest_auth_key", "(", "None", ")", ",", "None", ")"])
        or _has(auth, ["assert", "!", "(", "configured_ingest_auth_key", "(", "None", ")", ".", "is_none", "(", ")"])
    )
    assigned = any(
        _has(auth[i:i + 12], ["let", auth[i + 1].value, "=", "configured_ingest_auth_key", "(", "None", ")"])
        and (_has(auth, ["assert", "!", "(", auth[i + 1].value, ".", "is_none", "(", ")", ")"])
             or _has(auth, ["assert_eq", "!", "(", auth[i + 1].value, ",", "None", ")"]))
        for i in range(max(0, len(auth) - 6)) if auth[i].value == "let" and auth[i + 1].kind == "ident"
    )
    uses_env = _has(auth, ["std", ":", ":", "env"]) or _has(auth, ["env", ":", ":", "var"])
    conditional = any(token.value in {"if", "match", "while", "for", "loop"} for token in auth)
    if not ((direct_none or assigned) and not uses_env and not conditional):
        gaps.append("secret-absent-is-environment-conditional")

    variants = _enum_variants(token_sets[INGEST], INGEST)
    code_body = _body(token_sets[INGEST], "code", INGEST)
    code_mapping = _stable_code_mapping(code_body, variants)
    stable_codes = {variant: _camel_reason(variant) for variant in variants}
    helper = _optional_body(token_sets[VALIDATE], "record_error_variant_name", VALIDATE)
    fixtures = _optional_body(token_sets[VALIDATE], "every_record_error_variant_has_a_rejection_fixture", VALIDATE)
    mentions = {variant: _all_count(fixtures or [], ["RecordError", ":", ":", variant]) for variant in variants}
    def fixture_has_expected_reason(variant: str) -> bool:
        assert fixtures is not None
        target = _camel_reason(variant)
        for index in range(len(fixtures) - 3):
            if [item.value for item in fixtures[index:index + 4]] == ["RecordError", ":", ":", variant]:
                # A fixture table may put its expected code before or after the
                # enum value.  Keeping the literal local prevents a detached
                # string (or a dead helper branch) from being credited.
                nearby = fixtures[max(0, index - 8):index + 12]
                if any(item.kind == "string" and item.value == target for item in nearby):
                    return True
        return False
    fixture_proof = (
        helper is not None and fixtures is not None
        and _match_arms_are_exhaustive(helper, variants)
        and code_mapping == stable_codes
        and all(count == 1 for count in mentions.values())
        and (
            _has(fixtures or [], ["assert_eq", "!", "(", "validate_record", "(", "wire", ")", ",", "Err", "(", "expected", ")"])
            or (
                _has(fixtures or [], ["let", "actual", "=", "validate_record", "("])
                and _has(fixtures or [], ["assert_eq", "!", "(", "actual", ",", "Err", "(", "expected", ")"])
            )
        )
        and (
            _has(fixtures or [], ["assert_eq", "!", "(", "expected", ".", "code", "(", ")", ",", "reason"])
            or _has(fixtures or [], ["assert_eq", "!", "(", "expected", ".", "code", "(", ")", ",", "expected_code"])
        )
        and all(fixture_has_expected_reason(variant) for variant in variants)
    )
    if not fixture_proof:
        gaps.append("record-error-fixtures-or-reason-codes-not-exhaustive")

    skip = _body(token_sets[SKIP], "bad_tenant_id_is_skipped_not_fatal", SKIP)
    skip_pins_own_reason = (
        _has(skip, ["assert_eq", "!", "("])
        and _has(skip, ["RecordError", ":", ":", "BadTenantId", ".", "code", "(", ")"])
        and any(token.kind == "string" and token.value == "bad_tenant_id" for token in skip)
    )
    if _has(skip, ["validate_record_reasons_pinned"]) or not skip_pins_own_reason:
        gaps.append("record-skip-delegation-does-not-reference-exhaustive-proof")
    return gaps


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--expect", choices=("open", "done"), required=True)
    args = parser.parse_args(argv)
    try:
        gaps = assess(args.root)
    except (OSError, UnicodeError, InstrumentError) as error:
        print(f"instrument error: {error}", file=sys.stderr)
        return 2
    actual = "open" if gaps else "done"
    print(f"B-149 {actual}: {len(gaps)} gap(s)")
    for gap in gaps:
        print(f"- {gap}")
    if actual != args.expect:
        print(f"expected {args.expect}, found {actual}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
