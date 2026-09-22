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
        `specs/_audits/sealed/2026-05-15-canonical-consistency-baseline.md`. Diff
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
         (e) any vendor review past its cadence window (staleness is a
             regression; the digest remains the evidence and escalation path)
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
import sys
from dataclasses import asdict
from datetime import date, datetime, timezone
from pathlib import Path

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

# The digest's models, source readers, and renderer are concern-specific modules.
# Keep their names re-exported here so existing script/test imports remain stable.
from compliance_weekly_digest_model import *
from compliance_weekly_digest_sources import *
from compliance_weekly_digest_render import _fmt_delta, render_markdown

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
    if vendor_breaches:
        reasons.append(
            f"{len(vendor_breaches)} vendor review(s) overdue past cadence"
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
