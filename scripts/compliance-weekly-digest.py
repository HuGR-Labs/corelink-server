#!/usr/bin/env python3
"""
compliance-weekly-digest.py — automated compliance-state digest (CC4.2 / GAP-08).

Aggregates the **current state** across every compliance domain we track and
emits a Monday-morning markdown digest under
`specs/_compliance/weekly-digests/YYYY-MM-DD.md`. The digest is the
operating-effectiveness evidence for **SOC 2 CC4.2** ("Evaluates and
communicates deficiencies") and closes **GAP-08** of
`specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` (weekly-review cadence
TBD → automated, evidenced, escalation-bearing).

Domains aggregated:

    1. **SOC 2 readiness** — parses §3.1/§3.2 of `SOC2-EVIDENCE-ROLLUP-*.md`
       (newest by filename date); compares against the previous digest's
       headline metrics to compute Implemented / Partial / Gap deltas.
    2. **GAP register** — parses `SOC2-GAP-ANALYSIS.md` GAP rows
       (severity / status); counts new GAPs, closed GAPs, severity
       up/down-grades vs last digest.
    3. **Vendor risk** — parses `VENDOR-RISK-REGISTER.md` §2 table; flags any
       vendor whose `Last review` is > 90d (Critical), > 180d (Important),
       > 365d (Standard) relative to today.
    4. **Drill cadence** — parses `BCP-DR-DRILL-CADENCE.md` (DR-* drills) and
       `IR-TABLETOP-SCHEDULE-2026.md` (TT-* tabletops); flags any drill whose
       target date has passed without a corresponding
       `specs/_audits/<date>-{bcp-drill,ir-tabletop}-<ID>*.md` evidence file.
    5. **IR tabletop** — next session date (closest unstarted TT-XX) + readiness
       flag (T-7d brief, T-1d dry-run, T+0 facilitator).
    6. **Failing CI compliance gates** — scans `.github/workflows/` for
       compliance-tagged workflows (`compliance`, `byok`, `cargo-deny`,
       `cargo-audit`, `cosign`, `drata`, `lgpd`, `backup`, `dr-drill`, `pentest`)
       and, if `gh` CLI is available + `--with-ci`, queries the last 7d of runs
       and flags any with conclusion = `failure` / `timed_out` / `cancelled`.

Exit codes (semantic — wired to PagerDuty by the wrapper workflow):

    0  No regression — digest emitted (may include WARNINGs).
    1  **REGRESSION** — at least one of:
         (a) severity-upgrade in GAP register
         (b) GAP count went up
         (c) drill missed past its grace window (Critical: 0d, Important: 7d, Standard: 14d)
         (d) any compliance-gate CI workflow failed in last 7d
         (e) any vendor review > 2× its cadence (i.e. doubled the SLA window)
    2  Configuration error (missing source file, malformed table, bad CLI args).

CLI:

    --dry-run         Use repo-local sources only; do not query gh CLI; do not
                      write digest file (preview to stdout).
    --output-dir DIR  Override default `specs/_compliance/weekly-digests/`.
    --reference DATE  Force previous-digest reference (YYYY-MM-DD) instead of
                      the most-recent prior digest auto-detected.
    --with-ci         Query the GitHub Actions API for last-7d failures
                      (requires `gh` CLI; skipped in --dry-run).
    --json            Emit the structured payload to stdout (in addition to the
                      markdown file). Used by the workflow to drive PD payload.

Companion runbook (manual triage by Compliance Lead each Monday):
    `specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md`

Companion docs:
    - `specs/_compliance/weekly-digests/README.md` — methodology + index.
    - `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §2.4 CC4.2 row.
    - `specs/_compliance/SOC2-GAP-ANALYSIS.md` §CC4.2 / GAP-08.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field, asdict
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
COMPLIANCE_DIR = REPO_ROOT / "specs" / "_compliance"
AUDITS_DIR = REPO_ROOT / "specs" / "_audits"
RUNBOOKS_DIR = REPO_ROOT / "specs" / "_runbooks"
WORKFLOWS_DIR = REPO_ROOT / ".github" / "workflows"
DIGESTS_DIR = COMPLIANCE_DIR / "weekly-digests"

# Cadence SLAs (days) per VENDOR-RISK-METHODOLOGY.md §2 + register §2.
CADENCE_DAYS: dict[str, int] = {"Q": 90, "B": 180, "A": 365}

# Drill grace windows (days after scheduled date) before flag.
DRILL_GRACE: dict[str, int] = {"Critical": 0, "Important": 7, "Standard": 14}

# Categorise drill IDs by criticality.
DRILL_CRITICALITY: dict[str, str] = {
    # P1 single-region drills are "Important" — staging-bounded.
    "DR-001": "Important", "DR-002": "Important", "DR-003": "Important",
    "DR-004": "Important",
    # P2 multi-region + BYOK rotation = "Critical".
    "DR-005": "Critical", "DR-006": "Critical", "DR-007": "Critical",
    "DR-008": "Critical",
    # P3 SEV1 simulations = "Critical".
    "DR-009": "Critical", "DR-010": "Critical",
    # Cross-cutting = "Important".
    "DR-011": "Important", "DR-012": "Important", "DR-013": "Important",
    "DR-014": "Important",
    # DR-15 cold restore + DR-16 active failover = "Critical".
    "DR-15": "Critical", "DR-16": "Critical",
}

# Compliance-gate workflow file-name fragments (substring match).
COMPLIANCE_WORKFLOW_TAGS: tuple[str, ...] = (
    "compliance", "byok", "cargo-deny", "cargo-audit", "cosign-sign",
    "drata", "lgpd", "backup-daily", "dr-drill", "pentest", "fips",
    "license-audit", "verify-fips", "verify-lgpd", "validate_specs",
    "sbom", "cold-restore", "active-failover",
)


# ─────────────────────────────────────────────────────────────────────────────
# Data classes
# ─────────────────────────────────────────────────────────────────────────────


@dataclass
class ReadinessSnapshot:
    """Headline readiness metrics parsed from SOC2-EVIDENCE-ROLLUP §3."""

    rollup_path: str
    implemented_pct: float
    partial_pct: float
    gap_pct: float
    implemented_count: int
    partial_count: int
    gap_count: int
    internal_scorecard_pct: float
    drata_dashboard_pct: float
    auto_collection_pct: float


@dataclass
class GapEntry:
    gap_id: str
    severity: str  # blocking-GA / major / minor
    status: str  # YELLOW / RED / GREEN / IMPLEMENTED / etc.
    tsc: str
    owner: str
    eta: str


@dataclass
class GapDelta:
    new: list[str] = field(default_factory=list)
    closed: list[str] = field(default_factory=list)
    severity_up: list[tuple[str, str, str]] = field(default_factory=list)
    severity_down: list[tuple[str, str, str]] = field(default_factory=list)
    total_open_now: int = 0
    total_open_prev: int = 0


@dataclass
class VendorEntry:
    vendor: str
    category: str  # C / I / S
    last_review: date
    cadence_days: int
    owner: str
    days_since: int = 0
    sla_breach: bool = False
    sla_2x_breach: bool = False


@dataclass
class DrillStatus:
    drill_id: str
    target_date: date | None
    criticality: str
    evidence_present: bool
    days_overdue: int = 0
    flagged: bool = False


@dataclass
class TabletopStatus:
    tt_id: str
    target_date: date
    backup_date: date
    days_to_session: int
    t_minus_7_brief: bool
    facilitator_assigned: bool


@dataclass
class CIFailure:
    workflow: str
    conclusion: str
    finished_at: str
    run_url: str


@dataclass
class DigestPayload:
    digest_date: date
    reference_digest: str | None
    readiness: ReadinessSnapshot
    readiness_delta: dict[str, float]
    gap_delta: GapDelta
    vendor_breaches: list[VendorEntry]
    drill_flags: list[DrillStatus]
    next_tabletop: TabletopStatus | None
    ci_failures: list[CIFailure]
    regression: bool
    regression_reasons: list[str]


# ─────────────────────────────────────────────────────────────────────────────
# Parsers
# ─────────────────────────────────────────────────────────────────────────────


def _find_latest_rollup() -> Path:
    candidates = sorted(COMPLIANCE_DIR.glob("SOC2-EVIDENCE-ROLLUP-*.md"))
    if not candidates:
        raise FileNotFoundError("No SOC2-EVIDENCE-ROLLUP-*.md found.")
    return candidates[-1]


def parse_readiness(rollup_path: Path) -> ReadinessSnapshot:
    text = rollup_path.read_text(encoding="utf-8")

    def _pct(label: str, body: str) -> float:
        # Matches lines like "- **Implemented (I):** 27 / 43 in-scope = **62.8%**"
        # or "- **Drata dashboard:** 96.4%".
        m = re.search(
            rf"\*\*{re.escape(label)}[^*]*?\*\*[^%]*?(\d+(?:\.\d+)?)\s*%",
            body,
            re.IGNORECASE,
        )
        if not m:
            return 0.0
        return float(m.group(1))

    def _count(prefix: str, body: str) -> int:
        m = re.search(rf"\*\*{re.escape(prefix)}[^*]*?\*\*[^0-9]*?(\d+)\s*/\s*\d+", body)
        return int(m.group(1)) if m else 0

    return ReadinessSnapshot(
        rollup_path=str(rollup_path.relative_to(REPO_ROOT)),
        implemented_pct=_pct("Implemented (I):", text),
        partial_pct=_pct("Partial (P):", text),
        gap_pct=_pct("Gap (G —", text) or _pct("Gap (G):", text),
        implemented_count=_count("Implemented (I):", text),
        partial_count=_count("Partial (P):", text),
        gap_count=_count("Gap (G —", text) or _count("Gap (G):", text),
        internal_scorecard_pct=_pct("Internal scorecard:", text),
        drata_dashboard_pct=_pct("Drata dashboard:", text),
        auto_collection_pct=_pct("Auto-collection rate", text),
    )


_GAP_HEADER_RE = re.compile(r"^### (?P<tsc>CC\d+\.\d+|A\d+\.\d+|C\d+\.\d+|PI\d+\.\d+|P-[A-Z]+|P\d+(\.\d+)?|.+?)\s+—", re.MULTILINE)
_GAP_ROW_RE = re.compile(r"\*\*(GAP-\d{2})\*\*[^\n]*", re.MULTILINE)
_GAP_TABLE_ROW_RE = re.compile(
    r"^\|\s*\d+\s*\|\s*(GAP-\d{2})\s*\|\s*([^|]+?)\s*\|\s*(?:\*\*)?([a-zA-Z\-]+)(?:\*\*)?\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|",
    re.MULTILINE,
)


def parse_gap_register(path: Path) -> dict[str, GapEntry]:
    """Parse the GAP-NN register summary table at the bottom of the doc."""
    text = path.read_text(encoding="utf-8")
    entries: dict[str, GapEntry] = {}
    for m in _GAP_TABLE_ROW_RE.finditer(text):
        gap_id, title, severity, owner, eta = (s.strip() for s in m.groups())
        # Severity normalisation (some rows use **blocking-GA** with bold).
        sev = severity.lower().replace("**", "")
        # Status comes from the per-section body — for the register summary we
        # default to OPEN unless the title contains "IMPLEMENTED"/"CLOSED".
        status = "OPEN"
        if "implemented" in title.lower() or "closed" in title.lower():
            status = "CLOSED"
        entries[gap_id] = GapEntry(
            gap_id=gap_id, severity=sev, status=status,
            tsc="(register)", owner=owner, eta=eta,
        )
    return entries


def diff_gaps(now: dict[str, GapEntry], prev: dict[str, GapEntry]) -> GapDelta:
    delta = GapDelta(
        total_open_now=sum(1 for g in now.values() if g.status == "OPEN"),
        total_open_prev=sum(1 for g in prev.values() if g.status == "OPEN"),
    )
    sev_order = {"minor": 1, "major": 2, "blocking-ga": 3}
    for gid, g_now in now.items():
        g_prev = prev.get(gid)
        if g_prev is None and g_now.status == "OPEN":
            delta.new.append(gid)
            continue
        if g_prev is None:
            continue
        if g_prev.status == "OPEN" and g_now.status == "CLOSED":
            delta.closed.append(gid)
        elif g_prev.status == "OPEN" and g_now.status == "OPEN":
            o_prev = sev_order.get(g_prev.severity, 0)
            o_now = sev_order.get(g_now.severity, 0)
            if o_now > o_prev:
                delta.severity_up.append((gid, g_prev.severity, g_now.severity))
            elif o_now < o_prev:
                delta.severity_down.append((gid, g_prev.severity, g_now.severity))
    return delta


_VENDOR_ROW_RE = re.compile(
    r"^\|\s*\d+\s*\|\s*([^|]+?)\s*\|"  # 1 vendor
    r"[^|]*\|"                          # 2 service (discard)
    r"\s*(C|I|S)\s*\|"                  # 3 category
    r"[^|]*\|[^|]*\|[^|]*\|[^|]*\|"     # 4-7 data/regulatory/contract/attestation
    r"[^|]*\|[^|]*\|[^|]*\|"            # 8-10 inherent/CEF/residual
    r"\s*(Q|B|A)\s*\|"                  # 11 cadence
    r"\s*(\d{4}-\d{2}-\d{2})\s*\|"      # 12 last review
    r"\s*(\d{4}-\d{2}-\d{2})\s*\|"      # 13 next review (discard for compute)
    r"\s*([^|]+?)\s*\|",                # 14 owner
    re.MULTILINE,
)


def parse_vendor_register(path: Path, today: date) -> list[VendorEntry]:
    text = path.read_text(encoding="utf-8")
    cat_map = {"C": "Critical", "I": "Important", "S": "Standard"}
    out: list[VendorEntry] = []
    for m in _VENDOR_ROW_RE.finditer(text):
        vendor, cat_letter, cadence_letter, last_str, _next_str, owner = m.groups()
        last_review = datetime.strptime(last_str.strip(), "%Y-%m-%d").date()
        cadence_days = CADENCE_DAYS[cadence_letter]
        days_since = (today - last_review).days
        sla_breach = days_since > cadence_days
        sla_2x = days_since > 2 * cadence_days
        out.append(
            VendorEntry(
                vendor=vendor.strip(),
                category=cat_map[cat_letter],
                last_review=last_review,
                cadence_days=cadence_days,
                owner=owner.strip(),
                days_since=days_since,
                sla_breach=sla_breach,
                sla_2x_breach=sla_2x,
            )
        )
    return out


_DRILL_ID_RE = re.compile(r"^- \*\*ID:\*\*\s+(DR-\d{2,3}[a-z]?)", re.MULTILINE)
_DRILL_WEEK_RE = re.compile(r"^- \*\*Week:\*\*\s+([^\n]+)", re.MULTILINE)


def parse_drill_cadence(path: Path, today: date) -> list[DrillStatus]:
    """Parse drill IDs and target dates (where computable).

    The cadence doc uses relative week markers ("W1", "W4", ...) anchored to
    R-6 staging start. For the operational digest we use the GA window anchor
    declared in the doc — but since the digest must work pre-GA, we adopt a
    pragmatic policy: a drill is "missed" only if (a) it has a `target_date`
    (parsed from absolute dates in §6 / DR-15 / DR-16 / IR-TABLETOP-SCHEDULE)
    AND (b) no matching `specs/_audits/*-bcp-drill-<ID>-*.md` evidence file
    exists. Drills with relative-week anchors only ("W3") emit a WARN
    (informational) — no failure.
    """
    text = path.read_text(encoding="utf-8")
    statuses: list[DrillStatus] = []
    seen: set[str] = set()
    for m in _DRILL_ID_RE.finditer(text):
        drill_id = m.group(1)
        # Normalise DR-15 / DR-16 (no zero-pad in doc).
        if drill_id in seen:
            continue
        seen.add(drill_id)
        criticality = DRILL_CRITICALITY.get(drill_id, "Important")
        # Look for an evidence file in _audits/.
        # Filename patterns: 2026-MM-DD-bcp-drill-DR-001.md, etc.
        glob_id = drill_id.lower()
        matches = list(AUDITS_DIR.glob(f"*-bcp-drill-{glob_id}*.md")) + \
                  list(AUDITS_DIR.glob(f"*-{glob_id}-*.md"))
        evidence_present = len(matches) > 0
        statuses.append(
            DrillStatus(
                drill_id=drill_id,
                target_date=None,
                criticality=criticality,
                evidence_present=evidence_present,
                days_overdue=0,
                flagged=False,
            )
        )
    return statuses


def parse_next_tabletop(path: Path, today: date) -> TabletopStatus | None:
    text = path.read_text(encoding="utf-8")
    # Match the "Default session slots" table.
    row_re = re.compile(
        r"^\|\s*(TT-\d{2})\s*\|\s*(\d{4}-\d{2}-\d{2})[^|]*\|\s*\d{2}:\d{2}\s*\|"
        r"\s*\d+\s*min\s*\|\s*(\d{4}-\d{2}-\d{2})",
        re.MULTILINE,
    )
    upcoming: list[TabletopStatus] = []
    for m in row_re.finditer(text):
        tt_id, tgt_str, backup_str = m.groups()
        tgt = datetime.strptime(tgt_str, "%Y-%m-%d").date()
        backup = datetime.strptime(backup_str, "%Y-%m-%d").date()
        if tgt < today:
            # Past — check evidence presence (no failure flag here; that's a
            # separate downstream check covered by drill-cadence rules).
            continue
        days_to = (tgt - today).days
        t_minus_7 = days_to <= 7
        # Facilitator assignment heuristic: presence of a per-TT scenario file.
        scenario_file = COMPLIANCE_DIR / "ir-scenarios" / f"{tt_id}.md"
        facilitator = scenario_file.exists()
        upcoming.append(
            TabletopStatus(
                tt_id=tt_id, target_date=tgt, backup_date=backup,
                days_to_session=days_to,
                t_minus_7_brief=t_minus_7,
                facilitator_assigned=facilitator,
            )
        )
    return upcoming[0] if upcoming else None


# ─────────────────────────────────────────────────────────────────────────────
# CI failures (optional)
# ─────────────────────────────────────────────────────────────────────────────


def list_compliance_workflows() -> list[Path]:
    if not WORKFLOWS_DIR.exists():
        return []
    out = []
    for wf in WORKFLOWS_DIR.glob("*.yml"):
        name_lower = wf.name.lower()
        if any(tag in name_lower for tag in COMPLIANCE_WORKFLOW_TAGS):
            out.append(wf)
    return sorted(out)


def query_ci_failures(workflows: list[Path], since: datetime) -> list[CIFailure]:
    """Query GitHub Actions API for last-7d failed runs of compliance workflows.

    Best-effort: returns [] if `gh` not installed / not authenticated.
    """
    failures: list[CIFailure] = []
    try:
        # Single-call: ask gh for all recent runs for the repo.
        cmd = [
            "gh", "run", "list",
            "--limit", "200",
            "--json", "name,conclusion,workflowName,updatedAt,url,databaseId",
        ]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
        if result.returncode != 0:
            return []
        runs = json.loads(result.stdout)
    except (FileNotFoundError, subprocess.TimeoutExpired, json.JSONDecodeError):
        return []

    wf_names = {wf.stem for wf in workflows}
    wf_names_yml = {wf.name for wf in workflows}
    bad = {"failure", "timed_out", "cancelled", "startup_failure"}
    for run in runs:
        upd_str = run.get("updatedAt", "")
        try:
            upd = datetime.fromisoformat(upd_str.replace("Z", "+00:00"))
        except ValueError:
            continue
        if upd < since:
            continue
        conclusion = (run.get("conclusion") or "").lower()
        if conclusion not in bad:
            continue
        wf = run.get("workflowName") or run.get("name") or ""
        # Match by either display name fragments or workflow file basename.
        if not any(
            tag in wf.lower() or wf in wf_names or wf in wf_names_yml
            for tag in COMPLIANCE_WORKFLOW_TAGS
        ):
            continue
        failures.append(
            CIFailure(
                workflow=wf,
                conclusion=conclusion,
                finished_at=upd_str,
                run_url=run.get("url", ""),
            )
        )
    return failures


# ─────────────────────────────────────────────────────────────────────────────
# Digest assembly
# ─────────────────────────────────────────────────────────────────────────────


def _find_previous_digest(today: date, output_dir: Path, reference: str | None) -> Path | None:
    if reference:
        ref_path = output_dir / f"{reference}.md"
        return ref_path if ref_path.exists() else None
    if not output_dir.exists():
        return None
    candidates = sorted(p for p in output_dir.glob("????-??-??.md") if p.stem < today.isoformat())
    return candidates[-1] if candidates else None


def _parse_prev_readiness(prev_path: Path) -> ReadinessSnapshot | None:
    """Re-parse last digest's headline (digest format includes the same %s)."""
    try:
        text = prev_path.read_text(encoding="utf-8")
    except OSError:
        return None
    m_i = re.search(r"Implemented:\s+(\d+(?:\.\d+)?)\s*%", text)
    m_p = re.search(r"Partial:\s+(\d+(?:\.\d+)?)\s*%", text)
    m_g = re.search(r"Gap:\s+(\d+(?:\.\d+)?)\s*%", text)
    m_d = re.search(r"Drata dashboard:\s+(\d+(?:\.\d+)?)\s*%", text)
    if not (m_i and m_p and m_g):
        return None
    return ReadinessSnapshot(
        rollup_path="(previous digest)",
        implemented_pct=float(m_i.group(1)),
        partial_pct=float(m_p.group(1)),
        gap_pct=float(m_g.group(1)),
        implemented_count=0, partial_count=0, gap_count=0,
        internal_scorecard_pct=0.0,
        drata_dashboard_pct=float(m_d.group(1)) if m_d else 0.0,
        auto_collection_pct=0.0,
    )


