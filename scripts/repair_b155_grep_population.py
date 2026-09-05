#!/usr/bin/env python3
"""Apply bounded comment guards to the B-155 grep population.

Only assertions the structural census classifies as comment-sensitive are
changed.  A pattern with an existing line anchor is changed only when its
prefix is the known ``[^#]``/``[^/]`` comment guard; ambiguous anchored forms
abort instead of silently weakening their item contract.
"""
from __future__ import annotations

import argparse
import importlib.util
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


COMMENT_GUARD = "^[^#/<*-]*"


def harden_pattern(pattern: str, options: str = "") -> str:
    # `grep .` is a non-empty-line predicate, so merely prefixing the guard
    # would still let the wildcard consume the comment marker at position 0.
    if "F" not in options and pattern in {".", ".*"}:
        return "^[^#/<*-].*"
    if pattern.startswith("^[^#]*"):
        return COMMENT_GUARD + pattern[len("^[^#]*") :]
    if pattern.startswith("^[^/]*"):
        return COMMENT_GUARD + pattern[len("^[^/]*") :]
    if pattern.startswith("^"):
        raise VERIFIER.InstrumentError(
            f"ambiguous anchored pattern requires item-specific repair: {pattern!r}"
        )
    # The guard must apply to the whole expression.  Without grouping,
    # `guardfoo|bar` leaves the `bar` alternative able to match a comment.
    if "F" in options:
        # A fixed-string assertion cannot carry a regex line guard.  The
        # caller switches F to E and escapes the literal body before applying
        # this guard, preserving the original fixed-string semantics.
        body = pattern if VERIFIER.SHELL_VARIABLE.search(pattern) else VERIFIER.re.escape(pattern)
    elif "E" in options and "|" in pattern:
        body = "(" + pattern + ")"
    elif "E" not in options and r"\|" in pattern:
        body = r"\(" + pattern + r"\)"
    else:
        body = pattern
    return COMMENT_GUARD + body


def _grep_matches(line: str):
    quoted = list(VERIFIER.GREP.finditer(line))
    starts = {match.start() for match in quoted}
    matches = list(quoted)
    for match in VERIFIER.GREP_UNQUOTED.finditer(line):
        if match.start() not in starts:
            matches.append(match)
    return matches


def _rewrite_verify(record: dict[str, object]) -> tuple[str, int]:
    verify = record.get("verify")
    if not isinstance(verify, str) or verify.strip() == "manual":
        return "", 0
    checks, _, indeterminate = VERIFIER._grep_checks(record)
    if indeterminate:
        names = ", ".join(f"{check.record_id}:{check.line}" for check in indeterminate)
        raise VERIFIER.InstrumentError(
            f"cannot repair indeterminate grep pattern(s): {names}"
        )
    unsafe = {(check.line, check.pattern, check.options) for check in checks if VERIFIER._matches_comment(check)}
    if not unsafe:
        return verify, 0
    lines = verify.splitlines(True)
    changed = 0
    for index, original in enumerate(lines):
        replacements: list[tuple[int, int, str]] = []
        for match in _grep_matches(original):
            if not VERIFIER.COMMAND_BOUNDARY.search(original[: match.start()]):
                continue
            options = match.group("options") or ""
            pattern = match.group("pattern")
            if "v" in options.replace("--", ""):
                continue
            if (index + 1, pattern, options) not in unsafe:
                continue
            replacement = harden_pattern(pattern, options)
            # The canonical guard contains shell metacharacters; quote a
            # previously bare pattern so the repaired command remains valid
            # shell and the parser cannot mistake `<`/`>` for redirection.
            if not match.groupdict().get("quote"):
                replacement = '"' + replacement.replace('"', '\\"') + '"'
            replacements.append((match.start("pattern"), match.end("pattern"), replacement))
            if "F" in options:
                replacements.append((match.start("options"), match.end("options"), options.replace("F", "E")))
        for start, end, replacement in reversed(replacements):
            original = original[:start] + replacement + original[end:]
        if replacements:
            # Count assertions, not replacement spans (`-F` repairs also
            # replace the option with `-E`).
            changed += 1
        lines[index] = original
    return "".join(lines), changed


def repair(backlog: str) -> tuple[str, int]:
    changes: list[tuple[int, int, str]] = []
    total_changed = 0
    for fence in VERIFIER.FENCE.finditer(backlog):
        block = fence.group(1)
        record = VERIFIER.yaml.safe_load(block)
        if not isinstance(record, dict):
            continue
        verify = record.get("verify")
        if not isinstance(verify, str) or verify.strip() == "manual":
            continue
        rewritten, changed = _rewrite_verify(record)
        if not changed:
            continue
        lines = block.splitlines(True)
        if any(line.startswith("verify: |") for line in lines):
            start = next(i for i, line in enumerate(lines) if line.startswith("verify: |")) + 1
            stop = next(i for i in range(start, len(lines)) if lines[i].startswith("verify-means:"))
            body = ["  " + line if line.strip() else line for line in rewritten.splitlines(True)]
            new_block = "".join(lines[:start] + body + lines[stop:])
        else:
            key = next(i for i, line in enumerate(lines) if line.startswith("verify:"))
            # Scalar YAML verifies are commonly double-quoted.  Re-encode the
            # complete shell string so newly inserted shell quotes and regex
            # backslashes cannot terminate or alter the YAML scalar.
            encoded = '"' + rewritten.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'
            lines[key] = "verify: " + encoded + "\n"
            new_block = "".join(lines)
        changes.append((fence.start(1), fence.end(1), new_block))
        total_changed += changed
    output = backlog
    # Apply offsets against the untouched source in reverse order; rebuilding
    # from an already-lengthened suffix would otherwise splice later edits at
    # stale offsets and silently lose earlier repairs.
    for start, end, new_block in reversed(changes):
        output = output[:start] + new_block + output[end:]
    return output, total_changed


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
