#!/usr/bin/env python3
"""
validate_workflow_path_filters — CI gate against path filters that match nothing.

WHY THIS GATE EXISTS
--------------------
A workflow gated on

    on:
      pull_request:
        paths:
          - 'apps/server/**'

only ever runs when a changed file matches one of those globs. If the directory
is renamed, moved, or deleted and the filter is not updated, the glob silently
matches ZERO files forever. GitHub does not warn; actionlint does not warn. The
workflow simply stops being triggered by that entry.

That is this repo's dominant defect class in its purest form: **a checker that
observes the failure and reports success.** A gate that never fires produces no
red check, so its absence reads as "no problem found". The real 2026-08-03
findings this gate was written for:

  * `perf-regression.yml` watched `crates/corelink-rate-limit/**` — the crate is
    spelled `corelink-ratelimit` (no hyphen), so the entry matched nothing.
  * `perf-regression.yml` watched `crates/corelink-tenant-path/**` — the PACKAGE
    is `corelink-tenant-path` but the DIRECTORY is `crates/tenant-path`.
  * `tenant-path.yml` watched `apps/server/**` — that whole tree was absorbed
    into `crates/corelink-container` on 2026-05-26 (commit 1d0c221e, wave-33
    stage 2.B.1) and the filter was never updated.

None of those three made any check go red. They just quietly narrowed the
trigger surface.

THE RULE
--------
Every glob in a `paths:` or `paths-ignore:` list, in every workflow, must match
at least ONE file tracked by git at HEAD. A glob matching zero tracked files is
either a typo or a stale path, and both are failures.

THE ONE LEGITIMATE EXCEPTION, AND HOW IT IS KEPT HONEST
-------------------------------------------------------
A filter is sometimes deliberately forward-looking: it must fire the day a file
that does NOT exist yet appears. `rustfmt.yml` is the worked example — its
filter has to be a superset of `cargo fmt --all --check`'s input surface, so it
watches `**rustfmt.toml` even though the repo has no rustfmt.toml today.
Deleting that glob would open a real hole the day someone adds one.

Such a glob is opted out with an inline marker ON THE SAME LINE, so the
justification lives exactly where the glob lives and cannot drift away from it:

    - '**rustfmt.toml'   # path-filter-allow: no rustfmt.toml today; a future one must trigger fmt

The marker is self-policing in both directions:
  * a glob with no marker that matches nothing  -> FAIL (dead filter)
  * a glob WITH a marker that now matches files -> FAIL (stale marker; the file
    arrived, so the exception has expired and the marker must be deleted)
This is deliberately not a separate allowlist file: an allowlist you edit in
another file is how a suppression outlives the reason for it.

MATCHING SEMANTICS
------------------
GitHub's filter-pattern syntax (docs: "Workflow syntax → onpushpull_requestpaths"):
  ``**``  matches zero or more of any character, including ``/``
  ``*``   matches zero or more of any character EXCEPT ``/``
  ``?``   matches exactly one character except ``/``
  a leading ``!`` negates the pattern (checked too — a dead exclude is drift)
Patterns must match the WHOLE path, which is why a bare directory name such as
``apps/server`` matches no files on GitHub's side either; we report those with a
hint to append ``/**``.

stdlib-only on purpose: the self-hosted mac fleet has no reliable PyYAML and
cannot provision one via setup-python (see action-sha-audit.yml).
"""

from __future__ import annotations

import os
import re
import subprocess
import sys

WORKFLOW_DIR = os.path.join(".github", "workflows")

# `paths:` / `paths-ignore:` opening a block-style list, e.g. "    paths:"
_KEY_BLOCK = re.compile(r"^(\s*)(paths|paths-ignore):\s*(#.*)?$")
# …or a flow-style list on one line, e.g. "    paths: ['a/**', 'b/**']"
_KEY_FLOW = re.compile(r"^(\s*)(paths|paths-ignore):\s*\[(?P<items>.*)\]\s*(#.*)?$")
# a "- 'glob'" list item
_ITEM = re.compile(r"^(?P<indent>\s*)-\s+(?P<value>.+?)\s*$")
# opt-out marker for a deliberately forward-looking glob (see module docstring)
_ALLOW = re.compile(r"#\s*path-filter-allow:\s*(?P<reason>.+?)\s*$")