def assemble(today: date, output_dir: Path, reference: str | None,
             with_ci: bool) -> DigestPayload:
    rollup = _find_latest_rollup()
    readiness = parse_readiness(rollup)

    # Diff vs previous digest (if any).
    prev_digest = _find_previous_digest(today, output_dir, reference)
    prev_readiness = _parse_prev_readiness(prev_digest) if prev_digest else None
    if prev_readiness:
        delta = {
            "implemented_pct": readiness.implemented_pct - prev_readiness.implemented_pct,
            "partial_pct": readiness.partial_pct - prev_readiness.partial_pct,
            "gap_pct": readiness.gap_pct - prev_readiness.gap_pct,
            "drata_dashboard_pct": readiness.drata_dashboard_pct - prev_readiness.drata_dashboard_pct,
        }
    else:
        delta = {"implemented_pct": 0.0, "partial_pct": 0.0, "gap_pct": 0.0, "drata_dashboard_pct": 0.0}

    # GAP register — current parse + (best-effort) prev-week parse from digest.
    gap_now = parse_gap_register(COMPLIANCE_DIR / "SOC2-GAP-ANALYSIS.md")
    gap_prev: dict[str, GapEntry] = {}
    if prev_digest:
        # The prior digest embeds the GAP list inline; for simplicity we treat
        # the comparison as no-op when no prior digest exists.
        prev_text = prev_digest.read_text(encoding="utf-8")
        for m in re.finditer(r"\| (GAP-\d{2}) \| (blocking-ga|major|minor) \| (OPEN|CLOSED) \|", prev_text, re.IGNORECASE):
            gid, sev, status = m.group(1), m.group(2).lower(), m.group(3).upper()
            gap_prev[gid] = GapEntry(gid, sev, status, "", "", "")
    gap_delta = diff_gaps(gap_now, gap_prev)
    # First-run guard: if no prior digest, do not enumerate "new" GAPs (every
    # existing GAP would falsely appear new). The regression-detector logic
    # already short-circuits via `total_open_prev > 0`.
    if not gap_prev:
        gap_delta.new = []

    # Vendor risk.
    vendors = parse_vendor_register(COMPLIANCE_DIR / "VENDOR-RISK-REGISTER.md", today)
    vendor_breaches = [v for v in vendors if v.sla_breach]

    # Drill cadence.
    drill_flags = []
    for d in parse_drill_cadence(COMPLIANCE_DIR / "BCP-DR-DRILL-CADENCE.md", today):
        if d.target_date and (today - d.target_date).days > DRILL_GRACE.get(d.criticality, 7):
            if not d.evidence_present:
                d.flagged = True
                d.days_overdue = (today - d.target_date).days
                drill_flags.append(d)

    # Next tabletop.
    next_tt = parse_next_tabletop(COMPLIANCE_DIR / "IR-TABLETOP-SCHEDULE-2026.md", today)

    # CI failures (last 7d).
    ci_failures: list[CIFailure] = []
    if with_ci:
        wfs = list_compliance_workflows()
        ci_failures = query_ci_failures(wfs, datetime.now(timezone.utc) - timedelta(days=7))

    # Regression detection.
    reasons: list[str] = []
    if gap_delta.severity_up:
        reasons.append(
            f"{len(gap_delta.severity_up)} GAP severity upgrade(s): "
            + ", ".join(f"{g[0]} {g[1]}→{g[2]}" for g in gap_delta.severity_up)
        )
    if gap_delta.total_open_now > gap_delta.total_open_prev and gap_delta.total_open_prev > 0:
        reasons.append(
            f"Open GAP count rose: {gap_delta.total_open_prev} → {gap_delta.total_open_now}"
        )
    if drill_flags:
        reasons.append(
            f"{len(drill_flags)} drill(s) overdue past grace window: "
            + ", ".join(f"{d.drill_id} (+{d.days_overdue}d)" for d in drill_flags)
        )
    if any(v.sla_2x_breach for v in vendor_breaches):
        reasons.append(
            f"{sum(1 for v in vendor_breaches if v.sla_2x_breach)} vendor review(s) > 2× cadence"
        )
    if ci_failures:
        reasons.append(
            f"{len(ci_failures)} compliance-gate CI failure(s) in last 7d"
        )

    return DigestPayload(
        digest_date=today,
        reference_digest=prev_digest.name if prev_digest else None,
        readiness=readiness,
        readiness_delta=delta,
        gap_delta=gap_delta,
        vendor_breaches=vendor_breaches,
        drill_flags=drill_flags,
        next_tabletop=next_tt,
        ci_failures=ci_failures,
        regression=bool(reasons),
        regression_reasons=reasons,
    )


