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
import shlex
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

try:
    import yaml
except ImportError as error:  # pragma: no cover - exercised by the CLI environment
    raise SystemExit(f"instrument error: PyYAML is required: {error}") from error


FENCE = re.compile(r"```backlog\n(.*?)\n```", re.DOTALL)
BACKLOG_OPEN = re.compile(r"^```backlog(?:[ \t]*)$", re.MULTILINE)
GREP = re.compile(
    r"\bgrep\s+(?P<options>(?:-[A-Za-z0-9-]+\s+)*)"
    # Keep shell-escaped quotes inside a quoted regex instead of truncating
    # the pattern at the first escaped quote.
    r"(?P<quote>[\"'])(?P<pattern>(?:\\.|(?!(?P=quote)).)*?)(?P=quote)"
)
GREP_UNQUOTED = re.compile(
    r"\bgrep\s+(?P<options>(?:-[A-Za-z0-9-]+\s+)*)(?P<pattern>[^\s;&|()<>]+)"
)
COMMAND_BOUNDARY = re.compile(
    r"(?:^|[;&|(!]|\$\(|\b(?:if|elif|then|while|until|do|command|builtin|exec)\b)"
    r"\s*(?:!\s*)?(?:[A-Za-z_][A-Za-z0-9_]*=(?:'[^']*'|\"[^\"]*\"|\S+)\s*)*$"
)
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
    quote: str = ""
    resolved_patterns: tuple[str, ...] = ()


@dataclass(frozen=True)
class Census:
    records: int
    command_records: int
    manual_records: int
    grep_invocations: int
    assertions: tuple[GrepCheck, ...]
    unsafe: tuple[GrepCheck, ...]
    indeterminate: tuple[GrepCheck, ...]


EXPECTED_RECORDS = 168
EXPECTED_COMMAND_RECORDS = 137
EXPECTED_MANUAL_RECORDS = 31
EXPECTED_GREP_INVOCATIONS = 348
EXPECTED_ASSERTIONS = 328


def _records(backlog: str) -> list[dict[str, object]]:
    blocks = list(FENCE.finditer(backlog))
    if not blocks:
        raise InstrumentError("no ```backlog records found")
    if len(BACKLOG_OPEN.findall(backlog)) != len(blocks):
        raise InstrumentError("unclosed or malformed ```backlog fence")
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


SHELL_VARIABLE = re.compile(r"(?<!\\)\$(?:\{(?P<braced>[A-Za-z_][A-Za-z0-9_]*)\}|(?P<plain>[A-Za-z_][A-Za-z0-9_]*))")


def _shell_values(verify: str) -> dict[str, tuple[str, ...]]:
    """Resolve only literal assignments and finite ``for x in ...`` lists."""
    values: dict[str, tuple[str, ...]] = {}
    # printf command substitutions are deliberately restricted to a single
    # quoted format string; no command or shell expansion is evaluated here.
    assignment = re.compile(
        r"\b([A-Za-z_][A-Za-z0-9_]*)=(?:\"([^\"$]*)\"|'([^']*)'|\$\(printf\s+\"([^\"$]*)\"\))"
    )
    for match in assignment.finditer(verify):
        value = next((item for item in match.groups()[1:] if item is not None), None)
        if value is not None:
            values[match.group(1)] = (value,)
    for match in re.finditer(r"\bfor\s+([A-Za-z_][A-Za-z0-9_]*)\s+in\s+(.+?);\s*do\b", verify):
        try:
            words = tuple(shlex.split(match.group(2)))
        except ValueError:
            continue
        expanded: list[str] = []
        for word in words:
            variable = SHELL_VARIABLE.fullmatch(word)
            if variable:
                expanded.extend(values.get(variable.group("braced") or variable.group("plain"), ()))
            else:
                expanded.append(word)
        if expanded and all("$" not in word and "$(" not in word for word in expanded):
            values[match.group(1)] = tuple(expanded)
    return values


def _resolved_patterns(pattern: str, verify: str) -> tuple[tuple[str, ...], bool]:
    variables = SHELL_VARIABLE.findall(pattern)
    if not variables:
        return (pattern,), False
    values = _shell_values(verify)
    resolved = [pattern]
    unresolved = False
    for braced, plain in variables:
        name = braced or plain
        options = values.get(name)
        if not options:
            unresolved = True
            continue
        expanded: list[str] = []
        for candidate in resolved:
            for value in options:
                expanded.append(candidate.replace("${" + name + "}", value).replace("$" + name, value))
        resolved = expanded
    return tuple(resolved), unresolved


def _generated_hex_pattern_file(check: GrepCheck, verify: str) -> bool:
    """Recognize B-061's generated ``^hex`` pattern file without executing it."""
    generated = any(
        not line.lstrip().startswith("#")
        and (("s|^|^|" in line and "$tmp/pat" in line) or "$tmp/hit" in line)
        and ">" in line
        for line in verify.splitlines()
    )
    return (
        "f" in check.options
        and ("$tmp/pat" in check.pattern or "$tmp/hit" in check.pattern)
        and generated
        and ("$tmp/reach" in check.source or "$tmp/pairs" in check.source)
    )


