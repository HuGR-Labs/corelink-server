#!/usr/bin/env python3
"""Audit every backlog verify command for comment-sensitive grep checks.

This is a structural census, not a grep for the word ``grep``.  It parses every
backlog record, every verify payload, and every grep invocation, distinguishes
``grep -v`` filters from assertions, and probes assertion regexes against
language/document comment prefixes.  Missing or unparsable records fail
closed.  The in-memory mutation test proves that changing a real population
member changes the verdict.
"""
from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

try:
    import yaml
except ImportError as error:  # pragma: no cover - exercised by the CLI environment
    raise SystemExit(f"instrument error: PyYAML is required: {error}") from error


FENCE = re.compile(r"```backlog\n(.*?)\n```", re.DOTALL)
GREP = re.compile(
    r"\bgrep\s+(?P<options>(?:-[A-Za-z0-9-]+\s+)*)"
    r"(?P<quote>[\"'])(?P<pattern>.*?)(?P=quote)"
)
COMMAND_BOUNDARY = re.compile(r"(?:^|[;&|(!]|\$\()\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S*\s*)*$")
ID = re.compile(r"^B-\d{3}$")
COMMENT_PREFIXES = ("//", "#", "/*", "<!--", "*", "--")
POSIX_CLASSES = {
    "[[:space:]]": r"\s",
    "[[:digit:]]": r"\d",
    "[[:alpha:]]": r"[A-Za-z]",
    "[[:alnum:]]": r"[A-Za-z0-9]",
}


class InstrumentError(RuntimeError):
    """The census cannot be trusted."""


@dataclass(frozen=True)
class GrepCheck:
    record_id: str
    line: int
    pattern: str
    options: str
    source: str


@dataclass(frozen=True)
class Census:
    records: int
    command_records: int
    manual_records: int
    grep_invocations: int
    assertions: tuple[GrepCheck, ...]
    unsafe: tuple[GrepCheck, ...]
    indeterminate: tuple[GrepCheck, ...]


def _records(backlog: str) -> list[dict[str, object]]:
    blocks = list(FENCE.finditer(backlog))
    if not blocks:
        raise InstrumentError("no ```backlog records found")
    records: list[dict[str, object]] = []
    seen: set[str] = set()
    for index, match in enumerate(blocks, 1):
        try:
            record = yaml.safe_load(match.group(1))
        except yaml.YAMLError as error:
            raise InstrumentError(f"record {index} has invalid YAML: {error}") from error
        if not isinstance(record, dict):
            raise InstrumentError(f"record {index} is not a mapping")
        record_id = record.get("id")
        if not isinstance(record_id, str) or not ID.fullmatch(record_id):
            raise InstrumentError(f"record {index} has malformed id: {record_id!r}")
        if record_id in seen:
            raise InstrumentError(f"duplicate backlog id: {record_id}")
        seen.add(record_id)
        records.append(record)
    return records


def _grep_checks(record: dict[str, object]) -> tuple[list[GrepCheck], int]:
    verify = record.get("verify")
    if not isinstance(verify, str) or verify.strip() == "manual":
        return [], 0
    checks: list[GrepCheck] = []
    invocations = 0
    for line_number, line in enumerate(verify.splitlines(), 1):
        for match in GREP.finditer(line):
            # A grep-looking string in an embedded Python/awk expression is
            # not a shell invocation. Require a shell command boundary (or a
            # command substitution/assignment immediately before it).
            if not COMMAND_BOUNDARY.search(line[: match.start()]):
                continue
            invocations += 1
            options = match.group("options") or ""
            check = GrepCheck(
                record_id=str(record["id"]),
                line=line_number,
                pattern=match.group("pattern"),
                options=options,
                source=line.strip(),
            )
            # `grep -v` is a filter in a pipeline, not a positive capability
            # assertion. It remains in the invocation census, but is not in the
            # population whose comment sensitivity determines this item.
            if "v" not in options.replace("--", ""):
                checks.append(check)
    return checks, invocations


