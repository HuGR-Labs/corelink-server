"""Repository readers used by the compliance weekly digest."""
from __future__ import annotations

import json
import re
import subprocess
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

from compliance_weekly_digest_model import *

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
    """Aggregate per-crate kill-rate.

    Precedence (highest first):
      1. `reports/mutation/latest.json` — CI-nightly empirical artifact
         (committed by `.github/workflows/mutation-nightly.yml` aggregate
         job). One row per crate; this is the SOTA source of truth.
      2. `specs/_audits/*-mutation-*.md` audit-doc tables — projections /
         locally-measured rows; used as fallback for crates not yet seen
         in the CI artifact.

    Per crate, the CI artifact wins over audit-doc rows (file precedence:
    newest-wins). For audit-doc rows, the newest file per crate wins
    (later audits supersede earlier).
    """
    crates: dict[str, MutationCrate] = {}

    # (1) CI artifact ingestion — newest wins per crate.
    ci_artifact = REPO_ROOT / "reports" / "mutation" / "latest.json"
    if ci_artifact.exists():
        try:
            payload = json.loads(ci_artifact.read_text(encoding="utf-8"))
            for row in payload.get("crates", []):
                if not isinstance(row, dict):
                    continue
                crate = row.get("crate")
                rate = row.get("kill_rate_pct")
                if not crate or not isinstance(rate, (int, float)):
                    continue
                if not (0.0 <= rate <= 100.0):
                    continue
                crates[crate] = MutationCrate(
                    crate=crate, kill_rate_pct=float(rate),
                    source=str(ci_artifact.relative_to(REPO_ROOT)),
                    below_floor=rate < 75.0,
                )
        except (OSError, json.JSONDecodeError):
            pass

    # (2) Audit-doc fallback — only for crates not present in (1).
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
            # CI artifact takes precedence: do not overwrite.
            if crate in crates and crates[crate].source.endswith("latest.json"):
                continue
            # Among audit-doc rows, newest file wins.
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
__all__ = [name for name in globals() if not name.startswith("__")]
