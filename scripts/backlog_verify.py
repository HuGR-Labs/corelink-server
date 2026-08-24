#!/usr/bin/env python3
"""Check that BACKLOG.md still agrees with reality.

WHY THIS EXISTS
---------------
On 2026-08-23 a day of planning was built on notes that had quietly gone stale.
Three items recorded as open had in fact shipped days earlier; one cited a count
of hosted CI lanes that no longer matched either the file count or the job count.
Nothing was dishonest — the notes were simply written once and never re-checked,
and no mechanism existed that could notice.

So a list is not enough. The list has to be able to disagree with the world and
say so out loud. Every item in BACKLOG.md therefore carries its own verification
command, and this script runs them:

    verify exits 0        -> the declared status is CONFIRMED
    verify exits non-zero -> DRIFTED; the item says one thing, the repo says another

An item that genuinely cannot be checked by a command declares `verify: manual`
and must carry a fresh `last-verified` date. Those decay: past `max-age-days`
they go STALE and fail the gate. That is the whole point — an unverifiable claim
is allowed, but it is not allowed to sit unchallenged forever, which is exactly
what happened to the notes this replaces.

USAGE
-----
    python3 scripts/backlog_verify.py            # check every item
    python3 scripts/backlog_verify.py --id B-003 # check one
    python3 scripts/backlog_verify.py --format json

Exit code is 0 only when every item is CONFIRMED. Anything else is a red gate.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parent.parent
BACKLOG_PATH = REPO_ROOT / "BACKLOG.md"

# A fenced ```backlog block is the machine-readable half of an item; the prose
# around it is the human half. One file, one source — no generated mirror to
# drift out of sync (this repo has been bitten by hand-editing a generated file).
BLOCK_RE = re.compile(r"^```backlog\n(.*?)^```", re.MULTILINE | re.DOTALL)

# The prose half of an item is titled `### B-0NN — …`, and every reference from
# outside this file — a CHANGELOG entry, a PR body, a runbook — cites that
# heading. The block's `id:` is what the gate reads. Nothing made the two agree,
# and on 2026-08-24 they silently disagreed: the forked-partitions item was
# headed B-026 while its block declared `id: B-024`, and the Turborepo item held
# the mirror image. Both ids were unique, so the duplicate check below was
# content; the register simply pointed the wrong way for anyone following an id.
HEADING_RE = re.compile(r"^### (B-\d+)\b", re.MULTILINE)

REQUIRED_FIELDS = ("id", "repo", "owner", "status", "verify", "verify-means", "last-verified")
VALID_STATUS = ("open", "done", "parked")
VALID_OWNER = ("tl", "owner")
DEFAULT_MAX_AGE_DAYS = 14
# A command that hangs would turn a red gate into a stuck one, which is worse.
VERIFY_TIMEOUT_S = 120

CONFIRMED, DRIFTED, STALE, BROKEN = "CONFIRMED", "DRIFTED", "STALE", "BROKEN"


@dataclass
class Item:
    raw: dict
    line: int
    id: str = ""
    verdict: str = ""
    detail: str = ""
    evidence: str = ""
    problems: list[str] = field(default_factory=list)


def parse(text: str) -> list[Item]:
    items: list[Item] = []
    for m in BLOCK_RE.finditer(text):
        line = text[: m.start()].count("\n") + 1
        try:
            data = yaml.safe_load(m.group(1)) or {}
        except yaml.YAMLError as e:
            items.append(Item(raw={}, line=line, id=f"<unparseable@{line}>", verdict=BROKEN,
                              detail=f"the backlog block is not valid YAML: {e}"))
            continue
        if not isinstance(data, dict):
            items.append(Item(raw={}, line=line, id=f"<malformed@{line}>", verdict=BROKEN,
                              detail="a backlog block must be a mapping of fields"))
            continue
        items.append(Item(raw=data, line=line, id=str(data.get("id", f"<no id@{line}>"))))
    return items


def validate_schema(item: Item) -> None:
    """Reject a malformed item outright rather than half-checking it.

    A silently skipped item is precisely the failure this gate exists to prevent,
    so a missing field is a hard failure, never a warning.
    """
    d = item.raw
    for f in REQUIRED_FIELDS:
        if f not in d or d[f] in (None, ""):
            item.problems.append(f"missing required field `{f}`")
    # YAML turns bare `true`, `yes`, `on`, `no`, `off` into booleans, so a verify
    # written without quotes silently stops being a command. Caught by this file's
    # own self-test on 2026-08-23; a real item could make the same mistake and would
    # otherwise report DRIFTED with a baffling "command not found".
    if "verify" in d and not isinstance(d["verify"], str):
        item.problems.append(
            f"verify must be a quoted string, got {type(d['verify']).__name__} "
            f"({d['verify']!r}) — YAML reads bare true/yes/on as booleans"
        )
    if d.get("status") not in VALID_STATUS and "status" in d:
        item.problems.append(f"status must be one of {VALID_STATUS}, got {d.get('status')!r}")
    if d.get("owner") not in VALID_OWNER and "owner" in d:
        item.problems.append(f"owner must be one of {VALID_OWNER}, got {d.get('owner')!r}")
    if "last-verified" in d:
        try:
            parse_date(d["last-verified"])
        except Exception:
            item.problems.append(f"last-verified must be YYYY-MM-DD, got {d.get('last-verified')!r}")


def parse_date(value) -> dt.date:
    if isinstance(value, dt.date):
        return value
    return dt.datetime.strptime(str(value), "%Y-%m-%d").date()


def age_days(item: Item, today: dt.date) -> int:
    return (today - parse_date(item.raw["last-verified"])).days


def run_verify(command: str) -> tuple[int, str]:
    try:
        p = subprocess.run(
            command, shell=True, cwd=REPO_ROOT, timeout=VERIFY_TIMEOUT_S,
            capture_output=True, text=True,
        )
    except subprocess.TimeoutExpired:
        # Distinguished from a normal failure: a timeout means the check itself
        # is broken, not that the item drifted.
        return 124, f"verify command exceeded {VERIFY_TIMEOUT_S}s"
    tail = (p.stdout + p.stderr).strip().splitlines()
    return p.returncode, tail[-1][:200] if tail else ""


def check(item: Item, today: dt.date, max_age: int) -> None:
    if item.verdict:  # already BROKEN at parse time
        return
    validate_schema(item)
    if item.problems:
        item.verdict = BROKEN
        item.detail = "; ".join(item.problems)
        return

    command = str(item.raw["verify"]).strip()
    if command == "manual":
        age = age_days(item, today)
        if age > max_age:
            item.verdict = STALE
            item.detail = (f"last verified {age} days ago (limit {max_age}); "
                           "re-verify it by hand and update last-verified, or give it a real check")
        else:
            item.verdict = CONFIRMED
            item.detail = f"manual, verified {age} day(s) ago"
        return

    code, tail = run_verify(command)
    item.evidence = tail
    if code == 0:
        item.verdict = CONFIRMED
        item.detail = "verify agrees with the declared status"
    elif code == 124:
        item.verdict = BROKEN
        item.detail = tail
    else:
        item.verdict = DRIFTED
        item.detail = (f"verify exited {code} — the item claims status "
                       f"`{item.raw['status']}` but the check for that no longer holds")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--id", help="check a single item")
    ap.add_argument("--format", choices=("text", "json"), default="text")
    ap.add_argument("--max-age-days", type=int, default=DEFAULT_MAX_AGE_DAYS,
                    help="how long a `verify: manual` item may go unchecked")
    ap.add_argument("--today", help="override today's date (YYYY-MM-DD), for testing")
    ap.add_argument("--file", help="check a different backlog file (used by the self-test)")
    args = ap.parse_args()

    path = Path(args.file).resolve() if args.file else BACKLOG_PATH
    if not path.exists():
        print(f"FATAL: {path} does not exist", file=sys.stderr)
        return 2

    today = dt.datetime.strptime(args.today, "%Y-%m-%d").date() if args.today else dt.date.today()
    items = parse(path.read_text())

    if not items:
        # An empty backlog is not a pass. It is far more likely that the format
        # broke, or the file was truncated, than that there is genuinely no work.
        print(f"FATAL: {path.name} contains no parseable ```backlog blocks", file=sys.stderr)
        return 2

    # A deleted item is invisible to per-item checks — every survivor still passes
    # while the record silently loses work. This happened on 2026-08-23: an edit
    # that rewrote one item removed its neighbour, and the gate reported all-green.
    # So ids must stay DENSE. Retiring an item means marking it, never deleting it.
    numbered = sorted(
        int(m.group(1))
        for i in items
        if (m := re.fullmatch(r"B-(\d+)", i.id))
    )
    if numbered:
        missing = sorted(set(range(1, max(numbered) + 1)) - set(numbered))
        if missing:
            gaps = ", ".join(f"B-{n:03d}" for n in missing)
            print(
                f"FATAL: BACKLOG.md is missing {gaps} — ids must be dense. An item was "
                "deleted rather than resolved. Restore it, or mark it retired in place.",
                file=sys.stderr,
            )
            return 2

    # Each block must sit under a heading that names the SAME id. Checked before
    # the per-item verifies so a mislabelled item cannot be "confirmed" under a
    # heading that describes different work.
    text = path.read_text()
    headings = [(m.start(), m.group(1)) for m in HEADING_RE.finditer(text)]
    mismatches: list[str] = []
    for m in BLOCK_RE.finditer(text):
        line = text[: m.start()].count("\n") + 1
        block_id = ""
        try:
            data = yaml.safe_load(m.group(1)) or {}
            if isinstance(data, dict):
                block_id = str(data.get("id", ""))
        except yaml.YAMLError:
            continue  # already reported as BROKEN by parse()
        if not re.fullmatch(r"B-\d+", block_id):
            continue
        prior = [h for pos, h in headings if pos < m.start()]
        if not prior:
            mismatches.append(f"  line {line}: block `id: {block_id}` has no `### B-…` heading above it")
        elif prior[-1] != block_id:
            mismatches.append(f"  line {line}: heading says {prior[-1]}, block says id: {block_id}")
    if mismatches:
        print(
            "FATAL: a heading and its block disagree about which item they are.\n"
            + "\n".join(mismatches)
            + "\nEvery reference from outside this file cites the HEADING; the gate reads\n"
            "the block. When they diverge the register points the wrong way and nothing\n"
            "notices, because both ids can still be unique.",
            file=sys.stderr,
        )
        return 2

    seen: dict[str, int] = {}
    for it in items:
        if it.id in seen:
            it.verdict = BROKEN
            it.detail = f"duplicate id — also declared at line {seen[it.id]}"
        else:
            seen[it.id] = it.line

    selected = [i for i in items if not args.id or i.id == args.id]
    if args.id and not selected:
        print(f"FATAL: no backlog item with id {args.id}", file=sys.stderr)
        return 2

    for it in selected:
        check(it, today, args.max_age_days)

    if args.format == "json":
        print(json.dumps([{
            "id": i.id, "line": i.line, "status": i.raw.get("status"),
            "owner": i.raw.get("owner"), "repo": i.raw.get("repo"),
            "verdict": i.verdict, "detail": i.detail, "evidence": i.evidence,
        } for i in selected], indent=2))
    else:
        width = max((len(i.id) for i in selected), default=8)
        for i in selected:
            print(f"  {i.verdict:9} {i.id:{width}}  {i.raw.get('status','?'):6} {i.detail}")
            if i.verdict in (DRIFTED, BROKEN) and i.evidence:
                print(f"  {'':9} {'':{width}}  └ {i.evidence}")
        counts = {v: sum(1 for i in selected if i.verdict == v) for v in (CONFIRMED, DRIFTED, STALE, BROKEN)}
        print(f"\n  {len(selected)} item(s): " + ", ".join(f"{v.lower()}={n}" for v, n in counts.items()))

    bad = [i for i in selected if i.verdict != CONFIRMED]
    if bad:
        print(f"\nBACKLOG.md disagrees with the repo on {len(bad)} item(s). "
              "Fix the item or fix the world — do not delete the check.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
