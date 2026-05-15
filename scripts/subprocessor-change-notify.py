#!/usr/bin/env python3
"""subprocessor-change-notify.py — Detect sub-processor additions/removals
between two versions of `VENDOR-RISK-REGISTER.md` and emit a
`corelink.privacy.subprocessor.notify_required` CloudEvent per addition,
starting the **30-day grace clock** required by:

- **LGPD Art. 27 §4º** + **Art. 39** (Brazil): controller must be informed
  before sub-processor changes; for CoreLink-side processors we contractually
  honour a ≥30-calendar-day notice (DPA §6).
- **GDPR Art. 28 §2** (EU): processor must obtain prior specific or general
  written authorisation; ≥30-day notice provides the controller the
  opportunity to object.

# Design

The crate `corelink-privacy-sub-processor-emit` ships the canonical event
taxonomy + `SubProcessorEmitter` trait (production wiring → CD pipeline
CloudEvents fan-out to R2 audit-`<region>` Object Lock 7y + Cloudflare D1
broadcast log). Per the autonomous execution charter's
`trait-abstraction-defer` rule, **this Python script does not bind any
production transport** — instead it emits a JSON CloudEvent envelope to
stdout (or a file with `--out`) and uses the `InMemoryEmitter` test fake
shape so the same envelope can be ingested by the production wiring later
without schema churn.

The 30-day grace clock is computed deterministically:

    effective_at = detected_at + 30 calendar days  (UTC midnight aligned)

# Usage

    # Diff between git revisions (default: HEAD~1..HEAD)
    python3 scripts/subprocessor-change-notify.py

    # Diff between two explicit register snapshots
    python3 scripts/subprocessor-change-notify.py \\
        --prev /tmp/register-before.md \\
        --curr specs/_compliance/VENDOR-RISK-REGISTER.md

    # Just print help / version
    python3 scripts/subprocessor-change-notify.py --help

    # Force an effective date (rehearsals; default = now + 30d UTC)
    python3 scripts/subprocessor-change-notify.py --effective-at 2026-06-15

    # Emit to file
    python3 scripts/subprocessor-change-notify.py --out /tmp/events.jsonl

# Exit codes

  0  no additions detected (no notice required)
  1  additions detected and notice events emitted (CI should label PR)
  2  hard error (parse failure, missing file)

# Cross-references

- Runbook:      `specs/_runbooks/RB-SUBPROCESSOR-CHANGE.md`
- Generator:    `scripts/gen-public-subprocessors.py`
- Workflow:     `.github/workflows/subprocessors-sync.yml`
- Emitter trait: `crates/corelink-privacy-sub-processor-emit/src/emitter.rs`
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import pathlib
import re
import subprocess
import sys
from dataclasses import dataclass, asdict
from typing import Sequence


REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
REGISTER_PATH = REPO_ROOT / "specs" / "_compliance" / "VENDOR-RISK-REGISTER.md"

# CloudEvent type for the notify-required signal. Matches the canonical
# namespace `corelink.privacy.subprocessor.*` used by
# `corelink-privacy-sub-processor-emit`.
EVENT_TYPE = "corelink.privacy.subprocessor.notify_required"
EVENT_SOURCE = "/scripts/subprocessor-change-notify"
EVENT_SPEC_VERSION = "1.0"

# Grace window required by LGPD Art. 27 §4º + GDPR Art. 28 §2 + DPA §6.
GRACE_DAYS = 30


_TABLE_ROW = re.compile(r"^\|\s*(\d+)\s*\|")


@dataclass(frozen=True)
class VendorEntry:
    """A minimal vendor identity slice used for diffing."""

    vendor: str
    service: str
    category: str
    data_sharing: str

    @property
    def id_hash(self) -> str:
        """Stable subject identifier (CTRL-PRIV-014: never emit raw vendor
        name in audit subjects when not strictly necessary; here we include
        both raw + hash so the audit envelope satisfies both human-readable
        and CTRL-PRIV-014 hash-only consumers)."""
        return hashlib.sha256(self.vendor.encode("utf-8")).hexdigest()


def parse_register_rows(text: str) -> list[VendorEntry]:
    rows: list[VendorEntry] = []
    in_section_2 = False
    for ln in text.splitlines():
        if ln.startswith("## 2. Register"):
            in_section_2 = True
            continue
        if in_section_2 and ln.startswith("## "):
            break
        if not in_section_2:
            continue
        if not _TABLE_ROW.match(ln):
            continue
        cells = [c.strip() for c in ln.strip().strip("|").split("|")]
        if len(cells) < 5:
            continue
        rows.append(
            VendorEntry(
                vendor=cells[1],
                service=cells[2],
                category=cells[3],
                data_sharing=cells[4],
            )
        )
    return rows


@dataclass(frozen=True)
class Diff:
    added: tuple[VendorEntry, ...]
    removed: tuple[VendorEntry, ...]
    unchanged: tuple[VendorEntry, ...]

    @property
    def needs_notice(self) -> bool:
        # Additions trigger the 30-day customer broadcast.
        # Removals also trigger an informational broadcast (no grace required
        # — vendor is leaving the data flow, not being added). We surface both
        # but the regulatory 30-day clock applies only to additions.
        return bool(self.added) or bool(self.removed)


def diff_registers(prev: Sequence[VendorEntry], curr: Sequence[VendorEntry]) -> Diff:
    prev_by_vendor = {v.vendor: v for v in prev}
    curr_by_vendor = {v.vendor: v for v in curr}
    added = tuple(v for k, v in curr_by_vendor.items() if k not in prev_by_vendor)
    removed = tuple(v for k, v in prev_by_vendor.items() if k not in curr_by_vendor)
    unchanged = tuple(v for k, v in curr_by_vendor.items() if k in prev_by_vendor)
    return Diff(added=added, removed=removed, unchanged=unchanged)


def utc_midnight(d: dt.datetime) -> dt.datetime:
    return d.replace(hour=0, minute=0, second=0, microsecond=0, tzinfo=dt.timezone.utc)


def compute_effective_at(detected_at: dt.datetime, override: str | None) -> dt.datetime:
    if override:
        return dt.datetime.fromisoformat(override).replace(tzinfo=dt.timezone.utc)
    return utc_midnight(detected_at + dt.timedelta(days=GRACE_DAYS))


def build_event(
    entry: VendorEntry,
    change_kind: str,
    detected_at: dt.datetime,
    effective_at: dt.datetime,
    register_sha: str,
) -> dict:
    """Build a CloudEvents v1.0 envelope matching the
    `SubProcessorChangedPayload` shape declared by
    `corelink-privacy-sub-processor-emit::event`."""
    event_id = hashlib.sha256(
        f"{entry.vendor}|{change_kind}|{detected_at.isoformat()}".encode("utf-8")
    ).hexdigest()[:26].upper()  # ULID-shaped placeholder
    return {
        "specversion": EVENT_SPEC_VERSION,
        "id": event_id,
        "type": EVENT_TYPE,
        "source": EVENT_SOURCE,
        "time": detected_at.isoformat(),
        "datacontenttype": "application/json",
        "subject": f"sub-processor:{entry.id_hash[:12]}",
        "data": {
            "change_kind": change_kind,  # "added" | "removed"
            "vendor_name": entry.vendor,
            "vendor_id_hash": entry.id_hash,
            "service": entry.service,
            "category": entry.category,
            "data_sharing": entry.data_sharing,
            "detected_at": detected_at.isoformat(),
            "effective_at": effective_at.isoformat(),
            "grace_window_days": GRACE_DAYS,
            "legal_basis": [
                "LGPD Art. 27 §4º",
                "LGPD Art. 39",
                "GDPR Art. 28 §2",
                "DPA §6.2 / §6.4",
            ],
            "register_sha": register_sha,
            "notification_type": "subprocessor_change_30d_notice",
            "purpose": "legal_obligation",  # NOT consent-revocable per CTRL-PRIV-021
            "broadcast_target_plans": ["free", "pro", "team", "enterprise", "lighthouse"],
        },
    }


def git_show(rev: str, path: pathlib.Path) -> str | None:
    """Return the file contents at `rev`, or None if the revision/file
    does not exist (e.g. file was newly added in HEAD)."""
    try:
        rel = path.relative_to(REPO_ROOT)
    except ValueError:
        rel = path
    try:
        return subprocess.check_output(
            ["git", "show", f"{rev}:{rel}"],
            cwd=REPO_ROOT,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except subprocess.CalledProcessError:
        return None


def read_or_show(path_or_rev: str) -> str:
    """Accept either a filesystem path or a `<rev>:<path>` spec
    (e.g. `HEAD~1:specs/_compliance/VENDOR-RISK-REGISTER.md`)."""
    if ":" in path_or_rev and not pathlib.Path(path_or_rev).exists():
        rev, _, p = path_or_rev.partition(":")
        text = git_show(rev, REPO_ROOT / p)
        if text is None:
            raise FileNotFoundError(f"could not resolve {path_or_rev} via git show")
        return text
    return pathlib.Path(path_or_rev).read_text(encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Detect sub-processor additions/removals and emit the 30-day notify-required CloudEvents.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument("--prev", default=f"HEAD~1:{REGISTER_PATH.relative_to(REPO_ROOT)}",
                        help="Previous register source (path or '<rev>:<path>'; default: HEAD~1).")
    parser.add_argument("--curr", default=str(REGISTER_PATH),
                        help="Current register source (path or '<rev>:<path>'; default: working tree).")
    parser.add_argument("--out", type=pathlib.Path, default=None,
                        help="Write JSONL events to this file (default: stdout).")
    parser.add_argument("--effective-at", default=None,
                        help="Override effective date (ISO 8601); default = detected_at + 30 calendar days UTC.")
    parser.add_argument("--quiet", action="store_true",
                        help="Suppress informational stderr output.")
    parser.add_argument("--allow-empty", action="store_true",
                        help="Exit 0 even when additions/removals detected (useful for dry-run gates).")
    args = parser.parse_args(argv)

    try:
        prev_text = read_or_show(args.prev)
    except FileNotFoundError:
        if not args.quiet:
            print(f"info: previous register snapshot not found ({args.prev}); treating as empty baseline.", file=sys.stderr)
        prev_text = ""
    try:
        curr_text = read_or_show(args.curr)
    except FileNotFoundError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    prev_rows = parse_register_rows(prev_text)
    curr_rows = parse_register_rows(curr_text)
    diff = diff_registers(prev_rows, curr_rows)

    detected_at = dt.datetime.now(tz=dt.timezone.utc)
    effective_at = compute_effective_at(detected_at, args.effective_at)
    register_sha = hashlib.sha256(curr_text.encode("utf-8")).hexdigest()[:12]

    events: list[dict] = []
    for entry in diff.added:
        events.append(build_event(entry, "added", detected_at, effective_at, register_sha))
    for entry in diff.removed:
        # Removals do not start a 30-day clock (vendor is exiting); we still
        # broadcast for transparency. Effective_at == detected_at for removals.
        events.append(build_event(entry, "removed", detected_at, detected_at, register_sha))

    serialised = "\n".join(json.dumps(ev, sort_keys=True) for ev in events)

    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(serialised + ("\n" if serialised else ""), encoding="utf-8")
        target = str(args.out)
    else:
        if serialised:
            print(serialised)
        target = "stdout"

    if not args.quiet:
        print(
            f"info: prev_rows={len(prev_rows)} curr_rows={len(curr_rows)} "
            f"added={len(diff.added)} removed={len(diff.removed)} "
            f"events_emitted={len(events)} target={target} "
            f"effective_at={effective_at.isoformat()} grace_days={GRACE_DAYS}",
            file=sys.stderr,
        )

    if diff.needs_notice and not args.allow_empty:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
