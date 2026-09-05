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
from functools import lru_cache
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
# These are still shell command boundaries: the grep executable is wrapped
# rather than invoked as the first word.  Keeping the wrappers explicit avoids
# treating prose such as `echo sudo grep ...` as an assertion while covering
# the common command forms used by backlog verifies.
WRAPPED_COMMAND_BOUNDARY = re.compile(
    r"(?:^|[;&|(!]|\$\(|\b(?:if|elif|then|while|until|do|command|builtin|exec)\b)"
    r"\s*(?:!\s*)?(?:[A-Za-z_][A-Za-z0-9_]*=(?:'[^']*'|\"[^\"]*\"|\S+)\s*)*"
    r"(?:(?:sudo|env|git|xargs)(?:\s+-[^\s;&|()]+|\s+[A-Za-z_][A-Za-z0-9_]*=[^\s;&|()]+)*\s+)+$"
)
REDIRECTION_BOUNDARY = re.compile(
    r"(?:^|[;&|(!]|\$\(|\b(?:if|elif|then|while|until|do|command|builtin|exec)\b)"
    r"\s*(?:!\s*)?(?:[A-Za-z_][A-Za-z0-9_]*=(?:'[^']*'|\"[^\"]*\"|\S+)\s*)*"
    r"(?:\d*(?:>>>|<<<|>>|<<|>|<)\s*(?:'[^']*'|\"[^\"]*\"|[^\s;&|()]+)\s*)+$"
)
NESTED_SHELL = re.compile(r"\b(?:bash|sh|zsh)\s+-c\b")
ID = re.compile(r"^B-\d{3}$")
COMMENT_PREFIXES = ("//", "#", "/*", "<!--", "*", "--")
MAX_BACKLOG_BYTES = 2_000_000
MAX_NESTED_SHELL_DEPTH = 8
MAX_NESTED_PAYLOAD_BYTES = 200_000
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
    source_kind: str = "unknown"
    comment_prefixes: tuple[str, ...] = ()


@dataclass(frozen=True)
class Census:
    records: int
    command_records: int
    manual_records: int
    grep_invocations: int
    assertions: tuple[GrepCheck, ...]
    unsafe: tuple[GrepCheck, ...]
    indeterminate: tuple[GrepCheck, ...]


EXPECTED_RECORDS = 170
EXPECTED_COMMAND_RECORDS = 139
EXPECTED_MANUAL_RECORDS = 31
EXPECTED_GREP_INVOCATIONS = 301
EXPECTED_ASSERTIONS = 284


@lru_cache(maxsize=32)
def _records(backlog: str) -> list[dict[str, object]]:
    if len(backlog.encode("utf-8")) > MAX_BACKLOG_BYTES:
        raise InstrumentError("backlog exceeds bounded census size")
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


@lru_cache(maxsize=512)
def _source_paths(verify: str) -> tuple[str, ...]:
    """Collect literal file/path hints without evaluating shell syntax."""
    paths: list[str] = []
    for match in re.finditer(
        r"\b[A-Za-z_][A-Za-z0-9_]*=(?:'([^']+)'|\"([^\"]+)\"|([^\s;&|()]+))",
        verify,
    ):
        value = next((part for part in match.groups() if part is not None), "")
        if "/" in value or re.search(r"\.(?:rs|ts|tsx|js|mjs|py|md|mdx|ya?ml|toml|json|sh)$", value):
            paths.append(value)
    for token in re.findall(r"(?:[A-Za-z0-9_.-]+/)+[^\s;&|()]+|[A-Za-z0-9_.-]+\.(?:rs|ts|tsx|js|mjs|py|md|mdx|ya?ml|toml|json|sh)", verify):
        paths.append(token.strip("'\""))
    return tuple(dict.fromkeys(paths))


