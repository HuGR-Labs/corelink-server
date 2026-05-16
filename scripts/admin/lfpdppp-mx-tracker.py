#!/usr/bin/env python3
"""
DEBT-025 — LFPDPPP MX attorney engagement tracker.

Reads a per-attorney JSON status file (`reports/lfpdppp-mx-tracker.json`)
and emits a markdown summary the Owner can paste into wave-28 audits
or use as the live attorney-engagement dashboard.

Per `specs/_audits/2026-05-16-lfpdppp-mx-engagement-package-final.md §4`
+ `docs/legal/lfpdppp-mx-attorney-shortlist.md §3.1`, the engagement flow is:

    NOT_CONTACTED → EMAIL_SENT → RESPONSE_RECEIVED → IN_NEGOTIATION
                  → RETAINER_SIGNED → OPINION_RECEIVED → ABSORBED   (terminal-success)
                                                       → DECLINED   (terminal-failure)

Per-attorney fields tracked:
    name, firm, contact_email, tier, score, email_sent_date,
    response_received_date, retainer_signed_date, opinion_received_date,
    absorption_date, engagement_window, state, notes

Usage:
    python3 scripts/admin/lfpdppp-mx-tracker.py                  # render markdown summary to stdout
    python3 scripts/admin/lfpdppp-mx-tracker.py --validate FILE  # validate-only; exit 0/1
    python3 scripts/admin/lfpdppp-mx-tracker.py --json           # machine-readable rollup

Exit codes:
    0 — render or validation succeeded
    1 — validation failed (schema, state, or attorney count)
    2 — file/parse error

Mirrors `scripts/pentest-rfp-tracker.py` (wave-26 precedent for the same
shortlist→tracker→email→contract pattern, applied here for LFPDPPP MX).
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import date, datetime
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
DEFAULT_TRACKER = REPO_ROOT / "reports" / "lfpdppp-mx-tracker.json"

# State machine (per engagement-package-final §4 phases + shortlist §3.1 flow).
STATES = [
    "NOT_CONTACTED",
    "EMAIL_SENT",
    "RESPONSE_RECEIVED",
    "IN_NEGOTIATION",
    "RETAINER_SIGNED",
    "OPINION_RECEIVED",
    "ABSORBED",
    "DECLINED",
]

TERMINAL = {"ABSORBED", "DECLINED"}

# Forward-progress topology — set of legal next-states from each state.
# DECLINED is reachable from any non-terminal state.
ALLOWED_TRANSITIONS = {
    "NOT_CONTACTED": {"EMAIL_SENT", "DECLINED"},
    "EMAIL_SENT": {"RESPONSE_RECEIVED", "DECLINED"},
    "RESPONSE_RECEIVED": {"IN_NEGOTIATION", "DECLINED"},
    "IN_NEGOTIATION": {"RETAINER_SIGNED", "DECLINED"},
    "RETAINER_SIGNED": {"OPINION_RECEIVED", "DECLINED"},
    "OPINION_RECEIVED": {"ABSORBED", "DECLINED"},
    "ABSORBED": set(),
    "DECLINED": set(),
}

# Required attorney fields (schema check).
REQUIRED_FIELDS = {
    "name",
    "firm",
    "contact_email",
    "tier",
    "state",
}

# Optional date fields (may be null when state hasn't reached them).
OPTIONAL_DATE_FIELDS = {
    "email_sent_date",
    "response_received_date",
    "retainer_signed_date",
    "opinion_received_date",
    "absorption_date",
}

# Per-state precondition: which dates MUST be populated by this state.
# (Forward-only: once a date is populated, all earlier-state dates must be too.)
DATE_PRECONDITION = {
    "NOT_CONTACTED": set(),
    "EMAIL_SENT": {"email_sent_date"},
    "RESPONSE_RECEIVED": {"email_sent_date", "response_received_date"},
    "IN_NEGOTIATION": {"email_sent_date", "response_received_date"},
    "RETAINER_SIGNED": {
        "email_sent_date",
        "response_received_date",
        "retainer_signed_date",
    },
    "OPINION_RECEIVED": {
        "email_sent_date",
        "response_received_date",
        "retainer_signed_date",
        "opinion_received_date",
    },
    "ABSORBED": {
        "email_sent_date",
        "response_received_date",
        "retainer_signed_date",
        "opinion_received_date",
        "absorption_date",
    },
    # DECLINED has no date precondition — declination can occur anywhere.
    "DECLINED": set(),
}

# Tier-1 recommended candidates per shortlist §1.
TIER_1 = {"OLIVARES", "Basham, Ringe y Correa", "Sánchez Devanny"}
# Tier-2 fallback candidates per shortlist §2.
TIER_2 = {"Galicia Abogados", "Creel, García-Cuéllar, Aiza y Enríquez"}


def _parse_date(raw: object, field: str, attorney: str) -> date | None:
    if raw is None:
        return None
    if not isinstance(raw, str):
        raise ValueError(
            f"attorney {attorney!r} field {field!r}: expected ISO date string, got {type(raw).__name__}"
        )
    try:
        return datetime.strptime(raw, "%Y-%m-%d").date()
    except ValueError as exc:
        raise ValueError(
            f"attorney {attorney!r} field {field!r}: invalid date {raw!r} ({exc})"
        ) from exc


def validate(tracker: dict) -> list[str]:
    """Return list of validation errors. Empty list = green."""
    errors: list[str] = []

    if not isinstance(tracker, dict):
        return ["root: tracker JSON must be an object"]

    # Top-level shape.
    for key in ("schema_version", "updated", "attorneys"):
        if key not in tracker:
            errors.append(f"root: missing required key {key!r}")

    if errors:
        return errors

    if tracker["schema_version"] != "1.0":
        errors.append(
            f"root: unsupported schema_version {tracker['schema_version']!r} (expected '1.0')"
        )

    try:
        _parse_date(tracker["updated"], "updated", "<root>")
    except ValueError as exc:
        errors.append(str(exc))

    attorneys = tracker["attorneys"]
    if not isinstance(attorneys, list):
        errors.append("attorneys: must be a list")
        return errors

    if len(attorneys) < 5:
        errors.append(
            f"attorneys: expected at least 5 entries (shortlist §1+§2 baseline), got {len(attorneys)}"
        )

    seen_firms: set[str] = set()
    for idx, attorney in enumerate(attorneys):
        if not isinstance(attorney, dict):
            errors.append(f"attorneys[{idx}]: must be an object")
            continue

        firm = attorney.get("firm", f"<attorneys[{idx}]>")

        missing = REQUIRED_FIELDS - attorney.keys()
        if missing:
            errors.append(
                f"attorney firm {firm!r}: missing required fields {sorted(missing)!r}"
            )
            continue

        if firm in seen_firms:
            errors.append(f"attorney firm {firm!r}: duplicate firm name")
        seen_firms.add(firm)

        state = attorney["state"]
        if state not in STATES:
            errors.append(
                f"attorney firm {firm!r}: invalid state {state!r} (must be one of {STATES})"
            )
            continue

        tier = attorney.get("tier")
        if tier not in (1, 2):
            errors.append(
                f"attorney firm {firm!r}: invalid tier {tier!r} (must be 1 or 2)"
            )

        # Per-state date precondition.
        required_dates = DATE_PRECONDITION[state]
        for field in required_dates:
            if attorney.get(field) is None:
                errors.append(
                    f"attorney firm {firm!r}: state {state} requires field {field!r} to be populated"
                )

        # Date parsing.
        parsed: dict[str, date | None] = {}
        for field in OPTIONAL_DATE_FIELDS:
            if field in attorney:
                try:
                    parsed[field] = _parse_date(attorney[field], field, firm)
                except ValueError as exc:
                    errors.append(str(exc))

        # Date ordering (monotonic).
        ordered_fields = [
            "email_sent_date",
            "response_received_date",
            "retainer_signed_date",
            "opinion_received_date",
            "absorption_date",
        ]
        last: date | None = None
        last_field: str | None = None
        for field in ordered_fields:
            value = parsed.get(field)
            if value is None:
                continue
            if last is not None and value < last:
                errors.append(
                    f"attorney firm {firm!r}: field {field!r} ({value}) precedes {last_field!r} ({last})"
                )
            last, last_field = value, field

        # Contact email basic shape (allow null/empty if NOT_CONTACTED).
        contact = attorney.get("contact_email")
        if state != "NOT_CONTACTED":
            if not contact or "@" not in (contact or ""):
                errors.append(
                    f"attorney firm {firm!r}: state {state} requires a valid contact_email (got {contact!r})"
                )

    return errors


def _state_marker(state: str) -> str:
    return {
        "NOT_CONTACTED": "[ ]",
        "EMAIL_SENT": "[>]",
        "RESPONSE_RECEIVED": "[+]",
        "IN_NEGOTIATION": "[~]",
        "RETAINER_SIGNED": "[#]",
        "OPINION_RECEIVED": "[*]",
        "ABSORBED": "[X]",
        "DECLINED": "[-]",
    }.get(state, "[ ]")


def _tier_label(firm: str, tier: int) -> str:
    if tier == 1:
        return "T1"
    if tier == 2:
        return "T2"
    return "T?"


def render_markdown(tracker: dict) -> str:
    lines: list[str] = []
    lines.append(
        f"# LFPDPPP MX Attorney Engagement Tracker — DEBT-025 (updated {tracker['updated']})"
    )
    lines.append("")
    lines.append(
        "Per `specs/_audits/2026-05-16-lfpdppp-mx-engagement-package-final.md` "
        "+ `docs/legal/lfpdppp-mx-attorney-shortlist.md`."
    )
    lines.append("")

    attorneys = sorted(
        tracker["attorneys"],
        key=lambda a: (
            STATES.index(a["state"]),
            a["tier"],
            a["firm"],
        ),
    )

    # Rollup counts.
    counts: dict[str, int] = {state: 0 for state in STATES}
    for a in attorneys:
        counts[a["state"]] += 1

    lines.append("## State rollup")
    lines.append("")
    lines.append("| State | Count |")
    lines.append("|---|---|")
    for state in STATES:
        if counts[state] > 0:
            lines.append(f"| {state} | {counts[state]} |")
    lines.append("")

    # Per-attorney table.
    lines.append("## Per-attorney status")
    lines.append("")
    lines.append(
        "| State | Tier | Firm | Attorney | Contact | Email sent | Response | Retainer signed | Opinion received | Absorbed |"
    )
    lines.append("|---|---|---|---|---|---|---|---|---|---|")
    for a in attorneys:
        lines.append(
            "| {marker} {state} | {tier} | {firm} | {name} | {contact} | {sent} | {resp} | {ret} | {opn} | {abs} |".format(
                marker=_state_marker(a["state"]),
                state=a["state"],
                tier=_tier_label(a["firm"], a["tier"]),
                firm=a["firm"],
                name=a.get("name") or "_(tbd)_",
                contact=a.get("contact_email") or "_(tbd)_",
                sent=a.get("email_sent_date") or "—",
                resp=a.get("response_received_date") or "—",
                ret=a.get("retainer_signed_date") or "—",
                opn=a.get("opinion_received_date") or "—",
                abs=a.get("absorption_date") or "—",
            )
        )
    lines.append("")

    # Notes section.
    lines.append("## Per-attorney notes")
    lines.append("")
    for a in attorneys:
        notes = a.get("notes") or "_(none)_"
        score = a.get("score")
        score_str = f", {score}/100" if score is not None else ""
        lines.append(
            f"- **{a['firm']}** ({_tier_label(a['firm'], a['tier'])}{score_str}, {a['state']}): {notes}"
        )
    lines.append("")

    # Owner action queue (derived).
    lines.append("## Owner action queue")
    lines.append("")
    queue: list[str] = []
    for a in attorneys:
        state = a["state"]
        if state == "NOT_CONTACTED" and a["tier"] == 1:
            queue.append(
                f"- [ ] Send engagement email to **{a['firm']}** (tier-1; per shortlist §1)"
            )
        elif state == "EMAIL_SENT":
            queue.append(
                f"- [ ] Await response from **{a['firm']}** (email sent {a.get('email_sent_date')}; 14-day deadline per shortlist §3.2)"
            )
        elif state == "RESPONSE_RECEIVED":
            queue.append(
                f"- [ ] Review proposal from **{a['firm']}** + reference-check (7-day vetting window)"
            )
        elif state == "IN_NEGOTIATION":
            queue.append(
                f"- [ ] Finalize retainer with **{a['firm']}** + pay 50% deposit"
            )
        elif state == "RETAINER_SIGNED":
            queue.append(
                f"- [ ] Await written opinion from **{a['firm']}** (target T+21d from {a.get('retainer_signed_date')})"
            )
        elif state == "OPINION_RECEIVED":
            queue.append(
                f"- [ ] Absorb opinion from **{a['firm']}** (privacy notice corrections + metadata.yaml mx_attorney update + EVT-044 PDFs to R2 + DEBT-025 → CLOSED)"
            )
    if not queue:
        queue.append("- _(no pending actions)_")
    lines.extend(queue)
    lines.append("")

    return "\n".join(lines)


def render_json_rollup(tracker: dict) -> str:
    counts: dict[str, int] = {state: 0 for state in STATES}
    for a in tracker["attorneys"]:
        counts[a["state"]] += 1
    rollup = {
        "schema_version": tracker["schema_version"],
        "updated": tracker["updated"],
        "attorney_count": len(tracker["attorneys"]),
        "state_counts": counts,
        "tier_1_status": {
            a["firm"]: a["state"]
            for a in tracker["attorneys"]
            if a["tier"] == 1
        },
        "tier_2_status": {
            a["firm"]: a["state"]
            for a in tracker["attorneys"]
            if a["tier"] == 2
        },
    }
    return json.dumps(rollup, indent=2, sort_keys=True)


def load(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        print(f"ERROR: tracker file not found: {path}", file=sys.stderr)
        sys.exit(2)
    except json.JSONDecodeError as exc:
        print(f"ERROR: invalid JSON in {path}: {exc}", file=sys.stderr)
        sys.exit(2)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="DEBT-025 LFPDPPP MX attorney engagement tracker"
    )
    parser.add_argument(
        "--validate",
        metavar="FILE",
        nargs="?",
        const=str(DEFAULT_TRACKER),
        help="validate the tracker JSON file and exit (0 green, 1 errors)",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="emit machine-readable JSON rollup instead of markdown",
    )
    parser.add_argument(
        "--file",
        default=str(DEFAULT_TRACKER),
        help=f"tracker JSON path (default: {DEFAULT_TRACKER})",
    )
    args = parser.parse_args(argv)

    path = Path(args.validate or args.file)
    tracker = load(path)

    errors = validate(tracker)

    if args.validate is not None:
        if errors:
            print(f"FAIL: {len(errors)} validation error(s) in {path}:", file=sys.stderr)
            for err in errors:
                print(f"  - {err}", file=sys.stderr)
            return 1
        print(f"OK: {path} valid ({len(tracker['attorneys'])} attorneys)")
        return 0

    if errors:
        print("ERROR: cannot render; tracker has validation errors:", file=sys.stderr)
        for err in errors:
            print(f"  - {err}", file=sys.stderr)
        return 1

    if args.json:
        print(render_json_rollup(tracker))
    else:
        print(render_markdown(tracker))
    return 0


if __name__ == "__main__":
    sys.exit(main())
