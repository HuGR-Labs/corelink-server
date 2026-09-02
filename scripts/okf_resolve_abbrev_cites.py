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
full-path citation in the SAME CONCEPT — and then checks that the resolved target
is a real, load-bearing line of the file it names.

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
import re
import sys
from pathlib import Path

# `python scripts/...` puts this directory on sys.path, but the contract suite
# imports this module by file path.  Keep the canonical frontmatter parser
# import stable in both execution modes.
SCRIPTS_DIR = Path(__file__).resolve().parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))
from validate_okf import FRONT_MATTER_RE, parse_frontmatter

REPO_ROOT = Path(__file__).resolve().parent.parent
WIKI = REPO_ROOT / "docs" / "knowledge"

# A full citation: `path/to/file.ext:123` or `path/to/file.ext:123-456`.
# Keep the path grammar in lockstep with validate_okf.py's CITE_RE.  Do not
# enumerate extensions: the repo cites SQL, workflow files, manifests, and
# plain-text artifacts as well as source files.
FULL_CITE = r"`([A-Za-z0-9._/\-]+):(\d+)(?:-(\d+))?`"
# An abbreviated citation: `:123` or `:123-456`, with no path.
BARE_CITE = r"`:(\d+)(?:-(\d+))?`"
# A path-only backtick is an explicit source anchor.  It is needed for prose
# such as `` `customer_d1.rs` — ... (`:947`, `:1011`) `` where the path is
# named without a line number before the abbreviated citations.  A root source
# file is equally valid: `` `README.md` ... `:12` ``.  Syntax alone is NOT an
# authority: dotted event names such as `customer.subscription.deleted` look
# like paths.  A path-only token inherits only when it is a grammar-valid,
# declared `source_files` member and a readable repo file.  This keeps
# the resolver fail-closed without a second, divergent frontmatter parser.
# Keep this path alphabet identical to CITE_RE.  Authority comes from the
# exact declared-source + readable-file checks below, not from a narrower
# spelling heuristic: valid repo paths include `.github/workflows/foo.yml` and
# `.gitignore`, while dotted event identifiers remain untrusted unless they
# are declared real source files.
PATH_ONLY_CITE = r"`(?P<path>[A-Za-z0-9._/\-]+)`"
BACKTICK_TOKEN = r"`([^`\n]*)`"
MALFORMED_BARE_CITE = r":\d+-.*"
MALFORMED_FULL_CITE = r"[A-Za-z0-9._/\-]+:\d+-.*"

COMMENT_STARTS = ("//", "///", "//!", "#", "*", "/*", "<!--")
DELIMITERS = {"}", "};", ")", ");", "},", "]", "];", "},)", "})", "});"}


def _compile():
    import re

    return re.compile(FULL_CITE), re.compile(BARE_CITE)


class ScanError(Exception):
    """The scan could not be trusted; this is distinct from a finding."""


class SourceCache:
    """Reads each cited file at most once."""

    def __init__(self, root: Path) -> None:
        self._root = root
        self._cache: dict[str, list[str] | None] = {}

    def lines(self, rel: str) -> list[str] | None:
        if rel not in self._cache:
            path = (self._root / rel).resolve()
            try:
                path.relative_to(self._root.resolve())
            except ValueError:
                # A citation is repo-relative by contract. Never let a crafted
                # path make the verifier read outside the repository.
                self._cache[rel] = None
                return None
            try:
                self._cache[rel] = path.read_text(encoding="utf-8", errors="replace").splitlines()
            except (OSError, ValueError, UnicodeError):
                self._cache[rel] = None
        return self._cache[rel]


def _declared_source_files(text: str) -> set[str]:
    """Return the exact valid `source_files` context for path-only anchors.

    `validate_okf` owns frontmatter parsing and C6 owns its validity.  Reusing
    its parser prevents the resolver from accepting a YAML shape the validator
    rejected (or the reverse).  Absent/malformed context is deliberately an
    empty set: a bare-looking token must never create an inherited path.
    """
    frontmatter = FRONT_MATTER_RE.match(text)
    if not frontmatter:
        return set()
    try:
        value = parse_frontmatter(frontmatter.group(1)).get("source_files", [])
    except Exception:
        return set()
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        return set()
    return set(value)


def _path_only_source_anchor(
    raw: str,
    declared_sources: set[str],
    cache: SourceCache,
) -> str | None:
    """Resolve a safe path-only anchor, never merely a dotted identifier."""
    match = re.fullmatch(PATH_ONLY_CITE, raw)
    if not match:
        return None
    candidate = match.group("path")
    if candidate not in declared_sources:
        return None
    if cache.lines(candidate) is None:
        return None
    return candidate