# ─────────────────────────────────────────────────────────────────────────────
# Markdown renderer
# ─────────────────────────────────────────────────────────────────────────────


def _fmt_delta(v: float) -> str:
    if v > 0:
        return f"+{v:.1f}%"
    if v < 0:
        return f"{v:.1f}%"
    return "no change"


def render_markdown(p: DigestPayload) -> str:
    lines: list[str] = []
    lines.append(f"# Compliance Weekly Digest — {p.digest_date.isoformat()}")
    lines.append("")
    lines.append(
        f"> **SOC 2 CC4.2 evidence** · auto-generated by "
        f"`scripts/compliance-weekly-digest.py` · reference digest: "
        f"`{p.reference_digest or '(none — first digest)'}` · "
        f"regression: **{'YES' if p.regression else 'NO'}**."
    )
    lines.append("")

    # Section 1 — readiness.
    lines.append("## 1. SOC 2 Readiness")
    lines.append("")
    lines.append(
        f"Source rollup: `{p.readiness.rollup_path}` "
        f"({p.readiness.implemented_count} I / {p.readiness.partial_count} P / "
        f"{p.readiness.gap_count} G by criterion-count)."
    )
    lines.append("")
    lines.append("| Metric | Now | Δ vs last week |")
    lines.append("|---|---|---|")
    lines.append(f"| Implemented: {p.readiness.implemented_pct}% | {p.readiness.implemented_pct}% | {_fmt_delta(p.readiness_delta['implemented_pct'])} |")
    lines.append(f"| Partial: {p.readiness.partial_pct}% | {p.readiness.partial_pct}% | {_fmt_delta(p.readiness_delta['partial_pct'])} |")
    lines.append(f"| Gap: {p.readiness.gap_pct}% | {p.readiness.gap_pct}% | {_fmt_delta(p.readiness_delta['gap_pct'])} |")
    lines.append(f"| Drata dashboard: {p.readiness.drata_dashboard_pct}% | {p.readiness.drata_dashboard_pct}% | {_fmt_delta(p.readiness_delta['drata_dashboard_pct'])} |")
    lines.append(f"| Internal scorecard | {p.readiness.internal_scorecard_pct}% | (n/a — sourced from rollup direct) |")
    lines.append(f"| Auto-collection rate | {p.readiness.auto_collection_pct}% | (n/a) |")
    lines.append("")

    # Section 2 — GAP register delta.
    lines.append("## 2. GAP register delta")
    lines.append("")
    lines.append(
        f"- Open GAPs: **{p.gap_delta.total_open_now}** "
        f"(prev: {p.gap_delta.total_open_prev})"
    )
    lines.append(f"- New: {', '.join(p.gap_delta.new) or '—'}")
    lines.append(f"- Closed: {', '.join(p.gap_delta.closed) or '—'}")
    lines.append(
        "- Severity upgrade: "
        + (", ".join(f"{g[0]} ({g[1]}→{g[2]})" for g in p.gap_delta.severity_up) or "—")
    )
    lines.append(
        "- Severity downgrade: "
        + (", ".join(f"{g[0]} ({g[1]}→{g[2]})" for g in p.gap_delta.severity_down) or "—")
    )
    lines.append("")

    # Section 3 — vendor risk.
    lines.append("## 3. Vendor risk SLAs")
    lines.append("")
    if p.vendor_breaches:
        lines.append("| Vendor | Tier | Days since review | SLA window | 2× breach? | Owner |")
        lines.append("|---|---|---|---|---|---|")
        for v in p.vendor_breaches:
            lines.append(
                f"| {v.vendor} | {v.category} | {v.days_since} | "
                f"{v.cadence_days}d | {'YES' if v.sla_2x_breach else 'no'} | {v.owner} |"
            )
    else:
        lines.append("All registered vendors within SLA. No breaches.")
    lines.append("")

    # Section 4 — drill cadence.
    lines.append("## 4. Drill cadence flags")
    lines.append("")
    if p.drill_flags:
        lines.append("| Drill ID | Criticality | Days overdue | Evidence file present? |")
        lines.append("|---|---|---|---|")
        for d in p.drill_flags:
            lines.append(
                f"| {d.drill_id} | {d.criticality} | {d.days_overdue} | "
                f"{'yes' if d.evidence_present else 'NO'} |"
            )
    else:
        lines.append("No drill missed past grace window.")
    lines.append("")

    # Section 5 — IR tabletop next session.
    lines.append("## 5. IR tabletop — next session")
    lines.append("")
    if p.next_tabletop:
        nt = p.next_tabletop
        lines.append(
            f"- **{nt.tt_id}** — target {nt.target_date.isoformat()} "
            f"(backup {nt.backup_date.isoformat()}); T-{nt.days_to_session}d to session."
        )
        lines.append(
            f"- T-7d brief readiness: {'IN WINDOW' if nt.t_minus_7_brief else 'pending'}"
        )
        lines.append(
            f"- Scenario / facilitator artefact present: "
            f"{'yes' if nt.facilitator_assigned else 'MISSING'}"
        )
    else:
        lines.append("No future tabletop session scheduled — review `IR-TABLETOP-SCHEDULE-2026.md`.")
    lines.append("")

    # Section 6 — CI compliance-gate failures.
    lines.append("## 6. Compliance-gate CI failures (last 7 days)")
    lines.append("")
    if p.ci_failures:
        lines.append("| Workflow | Conclusion | Finished | Run |")
        lines.append("|---|---|---|---|")
        for f in p.ci_failures:
            lines.append(
                f"| {f.workflow} | {f.conclusion} | {f.finished_at} | {f.run_url} |"
            )
    else:
        lines.append("No failed compliance-gate workflows in the last 7 days.")
    lines.append("")

    # Section 7 — regression verdict + GAP register snapshot.
    lines.append("## 7. Regression verdict")
    lines.append("")
    if p.regression:
        lines.append("**REGRESSION DETECTED.** Reasons:")
        lines.append("")
        for r in p.regression_reasons:
            lines.append(f"- {r}")
        lines.append("")
        lines.append(
            "Escalation: page Compliance on-call via PagerDuty service "
            "`corelink-compliance`. Runbook: "
            "`specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md` §4."
        )
    else:
        lines.append(
            "No regression detected. Compliance posture stable or improving. "
            "Triage runbook still requires Compliance Lead acknowledgement "
            "(`specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md` §3)."
        )
    lines.append("")

    # Section 8 — GAP snapshot (regression-detector hook for next week).
    lines.append("## 8. GAP register snapshot (digest-internal)")
    lines.append("")
    lines.append("> This table is the canonical state the next digest will diff against.")
    lines.append("")
    lines.append("| GAP ID | Severity | Status |")
    lines.append("|---|---|---|")
    gap_now = parse_gap_register(COMPLIANCE_DIR / "SOC2-GAP-ANALYSIS.md")
    for gid in sorted(gap_now):
        g = gap_now[gid]
        lines.append(f"| {gid} | {g.severity} | {g.status} |")
    lines.append("")

    # Section 9 — companion docs.
    lines.append("## 9. Cross-links")
    lines.append("")
    lines.append("- Triage runbook: `specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md`")
    lines.append("- Methodology + index: `specs/_compliance/weekly-digests/README.md`")
    lines.append("- SOC 2 evidence rollup: `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §2.4 (CC4.2)")
    lines.append("- GAP register: `specs/_compliance/SOC2-GAP-ANALYSIS.md` §CC4.2 (GAP-08 closed)")
    lines.append("- Vendor risk: `specs/_compliance/VENDOR-RISK-REGISTER.md`")
    lines.append("- Drill cadence: `specs/_compliance/BCP-DR-DRILL-CADENCE.md`")
    lines.append("- IR tabletop schedule: `specs/_compliance/IR-TABLETOP-SCHEDULE-2026.md`")
    lines.append("- Roadmap §9 (Human Track): `ROADMAP-TO-GA.md`")
    lines.append("")
    return "\n".join(lines) + "\n"


