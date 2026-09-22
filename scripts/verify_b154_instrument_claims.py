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
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DPA = ROOT / "legal/dpa/v1.0.0.en-US.md"
SLA = ROOT / "legal/sla/v1.0.0.md"
B083_RECEIPT = Path("evidence/owner-actions/B-083/byok-real-kms-lifecycle.json")
B046_PROBE = Path("evidence/owner-actions/B-046/object-lock-probe.json")
DOCKERFILE = Path("Dockerfile")
B083_UNEXECUTED_LIFECYCLE = frozenset({
    "customer_create_or_import", "provider_access", "wrap_unwrap", "revoke_restore",
    "rotate", "deletion_schedule", "audit_receipts", "tenant_isolation",
    "failure_retry", "residency",
})


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
SENTENCE_END_RE = re.compile(r"[.!?](?=\s|$)")


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


def _claim_sentence_scope(
    active_lines: list[tuple[int, str]], line_index: int, match: re.Match[str]
) -> str:
    """Return the sentence containing a claim, including wrapped Markdown lines."""
    current = active_lines[line_index][1]
    if current.lstrip().startswith("|"):
        # Markdown table rows are independent claims; never borrow polarity
        # from the row above or below.
        combined = current
        claim_start = match.start()
    else:
        start = line_index
        while start > 0:
            previous_number, previous = active_lines[start - 1]
            if previous_number != active_lines[start][0] - 1:
                break
            if previous.lstrip().startswith("|"):
                break
            start -= 1
        end = line_index
        while end + 1 < len(active_lines):
            next_number, following = active_lines[end + 1]
            if next_number != active_lines[end][0] + 1:
                break
            if following.lstrip().startswith("|"):
                break
            end += 1
        combined = " ".join(line for _number, line in active_lines[start : end + 1])
        claim_start = sum(
            len(line) + 1 for _number, line in active_lines[start:line_index]
        ) + match.start()

    scope_start = 0
    for boundary in SENTENCE_END_RE.finditer(combined, 0, claim_start):
        scope_start = boundary.end()
    scope_end_match = SENTENCE_END_RE.search(combined, claim_start + len(match.group(0)))
    scope_end = scope_end_match.start() if scope_end_match else len(combined)
    return combined[scope_start:scope_end]


def scan_claims(markdown: str, path: Path) -> list[tuple[str, int, str]]:
    found: list[tuple[str, int, str]] = []
    active_lines = _active_lines(markdown)
    for claim in CLAIMS:
        if claim.path != path:
            continue
        for line_index, (line_number, line) in enumerate(active_lines):
            for match in claim.pattern.finditer(line):
                # A negated/disclaimed sentence is not evidence of an active
                # positive instrument claim.  Scope the polarity check to the
                # whole sentence containing the claim; a fixed character
                # window would let a long filler string separate ``No`` from
                # the claim and turn a disclaimer into a false positive.
                sentence = _claim_sentence_scope(active_lines, line_index, match)
                if POLARITY_RE.search(sentence):
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


def verify_capability_state(byok: dict, object_lock: dict, dockerfile: str) -> None:
    """Require the exact current evidence boundary, never a stray 501/NotImplemented."""
    try:
        lifecycle = byok["lifecycle"]
        byok_unverified = (
            byok["schema_version"] == 2
            and byok["evidence_state"] == "BLOCKED"
            and byok["tenant_redacted"] == "NOT_PROVISIONED"
            and byok["runtime"] == {
                "provider": "aws", "image_digest": None, "execution_region": None,
            }
            and set(lifecycle) == B083_UNEXECUTED_LIFECYCLE
            and all(
                set(row) == {"status", "receipt_reference", "completed_at", "blocker"}
                and row["status"] == "NOT_EXECUTED"
                and row["receipt_reference"] is None
                and row["completed_at"] is None
                and isinstance(row["blocker"], str) and row["blocker"]
                for row in lifecycle.values()
            )
        )
        probe_indeterminate = (
            object_lock["classification"] == "INDETERMINATE"
            and object_lock["bucket_operation"]["status"] == "INDETERMINATE"
            and object_lock["object_operation"]["status"] == "SKIPPED"
        )
    except (KeyError, TypeError):
        raise VerificationError("B-154 capability evidence shape changed; re-review") from None
    if not byok_unverified or not probe_indeterminate:
        raise VerificationError("B-154 capability evidence changed; re-review runtime/probe before closing")

    active = "\n".join(
        line.split("#", 1)[0]
        for line in dockerfile.splitlines()
        if not line.lstrip().startswith("#")
    )
    commands = re.findall(r"(?m)^\s*cargo build\b[^;]*;", active)
    shipped = [
        command for command in commands
        if re.search(r"(?:^|\s)-p\s+corelink-server\b", command)
        and re.search(r"(?:^|\s)--bin\s+corelink-server\b", command)
    ]
    if len(shipped) != 1 or re.findall(r"--features\s+([\w-]+)", shipped[0]) != ["byok-aws-real"]:
        raise VerificationError("B-154 production Dockerfile BYOK feature changed; re-review")


def verify_repository_state(root: Path = ROOT) -> None:
    sources = (B083_RECEIPT, B046_PROBE, DOCKERFILE)
    for relative in sources:
        path = root / relative
        if not path.is_file() or path.is_symlink():
            raise VerificationError(f"B-154 source missing or non-regular: {relative}")
    try:
        byok = json.loads((root / B083_RECEIPT).read_text(encoding="utf-8"))
        object_lock = json.loads((root / B046_PROBE).read_text(encoding="utf-8"))
        dockerfile = (root / DOCKERFILE).read_text(encoding="utf-8")
    except (OSError, UnicodeError, ValueError) as exc:
        raise VerificationError(f"B-154 source unreadable or malformed: {exc}") from exc
    verify_capability_state(byok, object_lock, dockerfile)


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
        verify_repository_state()
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
