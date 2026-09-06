"""Typed payload models for the compliance weekly digest."""
from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date
from pathlib import Path
from typing import Any

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
__all__ = [name for name in globals() if not name.startswith("__")]
