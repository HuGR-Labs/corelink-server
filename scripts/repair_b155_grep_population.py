#!/usr/bin/env python3
"""Apply the reviewed, syntax-specific B-155 repairs.

The repair is deliberately manifest-driven.  Every assertion is classified by
the census (Rust/TypeScript, config/shell, Markdown, data, or stream), and the
policy below supplies the smallest comment guard for that target syntax.  A
new or ambiguous classification aborts; this tool never performs a global
prefix rewrite.  Running it twice is a no-op.
"""
from __future__ import annotations

import argparse
import importlib.util
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "b155_verifier", ROOT / "scripts/verify_b155_backlog_grep_population.py"
)
if SPEC is None or SPEC.loader is None:  # pragma: no cover
    raise SystemExit("cannot load B-155 verifier")
VERIFIER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = VERIFIER
SPEC.loader.exec_module(VERIFIER)


# This is the classification manifest.  Prefixes are intentionally different
# because the target languages have different line-comment grammars.
POLICY: dict[str, str] = {
    "rust": "[^/*]*",
    "typescript": "[^/*]*",
    "config": "[^#]*",
    "shell": "[^#]*",
    "python": "[^#]*",
    "markdown": "[^#<*>-]*",
    "data": "",
    "stream": "[^#/*-]*",
    "unknown": "[^#/*-]*",
}


def _guarded_body(pattern: str, options: str, guard: str) -> tuple[str, str]:
    """Return ``(pattern, options)`` with a whole-expression line guard."""
    if not guard:
        return pattern, options
    if pattern.startswith("^" + guard):
        if pattern == "^" + guard + ".*" and guard.endswith("*"):
            return "^" + guard[:-1] + ".*", options
        return pattern, options
    # Existing guards from earlier, reviewed repairs are narrowed to this
    # target's grammar rather than wrapped a second time.
    replaced = False
    for old in ("[^#]*", "[^/]*", "[^/*]*", "[^#/*<*>-]*", "[^#<*>-]*"):
        if pattern.startswith("^" + old):
            pattern = pattern[len("^" + old) :]
            replaced = True
            break
    if not replaced and pattern.startswith("^[^"):
        end = pattern.find("]", 3)
        if end > 0:
            pattern = pattern[end + 1 :]
            replaced = True
    if not replaced and pattern.startswith("^"):
        pattern = pattern[1:]
    if pattern in {".", ".*"}:
        return "^" + (guard[:-1] if guard.endswith("*") else guard) + ".*", options
    if "F" in options:
        pattern = VERIFIER.re.escape(pattern)
        options = options.replace("F", "E")
    if "E" in options and "|" in pattern:
        pattern = "(" + pattern + ")"
    elif "E" not in options and r"\|" in pattern:
        pattern = r"\(" + pattern + r"\)"
    return "^" + guard + pattern, options


def _matches(line: str):
    quoted = list(VERIFIER.GREP.finditer(line))
    starts = {match.start() for match in quoted}
    matches = list(quoted)
    for match in VERIFIER.GREP_UNQUOTED.finditer(line):
        if match.start() not in starts:
            matches.append(match)
    return matches