def _source_kind(line: str, verify: str) -> tuple[str, tuple[str, ...]]:
    """Classify the likely target syntax for a grep assertion.

    The classifier is intentionally conservative: a missing or mixed target
    keeps the full comment-prefix set, which makes the check fail closed.
    """
    assignments: dict[str, str] = {}
    for match in re.finditer(
        r"\b([A-Za-z_][A-Za-z0-9_]*)=(?:'([^']+)'|\"([^\"]+)\"|([^\s;&|()]+))",
        verify,
    ):
        assignments[match.group(1)] = next(
            part for part in match.groups()[1:] if part is not None
        )
    linked = []
    for name in re.findall(r"\$(?:\{)?([A-Za-z_][A-Za-z0-9_]*)", line):
        if name in assignments:
            linked.append(assignments[name])
    direct = re.findall(
        r"(?:[A-Za-z0-9_.-]+/)+[^\s;&|()]+|[A-Za-z0-9_.-]+\.(?:rs|ts|tsx|js|mjs|py|md|mdx|ya?ml|toml|json|sh|sql|bazelrc)",
        line,
    )
    hints = " ".join(linked or direct or _source_paths(verify)) + " " + line
    suffixes = {suffix.lower() for suffix in re.findall(r"\.([A-Za-z0-9]+)", hints)}
    if "rs" in suffixes:
        return "rust", ("//", "/*", "*")
    if suffixes & {"ts", "tsx", "js", "mjs"}:
        return "typescript", ("//", "/*", "*")
    if suffixes & {"yaml", "yml", "toml", "sql", "bazelrc", "env"} or ".github/workflows" in hints or "migrations/" in hints:
        return "config", ("#",)
    if suffixes & {"sh"}:
        return "shell", ("#",)
    if suffixes & {"py"}:
        return "python", ("#",)
    if suffixes & {"md", "mdx"} or "apps/docs" in hints or "marketing/" in hints:
        return "markdown", ("#", "<!--", ">", "*", "-")
    if suffixes & {"json"}:
        return "data", ()
    if "|" in line or "$" in line:
        return "stream", COMMENT_PREFIXES
    if "grep -r" in line or "grep -l" in line:
        return "data", ()
    return "unknown", COMMENT_PREFIXES


def _is_command_boundary(text: str) -> bool:
    """Recognize direct, wrapped, and redirection-prefixed shell commands."""
    return bool(
        COMMAND_BOUNDARY.search(text)
        or WRAPPED_COMMAND_BOUNDARY.search(text)
        or REDIRECTION_BOUNDARY.search(text)
    )


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
    return _grep_checks_text(verify, str(record["id"]), verify)


def _grep_checks_text(
    verify: str,
    record_id: str,
    context: str,
    line_offset: int = 0,
    depth: int = 0,
) -> tuple[list[GrepCheck], int, list[GrepCheck]]:
    """Census shell text and recurse into literal ``shell -c`` payloads.

    A quoted ``bash -c '...'`` is a second shell program, not prose. Its
    payload is masked from the outer scan (newlines are retained for line
    numbers) and scanned recursively. Dynamic or unterminated payloads fail
    closed instead of silently shrinking the grep population.
    """
    if depth > MAX_NESTED_SHELL_DEPTH:
        raise InstrumentError("nested shell depth exceeds bounded census limit")
    if len(verify.encode("utf-8")) > MAX_NESTED_PAYLOAD_BYTES:
        raise InstrumentError("nested shell payload exceeds bounded census limit")
    masked, nested = _mask_nested_shells(verify)
    checks: list[GrepCheck] = []
    indeterminate: list[GrepCheck] = []
    invocations = 0
    known_starts: set[int] = set()
    for line_number, line in enumerate(masked.splitlines(), 1):
        known_starts: set[int] = set()
        for match in GREP.finditer(line):
            # A grep-looking string in an embedded Python/awk expression is
            # not a shell invocation. Require a shell command boundary (or a
            # command substitution/assignment immediately before it).
            if not _is_command_boundary(line[: match.start()]):
                continue
            invocations += 1
            known_starts.add(match.start())
            options = match.group("options") or ""
            pattern = match.group("pattern")
            resolved, unresolved = _resolved_patterns(pattern, context)
            kind, prefixes = _source_kind(line, context)
            check = GrepCheck(
                record_id=record_id,
                line=line_number + line_offset,
                pattern=pattern,
                options=options,
                source=line.strip(),
                quote=match.group("quote"),
                resolved_patterns=resolved,
                source_kind=kind,
                comment_prefixes=prefixes,
            )
            # `grep -v` is a filter in a pipeline, not a positive capability
            # assertion. It remains in the invocation census, but is not in the
            # population whose comment sensitivity determines this item.
            if "v" not in options.replace("--", ""):
                checks.append(check)
            if unresolved and not _generated_hex_pattern_file(check, verify):
                indeterminate.append(check)
            if check.source_kind == "unknown":
                indeterminate.append(check)
        # The shell permits a bare regex word (`grep unsafe file`) in addition
        # to the quoted form above.  Parse it separately, retaining the same
        # boundary and polarity rules.  A variable or shell expression is
        # intentionally not guessed: it becomes an explicit indeterminate
        # population member below.
        for match in GREP_UNQUOTED.finditer(line):
            if match.start() in known_starts:
                continue
            if not _is_command_boundary(line[: match.start()]):
                continue
            invocations += 1
            known_starts.add(match.start())
            options = match.group("options") or ""
            pattern = match.group("pattern")
            resolved, unresolved = _resolved_patterns(pattern, context)
            kind, prefixes = _source_kind(line, context)
            check = GrepCheck(
                record_id=record_id,
                line=line_number + line_offset,
                pattern=pattern,
                options=options,
                source=line.strip(),
                resolved_patterns=resolved,
                source_kind=kind,
                comment_prefixes=prefixes,
            )
            if "v" not in options.replace("--", ""):
                checks.append(check)
            if unresolved and not _generated_hex_pattern_file(check, verify):
                indeterminate.append(check)
            if check.source_kind == "unknown":
                indeterminate.append(check)
        # A grep invocation with an unquoted/dynamic pattern is still an
        # invocation, but its dialect and polarity cannot be proven by this
        # parser.  Count it and fail closed instead of silently shrinking the
        # assertion population.
        for word in re.finditer(r"\bgrep\b", line):
            if word.start() in known_starts:
                continue
            if not _is_command_boundary(line[: word.start()]):
                continue
            invocations += 1
            indeterminate.append(
                GrepCheck(
                    record_id=record_id,
                    line=line_number + line_offset,
                    pattern="",
                    options="",
                    source=line.strip(),
                )
            )
    for payload, first_line in nested:
        nested_checks, nested_invocations, nested_indeterminate = _grep_checks_text(
            payload, record_id, context, line_offset + first_line - 1, depth + 1
        )
        checks.extend(nested_checks)
        invocations += nested_invocations
        indeterminate.extend(nested_indeterminate)
    return checks, invocations, indeterminate


