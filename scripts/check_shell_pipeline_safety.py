#!/usr/bin/env python3
"""Refuse the two shell defects that make a gate lie instead of fail.

RULE 1 — no early-exiting boolean consumer on the right of a pipe.

    producer | grep -<q> PATTERN

`grep` with the quiet flag exits at the FIRST match. Under `set -o pipefail`
(207 files in this repo set it) the producer is still writing, takes SIGPIPE,
exits 141, and the pipeline reports failure. The `if` then reads "no match" —
so the idiom inverts EXACTLY when the pattern IS present and the producer's
output outruns the 64 KiB pipe buffer. It is a race, so it survives review and
CI: measured on PR #1271's 326-file diff, `git diff --name-only | grep -<q>x
CHANGELOG.md` misreported 38 of 40 runs, while reporting correctly on every
small PR.

The failure is SILENT and WRONG, which is why it is banned outright rather than
judged site-by-site. Two sites failed OPEN on main: the CTRL-CRED-001 scan of
`docker history` (finding a credential is what kills the producer, so the
control printed PASS) and okf-autoreconcile's "agent edited files outside
docs/knowledge" guard.

Fix: drop the quiet flag and redirect. `producer | grep P >/dev/null` has the
same exit status, reads all input, and cannot SIGPIPE. Or use a here-string:
`grep -<q> P <<<"$var"`, which involves no pipe at all.

NOT banned here: `producer | head -n1`. Same SIGPIPE mechanism, different
blast radius — `head` cannot answer wrongly, it can only abort with 141 when
something reads the pipeline's status. That is loud and self-announcing, and it
depends on the *position* of the pipeline rather than on its presence, so it
needs a classifier rather than a pattern ban. It has one:
`scripts/check_head_under_pipefail.py`, enforced by the same workflow as this
gate. This gate exists for the silent-wrong class.

RULE 2 — every tracked shell script must parse.

`bash -n` on all of `*.sh` / `*.bash`. This is not theoretical: it is how
`scripts/okf-reconcile-local.sh` was found dead on main. Its prompt was a
single-quoted string containing the words "file's anchor"; the apostrophe
terminated the string and the remaining prose was parsed as shell. The script
could never have run, and nothing noticed.

Self-test (`--self-test`) asserts BOTH directions on fixtures: a violating file
must be rejected, a clean file must pass. A gate that has only ever been seen
green proves nothing.
"""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile

SCAN_ROOTS = (".github/workflows", "scripts", "tests", "tools")
SHELL_SUFFIXES = (".sh", ".bash")
SCANNED_SUFFIXES = (".yml", ".yaml") + SHELL_SUFFIXES

# Assembled from fragments on purpose: this file must not contain the literal
# text it forbids, or it would report itself and the gate would need an
# allowlist entry for its own source. A gate that needs to be excused from its
# own rule is the first step toward a gate nobody trusts.
_Q = "q"
_QUIET_FLAG = rf"-[A-Za-z]*{_Q}[A-Za-z]*|--{_Q}uiet|--silent"
FORBIDDEN = re.compile(rf"\|\s*(?:grep|rg)\s+(?:{_QUIET_FLAG})(?:\s|$)")

REMEDY = (
    "drop the quiet flag and redirect instead: `... | grep PATTERN >/dev/null` "
    "(same exit status, reads all input, cannot SIGPIPE), or avoid the pipe "
    "entirely with `grep -<quiet> PATTERN <<<\"$var\"`"
)


def tracked_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files"], capture_output=True, text=True, check=True
    ).stdout.split("\n")
    return [f for f in out if f]


def scan_forbidden(files: list[str]) -> list[tuple[str, int, str]]:
    hits: list[tuple[str, int, str]] = []
    for path in files:
        if not path.endswith(SCANNED_SUFFIXES):
            continue
        if not any(path.startswith(r + "/") or path == r for r in SCAN_ROOTS):
            continue
        try:
            lines = open(path, encoding="utf-8", errors="replace").read().split("\n")
        except OSError:
            continue
        for num, line in enumerate(lines, 1):
            if FORBIDDEN.search(line):
                hits.append((path, num, line.strip()))
    return hits


def scan_unparsable(files: list[str]) -> list[tuple[str, str]]:
    bad: list[tuple[str, str]] = []
    for path in files:
        if not path.endswith(SHELL_SUFFIXES):
            continue
        proc = subprocess.run(
            ["bash", "-n", path], capture_output=True, text=True
        )
        if proc.returncode != 0:
            first = (proc.stderr.strip().split("\n") or [""])[0]
            bad.append((path, first))
    return bad


def run_gate(files: list[str]) -> int:
    rc = 0
    hits = scan_forbidden(files)
    if hits:
        rc = 1
        print(f"::error::{len(hits)} pipeline(s) whose exit status inverts on match")
        for path, num, line in hits:
            print(f"::error file={path},line={num}::{line}")
        print(f"  remedy: {REMEDY}")
    else:
        print(f"rule 1 (early-exit boolean consumer): clean across {len(files)} tracked files")

    bad = scan_unparsable(files)
    if bad:
        rc = 1
        print(f"::error::{len(bad)} shell script(s) do not parse")
        for path, msg in bad:
            print(f"::error file={path}::{msg}")
    else:
        n = sum(1 for f in files if f.endswith(SHELL_SUFFIXES))
        print(f"rule 2 (bash -n): all {n} tracked shell scripts parse")
    return rc


VIOLATION_FIXTURE = "if git diff --name-only | grep -" + _Q + "x 'CHANGELOG.md'; then :; fi\n"
CLEAN_FIXTURE = "if git diff --name-only | grep -x 'CHANGELOG.md' >/dev/null; then :; fi\n"
UNPARSABLE_FIXTURE = "MSG='the file's anchor'\necho \"$MSG\"\n"


def self_test() -> int:
    failures = 0
    with tempfile.TemporaryDirectory() as td:
        cases = [
            ("violation is caught", VIOLATION_FIXTURE, True),
            ("clean form passes", CLEAN_FIXTURE, False),
        ]
        for name, body, want_hit in cases:
            p = os.path.join(td, "f.sh")
            open(p, "w", encoding="utf-8").write("#!/usr/bin/env bash\nset -euo pipefail\n" + body)
            got = bool(FORBIDDEN.search(body))
            ok = got == want_hit
            failures += 0 if ok else 1
            print(f"  [{'PASS' if ok else 'FAIL'}] rule 1: {name}")

        p = os.path.join(td, "broken.sh")
        open(p, "w", encoding="utf-8").write(UNPARSABLE_FIXTURE)
        got = bool(scan_unparsable([p]))
        ok = got is True
        failures += 0 if ok else 1
        print(f"  [{'PASS' if ok else 'FAIL'}] rule 2: apostrophe-in-string script is rejected")

        p2 = os.path.join(td, "fine.sh")
        open(p2, "w", encoding="utf-8").write("#!/usr/bin/env bash\necho ok\n")
        got = bool(scan_unparsable([p2]))
        ok = got is False
        failures += 0 if ok else 1
        print(f"  [{'PASS' if ok else 'FAIL'}] rule 2: a valid script is accepted")

    print(f"self-test: {4 - failures}/4 passed")
    return 1 if failures else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--self-test", action="store_true", help="assert the gate fails on a known-bad fixture and passes on a known-good one")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    return run_gate(tracked_files())


if __name__ == "__main__":
    sys.exit(main())
