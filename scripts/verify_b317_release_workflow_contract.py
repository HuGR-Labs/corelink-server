#!/usr/bin/env python3
"""Fail-closed static guard for the B-317 release-contract test repair."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "tools/cli/tests/release_workflow_contract.rs"
MAX_SOURCE_BYTES = 200_000


class VerificationError(RuntimeError):
    """The target is absent, ambiguous, or no longer has the reviewed shape."""


def _read(root: Path, overrides: dict[str, str]) -> str:
    if TARGET in overrides:
        source = overrides[TARGET]
        if not isinstance(source, str):
            raise VerificationError("target override is not text")
    else:
        path = root / TARGET
        try:
            if path.is_symlink() or not path.is_file():
                raise VerificationError(f"missing/non-regular target: {TARGET}")
            source = path.read_text(encoding="utf-8")
        except OSError as exc:
            raise VerificationError(f"cannot read target: {TARGET}") from exc
    if not source or len(source.encode("utf-8")) > MAX_SOURCE_BYTES:
        raise VerificationError("target is empty or exceeds the bounded source size")
    return source


def _code(source: str) -> str:
    """Blank comments and literals so reviewer bait cannot satisfy predicates."""
    out: list[str] = []
    index = 0
    state = "code"
    block_depth = 0
    while index < len(source):
        char = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""
        if state == "line":
            if char == "\n":
                out.append(char)
                state = "code"
            else:
                out.append(" ")
            index += 1
            continue
        if state == "block":
            if char == "/" and following == "*":
                block_depth += 1
                out.extend((" ", " "))
                index += 2
            elif char == "*" and following == "/":
                block_depth -= 1
                out.extend((" ", " "))
                index += 2
                if block_depth == 0:
                    state = "code"
            else:
                out.append("\n" if char == "\n" else " ")
                index += 1
            continue
        if state in {"string", "char"}:
            quote = '"' if state == "string" else "'"
            if char == "\\":
                out.append(" ")
                if index + 1 < len(source):
                    out.append("\n" if source[index + 1] == "\n" else " ")
                    index += 2
                else:
                    index += 1
            elif char == quote:
                out.append(" ")
                state = "code"
                index += 1
            else:
                out.append("\n" if char == "\n" else " ")
                index += 1
            continue
        raw_prefix = re.match(r"(?:br|r)(?P<hashes>#{0,255})\"", source[index:])
        if raw_prefix:
            terminator = '"' + raw_prefix.group("hashes")
            end = source.find(terminator, index + raw_prefix.end())
            if end < 0:
                raise VerificationError("unterminated Rust raw string")
            end += len(terminator)
            out.extend("\n" if item == "\n" else " " for item in source[index:end])
            index = end
        elif char == "/" and following == "/":
            out.extend((" ", " "))
            state = "line"
            index += 2
        elif char == "/" and following == "*":
            out.extend((" ", " "))
            state = "block"
            block_depth = 1
            index += 2
        elif char == '"':
            out.append(" ")
            state = "string"
            index += 1
        elif char == "'" and following and following not in {" ", "\n"}:
            # Lifetimes use an apostrophe without a closing quote; only classify
            # the compact Rust character-literal forms needed by this target.
            char_end = index + (4 if following == "\\" else 2)
            if char_end < len(source) and source[char_end] == "'":
                out.append(" ")
                state = "char"
                index += 1
            else:
                out.append(char)
                index += 1
        else:
            out.append(char)
            index += 1
    if state in {"block", "string", "char"}:
        raise VerificationError("unterminated Rust comment or literal")
    return "".join(out)


def _function(code: str, name: str) -> tuple[str, str]:
    matches = list(re.finditer(rf"\bfn\s+{re.escape(name)}\s*\(", code))
    if len(matches) != 1:
        raise VerificationError(f"{name}: expected one function, found {len(matches)}")
    start = matches[0].start()
    brace = code.find("{", matches[0].end())
    if brace < 0:
        raise VerificationError(f"{name}: missing body")
    depth = 0
    for index in range(brace, len(code)):
        if code[index] == "{":
            depth += 1
        elif code[index] == "}":
            depth -= 1
            if depth == 0:
                return code[start:brace], code[brace + 1 : index]
    raise VerificationError(f"{name}: unterminated body")


def _require(pattern: str, text: str, label: str) -> None:
    if re.search(pattern, text, re.MULTILINE | re.DOTALL) is None:
        raise VerificationError(f"missing {label}")


def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    source = _read(root, overrides or {})
    code = _code(source)

    if re.search(r"\bpanic\s*!|\.\s*(?:expect|unwrap)\s*\(", code):
        raise VerificationError("panic/expect/unwrap suppression returned to the target")
    if re.search(r"#\s*!?\s*\[\s*allow\b", code):
        raise VerificationError("an allow attribute suppresses the strict lint contract")

    for name in ("release_workflow", "load_workflow", "load_script"):
        signature, _ = _function(code, name)
        _require(r"->\s*Result\s*<\s*String\s*,\s*String\s*>", signature, f"{name} Result return")

    for name in ("load_workflow", "load_script"):
        _, body = _function(code, name)
        _require(r"std\s*::\s*fs\s*::\s*read_to_string\s*\(\s*path\s*\)\s*\.\s*map_err\s*\(", body, f"{name} fallible read")

    _, retry = _function(code, "assert_retry_manifest_contract")
    _require(r"assert\s*!\s*\(\s*retry\s*\.\s*is_some\s*\(", retry, "retry presence assertion")
    _require(r"if\s+let\s+Some\s*\(\s*retry\s*\)\s*=\s*retry\s*\{", retry, "guarded retry assertions")

    test_name = "release_workflow_preserves_the_installer_and_signer_contract_and_rejects_mutations"
    signature, body = _function(code, test_name)
    _require(r"->\s*Result\s*<\s*\(\s*\)\s*,\s*String\s*>", signature, "test Result return")
    _require(r"let\s+workflow\s*=\s*release_workflow\s*\(\s*\)\s*\?\s*;", body, "release workflow error propagation")
    for loader in ("load_workflow", "load_script"):
        _require(rf"\b{loader}\s*\([^;]+?\)\s*\?", body, f"{loader} error propagation")
    _require(r"Ok\s*\(\s*\(\s*\)\s*\)\s*$", body.strip(), "successful Result completion")


def self_test() -> None:
    source = _read(ROOT, {})
    mutations = (
        source.replace("fn release_workflow() -> Result<String, String>", "fn release_workflow() -> String", 1),
        source.replace(".map_err(|_| format!(\"{name} workflow must be readable\"))", ".unwrap_or_else(|_| panic!(\"unreadable\"))", 1),
        source.replace("if let Some(retry) = retry {", "if retry.is_some() {", 1),
        source.replace("fn release_workflow_preserves_the_installer_and_signer_contract_and_rejects_mutations(\n) -> Result<(), String> {", "fn release_workflow_preserves_the_installer_and_signer_contract_and_rejects_mutations() {", 1),
        "#![allow(clippy::panic)]\n" + source,
    )
    for index, mutation in enumerate(mutations, 1):
        if mutation == source:
            raise VerificationError(f"self-test mutation {index} did not alter the target")
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
    except VerificationError as exc:
        print(f"B-317 release workflow contract: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    print("B-317 release workflow contract: PASS")
