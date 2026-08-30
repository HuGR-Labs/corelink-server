#!/usr/bin/env python3
"""Assemble `changelog.d/` fragments into the `## [Unreleased]` block of CHANGELOG.md.

WHY THIS EXISTS
---------------
`CHANGELOG.md` was a single shared file edited by 10 of 13 simultaneously-open
PRs (#1389 #1393 #1395 #1396 #1398 #1399 #1400 #1402 #1410 #1412, measured
2026-08-29). One shared file serializes every branch BY CONSTRUCTION: each
rebase invalidates the next PR's context, so the merge queue can only ever be
drained one PR at a time. The towncrier/scriv fix is a NEW FILE PER PR — two
new files never conflict, so the serialization disappears rather than being
managed.

CONTRACT (frozen)
-----------------
  * Fragment path   : `changelog.d/<pr-number>-<slug>.md`
  * Fragment format : first non-blank line is exactly one of
                      `### Added` | `### Changed` | `### Fixed` | `### Removed`
                      (`### Security` and `### Deprecated` are also accepted —
                      both already appear in the existing Unreleased block, and
                      a gate that rejects a section the file itself uses would
                      be a gate against the repo's own history);
                      everything after it is the entry prose, in the same style
                      as the existing CHANGELOG entries.
  * Ordering        : deterministic — fragments sorted by filename within each
                      section; sections emitted in Keep-A-Changelog order.
  * Idempotence     : assembling ZERO fragments does not write, so CHANGELOG.md
                      stays byte-identical. This is asserted by `--check`.

MODES
-----
  (default)         assemble fragments into CHANGELOG.md and delete them
  --dry-run         print what would be written; touch nothing
  --check           exit non-zero if assembling would change CHANGELOG.md
                    (i.e. exit 0 iff there are no fragments) — the round-trip
                    assertion, usable from CI
  --lint [PATH...]  validate fragment format only. With no PATH, lints every
                    fragment in `changelog.d/`. This is what the PR gate runs.
  --keep            assemble but do NOT delete the consumed fragments

Exit codes: 0 success / 1 validation or precondition failure.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CHANGELOG = REPO_ROOT / "CHANGELOG.md"
FRAGMENT_DIR = REPO_ROOT / "changelog.d"

UNRELEASED_HEADING = "## [Unreleased]"

# Keep-A-Changelog v1.1.0 section order. `Security` and `Deprecated` are part of
# the spec and both already occur inside this repo's Unreleased block, so they
# are accepted rather than rejected.
SECTION_ORDER = [
    "Added",
    "Changed",
    "Deprecated",
    "Removed",
    "Fixed",
    "Security",
]
VALID_HEADINGS = {f"### {s}": s for s in SECTION_ORDER}

# Files in changelog.d/ that are documentation, not fragments.
NON_FRAGMENTS = {"README.md"}


class FragmentError(Exception):
    """A fragment violates the frozen format contract."""


def fragment_paths(directory: Path = FRAGMENT_DIR) -> list[Path]:
    """Every fragment in `directory`, sorted by filename (deterministic order)."""
    if not directory.is_dir():
        return []
    return sorted(
        p
        for p in directory.glob("*.md")
        if p.name not in NON_FRAGMENTS and not p.name.startswith(".")
    )


def parse_fragment(path: Path) -> tuple[str, str]:
    """Return `(section, body)` for one fragment, or raise FragmentError.

    The first non-blank line must be a valid `### <Section>` heading; the
    remainder (stripped) is the entry body and must be non-empty. A fragment
    that carries a heading and no prose is an empty changelog entry, which is
    exactly the drift the changelog gate exists to prevent.
    """
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError as exc:  # pragma: no cover - filesystem failure
        raise FragmentError(f"{path.name}: cannot read: {exc}") from exc

    lines = raw.splitlines()
    idx = 0
    while idx < len(lines) and not lines[idx].strip():
        idx += 1
    if idx >= len(lines):
        raise FragmentError(f"{path.name}: file is empty")

    heading = lines[idx].strip()
    if heading not in VALID_HEADINGS:
        raise FragmentError(
            f"{path.name}: first non-blank line is {heading!r}; expected one of "
            + ", ".join(repr(h) for h in VALID_HEADINGS)
        )

    body = "\n".join(lines[idx + 1 :]).strip("\n").rstrip()
    if not body.strip():
        raise FragmentError(
            f"{path.name}: has a '{heading}' heading but no entry text underneath"
        )
    if not body.lstrip().startswith("-"):
        raise FragmentError(
            f"{path.name}: entry text must be a Markdown list item starting with "
            f"'- ' (matching every existing CHANGELOG.md entry); got "
            f"{body.lstrip().splitlines()[0][:60]!r}"
        )
    return VALID_HEADINGS[heading], body


def lint(paths: list[Path]) -> int:
    """Validate fragments. Returns a process exit code."""
    if not paths:
        print("assemble_changelog: no fragments to lint")
        return 0
    errors: list[str] = []
    for path in paths:
        try:
            section, _ = parse_fragment(path)
        except FragmentError as exc:
            errors.append(str(exc))
        else:
            print(f"OK   {path.name}  [{section}]")
    for err in errors:
        print(f"FAIL {err}", file=sys.stderr)
    if errors:
        print(
            f"\n{len(errors)} invalid changelog fragment(s). "
            "See changelog.d/README.md for the format.",
            file=sys.stderr,
        )
        return 1
    print(f"assemble_changelog: {len(paths)} fragment(s) valid")
    return 0


def build_block(paths: list[Path]) -> str:
    """Render the fragments as Markdown to splice under `## [Unreleased]`.

    Returns "" when there are no fragments — the property that makes the
    zero-fragment round-trip byte-identical.
    """
    if not paths:
        return ""
    grouped: dict[str, list[str]] = {}
    for path in paths:
        section, body = parse_fragment(path)
        grouped.setdefault(section, []).append(body)

    chunks: list[str] = []
    for section in SECTION_ORDER:
        entries = grouped.get(section)
        if not entries:
            continue
        chunks.append(f"### {section}\n\n" + "\n\n".join(entries) + "\n")
    return "\n".join(chunks)


def splice(changelog_text: str, block: str) -> str:
    """Insert `block` directly beneath the `## [Unreleased]` heading.

    New entries go at the TOP of the Unreleased block, which is where every
    existing entry in this file was added (the block already carries 124
    `###` headings in reverse-chronological order — merging into the first
    matching heading would silently reorder history, so we do not).
    """
    if not block:
        return changelog_text

    lines = changelog_text.split("\n")
    for i, line in enumerate(lines):
        if line.strip() == UNRELEASED_HEADING:
            head = "\n".join(lines[: i + 1])
            tail = "\n".join(lines[i + 1 :])
            if not tail.startswith("\n"):
                tail = "\n" + tail
            return head + "\n\n" + block + tail
    raise FragmentError(f"CHANGELOG.md: no '{UNRELEASED_HEADING}' heading found")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true", help="print, do not write")
    mode.add_argument(
        "--check",
        action="store_true",
        help="exit 1 if assembling would modify CHANGELOG.md",
    )
    mode.add_argument(
        "--lint",
        nargs="*",
        metavar="PATH",
        help="validate fragment format only (all of changelog.d/ if no PATH)",
    )
    parser.add_argument(
        "--keep", action="store_true", help="do not delete consumed fragments"
    )
    args = parser.parse_args(argv)

    if args.lint is not None:
        paths = [Path(p) for p in args.lint] if args.lint else fragment_paths()
        missing = [p for p in paths if not p.is_file()]
        if missing:
            for p in missing:
                print(f"FAIL {p}: no such file", file=sys.stderr)
            return 1
        return lint(paths)

    paths = fragment_paths()

    if args.check:
        if paths:
            print(
                "assemble_changelog --check: "
                f"{len(paths)} unassembled fragment(s) would modify CHANGELOG.md:",
                file=sys.stderr,
            )
            for p in paths:
                print(f"  {p.relative_to(REPO_ROOT)}", file=sys.stderr)
            return 1
        print("assemble_changelog --check: no fragments; CHANGELOG.md unchanged")
        return 0

    if not paths:
        print("assemble_changelog: no fragments; CHANGELOG.md left byte-identical")
        return 0

    try:
        block = build_block(paths)
        original = CHANGELOG.read_text(encoding="utf-8")
        updated = splice(original, block)
    except FragmentError as exc:
        print(f"FAIL {exc}", file=sys.stderr)
        return 1

    if args.dry_run:
        sys.stdout.write(block)
        print(
            f"\n-- dry run: {len(paths)} fragment(s) would be consumed, "
            "CHANGELOG.md not written",
        )
        return 0

    CHANGELOG.write_text(updated, encoding="utf-8")
    if not args.keep:
        for path in paths:
            os.remove(path)
    print(
        f"assemble_changelog: spliced {len(paths)} fragment(s) into "
        f"{UNRELEASED_HEADING}"
        + ("" if args.keep else " and removed them from changelog.d/")
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
