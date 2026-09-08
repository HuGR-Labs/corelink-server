#!/usr/bin/env python3
"""Fail-closed static verifier for the B-063 archive Worker caller.

This is a local contract check only. It does not contact D1, Cloudflare,
GitHub, PagerDuty, or the archive endpoint, and therefore cannot claim that
the 188 production rows are recovered. It guards the repo-owned boundary that
must report an uncertain archive response as failed and must not authorize the
irreversible route with the shared key.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import re
import sys
from pathlib import Path


CRON = Path("apps/signup-worker/src/webhooks/audit_archive_cron.ts")
CRON_TEST = Path("apps/signup-worker/tests/audit_archive_cron.test.ts")
COUNTER_FIELDS = (
    "rows_archived",
    "chunks_created",
    "chunks_already_present",
    "partitions_archived",
    "partitions_failed",
    "rows_quarantined",
    "partitions_quarantined",
)


@dataclass(frozen=True)
class _Token:
    kind: str
    value: str


def _tokenize_typescript(source: str) -> list[_Token]:
    """Tokenize enough TS to ignore comments and literals used as bait."""
    tokens: list[_Token] = []
    i = 0
    while i < len(source):
        char = source[i]
        if char.isspace():
            i += 1
            continue
        if source.startswith("//", i):
            newline = source.find("\n", i + 2)
            i = len(source) if newline < 0 else newline + 1
            continue
        if source.startswith("/*", i):
            end = source.find("*/", i + 2)
            if end < 0:
                return tokens
            i = end + 2
            continue
        if char in "'\"`":
            quote = char
            i += 1
            value: list[str] = []
            while i < len(source):
                char = source[i]
                if char == "\\" and i + 1 < len(source):
                    value.append(source[i : i + 2])
                    i += 2
                elif char == quote:
                    i += 1
                    break
                else:
                    value.append(char)
                    i += 1
            tokens.append(_Token("string", "".join(value)))
            continue
        if char.isalpha() or char in "_$":
            start = i
            i += 1
            while i < len(source) and (source[i].isalnum() or source[i] in "_$"):
                i += 1
            tokens.append(_Token("ident", source[start:i]))
            continue
        if char.isdigit():
            start = i
            i += 1
            while i < len(source) and (source[i].isalnum() or source[i] in "._"):
                i += 1
            tokens.append(_Token("number", source[start:i]))
            continue
        tokens.append(_Token("punct", char))
        i += 1
    return tokens


def _has_sequence(tokens: list[_Token], values: tuple[str, ...]) -> bool:
    return any(
        [token.value for token in tokens[index : index + len(values)]] == list(values)
        for index in range(len(tokens) - len(values) + 1)
    )


def _has_direct_sequence(tokens: list[_Token], values: tuple[str, ...]) -> bool:
    """Find a statement directly in a block, not inside a nested function."""
    depth = 0
    for index, token in enumerate(tokens):
        if token.value == "{":
            depth += 1
        elif token.value == "}":
            depth = max(0, depth - 1)
        elif depth == 0 and [
            item.value for item in tokens[index : index + len(values)]
        ] == list(values):
            return True
    return False


def _counter_array_is_strict(tokens: list[_Token]) -> bool:
    declaration = ("const", "ARCHIVE_COUNTER_FIELDS", "=", "[")
    for index in range(len(tokens) - len(declaration) + 1):
        if [token.value for token in tokens[index : index + len(declaration)]] != list(
            declaration
        ):
            continue
        end = index + len(declaration)
        fields: list[str] = []
        while end < len(tokens) and tokens[end].value != "]":
            if tokens[end].kind == "string":
                fields.append(tokens[end].value)
            end += 1
        if end < len(tokens) and tuple(fields) == COUNTER_FIELDS:
            return True
    return False


def _non_2xx_branch_is_fail_closed(tokens: list[_Token]) -> bool:
    condition = ("if", "(", "!", "resp", ".", "ok", ")", "{")
    for index in range(len(tokens) - len(condition) + 1):
        if [token.value for token in tokens[index : index + len(condition)]] != list(
            condition
        ):
            continue
        body_start = index + len(condition)
        depth = 1
        body_end = body_start
        while body_end < len(tokens) and depth:
            if tokens[body_end].value == "{":
                depth += 1
            elif tokens[body_end].value == "}":
                depth -= 1
            body_end += 1
        if depth:
            continue
        body = tokens[body_start : body_end - 1]
        if _has_direct_sequence(body, ("ok", "=", "false", ";")) and _has_direct_sequence(
            body, ("incomplete", "=", "true", ";")
        ):
            return True
    return False


def _has_test_case(tokens: list[_Token], title: str) -> bool:
    for index in range(len(tokens) - 2):
        if (
            tokens[index].value == "it"
            and tokens[index + 1].value == "("
            and tokens[index + 2].kind == "string"
            and tokens[index + 2].value == title
        ):
            return True
    return False


class InstrumentError(ValueError):
    """The static evidence is absent or drifted, so no green result is valid."""


def _read_regular(root: Path, relative: Path) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise InstrumentError(f"missing or non-regular source: {relative}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise InstrumentError(f"cannot read {relative}: {exc}") from exc


def check_source(cron: str, tests: str) -> None:
    """Require each fail-closed boundary and its executable mutation controls."""
    cron_tokens = _tokenize_typescript(cron)
    if not _has_sequence(
        cron_tokens,
        ("import", "{", "resolveDedicatedEraseAuthKey", "}", "from"),
    ):
        raise InstrumentError("archive caller is missing the active dedicated-key import")
    if any(
        token.kind == "ident" and token.value == "resolveEraseAuthKey"
        for token in cron_tokens
    ):
        raise InstrumentError("archive caller must not use the shared-key fallback")
    if not _counter_array_is_strict(cron_tokens):
        raise InstrumentError("archive caller is missing the active strict seven-counter contract")
    if not _has_sequence(
        cron_tokens,
        ("Number", ".", "isSafeInteger", "(", "counter", ")"),
    ):
        raise InstrumentError("archive caller must validate counters as safe nonnegative integers")
    if not _non_2xx_branch_is_fail_closed(cron_tokens):
        raise InstrumentError(
            "archive caller is missing an executable non-2xx fail-closed branch"
        )

    test_tokens = _tokenize_typescript(tests)
    missing_tests = [
        title
        for title in (
            "skips when only the shared key is bound",
            "fails closed on a non-JSON body without throwing",
        )
        if not _has_test_case(test_tokens, title)
    ]
    if not _has_sequence(
        test_tokens,
        ("expect", "(", "r", ".", "incomplete", ")", ".", "toBe", "(", "true", ")"),
    ):
        missing_tests.append("active incomplete=true assertion")
    if missing_tests:
        raise InstrumentError(f"archive tests missing mutation controls: {missing_tests}")


def mutation_self_test(cron: str, tests: str) -> int:
    """Kill representative source mutations instead of trusting marker presence."""
    counter_block = '''const ARCHIVE_COUNTER_FIELDS = [
  "rows_archived",
  "chunks_created",
  "chunks_already_present",
  "partitions_archived",
  "partitions_failed",
  "rows_quarantined",
  "partitions_quarantined",
] as const;'''
    branch = re.search(
        r"\n    if \(!resp\.ok\) \{.*?\n    \}\n    return \{",
        cron,
        re.DOTALL,
    )
    if branch is None:
        raise InstrumentError("cannot construct the non-2xx branch mutation")
    branch_bait = "\n    // if (!resp.ok) { ok = false; incomplete = true; }\n    return {"
    mutations = (
        cron.replace(
            'import { resolveDedicatedEraseAuthKey } from "../lib/erase-auth-key.js";',
            '// import { resolveDedicatedEraseAuthKey } from "../lib/erase-auth-key.js";\nconst keyBait = "resolveDedicatedEraseAuthKey";',
            1,
        ),
        cron.replace(
            counter_block,
            '// ' + counter_block.replace("\n", "\n// ") + '\nconst counterBait = "rows_archived chunks_created chunks_already_present partitions_archived partitions_failed rows_quarantined partitions_quarantined";',
            1,
        ),
        cron[: branch.start()] + branch_bait + cron[branch.end() :],
        cron.replace("Number.isSafeInteger(counter)", "true", 1),
    )
    killed = 0
    for mutated in mutations:
        try:
            check_source(mutated, tests)
        except InstrumentError:
            killed += 1
        else:
            raise InstrumentError("a B-063 source mutation survived the static guard")
    return killed


def verify(root: Path) -> int:
    cron = _read_regular(root, CRON)
    tests = _read_regular(root, CRON_TEST)
    check_source(cron, tests)
    return mutation_self_test(cron, tests)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args(argv)
    try:
        killed = verify(args.root)
    except (OSError, InstrumentError) as exc:
        print(f"B-063 instrument error: {exc}", file=sys.stderr)
        return 2
    print(f"B-063 archive pipeline guard: clean; killed_mutations={killed}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