def _rewrite_verify(verify: str, unsafe: list[object]) -> tuple[str, int]:
    """Rewrite exact assertion spans, including recursively quoted shells."""
    by_key = {(x.pattern, x.options): x for x in unsafe}
    matches = list(VERIFIER.GREP.finditer(verify))
    starts = {m.start() for m in matches}
    matches.extend(m for m in VERIFIER.GREP_UNQUOTED.finditer(verify) if m.start() not in starts)
    replacements: list[tuple[int, int, str]] = []
    used: set[tuple[int, int]] = set()
    for match in matches:
        line_start = verify.rfind("\n", 0, match.start()) + 1
        options = match.group("options") or ""
        pattern = match.group("pattern")
        normalized = pattern
        check = by_key.get((pattern, options))
        if check is None:
            check = next(
                (candidate for (candidate_pattern, candidate_options), candidate in by_key.items()
                 if candidate_options == options and normalized.startswith(candidate_pattern)),
                None,
            )
        if check is None or "v" in options.replace("--", ""):
            continue
        boundary = VERIFIER.COMMAND_BOUNDARY.search(verify[line_start : match.start()])
        nested_context = VERIFIER.NESTED_SHELL.search(verify[: match.start()])
        if boundary is None and nested_context is None:
            continue
        guard = POLICY.get(check.source_kind)
        if guard is None:
            raise VERIFIER.InstrumentError(
                f"no B-155 classification policy for {check.source_kind!r}"
            )
        repaired, new_options = _guarded_body(normalized, options, guard)
        if not match.groupdict().get("quote"):
            repaired = '"' + repaired.replace('"', '\\"') + '"'
        replacement = verify[match.start() : match.end()]
        pattern_start = match.start("pattern") - match.start()
        pattern_end = match.end("pattern") - match.start()
        replacement = replacement[:pattern_start] + repaired + replacement[pattern_end:]
        if new_options != options:
            options_start = match.start("options") - match.start()
            options_end = match.end("options") - match.start()
            replacement = replacement[:options_start] + new_options + replacement[options_end:]
        replacements.append((match.start(), match.end(), replacement))
        used.add(match.span())
    for start, end, replacement in reversed(replacements):
        verify = verify[:start] + replacement + verify[end:]
    return verify, len(used)


def repair(backlog: str) -> tuple[str, int]:
    changes: list[tuple[int, int, str]] = []
    total = 0
    for fence in VERIFIER.FENCE.finditer(backlog):
        block = fence.group(1)
        record = VERIFIER.yaml.safe_load(block)
        if not isinstance(record, dict):
            raise VERIFIER.InstrumentError("manifest record is not a mapping")
        verify = record.get("verify")
        if not isinstance(verify, str) or verify.strip() == "manual":
            continue
        checks, _, indeterminate = VERIFIER._grep_checks(record)
        if indeterminate:
            names = ", ".join(f"{x.record_id}:{x.line}" for x in indeterminate)
            raise VERIFIER.InstrumentError(f"indeterminate assertion(s): {names}")
        unsafe = {
            (x.line, x.pattern, x.options): x
            for x in checks
            if VERIFIER._matches_comment(x)
        }
        if not unsafe:
            continue
        rewritten, changed = _rewrite_verify(verify, list(unsafe.values()))
        if not changed:
            raise VERIFIER.InstrumentError(f"unsafe assertions in {record['id']} were not repaired")
        raw_lines = block.splitlines(True)
        if any(line.startswith("verify: |") for line in raw_lines):
            start = next(i for i, line in enumerate(raw_lines) if line.startswith("verify: |")) + 1
            stop = next(i for i in range(start, len(raw_lines)) if raw_lines[i].startswith("verify-means:"))
            body = ["  " + line if line.strip() else line for line in rewritten.splitlines(True)]
            new_block = "".join(raw_lines[:start] + body + raw_lines[stop:])
        else:
            key = next(i for i, line in enumerate(raw_lines) if line.startswith("verify:"))
            encoded = '"' + rewritten.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"\n'
            raw_lines[key] = "verify: " + encoded
            new_block = "".join(raw_lines)
        changes.append((fence.start(1), fence.end(1), new_block))
        total += changed
    output = backlog
    for start, end, replacement in reversed(changes):
        output = output[:start] + replacement + output[end:]
    return output, total


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backlog", type=Path, default=ROOT / "BACKLOG.md")
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args(argv)
    try:
        original = args.backlog.read_text(encoding="utf-8")
        rewritten, changed = repair(original)
        result = VERIFIER.census(rewritten)
    except (OSError, VERIFIER.InstrumentError) as error:
        print(f"instrument error: {error}", file=sys.stderr)
        return 2
    print(
        f"B-155 repair: changed_assertions={changed} "
        f"remaining_comment_sensitive={len(result.unsafe)} "
        f"remaining_indeterminate={len(result.indeterminate)}"
    )
    if args.write:
        args.backlog.write_text(rewritten, encoding="utf-8")
    return 0 if not result.unsafe and not result.indeterminate else 1


if __name__ == "__main__":
    raise SystemExit(main())