def _grep_checks(record: dict[str, object]) -> tuple[list[GrepCheck], int, list[GrepCheck]]:
    verify = record.get("verify")
    if not isinstance(verify, str) or verify.strip() == "manual":
        return [], 0, []
    checks: list[GrepCheck] = []
    indeterminate: list[GrepCheck] = []
    invocations = 0
    known_starts: set[int] = set()
    for line_number, line in enumerate(verify.splitlines(), 1):
        known_starts: set[int] = set()
        for match in GREP.finditer(line):
            # A grep-looking string in an embedded Python/awk expression is
            # not a shell invocation. Require a shell command boundary (or a
            # command substitution/assignment immediately before it).
            if not COMMAND_BOUNDARY.search(line[: match.start()]):
                continue
            invocations += 1
            known_starts.add(match.start())
            options = match.group("options") or ""
            pattern = match.group("pattern")
            resolved, unresolved = _resolved_patterns(pattern, verify)
            check = GrepCheck(
                record_id=str(record["id"]),
                line=line_number,
                pattern=pattern,
                options=options,
                source=line.strip(),
                quote=match.group("quote"),
                resolved_patterns=resolved,
            )
            # `grep -v` is a filter in a pipeline, not a positive capability
            # assertion. It remains in the invocation census, but is not in the
            # population whose comment sensitivity determines this item.
            if "v" not in options.replace("--", ""):
                checks.append(check)
            if unresolved and not _generated_hex_pattern_file(check, verify):
                indeterminate.append(check)
        # The shell permits a bare regex word (`grep unsafe file`) in addition
        # to the quoted form above.  Parse it separately, retaining the same
        # boundary and polarity rules.  A variable or shell expression is
        # intentionally not guessed: it becomes an explicit indeterminate
        # population member below.
        for match in GREP_UNQUOTED.finditer(line):
            if match.start() in known_starts:
                continue
            if not COMMAND_BOUNDARY.search(line[: match.start()]):
                continue
            invocations += 1
            known_starts.add(match.start())
            options = match.group("options") or ""
            pattern = match.group("pattern")
            resolved, unresolved = _resolved_patterns(pattern, verify)
            check = GrepCheck(
                record_id=str(record["id"]),
                line=line_number,
                pattern=pattern,
                options=options,
                source=line.strip(),
                resolved_patterns=resolved,
            )
            if "v" not in options.replace("--", ""):
                checks.append(check)
            if unresolved and not _generated_hex_pattern_file(check, verify):
                indeterminate.append(check)
        # A grep invocation with an unquoted/dynamic pattern is still an
        # invocation, but its dialect and polarity cannot be proven by this
        # parser.  Count it and fail closed instead of silently shrinking the
        # assertion population.
        for word in re.finditer(r"\bgrep\b", line):
            if word.start() in known_starts:
                continue
            if not COMMAND_BOUNDARY.search(line[: word.start()]):
                continue
            invocations += 1
            indeterminate.append(
                GrepCheck(
                    record_id=str(record["id"]),
                    line=line_number,
                    pattern="",
                    options="",
                    source=line.strip(),
                )
            )
    return checks, invocations, indeterminate


def _as_python_regex(pattern: str, options: str = "") -> re.Pattern[str]:
    # The census sees shell source before double-quoted `\\` escapes are
    # reduced to one regex escape by the shell.
    pattern = pattern.replace("\\\\", "\\")
    if "F" in options:
        return re.compile(re.escape(pattern))
    translated = pattern
    translated = translated.replace("[^[:space:]]", r"[^\s]")
    translated = translated.replace("[[:space:]-]", r"[\s-]")
    for source, target in POSIX_CLASSES.items():
        translated = translated.replace(source, target)
    # In basic grep, escaped `|` is alternation while an unescaped pipe is
    # literal.  ERE reverses that rule.  Preserve the distinction for Python's
    # regex dialect, especially for Markdown-table and shell-`||` checks.
    if "E" not in options:
        marker = "__B155_ALTERNATION__"
        translated = translated.replace(r"\|", marker)
        translated = translated.replace("|", r"\|")
        translated = translated.replace(marker, "|")
    return re.compile(translated)


def _split_alternatives(pattern: str, options: str) -> list[str]:
    """Split only real BRE/ERE alternation operators, not quoted pipes."""
    needle = "|" if "E" in options else r"\|"
    return pattern.split(needle) if needle in pattern else [pattern]


