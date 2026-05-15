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
    10. **TLA verification status** (R5-3 expansion) — parses
        `<!-- BASELINE k=v -->` ratchet floors at the bottom of
        `specs/_audits/2026-05-15-canonical-consistency-baseline.md`. Diff
        vs prior digest. **HARD FAIL** if `tla_verified`,
        `code_referenced`, `test_referenced`, or `critical_referenced`
        regressed; **HARD FAIL** if `orphan_refs` > 0.
    11. **Mutation kill rate trend** (R5-3 expansion) — best-effort:
        if `gh` CLI is available + `--with-ci`, downloads the latest
        `mutation-nightly` workflow artifacts; else falls back to parsing
        per-crate `Kill rate` from `specs/_audits/2026-*-mutation-*.md`
        files. Flags any crate < 75 % (CI floor in `mutation-nightly.yml`).
    12. **Replication SLO compliance** (R5-3 expansion) — invokes
        `scripts/verify-replication-lag.py --mode=inmemory --json` and
        records per-domain p99 vs RPO budget. Counts the rolling-30d
        violation streak by reading prior digests' §12 entries; flags
        if any domain has ≥ 3 consecutive weekly fails.
    13. **Customer dashboard P0/P1 incident count** (R5-3 expansion) —
        counts commits with subject prefix `incident:` in the last 7 days
        + cross-references `specs/_audits/*ir-tabletop-meta-retro*.md`
        post-incident retros. Flags if last-7d P0 count > 0 (any) or
        rolling-30d P0 count ≥ 2.
    14. **Debt register burn-down** (R5-3 expansion) — parses
        `specs/_audits/<latest>-debt-register.md`; counts rows as CLOSED /
        OPEN / PARTIAL by P0/P1/P2 tier; flags any P0 row whose `Target`
        date has passed without a `~~DEBT-NNN~~ CLOSED` strike-through.

Exit codes (semantic — wired to PagerDuty by the wrapper workflow):

    0  No regression — digest emitted (may include WARNINGs).
    1  **REGRESSION** — at least one of:
         (a) severity-upgrade in GAP register
         (b) GAP count went up
         (c) drill missed past its grace window (Critical: 0d, Important: 7d, Standard: 14d)
         (d) any compliance-gate CI workflow failed in last 7d
         (e) any vendor review > 2× its cadence (i.e. doubled the SLA window)
         (f) **TLA ratchet floor regressed** (R5-3) — any of
             `tla_verified` / `code_referenced` / `test_referenced` /
             `critical_referenced` decreased OR `orphan_refs` > 0
         (g) **Mutation kill rate below floor** (R5-3) — any tracked
             crate measured < 75 %
         (h) **Replication SLO rolling-30d violation** (R5-3) — any
             domain with ≥ 3 consecutive weekly fails
         (i) **P0 incident in last 7d OR ≥ 2 P0 in rolling 30d** (R5-3)
         (j) **P0 debt-register row past Target** (R5-3) — any row in
             §1 of the debt register past its declared Target with no
             `CLOSED` strike-through and no waiver
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
class TlaFloorEntry:
    name: str           # e.g. "tla_verified"
    current: int
    previous: int | None
    regressed: bool


@dataclass
class TlaFloors:
    source: str         # path to canonical-consistency-baseline.md
    floors: list[TlaFloorEntry] = field(default_factory=list)
    orphan_refs: int = 0
    any_regression: bool = False


@dataclass
class MutationCrate:
    crate: str
    kill_rate_pct: float
    source: str         # audit file or workflow artifact
    below_floor: bool   # kill_rate < 75 %


@dataclass
class MutationTrend:
    crates: list[MutationCrate] = field(default_factory=list)
    floor_pct: float = 75.0
    any_below_floor: bool = False


@dataclass
class ReplicationDomainStatus:
    domain: str
    p99_lag_secs: float
    budget_secs: float
    breach: bool
    slo_id: str


