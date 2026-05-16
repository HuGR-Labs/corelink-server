#!/usr/bin/env python3
"""pre-cutover-state-diff.py — compare the last two pre-cutover weekly digests.

Reads the BEGIN_STATE_MACHINE / END_STATE_MACHINE blocks from each of the
two most recent ``reports/pre-cutover-weekly/<YYYY-MM-DD>-digest.md`` files
(or the two paths passed positionally) and emits a per-item state delta
table to stdout.

The state ordering is the same scale used by ``pre-cutover-weekly-verify.sh``:

    UNKNOWN < NOT_STARTED == NOT_CONTACTED < IN_FLIGHT < DRAFT_READY
        < VENDOR_SELECTED < SIGNED < CLOSED

Exit codes:
    0  no regression (every item is at or above its prior state)
    1  regression (one or more items dropped state)
    2  setup error (missing files, missing block, …)

Usage:
    python3 scripts/pre-cutover-state-diff.py
    python3 scripts/pre-cutover-state-diff.py path/to/older.md path/to/newer.md
    python3 scripts/pre-cutover-state-diff.py --json     # JSON output
    python3 scripts/pre-cutover-state-diff.py --quiet    # no stdout; exit-code only

Charter: SYNCHRONOUS only (no asyncio). Pure stdlib. No PII handling beyond
what the digest already pseudonymized.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Optional


REPO_ROOT = Path(__file__).resolve().parent.parent
DIGEST_DIR = REPO_ROOT / "reports" / "pre-cutover-weekly"

STATE_ORDER = {
    "UNKNOWN": 0,
    "NOT_STARTED": 1,
    "NOT_CONTACTED": 1,
    "IN_FLIGHT": 2,
    "DRAFT_READY": 3,
    "VENDOR_SELECTED": 4,
    "SIGNED": 5,
    "CLOSED": 6,
}

BEGIN = "<!-- BEGIN_STATE_MACHINE -->"
END = "<!-- END_STATE_MACHINE -->"
ROW_RE = re.compile(r"^(?P<idx>\d+)\|(?P<state>[A-Z_]+)\|(?P<rc>[A-Z_]+)\|(?P<lt>[^|]*)\|(?P<title>.+)$")


def find_digests() -> tuple[Path, Path]:
    """Return (older, newer) paths from the digest dir."""
    if not DIGEST_DIR.is_dir():
        print(f"::error::digest dir not found: {DIGEST_DIR}", file=sys.stderr)
        sys.exit(2)
    digests = sorted(DIGEST_DIR.glob("*-digest.md"))
    if len(digests) < 2:
        print(
            f"::error::need at least 2 digests in {DIGEST_DIR}, found {len(digests)}",
            file=sys.stderr,
        )
        sys.exit(2)
    return digests[-2], digests[-1]


def parse_state_block(path: Path) -> dict[str, dict[str, str]]:
    """Extract the machine-block from a digest into a {idx: {state, rc, lt, title}} map."""
    if not path.is_file():
        print(f"::error::digest missing: {path}", file=sys.stderr)
        sys.exit(2)
    text = path.read_text(encoding="utf-8")
    try:
        block = text.split(BEGIN, 1)[1].split(END, 1)[0]
    except IndexError:
        print(f"::error::no machine-block in {path}", file=sys.stderr)
        sys.exit(2)
    out: dict[str, dict[str, str]] = {}
    for raw in block.splitlines():
        line = raw.strip()
        if not line or line.startswith("```"):
            continue
        m = ROW_RE.match(line)
        if not m:
            continue
        out[m.group("idx")] = {
            "state": m.group("state"),
            "rc": m.group("rc"),
            "lt": m.group("lt"),
            "title": m.group("title"),
        }
    return out


def state_ord(state: str) -> int:
    return STATE_ORDER.get(state, 0)


def compute_diff(
    older: dict[str, dict[str, str]],
    newer: dict[str, dict[str, str]],
) -> list[dict[str, object]]:
    """Per-item delta entries."""
    idxs = sorted(set(older) | set(newer), key=lambda x: int(x))
    rows: list[dict[str, object]] = []
    for idx in idxs:
        o = older.get(idx, {})
        n = newer.get(idx, {})
        os_ = o.get("state", "MISSING")
        ns_ = n.get("state", "MISSING")
        title = n.get("title") or o.get("title") or "(unknown)"
        if os_ == ns_:
            delta = "UNCHANGED"
        else:
            o_ord = state_ord(os_)
            n_ord = state_ord(ns_)
            if n_ord > o_ord:
                delta = "PROGRESS"
            elif n_ord < o_ord:
                delta = "REGRESSION"
            else:
                # Same ordinal but different label (e.g. NOT_STARTED ↔ NOT_CONTACTED).
                delta = "RELABEL"
        rows.append(
            {
                "idx": idx,
                "title": title,
                "prior_state": os_,
                "current_state": ns_,
                "delta": delta,
                "current_readiness_class": n.get("rc", "UNKNOWN"),
                "current_last_touched": n.get("lt", "unknown"),
            }
        )
    return rows


def render_markdown(older: Path, newer: Path, rows: list[dict[str, object]]) -> str:
    out: list[str] = []
    out.append(f"# Pre-Cutover State Diff — `{older.name}` → `{newer.name}`\n")
    out.append("| # | Item | Prior | Current | Δ | Readiness | Last touched |")
    out.append("|---|---|---|---|---|---|---|")
    for r in rows:
        marker = {
            "PROGRESS": "✅",
            "UNCHANGED": "·",
            "REGRESSION": "❌",
            "RELABEL": "≈",
        }.get(str(r["delta"]), "?")
        out.append(
            f"| {r['idx']} | {r['title']} | `{r['prior_state']}` | "
            f"`{r['current_state']}` | {marker} {r['delta']} | "
            f"`{r['current_readiness_class']}` | {r['current_last_touched']} |"
        )
    progress = sum(1 for r in rows if r["delta"] == "PROGRESS")
    unchanged = sum(1 for r in rows if r["delta"] == "UNCHANGED")
    regress = sum(1 for r in rows if r["delta"] == "REGRESSION")
    relabel = sum(1 for r in rows if r["delta"] == "RELABEL")
    out.append("")
    out.append(
        f"**Summary:** {progress} progress · {unchanged} unchanged · "
        f"{relabel} relabel · {regress} regression"
    )
    return "\n".join(out) + "\n"


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("older", nargs="?", help="older digest path (default: 2nd-newest in dir)")
    ap.add_argument("newer", nargs="?", help="newer digest path (default: newest in dir)")
    ap.add_argument("--json", action="store_true", help="emit JSON instead of markdown")
    ap.add_argument("--quiet", action="store_true", help="no stdout; exit-code only")
    args = ap.parse_args(argv)

    if args.older and args.newer:
        older_path = Path(args.older)
        newer_path = Path(args.newer)
    else:
        older_path, newer_path = find_digests()

    older = parse_state_block(older_path)
    newer = parse_state_block(newer_path)
    rows = compute_diff(older, newer)

    regressions = [r for r in rows if r["delta"] == "REGRESSION"]
    exit_code = 1 if regressions else 0

    if not args.quiet:
        if args.json:
            payload = {
                "older": str(older_path),
                "newer": str(newer_path),
                "rows": rows,
                "regression_count": len(regressions),
                "exit_code": exit_code,
            }
            print(json.dumps(payload, indent=2))
        else:
            print(render_markdown(older_path, newer_path, rows))

    return exit_code


if __name__ == "__main__":
    sys.exit(main())