def tracked_files() -> list[str]:
    """Every path git tracks at HEAD, as forward-slash relative paths."""
    out = subprocess.run(
        ["git", "ls-files", "-z"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [p for p in out.split("\0") if p]


def _strip_scalar(raw: str) -> str:
    """Unquote a YAML scalar and drop a trailing comment."""
    raw = raw.strip()
    if raw[:1] in {"'", '"'}:
        quote = raw[0]
        end = raw.find(quote, 1)
        if end != -1:
            return raw[1:end]
        return raw[1:]
    # unquoted: a ` #` starts a comment
    return raw.split(" #", 1)[0].strip()


def glob_to_regex(pattern: str) -> re.Pattern[str]:
    """Translate a GitHub Actions filter pattern into a full-match regex."""
    out: list[str] = []
    i = 0
    n = len(pattern)
    while i < n:
        ch = pattern[i]
        if ch == "*":
            if pattern.startswith("**", i):
                # "**/" collapses so that "a/**/b" also matches "a/b"
                if pattern.startswith("**/", i):
                    out.append("(?:.*/)?")
                    i += 3
                    continue
                out.append(".*")
                i += 2
                continue
            out.append("[^/]*")
            i += 1
            continue
        if ch == "?":
            out.append("[^/]")
            i += 1
            continue
        out.append(re.escape(ch))
        i += 1
    return re.compile("".join(out) + r"\Z")


def _allow_reason(line: str) -> str | None:
    """The `# path-filter-allow: …` reason on this line, if any."""
    marker = _ALLOW.search(line)
    return marker.group("reason") if marker else None


def extract_patterns(path: str) -> list[tuple[int, str, str | None]]:
    """Return (line_number, glob, allow_reason) for every paths entry in a file."""
    with open(path, encoding="utf-8") as handle:
        lines = handle.read().splitlines()

    found: list[tuple[int, str, str | None]] = []
    idx = 0
    while idx < len(lines):
        line = lines[idx]

        flow = _KEY_FLOW.match(line)
        if flow:
            reason = _allow_reason(line)
            for raw in flow.group("items").split(","):
                value = _strip_scalar(raw)
                if value:
                    found.append((idx + 1, value, reason))
            idx += 1
            continue

        block = _KEY_BLOCK.match(line)
        if not block:
            idx += 1
            continue

        key_indent = len(block.group(1))
        idx += 1
        while idx < len(lines):
            nxt = lines[idx]
            if not nxt.strip() or nxt.lstrip().startswith("#"):
                idx += 1
                continue
            item = _ITEM.match(nxt)
            if not item or len(item.group("indent")) <= key_indent:
                break
            value = _strip_scalar(item.group("value"))
            if value:
                found.append((idx + 1, value, _allow_reason(nxt)))
            idx += 1
    return found


def main() -> int:
    if not os.path.isdir(WORKFLOW_DIR):
        print(f"FAIL: {WORKFLOW_DIR} not found (run from the repo root)")
        return 2

    files = sorted(tracked_files())
    workflows = sorted(
        os.path.join(WORKFLOW_DIR, name)
        for name in os.listdir(WORKFLOW_DIR)
        if name.endswith((".yml", ".yaml"))
    )

    offenders: list[tuple[str, int, str, str]] = []
    stale_markers: list[tuple[str, int, str]] = []
    checked = 0
    allowed = 0

    for workflow in workflows:
        for lineno, pattern, reason in extract_patterns(workflow):
            checked += 1
            bare = pattern[1:] if pattern.startswith("!") else pattern
            if not bare:
                continue
            matched = any(glob_to_regex(bare).match(f) for f in files)

            if reason is not None:
                # Opted out. Valid only while the glob really matches nothing;
                # once the file lands, the exception has expired.
                if matched:
                    stale_markers.append((workflow, lineno, pattern))
                else:
                    allowed += 1
                continue

            if matched:
                continue
            # No file matched. Is it a bare directory that just needs "/**"?
            hint = ""
            probe = bare.rstrip("/") + "/"
            if any(f.startswith(probe) for f in files):
                hint = f" (directory exists — did you mean '{bare.rstrip('/')}/**'?)"
            offenders.append((workflow, lineno, pattern, hint))

    if checked == 0:
        print("FAIL: inspected 0 path-filter patterns — the parser is broken.")
        return 2

    failed = False

    if offenders:
        failed = True
        print("FAIL: workflow path filters that match ZERO tracked files.\n")
        print("A `paths:` glob matching nothing means the workflow is never")
        print("triggered by that entry. It goes silent instead of going red.\n")
        for workflow, lineno, pattern, hint in offenders:
            print(f"  {workflow}:{lineno}: '{pattern}' matches no tracked file{hint}")
        print(
            f"\n{len(offenders)} dead pattern(s) of {checked} checked "
            f"across {len(workflows)} workflow(s)."
        )
        print("Repoint each glob at the code it was written to guard, or delete it.")
        print(
            "If the glob is deliberately forward-looking, justify it inline with"
            "\n  # path-filter-allow: <why this must fire for a file that does not exist yet>"
        )

    if stale_markers:
        failed = True
        if offenders:
            print()
        print("FAIL: stale `path-filter-allow` markers — the file they waited")
        print("for now EXISTS, so the exception has expired. Delete the marker.\n")
        for workflow, lineno, pattern in stale_markers:
            print(f"  {workflow}:{lineno}: '{pattern}' now matches tracked files")

    if failed:
        return 1

    print(
        f"OK: all {checked} path-filter pattern(s) across {len(workflows)} "
        f"workflow(s) match at least one tracked file "
        f"({allowed} deliberately forward-looking, justified inline)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
