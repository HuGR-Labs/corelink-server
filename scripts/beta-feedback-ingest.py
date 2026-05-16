#!/usr/bin/env python3
"""
beta-feedback-ingest.py — beta-pilot feedback triage ingest CLI.

Reads beta-feedback reports in NDJSON (one JSON object per line) — from a
webhook drop, a Slack export, or a manual CSV-import-then-NDJSON-convert
flow — and:

1. **Validates** each record against the canonical intake schema
   (`docs/internal/beta-feedback-triage.md` §1). Rejected lines are
   surfaced in the report with a non-zero rejection count; pipeline still
   produces output for the accepted lines.
2. **Classifies** each record into a severity bucket
   (P0 / P1 / P2 / P3) per the §3 rubric, allowing the report itself to
   pre-classify (`severity` field) and the rubric to ratchet it up if
   `tenant_impact` warrants.
3. **Routes** each record to the per-area-owner backlog per the §4
   routing matrix. Missing or unknown areas land on `area/triage`.
4. **Assigns a wave-N target** per §4 ("Wave assignment"). If the input
   record carries `wave_target` the dispatcher respects it; else it
   computes one from the current wave (passed via `--current-wave`) and
   the severity floor.
5. **Computes the ack-SLA deadline** per §2, returning an
   `ack_deadline_at` field per record.
6. **Emits** a deterministic markdown summary on stdout (or to
   `--out FILE`) grouped by severity, with a routing table and a wave
   dispatch table.

Usage:
    python3 scripts/beta-feedback-ingest.py --in feedback.ndjson \\
        --current-wave 23 --out summary.md

    cat feedback.ndjson | python3 scripts/beta-feedback-ingest.py \\
        --current-wave 23

    python3 scripts/beta-feedback-ingest.py --schema

Exit codes:
    0  success (zero rejected records).
    1  parse / schema rejections in the input stream.
    2  CLI / IO error.

Design notes:
- Pure stdlib (json, argparse, datetime, sys, pathlib). No third-party
  deps so it runs in any CI worker and in the Friday-digest cron job.
- Deterministic ordering: records sorted by (severity_rank, received_at,
  id) so two runs over the same input produce byte-identical markdown,
  which keeps the audit trail stable.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Iterable, Iterator

# ---------------------------------------------------------------------------
# Canonical schema — mirror of docs/internal/beta-feedback-triage.md §1.
# ---------------------------------------------------------------------------

REQUIRED_FIELDS: tuple[str, ...] = (
    "id",
    "received_at",
    "tenant_id",
    "reporter",
    "severity",
    "surface_area",
    "title",
    "repro_steps",
    "expected",
    "actual",
    "tenant_impact",
)

OPTIONAL_FIELDS: tuple[str, ...] = (
    "attachment_url",
    "wave_target",
    "notes",
)

ALL_FIELDS: tuple[str, ...] = REQUIRED_FIELDS + OPTIONAL_FIELDS

VALID_SEVERITIES: tuple[str, ...] = ("P0", "P1", "P2", "P3")

VALID_SURFACES: tuple[str, ...] = (
    "api",
    "cli",
    "dashboard",
    "webhook",
    "auth",
    "billing",
    "audit",
    "docs",
    "infra",
    "other",
)

VALID_IMPACTS: tuple[str, ...] = (
    "full_outage",
    "degraded",
    "workable",
    "cosmetic",
)

# §2 SLA — keep in lockstep with the doc.
ACK_SLA_HOURS: dict[str, int] = {
    "P0": 4,
    "P1": 24,
    "P2": 7 * 24,
    "P3": 30 * 24,
}

RESOLVE_SLA_HOURS: dict[str, int] = {
    "P0": 24,
    "P1": 7 * 24,
    "P2": 30 * 24,
    "P3": 90 * 24,
}

# §4 routing matrix — keep in lockstep with the doc.
ROUTING_MATRIX: dict[str, dict[str, str]] = {
    "api":       {"owner": "API platform",       "secondary": "TechLead",            "label": "area/api"},
    "cli":       {"owner": "CLI / DX",           "secondary": "API platform",        "label": "area/cli"},
    "dashboard": {"owner": "Admin Plane / UX",   "secondary": "TechLead",            "label": "area/dashboard"},
    "webhook":   {"owner": "Webhooks & DLQ",     "secondary": "API platform",        "label": "area/webhook"},
    "auth":      {"owner": "Identity & AuthZ",   "secondary": "TechLead (Security)", "label": "area/auth"},
    "billing":   {"owner": "Billing / Stripe",   "secondary": "TechLead",            "label": "area/billing"},
    "audit":     {"owner": "Audit chain & Neon", "secondary": "TechLead (Security)", "label": "area/audit"},
    "docs":      {"owner": "DocsOps",            "secondary": "Author of the doc",   "label": "area/docs"},
    "infra":     {"owner": "Platform / SRE",     "secondary": "TechLead",            "label": "area/infra"},
    "other":     {"owner": "TechLead (triage)",  "secondary": "—",                   "label": "area/triage"},
}

SEVERITY_RANK: dict[str, int] = {"P0": 0, "P1": 1, "P2": 2, "P3": 3}

# §4 wave assignment offsets.
WAVE_OFFSET: dict[str, int] = {"P0": 0, "P1": 1, "P2": 2, "P3": 3}


# ---------------------------------------------------------------------------
# Public API — exposed for tests (tests/beta_feedback_triage_test.py).
# ---------------------------------------------------------------------------


def classify(record: dict) -> str:
    """
    Apply the §3 rubric to a single intake record. Returns one of
    `VALID_SEVERITIES`.

    The reporter-supplied `severity` is the **floor** — the rubric never
    downgrades. The rubric ratchets up when the impact field warrants.
    """
    supplied = record.get("severity")
    if supplied not in VALID_SEVERITIES:
        raise ValueError(f"invalid severity: {supplied!r}")

    impact = record.get("tenant_impact")
    if impact not in VALID_IMPACTS:
        raise ValueError(f"invalid tenant_impact: {impact!r}")

    # §3 — a full_outage on a pilot tenant floors at P0.
    # The doc qualifies: only if it's a *pilot* tenant. We treat any
    # tenant_id starting with "t_pilot_" as a pilot for the rubric, with
    # an opt-out via `notes` containing `non-pilot-tenant` for the rare
    # internal-test case.
    tenant_id = record.get("tenant_id", "")
    is_pilot = tenant_id.startswith("t_pilot_") and "non-pilot-tenant" not in (
        record.get("notes") or ""
    )

    floor = supplied
    if impact == "full_outage":
        floor = "P0" if is_pilot else "P1"
    elif impact == "degraded":
        # degraded floors at P1 (significant impairment).
        floor = _max_severity(floor, "P1")
    elif impact == "workable":
        floor = _max_severity(floor, "P2")
    elif impact == "cosmetic":
        floor = _max_severity(floor, "P3")

    # Regression edge case: a record may carry `notes` containing
    # `regression-of:<prior_id>:<prior_severity>` — the rubric inherits
    # the prior severity as a floor.
    notes = record.get("notes") or ""
    for token in notes.split():
        if token.startswith("regression-of:"):
            parts = token.split(":")
            if len(parts) == 3 and parts[2] in VALID_SEVERITIES:
                floor = _max_severity(floor, parts[2])

    # Cross-tenant exposure short-circuits to P0 regardless of impact.
    title = (record.get("title") or "").lower()
    actual = (record.get("actual") or "").lower()
    for needle in ("cross-tenant", "rls bypass", "data leak", "plaintext leak"):
        if needle in title or needle in actual:
            floor = "P0"
            break

    return floor


def _max_severity(a: str, b: str) -> str:
    """Return the higher-priority severity (lower rank == higher prio)."""
    return a if SEVERITY_RANK[a] <= SEVERITY_RANK[b] else b


def route(record: dict) -> dict[str, str]:
    """
    Apply the §4 routing matrix. Returns dict with `owner`, `secondary`,
    `label`. Unknown / missing surfaces fall back to `other` → triage.
    """
    surface = record.get("surface_area") or "other"
    if surface not in ROUTING_MATRIX:
        surface = "other"
    return dict(ROUTING_MATRIX[surface])


def compute_sla(record: dict, severity: str) -> dict[str, str]:
    """
    Compute ack & resolve deadlines (RFC3339) from `received_at` and the
    classified severity. Returns dict with `ack_deadline_at`,
    `resolve_deadline_at`, `ack_sla_hours`, `resolve_sla_hours`.
    """
    received = _parse_rfc3339(record["received_at"])
    ack_hours = ACK_SLA_HOURS[severity]
    resolve_hours = RESOLVE_SLA_HOURS[severity]
    ack_deadline = received + timedelta(hours=ack_hours)
    resolve_deadline = received + timedelta(hours=resolve_hours)
    return {
        "ack_deadline_at": _fmt_rfc3339(ack_deadline),
        "resolve_deadline_at": _fmt_rfc3339(resolve_deadline),
        "ack_sla_hours": str(ack_hours),
        "resolve_sla_hours": str(resolve_hours),
    }


def assign_wave(record: dict, severity: str, current_wave: int) -> str:
    """
    Apply the §4 wave assignment rule. If the record carries a
    `wave_target`, respect it (triager-overridden). Otherwise:
    `wave-N+offset`. P0 preempts current wave.
    """
    explicit = record.get("wave_target")
    if explicit:
        return explicit
    offset = WAVE_OFFSET[severity]
    return f"wave-{current_wave + offset}"


def validate(record: dict) -> list[str]:
    """
    Return a list of validation errors (empty if record is well-formed).
    Checks required fields presence + enum-typed field values.
    """
    errors: list[str] = []
    for field_name in REQUIRED_FIELDS:
        if field_name not in record or record[field_name] in (None, ""):
            errors.append(f"missing required field: {field_name}")
    sev = record.get("severity")
    if sev is not None and sev not in VALID_SEVERITIES:
        errors.append(f"severity {sev!r} not in {VALID_SEVERITIES}")
    sa = record.get("surface_area")
    if sa is not None and sa not in VALID_SURFACES:
        errors.append(f"surface_area {sa!r} not in {VALID_SURFACES}")
    ti = record.get("tenant_impact")
    if ti is not None and ti not in VALID_IMPACTS:
        errors.append(f"tenant_impact {ti!r} not in {VALID_IMPACTS}")
    rcv = record.get("received_at")
    if rcv:
        try:
            _parse_rfc3339(rcv)
        except ValueError as exc:
            errors.append(f"received_at not RFC3339: {exc}")
    return errors


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------


def _parse_rfc3339(s: str) -> datetime:
    """Parse RFC3339 timestamp into a tz-aware datetime (UTC)."""
    # datetime.fromisoformat accepts most RFC3339 inputs since 3.11;
    # handle the trailing "Z" explicitly for older runtimes.
    if s.endswith("Z"):
        s = s[:-1] + "+00:00"
    dt = datetime.fromisoformat(s)
    if dt.tzinfo is None:
        raise ValueError(f"received_at missing timezone: {s!r}")
    return dt.astimezone(timezone.utc)


def _fmt_rfc3339(dt: datetime) -> str:
    """Format a tz-aware datetime as RFC3339 with trailing Z."""
    return dt.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


@dataclass(frozen=True)
class Processed:
    """An accepted record after classify + route + sla + wave assignment."""

    record: dict
    severity: str
    routing: dict[str, str]
    sla: dict[str, str]
    wave: str

    @property
    def rank(self) -> int:
        return SEVERITY_RANK[self.severity]


@dataclass
class IngestReport:
    """The end-to-end ingest result. Drives the markdown emitter + exit code."""

    processed: list[Processed] = field(default_factory=list)
    rejected: list[tuple[int, str, list[str]]] = field(default_factory=list)

    @property
    def accepted_count(self) -> int:
        return len(self.processed)

    @property
    def rejected_count(self) -> int:
        return len(self.rejected)

    def by_severity(self) -> dict[str, list[Processed]]:
        buckets: dict[str, list[Processed]] = {s: [] for s in VALID_SEVERITIES}
        for p in self.processed:
            buckets[p.severity].append(p)
        return buckets


def ingest(lines: Iterable[str], current_wave: int) -> IngestReport:
    """
    Iterate over NDJSON lines, validate + classify + route + assign wave,
    and return an IngestReport. Blank lines are skipped.
    """
    report = IngestReport()
    for lineno, raw in enumerate(lines, start=1):
        stripped = raw.strip()
        if not stripped:
            continue
        try:
            record = json.loads(stripped)
        except json.JSONDecodeError as exc:
            report.rejected.append((lineno, stripped[:80], [f"json: {exc}"]))
            continue
        errors = validate(record)
        if errors:
            report.rejected.append((lineno, record.get("id", "<no-id>"), errors))
            continue
        try:
            severity = classify(record)
            routing = route(record)
            sla = compute_sla(record, severity)
            wave = assign_wave(record, severity, current_wave)
        except (ValueError, KeyError) as exc:
            report.rejected.append(
                (lineno, record.get("id", "<no-id>"), [f"classify/route: {exc}"])
            )
            continue
        report.processed.append(Processed(record, severity, routing, sla, wave))
    return report


# ---------------------------------------------------------------------------
# Markdown emitter
# ---------------------------------------------------------------------------


def emit_markdown(report: IngestReport, current_wave: int) -> str:
    """Produce a deterministic markdown summary. Pure function — no IO."""
    lines: list[str] = []
    lines.append("# Beta feedback ingest summary")
    lines.append("")
    lines.append(f"- Current wave: **wave-{current_wave}**")
    lines.append(f"- Accepted records: **{report.accepted_count}**")
    lines.append(f"- Rejected records: **{report.rejected_count}**")
    lines.append("")

    buckets = report.by_severity()
    lines.append("## Summary by severity")
    lines.append("")
    lines.append("| Severity | Count | Ack SLA | Resolve SLA |")
    lines.append("| -------- | ----- | ------- | ----------- |")
    for sev in VALID_SEVERITIES:
        lines.append(
            f"| {sev} | {len(buckets[sev])} | "
            f"{ACK_SLA_HOURS[sev]}h | {RESOLVE_SLA_HOURS[sev]}h |"
        )
    lines.append("")

    for sev in VALID_SEVERITIES:
        bucket = sorted(
            buckets[sev],
            key=lambda p: (p.record["received_at"], p.record["id"]),
        )
        if not bucket:
            continue
        lines.append(f"## {sev} — {len(bucket)} report(s)")
        lines.append("")
        lines.append(
            "| ID | Tenant | Surface | Title | Owner | Wave | Ack deadline |"
        )
        lines.append(
            "| -- | ------ | ------- | ----- | ----- | ---- | ------------ |"
        )
        for p in bucket:
            r = p.record
            lines.append(
                f"| {r['id']} | {r['tenant_id']} | {r['surface_area']} "
                f"| {_md_escape(r['title'])} | {p.routing['owner']} "
                f"| {p.wave} | {p.sla['ack_deadline_at']} |"
            )
        lines.append("")

    if report.rejected:
        lines.append("## Rejected records")
        lines.append("")
        lines.append("| Line | ID / preview | Errors |")
        lines.append("| ---- | ------------ | ------ |")
        for lineno, ident, errs in report.rejected:
            joined = "; ".join(errs).replace("|", "/")
            lines.append(f"| {lineno} | {_md_escape(ident)} | {joined} |")
        lines.append("")

    return "\n".join(lines) + "\n"


def _md_escape(s: str) -> str:
    """Escape pipes for markdown table cells."""
    return s.replace("|", "\\|")


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------


def _print_schema() -> None:
    """Emit a JSON Schema-ish description of the intake record."""
    schema = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "BetaFeedbackIntake",
        "type": "object",
        "required": list(REQUIRED_FIELDS),
        "additionalProperties": False,
        "properties": {
            "id": {"type": "string", "pattern": r"^BFB-\d{4}-\d{2}-\d{2}-\d+$"},
            "received_at": {"type": "string", "format": "date-time"},
            "tenant_id": {"type": "string"},
            "reporter": {"type": "string"},
            "severity": {"type": "string", "enum": list(VALID_SEVERITIES)},
            "surface_area": {"type": "string", "enum": list(VALID_SURFACES)},
            "title": {"type": "string"},
            "repro_steps": {"type": "string"},
            "expected": {"type": "string"},
            "actual": {"type": "string"},
            "tenant_impact": {"type": "string", "enum": list(VALID_IMPACTS)},
            "attachment_url": {"type": "string"},
            "wave_target": {"type": "string"},
            "notes": {"type": "string"},
        },
    }
    print(json.dumps(schema, indent=2, sort_keys=True))


def _read_lines(path: str | None) -> Iterator[str]:
    if path is None or path == "-":
        yield from sys.stdin
    else:
        with open(path, encoding="utf-8") as fh:
            yield from fh


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="beta-feedback-ingest",
        description="Ingest beta-feedback NDJSON, classify, route, dispatch to wave.",
    )
    parser.add_argument(
        "--in", dest="in_path", default="-",
        help="Input NDJSON file (default: stdin).",
    )
    parser.add_argument(
        "--out", dest="out_path", default=None,
        help="Write markdown summary here (default: stdout).",
    )
    parser.add_argument(
        "--current-wave", type=int, default=None,
        help="Current engineering wave number (required unless --schema).",
    )
    parser.add_argument(
        "--schema", action="store_true",
        help="Print the canonical intake JSON schema and exit.",
    )
    args = parser.parse_args(argv)

    if args.schema:
        _print_schema()
        return 0

    if args.current_wave is None:
        parser.error("--current-wave is required unless --schema is set")

    try:
        report = ingest(_read_lines(args.in_path), args.current_wave)
    except OSError as exc:
        print(f"beta-feedback-ingest: io error: {exc}", file=sys.stderr)
        return 2

    md = emit_markdown(report, args.current_wave)
    if args.out_path and args.out_path != "-":
        Path(args.out_path).write_text(md, encoding="utf-8")
    else:
        sys.stdout.write(md)

    return 1 if report.rejected_count else 0


if __name__ == "__main__":
    sys.exit(main())
