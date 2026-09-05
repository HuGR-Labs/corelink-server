#!/usr/bin/env python3
"""Fail-closed B-169 guard for the legal evidence-pack SLO rows.

The index is a published sales/legal wayfinder. This small verifier keeps the
merge conflict out of the table and makes the SLO claim explicit: the catalog
is internal, and there is no public SLO page to offer an auditor.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


INDEX = Path("marketing/sales/legal-questionnaires/EVIDENCE-PACK-INDEX.md")

# These are deliberately separate families so a mutation of any one side of a
# conflict is reported by name instead of being hidden by the other markers.
MARKER_FAMILIES = {
    "merge-marker-start": re.compile(r"<<<<<<<"),
    "merge-marker-separator": re.compile(r"=======+"),
    "merge-marker-end": re.compile(r">>>>>>>"),
}
ROW_RE = re.compile(r"^\|\s*(88|89)\s*\|.*$", re.MULTILINE)

EXPECTED_ROW_88 = "specs/03_architecture/slo_catalog.md"
ROW_89_REQUIREMENTS = (
    "**not published**",
    "/slo",
    "404",
    "no source page exists",
    "row 88",
)


class InstrumentError(RuntimeError):
    """The verifier could not read the requested evidence index."""


def assess(index: Path) -> list[str]:
    """Return named B-169 gaps; an empty list means the repaired index is true."""
    try:
        text = index.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise InstrumentError(f"cannot read evidence index {index}: {error}") from error

    lines = text.splitlines()
    gaps: list[str] = []
    for family, pattern in MARKER_FAMILIES.items():
        if any(pattern.match(line) for line in lines):
            gaps.append(family)

    rows = {"88": [], "89": []}
    for match in ROW_RE.finditer(text):
        rows[match.group(1)].append(match.group(0))

    for number in ("88", "89"):
        count = len(rows[number])
        if count == 0:
            gaps.append(f"row-{number}-missing")
        elif count > 1:
            gaps.append(f"row-{number}-duplicate")

    if len(rows["88"]) == 1:
        row_88 = rows["88"][0]
        if EXPECTED_ROW_88 not in row_88 or not re.search(r"\|\s*NDA\s*\|\s*$", row_88):
            gaps.append("row-88-canonical-artifact")

    # Check every surviving row for a public URL, even when duplicate-row
    # integrity is already broken, so the pre-fix conflict reports every
    # independently dangerous alternative by its named reason.
    if any(re.search(r"https?://|\|\s*PUBLIC\s*\|", row) for row in rows["89"]):
        gaps.append("row-89-public-url")

    if len(rows["89"]) == 1:
        row_89 = rows["89"][0]
        # Any URL or PUBLIC access tier would invite an auditor to a page that
        # does not exist. Keep this reason distinct from a missing assertion.
        missing = [phrase for phrase in ROW_89_REQUIREMENTS if phrase not in row_89]
        if missing or not re.search(r"\|\s*NDA\s*\|\s*$", row_89):
            gaps.append("row-89-truthful-internal-reference")

    return gaps


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", type=Path, default=INDEX)
    parser.add_argument("--expect", choices=("open", "done"), required=True)
    args = parser.parse_args(argv)
    try:
        gaps = assess(args.index)
    except InstrumentError as error:
        print(f"instrument error: {error}", file=sys.stderr)
        return 2

    actual = "open" if gaps else "done"
    print(f"B-169 {actual}: {len(gaps)} gap(s)")
    for gap in gaps:
        print(f"- {gap}")
    if actual != args.expect:
        print(f"expected {args.expect}, found {actual}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
