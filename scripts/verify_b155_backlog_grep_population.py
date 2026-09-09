#!/usr/bin/env python3
"""Audit backlog verify commands for comment-sensitive grep checks.

This path remains the executable and import-compatible B-155 entry point.
Parser implementation lives in :mod:`b155_backlog_grep_parser`; all helpers
used by the tests and repair tool are re-exported below so the split does not
change the established public surface.
"""
from __future__ import annotations

import argparse
import importlib.util
import re
import sys
from pathlib import Path

try:
    from . import b155_backlog_grep_parser as _parser
except ImportError:  # Direct file loading has no package context.
    parser_path = Path(__file__).with_name("b155_backlog_grep_parser.py")
    parser_spec = importlib.util.spec_from_file_location(
        "b155_backlog_grep_parser", parser_path
    )
    if parser_spec is None or parser_spec.loader is None:  # pragma: no cover
        raise ImportError(f"cannot load B-155 parser: {parser_path}")
    _parser = importlib.util.module_from_spec(parser_spec)
    sys.modules[parser_spec.name] = _parser
    parser_spec.loader.exec_module(_parser)


# Keep the legacy script's import surface stable.  The explicit manifest is
# intentional: adding parser internals requires consciously extending this
# compatibility contract rather than silently exporting implementation names.
_PARSER_EXPORTS = (
    "yaml", "BACKLOG_OPEN", "COMMENT_PREFIXES", "COMMAND_BOUNDARY", "Census",
    "EXPECTED_ASSERTIONS", "EXPECTED_COMMAND_RECORDS", "EXPECTED_GREP_INVOCATIONS",
    "EXPECTED_MANUAL_RECORDS", "EXPECTED_RECORDS", "FENCE", "GREP", "GREP_UNQUOTED",
    "GrepCheck", "_ShellToken", "ID", "InstrumentError", "MAX_BACKLOG_BYTES",
    "MAX_NESTED_PAYLOAD_BYTES", "MAX_NESTED_SHELL_DEPTH", "NESTED_SHELL",
    "POSIX_CLASSES", "REDIRECTION_BOUNDARY", "SHELL_VARIABLE", "WRAPPED_COMMAND_BOUNDARY",
    "_as_python_regex", "_decode_nested_payload", "_generated_hex_pattern_file",
    "_grep_checks", "_grep_checks_text", "_grep_command_kind", "_grep_file_operands",
    "_is_command_boundary", "_is_shell_expansion", "_mask_nested_shells",
    "_matches_comment", "_records", "_redirection_parts", "_regex_witness",
    "_resolved_patterns", "_shell_tokens", "_shell_values", "_skip_grep_option",
    "_skip_grep_redirection", "_source_kind", "_GREP_SHORT_VALUE_OPTIONS",
    "_GREP_LONG_VALUE_OPTIONS", "_COMMAND_SEPARATORS", "_CONTROL_WORDS",
    "_KNOWN_WRAPPERS", "_ASSIGNMENT", "_REDIRECTION", "_split_alternatives", "census",
)
# Keep the names consumed by this module statically visible to linters while
# the manifest above handles the broader compatibility surface.
census = _parser.census
InstrumentError = _parser.InstrumentError
for _name in _PARSER_EXPORTS:
    globals()[_name] = getattr(_parser, _name)
del _name


def mutation_self_test(backlog: str) -> None:
    """Require a known real population mutation to change the census."""
    baseline = census(backlog)
    target = re.search(
        r"(id: B-033\n.*?verify: \|\n(?:  .*\n)+?)",
        backlog,
        re.DOTALL,
    )
    # B-033 is a current real assertion. Removing its syntax-specific guard
    # must reopen the census, proving the positive population is load-bearing.
    guarded = 'grep -q "^[^#]*clippy --workspace --all-targets"'
    old = 'grep -q "clippy --workspace --all-targets"'
    if target is None or guarded not in target.group(1):
        raise InstrumentError("B-033 mutation target is not the canonical guarded member")
    if guarded not in target.group(1):
        raise InstrumentError("B-033 mutation target is not the canonical guarded member")
    mutated_block = target.group(1).replace(guarded, old, 1)
    if mutated_block == target.group(1):
        raise InstrumentError("B-033 mutation did not change the fixture")
    mutated = backlog[: target.start()] + mutated_block + backlog[target.end() :]
    changed = census(mutated)
    if len(changed.unsafe) <= len(baseline.unsafe):
        raise InstrumentError("removing a real B-033 guard did not increase risk")

    # B-055 starts its nested `bash -c` body with guarded greps, the boundary
    # that a flat line scanner used to miss. Removing that guard must reopen
    # risk.
    nested_target = re.search(
        r"(id: B-055\n.*?verify: \|\n(?:  .*\n)+?)",
        backlog,
        re.DOTALL,
    )
    if nested_target is None:
        raise InstrumentError("B-055 nested-shell mutation fixture is missing")
    nested_guarded = 'grep -q "^[^/*]*Sli::AvailCasGet"'
    nested_open = 'grep -q "Sli::AvailCasGet"'
    if nested_guarded not in nested_target.group(1):
        raise InstrumentError("B-055 nested grep is not the canonical guarded member")
    nested_block = nested_target.group(1).replace(nested_guarded, nested_open, 1)
    if nested_block == nested_target.group(1):
        raise InstrumentError("B-055 mutation did not change the fixture")
    nested_mutated = (
        backlog[: nested_target.start()]
        + nested_block
        + backlog[nested_target.end() :]
    )
    nested_changed = census(nested_mutated)
    if len(nested_changed.unsafe) <= len(baseline.unsafe):
        raise InstrumentError("removing the B-055 nested grep guard did not increase risk")

    # Parser completeness is also load-bearing: removing every fenced record
    # cannot become a falsely clean zero-population result.
    try:
        census("no backlog records")
    except InstrumentError:
        pass
    else:
        raise InstrumentError("empty backlog mutation was accepted as a clean census")

    # B-373 is the newest real record and must remain part of the closed
    # population denominator. Removing its complete fence must fail closed,
    # just like the established B-084 population mutation below.
    b373_fence = next(
        (match for match in _parser.FENCE.finditer(backlog)
         if "id: B-373\n" in match.group(1)),
        None,
    )
    if b373_fence is None:
        raise InstrumentError("B-373 mutation target is missing")
    without_b373 = backlog[: b373_fence.start()] + backlog[b373_fence.end() :]
    try:
        census(without_b373)
    except InstrumentError:
        pass
    else:
        raise InstrumentError("removing B-373 was accepted as a clean census")


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