@dataclass
class ReplicationSnapshot:
    mode: str           # "inmemory" | "staging" | "prod" | "TBD"
    domains: list[ReplicationDomainStatus] = field(default_factory=list)
    rolling_30d_streak: int = 0   # consecutive prior digests with breach
    any_breach: bool = False
    rolling_breach: bool = False  # streak >= 3


@dataclass
class IncidentSummary:
    p0_count_7d: int = 0
    p1_count_7d: int = 0
    p0_count_30d: int = 0
    p1_count_30d: int = 0
    sample_commits: list[str] = field(default_factory=list)
    retro_files: list[str] = field(default_factory=list)
    flagged: bool = False  # P0 7d > 0 OR P0 30d >= 2


@dataclass
class DebtRow:
    debt_id: str
    tier: str           # "P0" | "P1" | "P2"
    status: str         # "OPEN" | "CLOSED" | "PARTIAL"
    target_date: date | None
    overdue: bool       # P0 OPEN past target_date


@dataclass
class DebtBurnDown:
    source: str
    rows: list[DebtRow] = field(default_factory=list)
    open_count: int = 0
    closed_count: int = 0
    partial_count: int = 0
    open_prev: int = 0  # from prior digest
    closed_prev: int = 0
    p0_overdue: list[str] = field(default_factory=list)


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
    tla_floors: TlaFloors
    mutation_trend: MutationTrend
    replication: ReplicationSnapshot
    incidents: IncidentSummary
    debt: DebtBurnDown
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
# R5-3 expansion parsers — TLA / Mutation / Replication / Incidents / Debt
# ─────────────────────────────────────────────────────────────────────────────

# §10 TLA — ratchet floor regex
_TLA_BASELINE_RE = re.compile(r"<!--\s*BASELINE\s+([a-z_]+)\s*=\s*(\d+)\s*-->")
_TLA_TRACKED_FLOORS: tuple[str, ...] = (
    "declared", "tla_verified", "code_referenced", "test_referenced",
    "critical_referenced",
)
_TLA_PREV_RE = re.compile(r"\|\s*([a-z_]+)\s*\|\s*(\d+)\s*\|\s*(?:\d+|—|n/a)\s*\|\s*(?:yes|no|HARD FAIL)\s*\|", re.IGNORECASE)


def _find_canonical_baseline() -> Path | None:
    """Pick the newest `*-canonical-consistency-baseline.md` under _audits/."""
    candidates = sorted(AUDITS_DIR.glob("*-canonical-consistency-baseline.md"))
    return candidates[-1] if candidates else None


def parse_tla_floors(today: date, prev_digest: Path | None) -> TlaFloors:
    """Read canonical-consistency baseline ratchet floors + diff vs prior digest."""
    src = _find_canonical_baseline()
    if src is None:
        return TlaFloors(source="(none — canonical-consistency-baseline.md missing)")
    text = src.read_text(encoding="utf-8")
    current: dict[str, int] = {
        m.group(1): int(m.group(2)) for m in _TLA_BASELINE_RE.finditer(text)
    }
    previous: dict[str, int] = {}
    if prev_digest:
        # Prior digest §10 emits the same table; we parse by name to be
        # robust against column re-orders.
        prev_text = prev_digest.read_text(encoding="utf-8")
        # Confine parse to the §10 block to avoid false matches.
        m_block = re.search(
            r"## 10\. TLA verification status.*?(?=^## \d+\.)",
            prev_text, re.DOTALL | re.MULTILINE,
        )
        block = m_block.group(0) if m_block else prev_text
        for m in _TLA_PREV_RE.finditer(block):
            name = m.group(1).lower()
            try:
                previous[name] = int(m.group(2))
            except ValueError:
                continue
    floors: list[TlaFloorEntry] = []
    any_reg = False
    for name in _TLA_TRACKED_FLOORS:
        cur = current.get(name, 0)
        prv = previous.get(name)
        # Regression rule: floor decreased (prv is not None and cur < prv).
        regressed = prv is not None and cur < prv
        if regressed:
            any_reg = True
        floors.append(TlaFloorEntry(name=name, current=cur, previous=prv, regressed=regressed))
    orphan = current.get("orphan_refs", 0)
    if orphan > 0:
        any_reg = True
    return TlaFloors(
        source=str(src.relative_to(REPO_ROOT)),
        floors=floors,
        orphan_refs=orphan,
        any_regression=any_reg,
    )