def _is_shell_expansion(text: str, offset: int) -> bool:
    """Return whether ``$`` at *offset* starts active shell syntax."""
    if offset + 1 >= len(text):
        return False
    next_char = text[offset + 1]
    return (
        next_char in "{("
        or next_char in "@*#?$!-0123456789"
        or next_char == "_"
        or next_char.isalpha()
    )


def _decode_nested_payload(text: str, start: int) -> tuple[str, int]:
    """Decode one quoted ``shell -c`` argument using shell quote rules."""
    if start >= len(text) or text[start] not in "\"'":
        raise InstrumentError("nested shell -c payload is not a literal quote")
    quote = text[start]
    index = start + 1
    decoded: list[str] = []
    while index < len(text):
        char = text[index]
        if quote == "'":
            if char == "'":
                quote = ""
                index += 1
                continue
            # Backslash has no special meaning in a POSIX single-quoted word.
            decoded.append(char)
            index += 1
            continue
        if quote == '"' and char == '"':
            quote = ""
            index += 1
            continue
        if not quote:
            # Adjacent quoted/unquoted pieces form one shell word.  This is
            # how a literal payload can contain an apostrophe, and it is also
            # where an expansion can be smuggled after a literal prefix.
            if char.isspace() or char in ";;&|()<>\n":
                return "".join(decoded), index
            if char in "\"'":
                quote = char
                index += 1
                continue
            if char == "\\":
                if index + 1 >= len(text):
                    raise InstrumentError("unterminated nested shell -c payload")
                decoded.append(text[index + 1])
                index += 2
                continue
            if char == "$" and _is_shell_expansion(text, index):
                raise InstrumentError(
                    "nested shell -c payload contains an active expansion"
                )
            if char == "`":
                raise InstrumentError(
                    "nested shell -c payload contains an active command substitution"
                )
            decoded.append(char)
            index += 1
            continue
        # In double quotes, backslash quotes only $, `, ", \\, and newline.
        if char == "\\":
            if index + 1 >= len(text):
                raise InstrumentError("unterminated nested shell -c payload")
            escaped = text[index + 1]
            if escaped in "$`\"\\\n":
                if escaped != "\n":
                    decoded.append(escaped)
            else:
                decoded.extend(("\\", escaped))
            index += 2
            continue
        if char == "$" and _is_shell_expansion(text, index):
            raise InstrumentError(
                "nested shell -c payload contains an active expansion"
            )
        if char == "`":
            raise InstrumentError(
                "nested shell -c payload contains an active command substitution"
            )
        decoded.append(char)
        index += 1
    if not quote:
        return "".join(decoded), index
    raise InstrumentError("unterminated nested shell -c payload")


