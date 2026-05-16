#!/usr/bin/env python3
"""GA-readiness DEFER drift detector (wave-25).

Scans the two GA-readiness sign-off docs for *stale* DEFER entries that
contradict the canonical-CI-is-local stance recorded in user memory
`feedback_ci_local` (CI runs locally; GHA infra is not the canonical
CI surface). The wave-25 scrub removed the prior "Docs CI billing
reinstatement" DEFER row; this detector keeps the corpus honest if a
future agent tries to re-add an equivalent stale row.

Scope (active rows only — scrub annotations / scrub audit doc are
exempt):

  - specs/_audits/2026-05-16-ga-final-checklist.md
      * Lines beginning with a checklist token (`- [ ]`, `- [x]`, or
        `- [X]`) — either unchecked DEFER rows the operator still
        evaluates, OR checked rows whose closure-narrative still
        carries a stale signal. Both are drift surfaces; the
        functionally-tighter regex below matches all three forms by
        design (see CHECKLIST_ROW_RE).

  - specs/_audits/2026-05-16-ga-readiness-final.md
      * Lines inside §11 table that are *data rows* (start with `| `
        and a numeric index in the first cell), i.e. the canonical
        DEFER counter table.

Stale signals (case-insensitive, exact phrase match):

  - "GHA billing"
  - "GitHub Actions billing"
  - "GitHub-billing"
  - "Docs CI billing"
  - "CI offline"
  - "Workflows offline"

Exit codes:

  0 — clean (no drift in active rows).
  1 — drift detected; one or more stale signals found.

Usage:
  python3 scripts/ga-readiness-defer-drift.py

Designed to be run as an advisory CI step (locally and/or as a
GHA-side advisory). Cheap (~ms), pure-stdlib, no third-party deps.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

READINESS_DOC = REPO_ROOT / "specs/_audits/2026-05-16-ga-readiness-final.md"
CHECKLIST_DOC = REPO_ROOT / "specs/_audits/2026-05-16-ga-final-checklist.md"

STALE_SIGNALS = (
    "GHA billing",
    "GitHub Actions billing",
    "GitHub-billing",
    "Docs CI billing",
    "CI offline",
    "Workflows offline",
)

# Phrase that, if present on a line, marks it as a scrub annotation
# (meta-reference to the removed row) and therefore exempt.
EXEMPT_MARKERS = (
    "wave-25 scrub",
    "wave-25-scrub",
    "scrub annotation",
    "_(wave-25",  # checklist parenthetical form
)

# Table row pattern for §11 DEFER table (e.g. "| 3 | **External pentest...").
TABLE_ROW_RE = re.compile(r"^\|\s*\d+\s*\|")
# Checklist DEFER row pattern.
CHECKLIST_ROW_RE = re.compile(r"^-\s*\[\s*[ xX]\s*\]")


def is_exempt(line: str) -> bool:
    lowered = line.lower()
    return any(marker in lowered for marker in EXEMPT_MARKERS)


def line_contains_stale(line: str) -> list[str]:
    lowered = line.lower()
    hits: list[str] = []
    for sig in STALE_SIGNALS:
        if sig.lower() in lowered:
            hits.append(sig)
    return hits


def scan(path: Path, row_pred) -> list[tuple[int, str, list[str]]]:
    """Return list of (line_no, line_text, stale_hits) for active rows
    matching row_pred that also carry stale signals.
    """
    findings: list[tuple[int, str, list[str]]] = []
    if not path.is_file():
        return findings
    with path.open("r", encoding="utf-8") as fh:
        for idx, raw in enumerate(fh, start=1):
            line = raw.rstrip("\n")
            if not row_pred(line):
                continue
            if is_exempt(line):
                continue
            hits = line_contains_stale(line)
            if hits:
                findings.append((idx, line, hits))
    return findings


def main() -> int:
    readiness_findings = scan(READINESS_DOC, lambda l: bool(TABLE_ROW_RE.match(l)))
    checklist_findings = scan(CHECKLIST_DOC, lambda l: bool(CHECKLIST_ROW_RE.match(l)))

    if not readiness_findings and not checklist_findings:
        print("ga-readiness-defer-drift: OK — no stale DEFER entries detected.")
        print(f"  scanned: {READINESS_DOC.relative_to(REPO_ROOT)} (§11 table rows)")
        print(f"  scanned: {CHECKLIST_DOC.relative_to(REPO_ROOT)} (checklist rows)")
        print(f"  signals: {', '.join(STALE_SIGNALS)}")
        return 0

    print("ga-readiness-defer-drift: WARN — stale DEFER entries detected.\n")
    if readiness_findings:
        print(f"[{READINESS_DOC.relative_to(REPO_ROOT)}] §11 DEFER table:")
        for ln, text, hits in readiness_findings:
            print(f"  L{ln}: signals={hits}")
            print(f"        {text}")
    if checklist_findings:
        print(f"[{CHECKLIST_DOC.relative_to(REPO_ROOT)}] checklist rows:")
        for ln, text, hits in checklist_findings:
            print(f"  L{ln}: signals={hits}")
            print(f"        {text}")
    print(
        "\nResolution: CI runs locally per `feedback_ci_local` user memory; "
        "GHA infrastructure is not the canonical CI surface. Any DEFER row "
        "framed as a GHA-billing / CI-offline blocker is stale by definition. "
        "Remove the row, decrement the §11 DEFER counter, and add a wave-25-"
        "style scrub annotation if you want to record the deletion in-band.\n"
        "See: specs/_audits/2026-05-16-ga-readiness-defer-scrub.md"
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