def _as_python_regex(pattern: str) -> re.Pattern[str]:
    translated = pattern
    translated = translated.replace("[^[:space:]]", r"[^\s]")
    translated = translated.replace("[[:space:]-]", r"[\s-]")
    for source, target in POSIX_CLASSES.items():
        translated = translated.replace(source, target)
    # Existing verify commands use grep -E's escaped alternation frequently.
    translated = translated.replace(r"\|", "|")
    return re.compile(translated)


def _matches_comment(check: GrepCheck) -> bool:
    """Return true when the assertion regex can match a comment-shaped line."""
    try:
        expression = _as_python_regex(check.pattern)
    except re.error:
        return True  # An unmodelled dialect is an instrument gap, fail closed.
    tokens = re.findall(r"[A-Za-z][A-Za-z0-9_.:/-]{2,}", check.pattern)
    token = max(tokens, key=len) if tokens else "verify"
    probes = tuple(f"{prefix} {token}" for prefix in COMMENT_PREFIXES)
    return any(expression.search(probe) for probe in probes)


def census(backlog: str) -> Census:
    records = _records(backlog)
    assertions: list[GrepCheck] = []
    invocations = 0
    command_records = 0
    manual_records = 0
    for record in records:
        verify = record.get("verify")
        if isinstance(verify, str) and verify.strip() == "manual":
            manual_records += 1
        elif isinstance(verify, str):
            command_records += 1
        checks, count = _grep_checks(record)
        assertions.extend(checks)
        invocations += count
    unsafe: list[GrepCheck] = []
    indeterminate: list[GrepCheck] = []
    for check in assertions:
        try:
            _as_python_regex(check.pattern)
        except re.error:
            indeterminate.append(check)
        if _matches_comment(check):
            unsafe.append(check)
    return Census(
        records=len(records),
        command_records=command_records,
        manual_records=manual_records,
        grep_invocations=invocations,
        assertions=tuple(assertions),
        unsafe=tuple(unsafe),
        indeterminate=tuple(indeterminate),
    )


def mutation_self_test(backlog: str) -> None:
    """Require a known real population mutation to change the census."""
    baseline = census(backlog)
    target = re.search(
        r"(id: B-083\n.*?verify: \|\n(?:  .*\n)+?)",
        backlog,
        re.DOTALL,
    )
    if target is None or "grep" not in target.group(1):
        raise InstrumentError("B-083 population member missing from mutation fixture")
    mutated_block = target.group(1).replace(
        'grep -q "byok"',
        'grep -q "^byok"',
        1,
    )
    if mutated_block == target.group(1):
        raise InstrumentError("B-083 mutation did not change the fixture")
    mutated = backlog[: target.start()] + mutated_block + backlog[target.end() :]
    changed = census(mutated)
    if len(changed.unsafe) >= len(baseline.unsafe):
        raise InstrumentError("anchoring a real B-083 population member did not reduce risk")

    # Parser completeness is also load-bearing: removing every fenced record
    # cannot become a falsely clean zero-population result.
    try:
        census("no backlog records")
    except InstrumentError:
        pass
    else:
        raise InstrumentError("empty backlog mutation was accepted as a clean census")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backlog", type=Path, default=Path("BACKLOG.md"))
    parser.add_argument("--expect", choices=("open", "done"), required=True)
    args = parser.parse_args(argv)
    try:
        backlog = args.backlog.read_text(encoding="utf-8")
        result = census(backlog)
        mutation_self_test(backlog)
    except (OSError, InstrumentError) as error:
        print(f"instrument error: {error}", file=sys.stderr)
        return 2
    status = "done" if not result.unsafe and not result.indeterminate else "open"
    print(
        f"B-155 {status}: records={result.records} command_records={result.command_records} "
        f"manual={result.manual_records} grep_invocations={result.grep_invocations} "
        f"assertions={len(result.assertions)} comment_sensitive={len(result.unsafe)} "
        f"indeterminate={len(result.indeterminate)}"
    )
    for check in result.unsafe[:8]:
        print(f"- {check.record_id}:{check.line}: {check.pattern!r}")
    if status != args.expect:
        print(f"expected {args.expect}, found {status}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