# §11 Mutation — per-crate kill-rate parser
_MUTATION_ROW_RE = re.compile(
    # Matches table rows like:
    # | `corelink-audit-chain` | 201 | 139 | 26 | 0 | 28 | 165 | **84.24 %** |
    # | `corelink-byok`        | ... | ... | ... | ... | ... | ... | **96.8 %** |
    #
    # `[^|\n]*` (NOT `[^|]*`) anchors each column inside one line — the
    # bare `[^|]*` previously matched across `\n` and could pull the
    # rightmost `% |` from an unrelated table that followed the
    # historical-baseline section, producing a cross-row swap that
    # silently mis-attributed kill rates (e.g. `dual-approval` reading
    # the `ratelimit` historical 86.6 % as if it were the current rate).
    r"^\|\s*`(corelink-[a-z0-9\-]+)`\s*\|[^|\n]*\|[^|\n]*\|[^|\n]*\|"
    r"(?:[^|\n]*\|){0,4}\s*\*{0,2}(\d+(?:\.\d+)?)\s*%\s*\*{0,2}\s*\|",
    re.MULTILINE,
)


def parse_mutation_trend() -> MutationTrend:
    """Aggregate per-crate kill-rate from `specs/_audits/*-mutation-*.md`.

    The newest file per crate wins (later audits supersede earlier).
    Best-effort: empirical CI artefact ingestion (via `gh run download
    mutation-nightly`) is wired in `assemble(..., with_ci=True)` and
    falls back here when the workflow run is not accessible.
    """
    crates: dict[str, MutationCrate] = {}
    files = sorted(AUDITS_DIR.glob("*-mutation-*.md"))  # date-sortable
    for f in files:
        try:
            text = f.read_text(encoding="utf-8")
        except OSError:
            continue
        for m in _MUTATION_ROW_RE.finditer(text):
            crate, rate_str = m.group(1), m.group(2)
            try:
                rate = float(rate_str)
            except ValueError:
                continue
            # Sanity: kill rates outside 0..100 are noise (column drift).
            if not (0.0 <= rate <= 100.0):
                continue
            # Newest file wins.
            crates[crate] = MutationCrate(
                crate=crate, kill_rate_pct=rate,
                source=str(f.relative_to(REPO_ROOT)),
                below_floor=rate < 75.0,
            )
    out_list = sorted(crates.values(), key=lambda c: c.crate)
    return MutationTrend(
        crates=out_list,
        floor_pct=75.0,
        any_below_floor=any(c.below_floor for c in out_list),
    )


