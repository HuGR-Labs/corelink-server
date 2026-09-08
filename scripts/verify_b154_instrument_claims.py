#!/usr/bin/env python3
"""Fail-closed detector for the two active B-154 Markdown instrument claims.

The historical B-154 shell check excluded ``*`` from its anchored character
class.  Both source claims use Markdown emphasis, so the check silently
returned an empty population and could report a false clean result.  This
guard scans active Markdown text after removing formatting markers, while
ignoring fenced code and HTML comments.  Missing claims are an error: deleting
the evidence must not turn an open item into a green result.

This is a local source check only.  It does not amend legal documents, contact
counsel, notify customers, or infer that the documents were executed.
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DPA = ROOT / "legal/dpa/v1.0.0.en-US.md"
SLA = ROOT / "legal/sla/v1.0.0.md"


class VerificationError(RuntimeError):
    """The active-claim population is missing or malformed."""


@dataclass(frozen=True)
class Claim:
    label: str
    path: Path
    pattern: re.Pattern[str]


CLAIMS = (
    Claim(
        "dpa_object_lock",
        DPA,
        re.compile(r"\bimmutable\s+R2\s+with\s+Object\s+Lock\b", re.IGNORECASE),
    ),
    Claim(
        "sla_byok_kill_switch",
        SLA,
        re.compile(
            r"\bBYOK\s+kill[-\u2010\u2011\u2012\u2013\u2014]?switch\s+p99\s*[\u2264<]\s*5\s*min\b",
            re.IGNORECASE,
        ),
    ),
)

POLARITY_RE = re.compile(
    r"\b(?:no|not|never|without|cannot|can't|does\s+not|doesn't|"
    r"isn't|is\s+not|aren't|are\s+not|unavailable|deferred|"
    r"future(?:[- ]only)?|not\s+guaranteed)\b",
    re.IGNORECASE,
)


def _active_lines(markdown: str) -> list[tuple[int, str]]:
    """Return active Markdown lines with presentation syntax removed."""
    lines: list[tuple[int, str]] = []
    fenced = False
    html_comment = False
    for line_number, raw in enumerate(markdown.splitlines(), 1):
        stripped = raw.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            fenced = not fenced
            continue
        if fenced:
            continue
        line = raw
        if "<!--" in line:
            line = line.split("<!--", 1)[0]
            html_comment = True
        if html_comment:
            if "-->" in raw:
                line = raw.split("-->", 1)[1]
                html_comment = False
            else:
                continue
        # Preserve visible link labels, then remove Markdown emphasis/heading
        # markers.  The claim words remain unchanged regardless of bold/italic
        # presentation, including the current ``**Object Lock**`` form.
        line = re.sub(r"!?(\[([^\]]+)\])\([^)]*\)", r"\2", line)
        line = re.sub(r"^\s{0,3}#{1,6}\s*", "", line)
        line = re.sub(r"^\s{0,3}>\s?", "", line)
        line = re.sub(r"[`*_~]", "", line)
        if line.strip():
            lines.append((line_number, line))
    return lines


def scan_claims(markdown: str, path: Path) -> list[tuple[str, int, str]]:
    found: list[tuple[str, int, str]] = []
    for claim in CLAIMS:
        if claim.path != path:
            continue
        for line_number, line in _active_lines(markdown):
            for match in claim.pattern.finditer(line):
                # A negated/disclaimed sentence is not evidence of an active
                # positive instrument claim.  Inspect a bounded context on
                # both sides so phrases such as ``No ... Object Lock`` and
                # ``... is not guaranteed`` cannot satisfy the contract.
                context_before = line[max(0, match.start() - 96) : match.start()]
                context_after = line[match.end() : match.end() + 96]
                if POLARITY_RE.search(context_before) or POLARITY_RE.search(context_after):
                    continue
                found.append((claim.label, line_number, line.strip()))
    return found


def verify_texts(dpa_text: str, sla_text: str) -> list[tuple[str, int, str]]:
    found = scan_claims(dpa_text, DPA) + scan_claims(sla_text, SLA)
    labels = {label for label, _line, _text in found}
    expected = {claim.label for claim in CLAIMS}
    missing = sorted(expected - labels)
    if missing:
        raise VerificationError(
            "active B-154 instrument claim population missing (fail-closed): "
            + ", ".join(missing)
        )
    return found


def _must_reject(dpa_text: str, sla_text: str, label: str) -> None:
    try:
        verify_texts(dpa_text, sla_text)
    except VerificationError:
        return
    raise AssertionError(f"B-154 negative mutation unexpectedly passed: {label}")


def self_test(dpa_text: str, sla_text: str) -> None:
    """Exercise formatting, deletion, comment, and negative-status mutations."""
    found = verify_texts(dpa_text, sla_text)
    if {label for label, _line, _text in found} != {claim.label for claim in CLAIMS}:
        raise AssertionError("B-154 baseline claim population drifted")

    # Removing emphasis must not hide either active claim.
    plain_dpa = dpa_text.replace("**immutable R2 with Object Lock**", "immutable R2 with Object Lock")
    plain_sla = sla_text.replace("**BYOK kill-switch p99 ≤ 5 min**", "BYOK kill-switch p99 ≤ 5 min")
    verify_texts(plain_dpa, plain_sla)

    _must_reject(
        dpa_text.replace("immutable R2 with Object Lock", "immutable R2 with retention metadata", 1),
        sla_text,
        "dpa-claim-deleted",
    )
    _must_reject(
        dpa_text,
        sla_text.replace("BYOK kill-switch p99 ≤ 5 min", "BYOK activation is unavailable", 1),
        "sla-claim-deleted",
    )
    _must_reject("## Object Lock\nR2 retention is not configured.\n", sla_text, "heading-only-object-lock")
    _must_reject("R2 does not implement Object Lock.\n", sla_text, "negative-object-lock-status")
    _must_reject(
        "No immutable R2 with Object Lock is guaranteed.\n",
        sla_text,
        "negated-object-lock-claim",
    )
    _must_reject(dpa_text, "BYOK kill-switch is not available.\n", "negative-byok-status")
    _must_reject(
        dpa_text,
        "BYOK kill-switch p99 ≤ 5 min is not guaranteed.\n",
        "disclaimed-byok-claim",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true", help="run in-memory negative mutations")
    args = parser.parse_args()
    try:
        dpa_text = DPA.read_text(encoding="utf-8")
        sla_text = SLA.read_text(encoding="utf-8")
        found = verify_texts(dpa_text, sla_text)
        if args.self_test:
            self_test(dpa_text, sla_text)
    except (OSError, UnicodeError, VerificationError, AssertionError) as exc:
        print(f"FAIL: B-154 active claim verifier: {exc}", file=sys.stderr)
        return 1
    details = "; ".join(f"{label}@{line}" for label, line, _text in found)
    suffix = "; negative mutations rejected" if args.self_test else ""
    print(f"B-154 open: active instrument claims={len(found)} ({details}){suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
