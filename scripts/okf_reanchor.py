#!/usr/bin/env python3
"""Advance the anchors of drifted OKF concepts to the current tree — mechanically.

The OKF freshness gate (C5) compares each concept's cited code against an
anchor: a `checkpoint_sha:` (a commit) and/or `source_blobs:` entries (a
`path@blob-oid` per re-authored source file). After you re-author a concept's
body against HEAD, those anchors must be advanced — and that manual step has two
sharp footguns that each cost a full CI cycle:

  1. a SHORT sha (`a862f467`) — the gate requires a 40-hex `checkpoint_sha`;
  2. an ORPHANED sha — e.g. one written before a rebase, no longer reachable,
     so the gate falls back to the base-ref and reports the concept stale.

This helper removes both: it sets `checkpoint_sha` to the FULL 40-hex
`git rev-parse HEAD` (always reachable at run time) and rewrites every
`source_blobs` entry to the working-tree `git hash-object` of its path (the
exact blob the gate compares against). It NEVER edits a concept body — run it
AFTER a real re-author (the reconcile skill's step 2), never as a substitute for
one (a checkpoint bump with no body change fails C5b by design).

Usage:
    # explicit concepts
    python3 scripts/okf_reanchor.py docs/knowledge/launch/money-path.md ...
    # or auto-detect every modified/added concept under docs/knowledge/
    python3 scripts/okf_reanchor.py

Idempotent: re-running with no further edits rewrites the same values.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

CHECKPOINT_RE = re.compile(r'^(?P<pre>\s*checkpoint_sha:\s*")(?P<sha>[^"]*)(?P<post>".*)$')
# a source_blobs list entry:  - "some/path.rs@<oid>"
BLOB_RE = re.compile(r'^(?P<pre>\s*-\s*")(?P<path>[^"@]+)@(?P<oid>[0-9a-f]+)(?P<post>".*)$')


def _git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], check=True, capture_output=True, text=True
    ).stdout.strip()


def _head_sha() -> str:
    sha = _git("rev-parse", "HEAD")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        sys.exit(f"error: `git rev-parse HEAD` did not return a 40-hex sha: {sha!r}")
    return sha


def _blob_oid(path: str) -> str | None:
    """The working-tree blob oid the freshness gate compares against."""
    if not Path(path).is_file():
        return None
    return _git("hash-object", path)


def _detect_concepts() -> list[str]:
    out = _git("diff", "--name-only", "HEAD", "--", "docs/knowledge")
    staged = _git("diff", "--name-only", "--cached", "--", "docs/knowledge")
    files = {f for f in (out + "\n" + staged).splitlines() if f.endswith(".md")}
    return sorted(files)


def reanchor(path: str, head: str) -> bool:
    p = Path(path)
    if not p.is_file():
        print(f"  skip (missing): {path}", file=sys.stderr)
        return False
    lines = p.read_text().splitlines(keepends=True)
    changed = False
    for i, line in enumerate(lines):
        m = CHECKPOINT_RE.match(line)
        if m:
            if m.group("sha") != head:
                lines[i] = f'{m.group("pre")}{head}{m.group("post")}\n'
                changed = True
                print(f"  {path}: checkpoint_sha -> {head}")
            continue
        m = BLOB_RE.match(line)
        if m:
            oid = _blob_oid(m.group("path"))
            if oid is None:
                print(
                    f"  warn: {path}: blob path not found, left as-is: {m.group('path')}",
                    file=sys.stderr,
                )
                continue
            if oid != m.group("oid"):
                lines[i] = f'{m.group("pre")}{m.group("path")}@{oid}{m.group("post")}\n'
                changed = True
                print(f"  {path}: {m.group('path')} blob -> {oid}")
    if changed:
        p.write_text("".join(lines))
    else:
        print(f"  {path}: already current")
    return changed


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "concepts",
        nargs="*",
        help="concept .md paths; if omitted, auto-detect modified docs/knowledge/*.md",
    )
    args = ap.parse_args()

    concepts = args.concepts or _detect_concepts()
    if not concepts:
        print("no concepts to re-anchor (none passed, none modified under docs/knowledge/)")
        return 0

    head = _head_sha()
    print(f"re-anchoring {len(concepts)} concept(s) to HEAD {head}")
    any_changed = False
    for c in concepts:
        any_changed |= reanchor(c, head)
    print(
        "done — run `python3 scripts/validate_okf.py` to confirm 0 stale, then commit."
        if any_changed
        else "done — nothing to change."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