def _regex_witness(pattern: str, options: str) -> list[str]:
    """Build conservative text witnesses for the complete regex expression."""
    if "F" in options:
        return [pattern]
    witnesses: list[str] = []
    for branch in _split_alternatives(pattern, options):
        value = branch
        # Shell-escaped regex punctuation is represented by its literal byte;
        # BRE grouping delimiters are syntax and therefore disappear.
        value = re.sub(r"\\([()])", "", value)
        value = re.sub(r"\\([.|+?{}])", r"\1", value)
        value = value.replace(r"\[", "[").replace(r"\]", "]")
        value = value.replace(r"\*", "*")
        value = value.replace(r"\^", "^").replace(r"\$", "$")
        value = value.replace(r"[[:space:]]", " ")
        value = value.replace(r"[[:digit:]]", "0")
        value = value.replace(r"[[:alpha:]]", "a")
        value = value.replace(r"[[:alnum:]]", "a")
        value = re.sub(r"\[\^?[^]]*\]", "a", value)
        value = re.sub(r"\.\*|\.\+|\.\?", "x", value)
        value = value.replace(".", "x")
        value = re.sub(r"\\[+?]", "x", value)
        value = re.sub(r"\\\{\d+(?:,\d*)?\\\}", "x", value)
        value = value.replace("^", "", 1) if value.startswith("^") else value
        if value.endswith("$"):
            value = value[:-1]
        value = re.sub(r"[+?*]", "", value)
        value = re.sub(r"\{\d+(?:,\d*)?\}", "", value)
        value = value.replace("(", "").replace(")", "")
        witnesses.append(value)
    return witnesses or ["verify"]


def _matches_comment(check: GrepCheck) -> bool:
    """Return true when the assertion regex can match a comment-shaped line."""
    # A double-quoted shell variable is expanded at runtime.  Its value is
    # unknown to the census, so conservatively mark it unsafe; the repair tool
    # can put a line guard around the expansion without resolving its value.
    if check.quote == '"' and re.search(r"(?<!\\)\$[A-Za-z_][A-Za-z0-9_]*", check.pattern) and not check.resolved_patterns:
        return True
    for pattern in check.resolved_patterns or (check.pattern,):
        try:
            expression = _as_python_regex(pattern, check.options)
        except re.error:
            return True  # An unmodelled dialect is an instrument gap, fail closed.
        for witness in _regex_witness(pattern, check.options):
            probes = tuple(f"{prefix} {witness}" for prefix in COMMENT_PREFIXES)
            if any(expression.search(probe) for probe in probes):
                return True
    return False


def census(backlog: str) -> Census:
    records = _records(backlog)
    assertions: list[GrepCheck] = []
    invocations = 0
    command_records = 0
    manual_records = 0
    unknown_indeterminate: list[GrepCheck] = []
    for record in records:
        verify = record.get("verify")
        if isinstance(verify, str) and verify.strip() == "manual":
            manual_records += 1
        elif isinstance(verify, str):
            command_records += 1
        checks, count, unknown = _grep_checks(record)
        assertions.extend(checks)
        unknown_indeterminate.extend(unknown)
        invocations += count
    unsafe: list[GrepCheck] = []
    indeterminate: list[GrepCheck] = list(unknown_indeterminate)
    for check in assertions:
        try:
            _as_python_regex(check.pattern, check.options)
        except re.error:
            indeterminate.append(check)
        if _matches_comment(check):
            unsafe.append(check)
    result = Census(
        records=len(records),
        command_records=command_records,
        manual_records=manual_records,
        grep_invocations=invocations,
        assertions=tuple(assertions),
        unsafe=tuple(unsafe),
        indeterminate=tuple(indeterminate),
    )
    if (
        result.records != EXPECTED_RECORDS
        or result.command_records != EXPECTED_COMMAND_RECORDS
        or result.manual_records != EXPECTED_MANUAL_RECORDS
        or result.grep_invocations != EXPECTED_GREP_INVOCATIONS
        or len(result.assertions) != EXPECTED_ASSERTIONS
    ):
        raise InstrumentError(
            "B-155 population drift: "
            f"records={result.records}, command_records={result.command_records}, "
            f"manual={result.manual_records}, grep_invocations={result.grep_invocations}, "
            f"assertions={len(result.assertions)}"
        )
    return result


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
    # B-083 is the historical reproducer.  In the open baseline its member is
    # unanchored; after repair it carries the canonical non-comment guard.
    # Exercise both directions so the self-test remains useful after closure.
    old = 'grep -q "byok"'
    guarded = 'grep -q "^[^#/<*-]*byok"'
    if old in target.group(1):
        mutated_block = target.group(1).replace(old, guarded, 1)
        expect = "reduction"
    elif guarded in target.group(1):
        mutated_block = target.group(1).replace(guarded, old, 1)
        expect = "increase"
    else:
        raise InstrumentError("B-083 mutation target is neither open nor guarded")
    if mutated_block == target.group(1):
        raise InstrumentError("B-083 mutation did not change the fixture")
    mutated = backlog[: target.start()] + mutated_block + backlog[target.end() :]
    changed = census(mutated)
    if expect == "reduction" and len(changed.unsafe) >= len(baseline.unsafe):
        raise InstrumentError("anchoring a real B-083 population member did not reduce risk")
    if expect == "increase" and len(changed.unsafe) <= len(baseline.unsafe):
        raise InstrumentError("removing the B-083 guard did not increase risk")

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
