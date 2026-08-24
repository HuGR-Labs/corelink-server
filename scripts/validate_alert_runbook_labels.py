#!/usr/bin/env python3
"""Every `runbook:` label in dashboards/alerts/*.yml must resolve to a real runbook.

A `runbook:` label is the pointer an on-call engineer follows at 03:00 while a
SEV-0 is burning. A label naming a file that does not exist is worse than no
label at all: it costs the responder a search before they conclude there is
nothing to read. Fourteen such labels were live on 2026-08-24 by exact-filename
match; nine of them actually resolved via the runbook front-matter `id:` (the
files live under `specs/05_quality/runbooks/` with a descriptive slug appended
to the id, e.g. `RB-FM-059-do-quota-exceeded.md`). This gate resolves by the
declared `id:` precisely so that slug drift is not mistaken for a missing
runbook -- and so that a genuinely missing one is not mistaken for slug drift.

Resolution index, built from every markdown file under the runbook roots:
  1. the YAML front-matter `id:` field (authoritative -- all 124 runbooks
     declare one), and
  2. the filename stem, as a fallback alias.

Fails closed. Exits non-zero if:
  - any `runbook:` label does not resolve, or
  - zero alert rule files were found (a glob that matches nothing must never
    report green -- see scripts/validate_workflow_path_filters.py for the same
    failure mode one level up), or
  - zero runbooks were indexed (a moved/renamed runbook root would otherwise
    make every label "resolve" against an empty index... by failing all of
    them, which is loud, but the explicit check names the real cause).

stdlib-only python3: the self-hosted fleet has no RUNNER_TOOL_CACHE and we do
not run setup-python on it. Same constraint as the sibling validators invoked
from .github/workflows/actionlint.yml.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

ALERT_GLOB = "dashboards/alerts/*.yml"

# Runbooks live in two roots (a historical split, not a mistake to fix here):
# `specs/_runbooks/` holds the operational/compliance set, and
# `specs/05_quality/runbooks/` holds the failure-mode (RB-FM-*) and SLO set.
RUNBOOK_ROOTS = (
    "specs/_runbooks",
    "specs/05_quality/runbooks",
    "specs/05_runbooks",
)

# `runbook: RB-FOO` or `runbook: "RB-FOO"` inside an alert rule's `labels:` map.
RUNBOOK_LABEL_RE = re.compile(r"^\s*runbook:\s*[\"']?([A-Za-z0-9._/-]+)[\"']?\s*$")

# Front-matter `id: "RB-FOO"` -- only honoured inside the leading `---` block.
FRONT_MATTER_ID_RE = re.compile(r"^id:\s*[\"']?([A-Za-z0-9._/-]+)[\"']?\s*$")


def build_runbook_index(repo_root: Path) -> dict[str, Path]:
    """Map every known runbook identifier to the file that defines it."""
    index: dict[str, Path] = {}
    for root_name in RUNBOOK_ROOTS:
        root = repo_root / root_name
        if not root.is_dir():
            continue
        for md in sorted(root.glob("*.md")):
            # Filename stem is always a valid alias.
            index.setdefault(md.stem, md)

            declared = read_front_matter_id(md)
            if declared:
                index[declared] = md
    return index


def read_front_matter_id(md: Path) -> str | None:
    """Return the `id:` declared in the leading YAML front-matter, if any."""
    try:
        with md.open(encoding="utf-8") as fh:
            first = fh.readline()
            if first.strip() != "---":
                return None
            for line in fh:
                if line.strip() == "---":
                    return None
                match = FRONT_MATTER_ID_RE.match(line)
                if match:
                    return match.group(1)
    except OSError as exc:  # pragma: no cover - unreadable file is a real error
        print(f"error: cannot read {md}: {exc}", file=sys.stderr)
        raise SystemExit(2) from exc
    return None


def collect_labels(alert_files: list[Path], repo_root: Path):
    """Yield (relative_path, line_number, runbook_id) for every label found."""
    found = []
    for path in alert_files:
        rel = path.relative_to(repo_root)
        for lineno, line in enumerate(
            path.read_text(encoding="utf-8").splitlines(), start=1
        ):
            match = RUNBOOK_LABEL_RE.match(line)
            if match:
                found.append((rel, lineno, match.group(1)))
    return found


def main() -> int:
    alert_files = sorted(REPO_ROOT.glob(ALERT_GLOB))
    if not alert_files:
        print(
            f"error: '{ALERT_GLOB}' matched ZERO files under {REPO_ROOT}.\n"
            "       Refusing to report green over an empty match set. If the "
            "alert rules moved, update ALERT_GLOB here and the `paths:` filter "
            "in .github/workflows/alerts-validate.yml together.",
            file=sys.stderr,
        )
        return 1

    index = build_runbook_index(REPO_ROOT)
    if not index:
        print(
            "error: indexed ZERO runbooks from "
            f"{', '.join(RUNBOOK_ROOTS)}. The runbook roots moved or the "
            "checkout is incomplete; every label would fail for the wrong "
            "reason.",
            file=sys.stderr,
        )
        return 1

    labels = collect_labels(alert_files, REPO_ROOT)
    if not labels:
        print(
            f"error: found ZERO `runbook:` labels across {len(alert_files)} "
            "alert file(s). Either the label key was renamed or the rules lost "
            "their routing metadata; this gate would then prove nothing.",
            file=sys.stderr,
        )
        return 1

    dangling = [(rel, lineno, rb) for rel, lineno, rb in labels if rb not in index]

    print(
        f"alert files: {len(alert_files)} | runbook identifiers indexed: "
        f"{len(index)} | `runbook:` labels: {len(labels)} | "
        f"distinct: {len({rb for _, _, rb in labels})}"
    )

    if dangling:
        print(
            f"\nFAIL: {len(dangling)} `runbook:` label(s) do not resolve to any "
            "runbook file:",
            file=sys.stderr,
        )
        for rel, lineno, rb in dangling:
            print(f"  {rel}:{lineno}: runbook: {rb}  -> NO SUCH RUNBOOK", file=sys.stderr)
        print(
            "\nFix by EITHER repointing the label at a runbook that genuinely "
            "covers the alert, OR deleting the label. Do not invent a stub "
            "runbook to satisfy this gate: a wrong pointer is a lie, an absent "
            "one is merely an absence.",
            file=sys.stderr,
        )
        return 1

    print("OK: every `runbook:` label resolves to a runbook file.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