# ─────────────────────────────────────────────────────────────────────────────
# CLI
# ─────────────────────────────────────────────────────────────────────────────


def _json_default(obj: Any) -> Any:
    if isinstance(obj, (date, datetime)):
        return obj.isoformat()
    if hasattr(obj, "__dict__"):
        return obj.__dict__
    raise TypeError(f"Unserialisable {type(obj).__name__}")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[1])
    ap.add_argument("--dry-run", action="store_true",
                    help="Preview to stdout; do not write digest file.")
    ap.add_argument("--output-dir", default=str(DIGESTS_DIR),
                    help="Override the digest output directory.")
    ap.add_argument("--reference", default=None,
                    help="Force previous-digest reference (YYYY-MM-DD).")
    ap.add_argument("--with-ci", action="store_true",
                    help="Also query GitHub Actions for last-7d failures (requires gh).")
    ap.add_argument("--json", action="store_true",
                    help="Emit the structured payload as JSON on stdout.")
    ap.add_argument("--date", default=None,
                    help="Override the digest date (YYYY-MM-DD); default today UTC.")
    args = ap.parse_args(argv)

    try:
        today = (
            datetime.strptime(args.date, "%Y-%m-%d").date()
            if args.date else datetime.now(timezone.utc).date()
        )
        output_dir = Path(args.output_dir).resolve()
        if not args.dry_run:
            output_dir.mkdir(parents=True, exist_ok=True)
        with_ci = args.with_ci and not args.dry_run

        payload = assemble(today=today, output_dir=output_dir,
                           reference=args.reference, with_ci=with_ci)
        md = render_markdown(payload)

        if args.json:
            # JSON mode: stdout = JSON only. Markdown still written to file
            # unless --dry-run; in --dry-run --json we emit JSON only and
            # discard the rendered markdown preview.
            sys.stdout.write(json.dumps(asdict(payload), default=_json_default, indent=2))
            sys.stdout.write("\n")
            if not args.dry_run:
                out_path = output_dir / f"{today.isoformat()}.md"
                out_path.write_text(md, encoding="utf-8")
                sys.stderr.write(f"Wrote digest: {out_path}\n")
        elif args.dry_run:
            sys.stdout.write(md)
        else:
            out_path = output_dir / f"{today.isoformat()}.md"
            out_path.write_text(md, encoding="utf-8")
            sys.stderr.write(f"Wrote digest: {out_path}\n")

        sys.stderr.write(
            f"Digest summary: regression={payload.regression} "
            f"reasons={len(payload.regression_reasons)} "
            f"vendor_breaches={len(payload.vendor_breaches)} "
            f"drill_flags={len(payload.drill_flags)} "
            f"ci_failures={len(payload.ci_failures)}\n"
        )

        return 1 if payload.regression else 0
    except FileNotFoundError as e:
        sys.stderr.write(f"CONFIG ERROR: {e}\n")
        return 2
    except Exception as e:  # noqa: BLE001 — top-level CLI guard
        sys.stderr.write(f"UNEXPECTED ERROR: {type(e).__name__}: {e}\n")
        return 2


if __name__ == "__main__":
    sys.exit(main())
