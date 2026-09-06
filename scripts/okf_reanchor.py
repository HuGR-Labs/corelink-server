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

This helper removes both by writing the FULL 40-hex `git rev-parse HEAD` into
`checkpoint_sha` (always reachable at run time).

BLOB ANCHORS ARE DIFFERENT — update them ONLY for files you actually re-authored.
C5 does NOT compare the whole-file blob; it uses the anchor as a REFERENCE POINT
and flags the concept only when the concept's CITED LINE RANGES differ between
the anchor blob and the current file. So a `source_blobs` entry whose oid !=
`git hash-object` is NORMAL and correct whenever the file changed OUTSIDE this
concept's cited lines — the concept is still fresh. Blindly advancing that anchor
to the current blob would move the reference PAST changes you never reviewed and
could MASK real drift. Therefore this helper updates a blob anchor ONLY when you
name its path with `--blob` (the file you re-authored in reconcile step 2c).
When that path has no entry yet, `--blob` creates its first immutable content
anchor. Every other `source_blobs` entry is left untouched.

It NEVER edits a concept body — run it AFTER a real re-author (the reconcile
skill's step 2), never as a substitute for one (a checkpoint bump with no body
change fails C5b by design).

Usage:
    # checkpoint_sha only (the common case — you edited a body, cited lines in
    # no source file's blob-anchored region moved):
    python3 scripts/okf_reanchor.py docs/knowledge/launch/money-path.md
    # auto-detect every modified/added concept under docs/knowledge/:
    python3 scripts/okf_reanchor.py
    # ALSO create or advance the blob anchor of a file you re-authored citations for:
    python3 scripts/okf_reanchor.py docs/knowledge/launch/money-path.md \
        --blob crates/corelink-container/src/routes/billing_ingest.rs

Idempotent: re-running with the same args rewrites the same values.
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
SOURCE_FILE_RE = re.compile(
    r'^\s*-\s*(?:"(?P<quoted>[^"]+)"|(?P<bare>\S+))\s*$'
)
FRONTMATTER_KEY_RE = re.compile(r'^(?P<indent>\s*)(?P<key>[A-Za-z0-9_-]+):\s*(?P<value>.*)$')


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


def _frontmatter_bounds(lines: list[str]) -> tuple[int, int] | None:
    """Return the inclusive-open [start,end) indexes of YAML frontmatter."""
    if not lines or lines[0].rstrip("\r\n") != "---":
        return None
    for i in range(1, len(lines)):
        if lines[i].rstrip("\r\n") == "---":
            return 1, i
    return None


def _declared_sources(lines: list[str], start: int, end: int) -> set[str]:
    """Read the flat source_files list used by the OKF profile."""
    paths: set[str] = set()
    active = False
    for line in lines[start:end]:
        key = FRONTMATTER_KEY_RE.match(line.rstrip("\r\n"))
        if key:
            active = key.group("key") == "source_files" and not key.group("value").strip()
            continue
        if active:
            item = SOURCE_FILE_RE.match(line.rstrip("\r\n"))
            if item:
                paths.add(item.group("quoted") or item.group("bare"))
            elif line.strip():
                active = False
    return paths


def _source_blobs_block(lines: list[str], start: int, end: int) -> tuple[int, int] | None:
    """Locate the flat source_blobs list, returning its item span."""
    key_index = None
    for i in range(start, end):
        m = FRONTMATTER_KEY_RE.match(lines[i].rstrip("\r\n"))
        if m and m.group("key") == "source_blobs":
            key_index = i
            break
    if key_index is None:
        return None
    item_end = key_index + 1
    while item_end < end:
        line = lines[item_end]
        if line.strip() and not line[:1].isspace():
            break
        # Blank lines and indented list items belong to this flat block. A
        # different indented shape is left for the validator to reject.
        item_end += 1
    return key_index + 1, item_end


def _insert_first_blob(lines: list[str], start: int, end: int, path: str, oid: str) -> None:
    """Insert a first entry, preserving the profile's flat-list shape."""
    nl = "\r\n" if any(line.endswith("\r\n") for line in lines[:end]) else "\n"
    entry = f'  - "{path}@{oid}"{nl}'
    block = _source_blobs_block(lines, start, end)
    if block is not None:
        item_start, item_end = block
        # Put the entry immediately after the key and before any existing
        # blank/indented content. This keeps an empty list deterministic.
        lines.insert(item_start, entry)
        return

    checkpoint = next(
        (i for i in range(start, end) if CHECKPOINT_RE.match(lines[i].rstrip("\r\n"))),
        None,
    )
    if checkpoint is None:
        raise ValueError("frontmatter has no `checkpoint_sha` insertion point")
    lines[checkpoint:checkpoint] = [f"source_blobs:{nl}", entry]