def classify(cache: SourceCache, path: str | None, first: int) -> tuple[str, str]:
    """Return (verdict, evidence). Verdict 'ok' means the citation lands on code."""
    bounds = validate_range_bounds(cache, path, first, None)
    if bounds[0] != "ok":
        return bounds
    if path is None:
        return "no-inherited-path", "no full-path citation precedes it"
    lines = cache.lines(path)
    assert lines is not None
    body = lines[first - 1].strip()
    if not body:
        return "blank", "resolves to a blank line"
    if body in DELIMITERS:
        return "delimiter", f"resolves to `{body}`"
    if body.startswith(COMMENT_STARTS):
        return "comment", f"resolves to a comment: {body[:60]}"
    return "ok", body[:60]


def validate_range_bounds(
    cache: SourceCache,
    path: str | None,
    first: int,
    last: int | None,
) -> tuple[str, str]:
    """Validate only citation bounds, including full-path ranges.

    Full citations are not otherwise classified here: an intentional range may
    end at a closing brace or comment even though its first line is executable.
    The invariant this helper owns is that both endpoints are real, 1-based
    lines and the range is ordered.
    """
    if path is None:
        return "no-inherited-path", "no full-path citation precedes it"
    lines = cache.lines(path)
    if lines is None:
        return "file-missing", f"{path} is not a readable file"
    end = first if last is None else last
    if first < 1 or end < 1:
        return "line-before-start", f"citation range starts before line 1 ({first}-{end})"
    if end < first:
        return "malformed-range", f"citation range is reversed ({first}-{end})"
    if end > len(lines):
        return "past-eof", f"{path} has {len(lines)} lines"
    return "ok", ""


def scan_concept(path: Path, cache: SourceCache, full_re, bare_re) -> tuple[int, list[str]]:
    """Return (abbreviated citations seen, findings) for one concept."""
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:  # unreadable is not a pass
        raise ScanError(f"cannot read {path}: {exc}") from exc

    seen = 0
    findings: list[str] = []
    try:
        rel = path.relative_to(REPO_ROOT)
    except ValueError as exc:
        raise ScanError(f"concept {path} is outside repository {REPO_ROOT}") from exc

    inherited: str | None = None
    declared_sources = _declared_source_files(text)

    for lineno, line in enumerate(text.splitlines(), 1):
        # Walk every backtick token in source order.  This preserves path-only
        # anchors and lets an abbreviated citation inherit the nearest full or
        # path-only source anchor anywhere earlier in the same concept.
        for token in re.finditer(BACKTICK_TOKEN, line):
            raw = token.group(0)
            inner = token.group(1)
            full = full_re.fullmatch(raw)
            bare = bare_re.fullmatch(raw)
            if full:
                cited_path = full.group(1)
                first = int(full.group(2))
                last = int(full.group(3)) if full.group(3) is not None else None
                inherited = cited_path
                verdict, evidence = validate_range_bounds(cache, cited_path, first, last)
                if verdict != "ok":
                    suffix = f"-{last}" if last is not None else ""
                    findings.append(
                        f"  {rel}:{lineno}  `{cited_path}:{first}{suffix}`  "
                        f"[full-{verdict}]  {evidence}"
                    )
                continue
            if bare:
                seen += 1
                first = int(bare.group(1))
                last = int(bare.group(2)) if bare.group(2) is not None else None
                verdict, evidence = validate_range_bounds(cache, inherited, first, last)
                if verdict == "ok":
                    verdict, evidence = classify(cache, inherited, first)
                if verdict != "ok":
                    suffix = f"-{last}" if last is not None else ""
                    findings.append(f"  {rel}:{lineno}  `:{first}{suffix}`  [{verdict}]  {evidence}")
                continue
            if re.fullmatch(MALFORMED_BARE_CITE, inner):
                seen += 1
                findings.append(
                    f"  {rel}:{lineno}  `{inner}`  [malformed-range]  "
                    "abbreviated citations must be `:N` or `:N-M` with a numeric end"
                )
                continue
            if re.fullmatch(MALFORMED_FULL_CITE, inner):
                findings.append(
                    f"  {rel}:{lineno}  `{inner}`  [malformed-range]  "
                    "full citations must be `path:N` or `path:N-M` with a numeric end"
                )
                continue
            path_only = _path_only_source_anchor(raw, declared_sources, cache)
            if path_only is not None:
                inherited = path_only

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
        try:
            seen, findings = scan_concept(concept.resolve(), cache, full_re, bare_re)
        except ScanError as exc:
            print(f"[okf-abbrev] FATAL: {exc}", file=sys.stderr)
            return 2
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
