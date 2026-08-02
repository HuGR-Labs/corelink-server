#!/usr/bin/env python3
"""ADR-0042 §A1 gate: the declared TLC pin and the pin CI actually uses must agree.

Why this exists
---------------
§A1 says every `tla2tools.jar` SHA-256 bump requires an entry in the ADR. On
2026-07-09 a bump shipped (commit `1b05baa3`) that never made it into the
addendum, so for three weeks the ADR declared `237332bd…` while every CI carrier
used `33de7da9…`. A supply-chain policy whose record of truth silently diverges
from the artifact it governs provides no assurance — the document says one thing,
the pipeline enforces another, and nobody notices because both look internally
consistent.

The policy was a sentence in a Markdown file. A sentence cannot fail a build.
This can.

What it checks
--------------
1. ADR-0042 §A1 declares exactly one current SHA-256 (64 lowercase hex).
2. Every carrier that pins TLC agrees with it, byte for byte.
3. No carrier that pins TLC has gone missing since this list was written — a
   silently-dropped carrier is how a stale pin survives a repo-wide grep.

Exit 0 = consistent. Exit 1 = drift, naming the offender.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ADR = REPO / "specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md"

# Every file that carries the pin. Kept EXPLICIT rather than globbed: a glob
# silently shrinks when a file is renamed, and "the gate found nothing so it
# passed" is precisely the failure mode this gate exists to prevent.
CARRIERS = [
    ".github/workflows/tla_check.yml",
    # 2026-08-02: was 8 carriers, then 4. tla_billing_check / tla_dsr_erasure_check /
    # tla_region_residency_check / tla_runbooks_check were retired as exact
    # duplicates of tla_check (same specs, same .cfg files), so their copies of
    # the pin went with them. Fewer carriers is the point: every extra copy of a
    # SHA-256 is another place for §A1 and CI to drift apart, which is the defect
    # this script exists to catch.
    # nightly.yml stays: it is NOT a duplicate — it model-checks the same specs
    # against SEPARATE `<spec>_nightly.cfg` bounds, i.e. different coverage.
    ".github/workflows/nightly.yml",
    # cas_foundation.yml dropped 2026-08-02 with its `tlc-canonical` job, which
    # ran 4 specs through the same .cfg files as tla_check.
    "scripts/run_tlc_corelink.sh",
]

SHA_RE = re.compile(r"\b([0-9a-f]{64})\b")
PIN_RE = re.compile(r"TLC_SHA256_PINNED\s*[:=]\s*['\"]?([0-9a-f]{64})['\"]?")


def declared_sha() -> str:
    """The SHA-256 under §A1 'Current pinned values'.

    Anchored to that heading on purpose: the addendum also lists SUPERSEDED
    pins and per-ceremony tables full of historical hashes, and matching the
    first hash in the file would happily accept any of them.
    """
    if not ADR.is_file():
        sys.exit(f"FAIL: ADR not found at {ADR.relative_to(REPO)}")
    text = ADR.read_text(encoding="utf-8")

    anchor = text.find("**Current pinned values**")
    if anchor == -1:
        sys.exit(
            "FAIL: ADR-0042 §A1 has no '**Current pinned values**' block — the "
            "gate cannot determine what the pin is DECLARED to be. Restore the "
            "block rather than deleting this check."
        )

    # Only the block itself: stop at the SUPERSEDED note or the next rule/heading.
    block = text[anchor:]
    for terminator in ("SUPERSEDED", "\n---", "\n#"):
        cut = block.find(terminator)
        if cut != -1:
            block = block[:cut]

    found = SHA_RE.findall(block)
    if len(found) != 1:
        sys.exit(
            f"FAIL: expected exactly ONE SHA-256 in the §A1 'Current pinned "
            f"values' block, found {len(found)}: {found}. An ambiguous "
            f"declaration cannot be enforced."
        )
    return found[0]


def main() -> int:
    want = declared_sha()
    problems: list[str] = []
    checked = 0

    for rel in CARRIERS:
        path = REPO / rel
        if not path.is_file():
            problems.append(
                f"{rel}: MISSING. It is listed as a TLC pin carrier but does not "
                f"exist. If it was renamed or retired, update CARRIERS in this "
                f"script in the same change — do not let the list rot silently."
            )
            continue

        pins = set(PIN_RE.findall(path.read_text(encoding="utf-8")))
        if not pins:
            problems.append(
                f"{rel}: no TLC_SHA256_PINNED found. Either the pin was dropped "
                f"(the artifact is now UNVERIFIED) or this file no longer carries "
                f"it — decide which, and fix the file or CARRIERS accordingly."
            )
            continue

        checked += 1
        for got in sorted(pins):
            if got != want:
                problems.append(
                    f"{rel}: pins {got}\n"
                    f"{' ' * len(rel)}  ADR §A1 declares {want}"
                )

    if problems:
        print("ADR-0042 §A1 TLC pin consistency: FAIL\n", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print(
            "\nThe ADR and CI disagree about which tla2tools.jar is trusted.\n"
            "Do NOT resolve this by editing whichever side is convenient: a pin\n"
            "changed without its §A1 ceremony is exactly the 2026-07-09 defect\n"
            "this gate was written for. Run the ceremony, record the evidence,\n"
            "then make both sides match.",
            file=sys.stderr,
        )
        return 1

    print(
        f"check_tlc_pin_consistency: OK — {checked}/{len(CARRIERS)} carriers "
        f"agree with ADR-0042 §A1 ({want[:12]}…)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