def reanchor(path: str, head: str, blob_paths: set[str]) -> tuple[bool, bool]:
    p = Path(path)
    if not p.is_file():
        print(f"  skip (missing): {path}", file=sys.stderr)
        return False, True
    lines = p.read_text().splitlines(keepends=True)
    changed = False
    error = False
    bounds = _frontmatter_bounds(lines)
    if bounds is None:
        print(f"  error: {path}: missing or unterminated YAML frontmatter", file=sys.stderr)
        return False, True
    fm_start, fm_end = bounds
    declared = _declared_sources(lines, fm_start, fm_end)
    undeclared = sorted(blob_paths - declared)
    if undeclared:
        print(
            f"  error: {path}: --blob path(s) not declared in source_files: "
            + ", ".join(undeclared),
            file=sys.stderr,
        )
        return False, True

    # Resolve every requested working-tree file before changing the concept.
    # This keeps the helper transactional: a typo or vanished source must not
    # leave a checkpoint bump behind while returning an error.
    blob_oids: dict[str, str] = {}
    for blob_path in sorted(blob_paths):
        oid = _blob_oid(blob_path)
        if oid is None:
            print(f"  error: {path}: --blob path not found: {blob_path}", file=sys.stderr)
            return False, True
        blob_oids[blob_path] = oid

    # Build the current entry index once. Entries not named with --blob remain
    # byte-for-byte untouched by design.
    blob_entries: dict[str, int] = {}
    for i, line in enumerate(lines):
        m = CHECKPOINT_RE.match(line)
        if m:
            if m.group("sha") != head:
                nl = "\r\n" if line.endswith("\r\n") else "\n"
                lines[i] = f'{m.group("pre")}{head}{m.group("post")}{nl}'
                changed = True
                print(f"  {path}: checkpoint_sha -> {head}")
            continue
        m = BLOB_RE.match(line.rstrip("\r\n"))
        if m:
            blob_entries.setdefault(m.group("path"), i)
            # Update a blob anchor ONLY when its path was explicitly named with
            # --blob (a file whose CITED lines you re-authored). Leaving every
            # other entry alone is deliberate: an anchor that != hash-object but
            # whose cited ranges are unchanged is fresh, and bumping it would
            # mask future drift (see module docstring).
            if m.group("path") not in blob_paths:
                continue
            oid = blob_oids[m.group("path")]
            if oid != m.group("oid"):
                nl = "\r\n" if line.endswith("\r\n") else "\n"
                lines[i] = f'{m.group("pre")}{m.group("path")}@{oid}{m.group("post")}{nl}'
                changed = True
                print(f"  {path}: {m.group('path')} blob -> {oid}")

    # A first reconcile has no existing BLOB_RE line to update. Create exactly
    # the explicitly re-authored path's entry, never all source files implicitly.
    for blob_path in sorted(blob_paths - set(blob_entries)):
        oid = blob_oids[blob_path]
        # Recompute the frontmatter end because a prior insertion shifts it.
        bounds = _frontmatter_bounds(lines)
        if bounds is None:
            print(f"  error: {path}: frontmatter disappeared while anchoring", file=sys.stderr)
            error = True
            continue
        try:
            _insert_first_blob(lines, bounds[0], bounds[1], blob_path, oid)
        except ValueError as exc:
            print(f"  error: {path}: {exc}", file=sys.stderr)
            error = True
            continue
        changed = True
        print(f"  {path}: {blob_path} blob -> {oid} (first anchor)")
    if changed:
        p.write_text("".join(lines))
    else:
        print(f"  {path}: already current")
    return changed, error


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "concepts",
        nargs="*",
        help="concept .md paths; if omitted, auto-detect modified docs/knowledge/*.md",
    )
    ap.add_argument(
        "--blob",
        action="append",
        default=[],
        metavar="PATH",
        help="source_files path whose blob anchor to create or advance (a file "
        "whose cited lines you re-authored). Repeatable. Omit to advance "
        "checkpoint_sha only — blob anchors of unchanged-citation files must NOT "
        "be bumped.",
    )
    args = ap.parse_args()

    concepts = args.concepts or _detect_concepts()
    if not concepts:
        print("no concepts to re-anchor (none passed, none modified under docs/knowledge/)")
        return 0

    blob_paths = set(args.blob)
    head = _head_sha()
    print(
        f"re-anchoring {len(concepts)} concept(s) to HEAD {head}"
        + (f"; blob anchors: {sorted(blob_paths)}" if blob_paths else " (checkpoint_sha only)")
    )
    any_changed = False
    any_error = False
    for c in concepts:
        changed, error = reanchor(c, head, blob_paths)
        any_changed |= changed
        any_error |= error
    print(
        "done — run `python3 scripts/validate_okf.py` to confirm 0 stale, then commit."
        if any_changed
        else "done — nothing to change."
    )
    return 2 if any_error else 0


if __name__ == "__main__":
    raise SystemExit(main())