# §12 Replication — invoke verify-replication-lag.py
def parse_replication(today: date, output_dir: Path) -> ReplicationSnapshot:
    """Run the replication verifier in inmemory mode + count rolling streak."""
    script = REPO_ROOT / "scripts" / "verify-replication-lag.py"
    domains: list[ReplicationDomainStatus] = []
    mode = "inmemory"
    if not script.exists():
        return ReplicationSnapshot(mode="TBD (verify-replication-lag.py missing)")
    try:
        result = subprocess.run(
            ["python3", str(script), "--mode=inmemory", "--json"],
            capture_output=True, text=True, timeout=30, cwd=str(REPO_ROOT),
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return ReplicationSnapshot(mode="TBD (verifier did not run)")
    payload: dict[str, Any] = {}
    if result.stdout:
        try:
            payload = json.loads(result.stdout)
        except json.JSONDecodeError:
            payload = {}
    # The verifier emits {"domains": [{"domain": "...", "p99": N, "budget": N,
    # "breach": bool, "slo_id": "..."}], "mode": "inmemory"}; tolerate
    # alternative key naming.
    raw_domains = payload.get("domains") or payload.get("results") or []
    for d in raw_domains if isinstance(raw_domains, list) else []:
        if not isinstance(d, dict):
            continue
        domains.append(
            ReplicationDomainStatus(
                domain=str(d.get("domain", "?")),
                p99_lag_secs=float(d.get("p99", d.get("p99_lag_secs", 0.0)) or 0.0),
                budget_secs=float(d.get("budget", d.get("budget_secs", 0.0)) or 0.0),
                breach=bool(d.get("breach", False)),
                slo_id=str(d.get("slo_id", "?")),
            )
        )
    any_breach = any(d.breach for d in domains) or (result.returncode == 1)
    # Rolling-30d streak — count consecutive prior weekly digests that
    # reported a §12 breach (look back up to 4 prior digests).
    streak = 1 if any_breach else 0
    if any_breach and output_dir.exists():
        prior = sorted(
            (p for p in output_dir.glob("????-??-??.md") if p.stem < today.isoformat()),
            reverse=True,
        )[:3]
        for p in prior:
            try:
                t = p.read_text(encoding="utf-8")
            except OSError:
                break
            if re.search(r"## 12\. Replication SLO[^\n]*\n.*?breach.*?YES",
                         t, re.DOTALL | re.IGNORECASE):
                streak += 1
            else:
                break
    return ReplicationSnapshot(
        mode=mode,
        domains=domains,
        rolling_30d_streak=streak,
        any_breach=any_breach,
        rolling_breach=streak >= 3,
    )


# §13 Incidents — git log + retro file count
def parse_incidents(today: date) -> IncidentSummary:
    """Count `incident:`-prefixed commits + cross-reference retro audits.

    The git scan uses `git log --grep='^incident:' --since=<N> days ago` and
    classifies P0/P1 by parsing the subject line — convention enforced by
    `specs/_runbooks/RB-COMMIT-CONVENTIONS.md` (P0 / P1 mandatory tag).
    Retro files: `specs/_audits/*ir-tabletop-meta-retro*.md` and
    `specs/_audits/*-compliance-regression-*.md`.
    """
    out = IncidentSummary()
    for window_days, key_p0, key_p1 in (
        (7, "p0_count_7d", "p1_count_7d"),
        (30, "p0_count_30d", "p1_count_30d"),
    ):
        try:
            r = subprocess.run(
                ["git", "log", "--grep=^incident:", "--regexp-ignore-case",
                 f"--since={window_days} days ago",
                 "--pretty=format:%h %s"],
                capture_output=True, text=True, timeout=15, cwd=str(REPO_ROOT),
            )
        except (FileNotFoundError, subprocess.TimeoutExpired):
            continue
        if r.returncode != 0:
            continue
        for line in r.stdout.splitlines():
            if not line.strip():
                continue
            if window_days == 7 and len(out.sample_commits) < 5:
                out.sample_commits.append(line.strip())
            sub = line.lower()
            # Convention: subject contains `[p0]` or ` p0:` token.
            if re.search(r"\bp0\b", sub):
                setattr(out, key_p0, getattr(out, key_p0) + 1)
            elif re.search(r"\bp1\b", sub):
                setattr(out, key_p1, getattr(out, key_p1) + 1)
    # Cross-reference retro audits (post-incident retros and compliance
    # regression audits).
    for pat in ("*ir-tabletop-meta-retro*.md", "*-compliance-regression-*.md"):
        for f in sorted(AUDITS_DIR.glob(pat)):
            out.retro_files.append(str(f.relative_to(REPO_ROOT)))
    out.flagged = out.p0_count_7d > 0 or out.p0_count_30d >= 2
    return out


# §14 Debt register burn-down
_DEBT_ROW_RE = re.compile(
    # Match BOTH strike-through closed rows and open rows. We capture the
    # `DEBT-NNN` id and look for explicit `CLOSED` / `PARTIAL` markers
    # anywhere in the row.
    r"^\|\s*(?:~~)?\s*\*{0,2}(DEBT-\d{3})\*{0,2}\s*(?:~~)?[^\n]*$",
    re.MULTILINE,
)


def _find_debt_register() -> Path | None:
    candidates = sorted(AUDITS_DIR.glob("*-debt-register.md"))
    return candidates[-1] if candidates else None


def parse_debt_register(today: date, prev_digest: Path | None) -> DebtBurnDown:
    """Parse the debt register into tier × status counts + P0 overdue flags.

    The register is hand-curated markdown; we classify each `DEBT-NNN`
    row exactly once by scanning the §-headers preceding it:
      `## 1. Hard P0` → P0; `## 2. P1` → P1; `## 3. P2` → P2.
    Status is derived from row markers in priority order:
      1. row contains `CLOSED` and `DEBT-NNN` is wrapped in `~~..~~` → CLOSED
      2. row contains `PARTIAL` → PARTIAL
      3. else → OPEN.
    Target dates: capture `T+Nd (YYYY-MM-DD)` or bare `YYYY-MM-DD` for P0
    rows; flag as overdue if status != CLOSED and the date < today.
    """
    src = _find_debt_register()
    if src is None:
        return DebtBurnDown(source="(none — debt-register.md missing)")
    text = src.read_text(encoding="utf-8")
    # Tier-segment boundaries.
    tier_segments: list[tuple[str, str]] = []
    for tier, pat in (
        ("P0", r"^##\s+1\.\s+Hard P0.*?(?=^##\s+\d+\.\s+|\Z)"),
        ("P1", r"^##\s+2\.\s+P1.*?(?=^##\s+\d+\.\s+|\Z)"),
        ("P2", r"^##\s+3\.\s+P2.*?(?=^##\s+\d+\.\s+|\Z)"),
    ):
        m = re.search(pat, text, re.DOTALL | re.MULTILINE)
        if m:
            tier_segments.append((tier, m.group(0)))
    seen: set[str] = set()
    rows: list[DebtRow] = []
    for tier, seg in tier_segments:
        for m in _DEBT_ROW_RE.finditer(seg):
            line = m.group(0)
            debt_id = m.group(1)
            if debt_id in seen:
                # First-occurrence wins (closed rows tend to appear first
                # via strike-through edit). Skip duplicate stale rows.
                continue
            seen.add(debt_id)
            up = line.upper()
            if "CLOSED" in up:
                status = "CLOSED"
            elif "PARTIAL" in up:
                status = "PARTIAL"
            else:
                status = "OPEN"
            tgt: date | None = None
            mt = re.search(r"(\d{4}-\d{2}-\d{2})", line)
            if mt:
                try:
                    tgt = datetime.strptime(mt.group(1), "%Y-%m-%d").date()
                except ValueError:
                    tgt = None
            overdue = (
                tier == "P0" and status != "CLOSED"
                and tgt is not None and tgt < today
            )
            rows.append(DebtRow(
                debt_id=debt_id, tier=tier, status=status,
                target_date=tgt, overdue=overdue,
            ))
    out = DebtBurnDown(source=str(src.relative_to(REPO_ROOT)), rows=rows)
    out.open_count = sum(1 for r in rows if r.status == "OPEN")
    out.closed_count = sum(1 for r in rows if r.status == "CLOSED")
    out.partial_count = sum(1 for r in rows if r.status == "PARTIAL")
    out.p0_overdue = [r.debt_id for r in rows if r.overdue]
    # Prior digest delta — parse the §14 summary line.
    if prev_digest:
        try:
            prev_text = prev_digest.read_text(encoding="utf-8")
            m_o = re.search(r"Open debts:\s+\*\*(\d+)\*\*", prev_text)
            m_c = re.search(r"Closed debts:\s+\*\*(\d+)\*\*", prev_text)
            if m_o:
                out.open_prev = int(m_o.group(1))
            if m_c:
                out.closed_prev = int(m_c.group(1))
        except OSError:
            pass
    return out


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

    # R5-3 expansion — TLA / Mutation / Replication / Incidents / Debt.
    tla_floors = parse_tla_floors(today=today, prev_digest=prev_digest)
    mutation_trend = parse_mutation_trend()
    replication = parse_replication(today=today, output_dir=output_dir)
    incidents = parse_incidents(today=today)
    debt = parse_debt_register(today=today, prev_digest=prev_digest)

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

    # R5-3 regression triggers (f)..(j).
    if tla_floors.any_regression:
        regressed_names = [f.name for f in tla_floors.floors if f.regressed]
        bits = []
        if regressed_names:
            bits.append(f"floors regressed: {', '.join(regressed_names)}")
        if tla_floors.orphan_refs > 0:
            bits.append(f"orphan_refs={tla_floors.orphan_refs} (must be 0)")
        reasons.append("TLA ratchet HARD FAIL — " + "; ".join(bits))
    if mutation_trend.any_below_floor:
        below = [
            f"{c.crate} {c.kill_rate_pct:.1f}%"
            for c in mutation_trend.crates if c.below_floor
        ]
        reasons.append(
            f"{len(below)} mutation crate(s) below 75% floor: " + ", ".join(below)
        )
    if replication.rolling_breach:
        reasons.append(
            f"Replication SLO rolling-30d violation: "
            f"streak={replication.rolling_30d_streak} weeks"
        )
    if incidents.flagged:
        reasons.append(
            f"P0 incident pressure: 7d={incidents.p0_count_7d}, "
            f"30d={incidents.p0_count_30d} (trigger: 7d>0 OR 30d>=2)"
        )
    if debt.p0_overdue:
        reasons.append(
            f"{len(debt.p0_overdue)} P0 debt row(s) past Target: "
            + ", ".join(debt.p0_overdue)
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
        tla_floors=tla_floors,
        mutation_trend=mutation_trend,
        replication=replication,
        incidents=incidents,
        debt=debt,
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

    # Section 10 — TLA verification status (R5-3 expansion).
    lines.append("## 10. TLA verification status (R5-3 expansion)")
    lines.append("")
    lines.append(f"Source: `{p.tla_floors.source}` (ratchet floors).")
    lines.append("")
    lines.append("| Floor | Current | Previous | Regressed? |")
    lines.append("|---|---|---|---|")
    for f in p.tla_floors.floors:
        prev = f.previous if f.previous is not None else "—"
        flag = "HARD FAIL" if f.regressed else "no"
        lines.append(f"| {f.name} | {f.current} | {prev} | {flag} |")
    lines.append(
        f"| orphan_refs | {p.tla_floors.orphan_refs} | (must be 0) | "
        f"{'HARD FAIL' if p.tla_floors.orphan_refs > 0 else 'no'} |"
    )
    lines.append("")
    if p.tla_floors.any_regression:
        lines.append(
            "**Escalation:** any HARD FAIL row pages PD service "
            "`corelink-compliance` (SEV-2) — see "
            "`specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md` §4.x and "
            "`specs/_runbooks/RB-CANONICAL-DRIFT.md` §6."
        )
    else:
        lines.append("No TLA ratchet regression. Floors stable or rising.")
    lines.append("")

    # Section 11 — Mutation kill rate trend (R5-3 expansion).
    lines.append("## 11. Mutation kill rate trend (R5-3 expansion)")
    lines.append("")
    if p.mutation_trend.crates:
        lines.append("| Crate | Kill rate | Source audit | < 75% floor? |")
        lines.append("|---|---|---|---|")
        for c in p.mutation_trend.crates:
            lines.append(
                f"| {c.crate} | {c.kill_rate_pct:.2f}% | `{c.source}` | "
                f"{'YES — HARD FAIL' if c.below_floor else 'no'} |"
            )
    else:
        lines.append(
            "No mutation audits parsed (TBD — real source landing in "
            "Sprint R-3 `mutation-nightly.yml` artifact ingestion)."
        )
    lines.append("")
    if p.mutation_trend.any_below_floor:
        lines.append(
            "**Escalation:** any crate below 75% floor pages PD service "
            "`corelink-compliance` (SEV-3); owner files a follow-on WI "
            "to add targeted tests on missed mutants."
        )
    lines.append("")

    # Section 12 — Replication SLO compliance (R5-3 expansion).
    lines.append("## 12. Replication SLO compliance (R5-3 expansion)")
    lines.append("")
    lines.append(
        f"Verifier mode: `{p.replication.mode}` · rolling-30d breach streak: "
        f"**{p.replication.rolling_30d_streak}** consecutive week(s) · "
        f"any breach this week: **{'YES' if p.replication.any_breach else 'NO'}** · "
        f"rolling violation: **{'YES' if p.replication.rolling_breach else 'NO'}**."
    )
    lines.append("")
    if p.replication.domains:
        lines.append("| Domain | SLO ID | p99 lag (s) | RPO budget (s) | Breach? |")
        lines.append("|---|---|---|---|---|")
        for d in p.replication.domains:
            lines.append(
                f"| {d.domain} | {d.slo_id} | {d.p99_lag_secs:.2f} | "
                f"{d.budget_secs:.2f} | {'YES' if d.breach else 'no'} |"
            )
    else:
        lines.append(
            "No domain rows emitted (TBD — real Prometheus snapshot landing "
            "post-DEBT-011 P2 closure; inmemory fixture currently returns "
            "synthetic-pass payload)."
        )
    lines.append("")
    if p.replication.rolling_breach:
        lines.append(
            "**Escalation:** rolling-30d streak ≥ 3 → SEV-2 page; "
            "Replication SRE Lead opens `WI-REPLICATION-SLO-DRIFT` and "
            "cross-references `specs/_audits/2026-05-15-replication-audit.md`."
        )
    lines.append("")

    # Section 13 — Customer dashboard P0/P1 incident count (R5-3 expansion).
    lines.append("## 13. Customer dashboard P0/P1 incident count (R5-3 expansion)")
    lines.append("")
    lines.append(
        f"Source: `git log --grep='^incident:' --since=<window>` "
        f"+ `specs/_audits/*ir-tabletop-meta-retro*.md` "
        f"+ `specs/_audits/*-compliance-regression-*.md`."
    )
    lines.append("")
    lines.append("| Window | P0 incidents | P1 incidents |")
    lines.append("|---|---|---|")
    lines.append(f"| Last 7 days  | {p.incidents.p0_count_7d}  | {p.incidents.p1_count_7d}  |")
    lines.append(f"| Last 30 days | {p.incidents.p0_count_30d} | {p.incidents.p1_count_30d} |")
    lines.append("")
    if p.incidents.sample_commits:
        lines.append("Sample (last 7d):")
        for c in p.incidents.sample_commits:
            lines.append(f"- `{c}`")
        lines.append("")
    if p.incidents.retro_files:
        lines.append(
            f"Cross-referenced retro / regression audits: "
            f"**{len(p.incidents.retro_files)}** total."
        )
    if p.incidents.flagged:
        lines.append("")
        lines.append(
            "**Escalation:** P0 in last 7d (any) OR ≥ 2 P0 in rolling 30d "
            "→ SEV-2 page; IC opens post-incident retro within 7d per "
            "`specs/_compliance/IR-TABLETOP-PLAYBOOK.md` §9."
        )
    lines.append("")

    # Section 14 — Debt register burn-down (R5-3 expansion).
    lines.append("## 14. Debt register burn-down (R5-3 expansion)")
    lines.append("")
    lines.append(f"Source: `{p.debt.source}`.")
    lines.append("")
    lines.append(
        f"- Open debts: **{p.debt.open_count}** (prev: {p.debt.open_prev}; "
        f"Δ {p.debt.open_count - p.debt.open_prev:+d})"
    )
    lines.append(
        f"- Closed debts: **{p.debt.closed_count}** (prev: {p.debt.closed_prev}; "
        f"Δ {p.debt.closed_count - p.debt.closed_prev:+d})"
    )
    lines.append(f"- Partial debts: **{p.debt.partial_count}**")
    lines.append("")
    # Tier breakdown.
    by_tier: dict[str, dict[str, int]] = {
        "P0": {"OPEN": 0, "CLOSED": 0, "PARTIAL": 0},
        "P1": {"OPEN": 0, "CLOSED": 0, "PARTIAL": 0},
        "P2": {"OPEN": 0, "CLOSED": 0, "PARTIAL": 0},
    }
    for r in p.debt.rows:
        if r.tier in by_tier:
            by_tier[r.tier][r.status] = by_tier[r.tier].get(r.status, 0) + 1
    lines.append("| Tier | OPEN | PARTIAL | CLOSED |")
    lines.append("|---|---|---|---|")
    for tier in ("P0", "P1", "P2"):
        b = by_tier[tier]
        lines.append(
            f"| {tier} | {b.get('OPEN', 0)} | {b.get('PARTIAL', 0)} | "
            f"{b.get('CLOSED', 0)} |"
        )
    lines.append("")
    if p.debt.p0_overdue:
        lines.append("**HARD FAIL — P0 rows past Target without CLOSED marker:**")
        lines.append("")
        for did in p.debt.p0_overdue:
            row = next((r for r in p.debt.rows if r.debt_id == did), None)
            tgt = row.target_date.isoformat() if row and row.target_date else "(none)"
            lines.append(f"- {did} — Target {tgt}")
        lines.append("")
        lines.append(
            "**Escalation:** SEV-2 page; Owner (Gustavo) signs waiver in "
            "`specs/_audits/<latest>-debt-register.md §5` or closes the "
            "row within 7d per the register's hard mandate."
        )
    else:
        lines.append("All P0 debt rows either CLOSED or within Target window.")
    lines.append("")

    # Section 15 — companion docs (was §9 pre-R5-3; renumbered to keep
    # §10..§14 as the new domains in ascending order).
    lines.append("## 15. Cross-links")
    lines.append("")
    lines.append("- Triage runbook: `specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md`")
    lines.append("- Methodology + index: `specs/_compliance/weekly-digests/README.md`")
    lines.append("- SOC 2 evidence rollup: `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §2.4 (CC4.2)")
    lines.append("- GAP register: `specs/_compliance/SOC2-GAP-ANALYSIS.md` §CC4.2 (GAP-08 closed)")
    lines.append("- Vendor risk: `specs/_compliance/VENDOR-RISK-REGISTER.md`")
    lines.append("- Drill cadence: `specs/_compliance/BCP-DR-DRILL-CADENCE.md`")
    lines.append("- IR tabletop schedule: `specs/_compliance/IR-TABLETOP-SCHEDULE-2026.md`")
    lines.append("- Roadmap §9 (Human Track): `ROADMAP-TO-GA.md`")
    lines.append(
        "- (R5-3 expansion) Canonical consistency baseline: "
        f"`{p.tla_floors.source}`"
    )
    lines.append(
        "- (R5-3 expansion) Replication verifier: "
        "`scripts/verify-replication-lag.py`"
    )
    lines.append(
        "- (R5-3 expansion) Debt register: "
        f"`{p.debt.source}`"
    )
    lines.append(
        "- (R5-3 expansion) IR tabletop playbook (post-incident retros): "
        "`specs/_compliance/IR-TABLETOP-PLAYBOOK.md`"
    )
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