def _mask_nested_shells(text: str) -> tuple[str, list[tuple[str, int]]]:
    """Mask static ``bash|sh|zsh -c`` bodies and return their payloads."""
    spans: list[tuple[int, int, str, int]] = []
    for match in NESTED_SHELL.finditer(text):
        # An outer match owns its quoted body; inner shell text is discovered
        # when that body is scanned recursively.
        if any(start <= match.start() < end for start, end, _, _ in spans):
            continue
        line_start = text.rfind("\n", 0, match.start()) + 1
        if not _is_command_boundary(text[line_start : match.start()]):
            continue
        pos = match.end()
        while pos < len(text) and text[pos].isspace():
            pos += 1
        payload, end = _decode_nested_payload(text, pos)
        first_line = text.count("\n", 0, pos + 1) + 1
        spans.append((match.start(), end, payload, first_line))

    if not spans:
        return text, []
    chars = list(text)
    payloads: list[tuple[str, int]] = []
    for start, end, payload, first_line in spans:
        payloads.append((payload, first_line))
        for index in range(start, end):
            if chars[index] != "\n":
                chars[index] = " "
    return "".join(chars), payloads


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
        # Python defaults to ERE.  BRE only enables grouping/quantifiers when
        # those operators are escaped, so normalize the opposite spellings
        # before compiling.  This matters for literal code such as
        # ``blockConcurrencyWhile(async () =>`` in the live backlog.
        normalized: list[str] = []
        index = 0
        while index < len(translated):
            char = translated[index]
            if char == "\\" and index + 1 < len(translated):
                escaped = translated[index + 1]
                if escaped in "(){}+?":
                    # BRE enables these operators only in escaped form.
                    normalized.append(escaped)
                    index += 2
                    continue
            if char in "(){}+?":
                normalized.append("\\" + char)
            else:
                normalized.append(char)
            index += 1
        translated = "".join(normalized)
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
            prefixes = check.comment_prefixes or COMMENT_PREFIXES
            probes = tuple(f"{prefix} {witness}" for prefix in prefixes)
            if any(expression.search(probe) for probe in probes):
                return True
    return False


@lru_cache(maxsize=32)
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
    # B-083 is the historical reproducer.  Removing its syntax-specific guard
    # must reopen the census, proving the positive population is load-bearing.
    guarded = 'grep -q "^[^#]*byok"'
    old = 'grep -q "byok"'
    if guarded not in target.group(1):
        raise InstrumentError("B-083 mutation target is not the canonical guarded member")
    mutated_block = target.group(1).replace(guarded, old, 1)
    if mutated_block == target.group(1):
        raise InstrumentError("B-083 mutation did not change the fixture")
    mutated = backlog[: target.start()] + mutated_block + backlog[target.end() :]
    changed = census(mutated)
    if len(changed.unsafe) <= len(baseline.unsafe):
        raise InstrumentError("removing a real B-083 guard did not increase risk")

    # B-112 starts its nested `bash -c` body with a grep, the boundary that a
    # flat line scanner used to miss.  Removing that guard must reopen risk.
    nested_target = re.search(
        r"(id: B-112\n.*?verify: \|\n(?:  .*\n)+?)",
        backlog,
        re.DOTALL,
    )
    if nested_target is None:
        raise InstrumentError("B-112 nested-shell mutation fixture is missing")
    nested_guarded = 'grep -q "^[^#]*cargo zigbuild"'
    nested_open = 'grep -q "cargo zigbuild"'
    if nested_guarded not in nested_target.group(1):
        raise InstrumentError("B-112 nested grep is not the canonical guarded member")
    nested_block = nested_target.group(1).replace(nested_guarded, nested_open, 1)
    if nested_block == nested_target.group(1):
        raise InstrumentError("B-112 mutation did not change the fixture")
    nested_mutated = (
        backlog[: nested_target.start()]
        + nested_block
        + backlog[nested_target.end() :]
    )
    nested_changed = census(nested_mutated)
    if len(nested_changed.unsafe) <= len(baseline.unsafe):
        raise InstrumentError("removing the B-112 nested grep guard did not increase risk")

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
