#!/usr/bin/env python3
"""Resolve the OKF wiki's ABBREVIATED citations and report the ones that land nowhere.

The wiki writes a citation in two forms. The full form names the file:

    (`crates/corelink-container/src/routes/build.rs:117`)

The abbreviated form drops the path to avoid repeating it in the same sentence:

    (`:167`)

`CITE_RE` in `validate_okf.py` requires a NON-EMPTY path, so the abbreviated form
is invisible to C5 (freshness), C6/C6b (citation<->source_files agreement) and to
the citation count. That is B-059. This script is the missing half: it resolves
each abbreviated citation the way a reader does — against the nearest preceding
full-path citation IN THE SAME LINE — and then checks that the resolved target is
a real, load-bearing line of the file it names.

Six ways an abbreviated citation fails, each reported by name:

  no-inherited-path  nothing full-path precedes it, so it names no file at all
  file-missing       the inherited path is not a file in the repo
  past-eof           the line number is beyond the end of the inherited file
  blank              it resolves to a blank line
  comment            it resolves to a comment, not to the code that acts
  delimiter          it resolves to a bare `}` / `};` / `)` — no claim there

`past-eof` is the one that motivated this script. The prose names a second file
mid-sentence WITHOUT a line number, so a human reads the following `:947` as
belonging to that second file while the machine inherits the first — and the
number is past the end of the file it actually inherits. A human reads it right;
nothing mechanical does.

Anti-vacuity: this script fails when it cannot do its job, not just when it finds
a problem. Zero concepts scanned, an unreadable concept, or zero abbreviated
citations anywhere are each a NAMED failure — the wiki uses the abbreviated form
freely, so finding none means the scan broke, not that the wiki is clean.

Usage:
    python3 scripts/okf_resolve_abbrev_cites.py            # scan docs/knowledge
    python3 scripts/okf_resolve_abbrev_cites.py --quiet    # exit code only
    python3 scripts/okf_resolve_abbrev_cites.py <file>...  # scan named concepts

Exit: 0 when every abbreviated citation resolves to a real code line;
      1 when any does not; 2 when the scan itself could not be trusted.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
WIKI = REPO_ROOT / "docs" / "knowledge"

# A full citation: `path/to/file.ext:123` or `path/to/file.ext:123-456`.
FULL_CITE = r"`([A-Za-z0-9_][A-Za-z0-9_./-]*\.(?:rs|ts|mts|cts|tsx|js|mjs|cjs|jsx|toml|jsonc|json|py|sh|md|yaml|yml)):(\d+)(?:-(\d+))?`"
# An abbreviated citation: `:123` or `:123-456`, with no path.
BARE_CITE = r"`:(\d+)(?:-(\d+))?`"

COMMENT_STARTS = ("//", "///", "//!", "#", "*", "/*", "<!--")
DELIMITERS = {"}", "};", ")", ");", "},", "]", "];", "},)", "})", "});"}


def _compile():
    import re

    return re.compile(FULL_CITE), re.compile(BARE_CITE)


class SourceCache:
    """Reads each cited file at most once."""

    def __init__(self, root: Path) -> None:
        self._root = root
        self._cache: dict[str, list[str] | None] = {}

    def lines(self, rel: str) -> list[str] | None:
        if rel not in self._cache:
            path = self._root / rel
            try:
                self._cache[rel] = path.read_text(errors="replace").splitlines()
            except (OSError, ValueError):
                self._cache[rel] = None
        return self._cache[rel]


def classify(cache: SourceCache, path: str | None, first: int) -> tuple[str, str]:
    """Return (verdict, evidence). Verdict 'ok' means the citation lands on code."""
    if path is None:
        return "no-inherited-path", "no full-path citation precedes it"
    lines = cache.lines(path)
    if lines is None:
        return "file-missing", f"{path} is not a readable file"
    if first > len(lines):
        return "past-eof", f"{path} has {len(lines)} lines"
    body = lines[first - 1].strip()
    if not body:
        return "blank", "resolves to a blank line"
    if body in DELIMITERS:
        return "delimiter", f"resolves to `{body}`"
    if body.startswith(COMMENT_STARTS):
        return "comment", f"resolves to a comment: {body[:60]}"
    return "ok", body[:60]


def scan_concept(path: Path, cache: SourceCache, full_re, bare_re) -> tuple[int, list[str]]:
    """Return (abbreviated citations seen, findings) for one concept."""
    try:
        text = path.read_text()
    except OSError as exc:  # a concept we cannot read is a scan failure, not a pass
        raise SystemExit(f"[okf-abbrev] FATAL: cannot read {path}: {exc}") from exc

    seen = 0
    findings: list[str] = []
    rel = path.relative_to(REPO_ROOT)

    for lineno, line in enumerate(text.splitlines(), 1):
        # Walk full and abbreviated citations in position order, so an
        # abbreviated one inherits the nearest full path that precedes it on
        # the same line — which is how the sentence reads.
        tokens = sorted(
            [(m.start(), "full", m.group(1), int(m.group(2))) for m in full_re.finditer(line)]
            + [(m.start(), "bare", None, int(m.group(1))) for m in bare_re.finditer(line)]
        )
        inherited: str | None = None
        for _pos, kind, cited_path, number in tokens:
            if kind == "full":
                inherited = cited_path
                continue
            seen += 1
            verdict, evidence = classify(cache, inherited, number)
            if verdict != "ok":
                findings.append(f"  {rel}:{lineno}  `:{number}`  [{verdict}]  {evidence}")

    return seen, findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("concepts", nargs="*", type=Path, help="concept files (default: all)")
    parser.add_argument("--quiet", action="store_true", help="suppress per-finding output")
    args = parser.parse_args()

    full_re, bare_re = _compile()
    cache = SourceCache(REPO_ROOT)

    concepts = args.concepts or sorted(WIKI.rglob("*.md"))
    if not concepts:
        print(f"[okf-abbrev] FATAL: no concepts found under {WIKI}", file=sys.stderr)
        return 2

    total_seen = 0
    all_findings: list[str] = []
    for concept in concepts:
        seen, findings = scan_concept(concept.resolve(), cache, full_re, bare_re)
        total_seen += seen
        all_findings.extend(findings)

    # Anti-vacuity: the wiki uses the abbreviated form freely. Finding NONE across
    # a full scan means the scan broke — a silent pass here would be the exact
    # failure this script exists to catch.
    if total_seen == 0 and not args.concepts:
        print(
            "[okf-abbrev] FATAL: scanned "
            f"{len(concepts)} concepts and found ZERO abbreviated citations. "
            "The wiki uses them; this is a broken scan, not a clean result.",
            file=sys.stderr,
        )
        return 2

    if all_findings:
        if not args.quiet:
            print(f"[okf-abbrev] {len(all_findings)} unresolvable of {total_seen} abbreviated citations:\n")
            print("\n".join(all_findings))
            print(
                "\nEach one is invisible to validate_okf.py, whose CITE_RE requires a "
                "non-empty path (B-059). Fix by writing the full path so the gate can "
                "see the citation, and point it at the line that IMPLEMENTS the claim."
            )
        return 1

    if not args.quiet:
        print(f"[okf-abbrev] OK: {total_seen} abbreviated citations, all resolve to real code lines.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
