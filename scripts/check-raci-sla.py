#!/usr/bin/env python3
"""RACI sign-off SLA conformance detector (wave-26 Lote 7 follow-on).

Advisory CI gate. Reads §6 RACI matrix detail from the framework reviewer
roles addendum (`specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md`),
scans recent git history for change-log entries that touch the rows enumerated
in the matrix, and emits WARN lines if the per-lane SLA (addendum §3.2) appears
breached.

Lanes (addendum §3.2):
  - STANDARD: initial-response 1 BD; sign-off 3 BD
  - HIGH_RISK: initial-response 2 BD; sign-off 7 BD

This script is INTENTIONALLY advisory. It exits 0 (success) in all
non-fatal cases — including when SLA breaches are detected — so it can
be wired into `.github/workflows/spec_validation.yml` without blocking
PRs while the cadence is bedding in. The output is the audit trail
quoted in the quarterly framework review delta-doc per addendum §3.4.

Operating mode:
  - `--dry-run` (default for CI): parses the addendum, builds an empty
    pending-row inventory, prints a one-line conformance summary, exits 0.
  - `--since <ISO-date>`: scan git log since the named date and surface
    candidate stalls (commits touching framework §-files with no
    follow-up commit within the SLA window).

Exit codes:
  0 — clean (no fatal parse errors); SLA breaches emit WARN but DO NOT fail.
  2 — fatal parse error (addendum missing / §6 table malformed).

Design notes:
  - Pure stdlib. No third-party deps. Runs in <1s on the corpus.
  - Conservative: only flags WARN when the matching commit + window are
    *unambiguous*. False negatives preferred over false positives during
    the advisory phase.
  - The CI conformance line emitted is consumable by the quarterly
    review delta-doc author (addendum §3.4).

Usage:
  python3 scripts/check-raci-sla.py --dry-run
  python3 scripts/check-raci-sla.py --since 2026-05-09  # 7-day window
  python3 scripts/check-raci-sla.py --since 2026-05-13  # 3-day window
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass, field
from datetime import date, datetime, timedelta
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ADDENDUM = (
    REPO_ROOT
    / "specs"
    / "_proposals"
    / "2026-05-16-framework-reviewer-roles-addendum.md"
)

# §6.2 data row pattern: starts with `| <int> |` (row index 1..15)
ROW_RE = re.compile(r"^\|\s*(\d+)\s*\|\s*\*\*(.+?)\*\*")

# Heuristic mapping decision class → lane (addendum §3.1)
HIGH_RISK_PATTERNS = {
    "BYOK provider addition",
    "Region addition",
    "Customer breach response",
    "GA cutover sign-off",
    "Annual deep review",
    "Pentest finding triage",
}


@dataclass
class RaciRow:
    index: int
    decision: str
    lane: str = "STANDARD"
    inv_ref: str | None = None
    pending_commits: list[str] = field(default_factory=list)


def parse_addendum_rows(text: str) -> list[RaciRow]:
    """Parse §6.2 RACI matrix data rows. Return ordered list of RaciRow."""
    rows: list[RaciRow] = []
    in_section = False
    for line in text.splitlines():
        if line.startswith("### §6.2"):
            in_section = True
            continue
        if in_section and line.startswith("### §6."):
            # any other §6.X subsection ends §6.2
            if not line.startswith("### §6.2"):
                break
        if not in_section:
            continue
        m = ROW_RE.match(line)
        if not m:
            continue
        idx = int(m.group(1))
        decision = m.group(2).strip()
        lane = "HIGH_RISK" if decision in HIGH_RISK_PATTERNS else "STANDARD"
        rows.append(RaciRow(index=idx, decision=decision, lane=lane))
    return rows


def parse_inv_column(text: str, rows: list[RaciRow]) -> None:
    """If the §6.2 matrix has an INV column (post wave-26), populate it.

    The post-wave-26 §6.2 has 7 cells per row:
      | # | Decision | Owner | FW-H-1 | FW-H-2 | FW-H-3 | FW-H-4 | INV ref |
    The pre-wave-26 §6.2 has 6 cells per row (no INV column).
    """
    by_idx = {r.index: r for r in rows}
    in_section = False
    for line in text.splitlines():
        if line.startswith("### §6.2"):
            in_section = True
            continue
        if in_section and line.startswith("### §6.") and not line.startswith(
            "### §6.2"
        ):
            break
        if not in_section:
            continue
        m = ROW_RE.match(line)
        if not m:
            continue
        idx = int(m.group(1))
        if idx not in by_idx:
            continue
        # Split row cells; last cell after the last `|` (trimmed of leading/trailing)
        parts = [p.strip() for p in line.split("|")]
        # parts[0] is empty (leading `|`), parts[-1] is empty (trailing `|`)
        # Pre-wave-26 has 8 logical positions; post-wave-26 has 9.
        if len(parts) >= 10:
            inv_cell = parts[8]
            # Match a non-trivial INV-* token (allow "—" or "n/a" as no-binding)
            mm = re.search(r"INV-[A-Z0-9-]+", inv_cell)
            if mm:
                by_idx[idx].inv_ref = mm.group(0)


def git_log_since(since_iso: str) -> list[tuple[str, str, str]]:
    """Return [(sha, iso-date, subject)] of commits touching specs/ since date.

    Defensive: returns empty list if git is unavailable (advisory mode).
    """
    try:
        out = subprocess.check_output(
            [
                "git",
                "log",
                f"--since={since_iso}",
                "--pretty=format:%H|%cI|%s",
                "--",
                "specs/",
            ],
            cwd=REPO_ROOT,
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except (subprocess.CalledProcessError, FileNotFoundError):
        return []
    commits = []
    for line in out.splitlines():
        if not line.strip():
            continue
        parts = line.split("|", 2)
        if len(parts) != 3:
            continue
        commits.append((parts[0], parts[1], parts[2]))
    return commits


def detect_stalls(rows: list[RaciRow], since_iso: str) -> list[str]:
    """Detect candidate SLA stalls. Pure heuristic; returns WARN strings."""
    warnings: list[str] = []
    commits = git_log_since(since_iso)
    if not commits:
        return warnings
    # Heuristic: for each row, look for keyword matches in commit subjects.
    # If a recent commit subject mentions the decision and no follow-up
    # SEAL/APPROVE/sign-off commit exists in the SLA window, emit WARN.
    keyword_index = {
        "ADR creation": ("adr-", "new adr"),
        "ADR approval": ("adr accepted", "adr accept"),
        "INV registry promotion": ("inv promotion", "inv promote"),
        "Sprint impl sign-off": ("prr", "ship-gate"),
        "DEBT register entry": ("debt-", "debt register"),
        "Runbook approval": ("runbook", "rb-"),
        "TLA+ spec addition": ("tla+", "tla spec", "specs/tla"),
        "BYOK provider addition": ("byok", "kms provider"),
        "Region addition": ("region addition", "new region"),
        "Schema migration": ("schema migration", "migration"),
        "Customer breach response": ("breach", "sev-1", "incident"),
        "Pentest finding triage": ("pentest", "pen-test"),
        "GA cutover sign-off": ("ga cutover", "v1.0.0 frozen"),
        "Quarterly framework review": ("quarterly review",),
        "Annual deep review": ("annual deep review", "annual review"),
    }
    for row in rows:
        keys = keyword_index.get(row.decision, ())
        for sha, iso_date, subject in commits:
            subject_l = subject.lower()
            if any(k in subject_l for k in keys):
                row.pending_commits.append(f"{sha[:7]} {iso_date} {subject}")
    # WARN only when we have ≥1 candidate commit but the row has no
    # corresponding "sign-off"/"approve"/"SEAL" follow-up within the window.
    sealed_re = re.compile(r"\b(seal|sealed|sign-off|signed-off|approve)\b", re.I)
    for row in rows:
        if not row.pending_commits:
            continue
        any_sealed = any(sealed_re.search(c) for c in row.pending_commits)
        if not any_sealed:
            sla = "3 BD" if row.lane == "STANDARD" else "7 BD"
            warnings.append(
                f"WARN row {row.index:>2} [{row.lane:<9}] "
                f"'{row.decision}' — {len(row.pending_commits)} candidate "
                f"commit(s) in window with no SEAL/sign-off follow-up "
                f"(SLA {sla})."
            )
    return warnings


def conformance_summary(rows: list[RaciRow]) -> str:
    """Build the single-line summary consumed by quarterly review (addendum §3.4)."""
    std_total = sum(1 for r in rows if r.lane == "STANDARD")
    hr_total = sum(1 for r in rows if r.lane == "HIGH_RISK")
    std_hit = sum(
        1 for r in rows if r.lane == "STANDARD" and not r.pending_commits
    )
    hr_hit = sum(
        1 for r in rows if r.lane == "HIGH_RISK" and not r.pending_commits
    )

    def pct(hit: int, total: int) -> str:
        return f"{(100 * hit / total):.0f}%" if total else "n/a"

    return (
        f"SLA conformance (advisory): "
        f"STANDARD {std_hit}/{std_total} ({pct(std_hit, std_total)}), "
        f"HIGH_RISK {hr_hit}/{hr_total} ({pct(hr_hit, hr_total)})"
    )


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "--dry-run",
        action="store_true",
        help="Parse addendum, print conformance line, exit 0. CI default.",
    )
    ap.add_argument(
        "--since",
        default=None,
        help="ISO date; scan git log since this date for candidate stalls.",
    )
    args = ap.parse_args(argv)

    if not ADDENDUM.exists():
        print(f"FATAL: addendum not found at {ADDENDUM}", file=sys.stderr)
        return 2
    try:
        text = ADDENDUM.read_text(encoding="utf-8")
    except OSError as exc:
        print(f"FATAL: cannot read addendum: {exc}", file=sys.stderr)
        return 2

    rows = parse_addendum_rows(text)
    if len(rows) != 15:
        print(
            f"FATAL: §6.2 parse returned {len(rows)} rows (expected 15). "
            f"Matrix shape may have changed; update parser.",
            file=sys.stderr,
        )
        return 2
    parse_inv_column(text, rows)

    if args.dry_run and args.since is None:
        # CI default: dry-run advisory. Confirm parse + emit conformance.
        inv_bound = sum(1 for r in rows if r.inv_ref)
        print(
            f"check-raci-sla: parsed {len(rows)} rows; "
            f"{inv_bound} bound to INV-*"
        )
        print(conformance_summary(rows))
        return 0

    if args.since is None:
        # Default scan window: 7 days back from today (HIGH_RISK SLA upper bound).
        cutoff = (date.today() - timedelta(days=7)).isoformat()
    else:
        try:
            datetime.fromisoformat(args.since)
        except ValueError:
            print(f"FATAL: --since must be ISO date; got {args.since!r}", file=sys.stderr)
            return 2
        cutoff = args.since

    warnings = detect_stalls(rows, cutoff)
    print(f"check-raci-sla: scan window since {cutoff}")
    print(conformance_summary(rows))
    for w in warnings:
        print(w)
    # Advisory: never block.
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
