#!/usr/bin/env python3
"""Classify every `| head` that runs under `pipefail`, and fail on the ones that can abort.

Why this exists
---------------
`head` closes its input as soon as the cap is reached. Under `set -o pipefail`
the producer that is still writing gets SIGPIPE, the pipeline reports 141, and
under `set -e` the script dies — on a line whose only job was to take the first
match. Unlike `grep -q` (whose SIGPIPE inverts a *result*, silently), `head`
cannot give a wrong answer: it can only abort. So the question is never "is
there a `| head`" but the much narrower "is this pipeline's exit status
consumed, and can the producer outrun the cap".

Counting occurrences answers neither, and a raw count is what made this item sit
open: 72 naive matches, 65 once comments were anchored out, and most of what
remained was prose or an argument whose status is discarded by construction.
This script answers the real question per site instead.

Classification
--------------
LITERAL   the `| head` sits inside a quoted string with no command substitution
          around it — operator instructions printed by `echo`, a docstring.
          Not shell code at all.
ARGUMENT  `$( … | head … )` in an argument position, e.g. `log "x: $(f | head -1)"`.
          The substitution's status is discarded by the enclosing command, which
          has its own (successful) status. SIGPIPE here changes nothing.
CONSUMED  the status reaches `set -e`: an assignment RHS (`V=$(f | head -1)`,
          `A+=("$(f | head -c 12)")`) or a bare pipeline statement. These abort.

CONSUMED sites are failures. Fix them by removing the pipe rather than muffling
it — take the whole output and slice it in the shell (`${v%%$'\\n'*}`,
`${v:0:64}`), or bound the producer itself (`grep -m1`, `find -print -quit`).
`| head … || true` is accepted only where the value is genuinely unused.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

PIPEFAIL = re.compile(r"set -[a-z]*o pipefail")
HEAD = re.compile(r"\|\s*head\b")
# `V=`, `V+=`, `local V=`, `export V=`, `A+=(` — an assignment whose RHS status
# is the status of the last command substitution it performs.
ASSIGN = re.compile(r"^\s*(?:local\s+|export\s+|declare\s+(?:-\w+\s+)?|readonly\s+)?[A-Za-z_]\w*\+?=")
GUARDED = re.compile(r"\|\|\s*(?:true|echo|:)\b")


def scan_line(line: str):
    """Yield (kind, column) for each `| head` on the line.

    Walks the line once tracking quote state and command-substitution depth, so
    a `| head` is judged by where it actually sits rather than by a regex over
    the whole line.
    """
    out = []
    i = 0
    quote = None          # None | "'" | '"'
    subst_depth = 0       # depth of $( … ) nesting
    quote_has_subst = []  # per open double-quote: did a $( open inside it?
    n = len(line)
    while i < n:
        c = line[i]
        if c == "\\" and quote != "'":
            i += 2
            continue
        if quote is None and c == "#" and (i == 0 or line[i - 1].isspace()):
            break  # comment tail
        if quote is None and c in "'\"":
            quote = c
            if c == '"':
                quote_has_subst.append(False)
            i += 1
            continue
        if quote is not None and c == quote:
            if c == '"':
                quote_has_subst.pop()
            quote = None
            i += 1
            continue
        if quote != "'" and line.startswith("$(", i):
            subst_depth += 1
            if quote == '"' and quote_has_subst:
                quote_has_subst[-1] = True
            i += 2
            continue
        if quote != "'" and c == ")" and subst_depth > 0:
            subst_depth -= 1
            i += 1
            continue
        if HEAD.match(line, i):
            if subst_depth > 0:
                out.append(("SUBST", i))
            elif quote is not None:
                out.append(("LITERAL", i))
            else:
                out.append(("BARE", i))
            i += 1
            continue
        i += 1
    return out


def classify(line: str):
    """Return the strongest classification on the line, or None."""
    hits = scan_line(line)
    if not hits:
        return None
    if GUARDED.search(line):
        return "GUARDED"
    kinds = {k for k, _ in hits}
    if "BARE" in kinds:
        return "CONSUMED"  # a pipeline statement — set -e sees its status
    if "SUBST" in kinds:
        # A substitution's status escapes only through an assignment RHS.
        return "CONSUMED" if ASSIGN.match(line) else "ARGUMENT"
    return "LITERAL"


RUN_KEY = re.compile(r"^(\s*)-?\s*run:\s*(\S.*)?$")


def shell_lines(path: Path, text: str):
    """Yield (lineno, line) for the lines that are actually shell.

    For `.sh` that is every line. For a workflow it is only what sits under a
    `run:` key — a step's `name:`, and the workflow's own header comments,
    are YAML prose. Scanning them is how a gate ends up flagging the sentence
    that documents it, which is the exact failure this file was written against.
    """
    if path.suffix not in {".yml", ".yaml"}:
        yield from enumerate(text.splitlines(), 1)
        return
    block_indent = None
    for lineno, line in enumerate(text.splitlines(), 1):
        if block_indent is not None:
            indent = len(line) - len(line.lstrip())
            if line.strip() and indent <= block_indent:
                block_indent = None
            else:
                yield lineno, line
                continue
        m = RUN_KEY.match(line)
        if m:
            rest = (m.group(2) or "").strip()
            if rest and rest not in {"|", ">", "|-", ">-", "|+", ">+"}:
                yield lineno, line  # inline `run: cmd`
            else:
                block_indent = len(m.group(1))


SHELL_SUFFIXES = {".sh", ".bash", ".yml", ".yaml"}


def tracked_files(roots):
    out = subprocess.run(
        ["git", "ls-files", "-z", *roots], capture_output=True, text=True, check=True
    ).stdout
    return [Path(p) for p in out.split("\0") if p]


SELF_TEST = [
    ("CONSUMED", 'V="$(ls | head -1)"'),
    ("CONSUMED", "ls | head -1"),
    ("CONSUMED", 'A+=("$(ls | head -c 12)")'),
    ("CONSUMED", "local v=$(ls | head -1)"),
    ("CONSUMED", "grep '^#' \"$0\" | head -30 | sed 's/^# //'"),
    ("ARGUMENT", 'log "version: $(ls | head -1)"'),
    ("ARGUMENT", 'echo "size: $(du -sh . | head -1)" >&2'),
    ("LITERAL", 'echo "  run: curl -sI https://x | head -1"'),
    ("GUARDED", "ls | head -1 || true"),
    (None, "# ls | head -1"),
    (None, "    ls | grep -m1 x"),
]


def self_test() -> int:
    """Assert the classifier on one line of each shape.

    Kept inline rather than in a fixture file: a fixture would be scanned by
    this very gate and would then need an exclusion, and an exclusion is how a
    gate starts lying.
    """
    bad = 0
    for expected, line in SELF_TEST:
        got = classify(line)
        if got != expected:
            print(f"self-test FAIL: expected {expected}, got {got}: {line}")
            bad += 1
    print(f"self-test: {len(SELF_TEST) - bad}/{len(SELF_TEST)} cases")
    return 1 if bad else 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    roots = sys.argv[1:] or [".github/workflows", "scripts", "tests", "tools"]
    tally = {"CONSUMED": [], "ARGUMENT": [], "LITERAL": [], "GUARDED": []}
    for path in tracked_files(roots):
        # Shell only. A Python docstring that *describes* the rule is prose, and
        # a scanner that flags its own documentation is the failure mode this
        # item was opened over.
        if path.suffix not in SHELL_SUFFIXES:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, FileNotFoundError):
            continue
        if not PIPEFAIL.search(text):
            continue
        for lineno, line in shell_lines(path, text):
            kind = classify(line)
            if kind:
                tally[kind].append((path, lineno, line.strip()))

    for path, lineno, line in tally["CONSUMED"]:
        print(f"::error file={path},line={lineno}::`| head` status is consumed here — "
              f"SIGPIPE on the producer aborts the script: {line}")
    print(
        "head-under-pipefail: "
        f"{len(tally['CONSUMED'])} consumed, {len(tally['ARGUMENT'])} argument "
        f"(status discarded), {len(tally['LITERAL'])} literal, "
        f"{len(tally['GUARDED'])} explicitly guarded"
    )
    return 1 if tally["CONSUMED"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
