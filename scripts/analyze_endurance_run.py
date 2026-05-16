#!/usr/bin/env python3
"""analyze_endurance_run.py — wave-22 endurance run analyser.

Ingests the artefacts produced by ``scripts/run_24h_endurance.sh`` and
emits a Markdown summary whose top line is the **greenlight verdict**
keyed to RB-GA-CUTOVER §4 G1–G6:

  G1  P99 latency per customer-facing route within drift floor.
  G2  Audit-chain integrity counter unchanged across the run.
  G3  Zero SEV-0/SEV-1 incidents recorded in run-meta.
  G4  Pilot tenant ack column present (manual; flagged for operator).
  G5  Neon shadow lag observation present (manual snapshot — script
       degrades to ``UNKNOWN`` if not provided).
  G6  DSR cron success rate per run-meta dsr_24h_success_pct.

Verdict mapping:
  - All G1–G6 GREEN  -> ``GREENLIGHT``.
  - Any G UNKNOWN    -> ``MANUAL-REVIEW`` (signals operator to fill gaps).
  - Any G RED        -> ``BLOCK``.

Inputs (read from ``--run-dir``):
  - run-meta.json           (mode/duration/target/operator + optional manual gates)
  - k6-summary.json         (k6 ``--summary-export`` output)
  - smoke-status.txt        (smoke-only short-circuit, if present)
  - k6-stdout.log           (free-form; we only parse threshold lines)

Output: ``--out`` markdown path (default: ``<run-dir>/analysis.md``).

This script intentionally has zero third-party dependencies so it runs
in CI containers and on operator laptops without provisioning.
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROUTE_FLOORS_MS_P99 = {
    "GET /v1/audit/analytics/event-count": 400,
    "GET /v1/audit/analytics/timeline": 600,
    "POST /v1/audit/export": 1500,
    "POST /v1/cas/upload": 800,
    "POST /v1/dsr/erasure": 1000,
    "POST /v1/clerk/auth": 300,
}


def _read_json(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None


def _route_metric(summary: dict[str, Any], route: str) -> dict[str, Any] | None:
    """Pull the per-route latency Trend out of a k6 summary export.

    k6 summary-export buckets metrics either under ``metrics`` (key is the
    metric name) or under ``root_group`` (per-scenario). We support both
    shapes and degrade gracefully if neither matches.
    """
    metrics = summary.get("metrics", {}) if summary else {}
    # k6 emits sub-metrics keyed as ``route_latency{route:...}``.
    sub_key = f"route_latency{{route:{route}}}"
    sub = metrics.get(sub_key)
    if isinstance(sub, dict):
        return sub
    # Fallback: walk every key looking for a route_latency variant.
    for k, v in metrics.items():
        if k.startswith("route_latency{") and route in k:
            return v
    return None


def _extract_p99(metric: dict[str, Any], floor_ms: float) -> tuple[float | None, bool | None]:
    """Return ``(p99_value, threshold_pass)``.

    k6 ``--summary-export`` does NOT include a numeric ``p(99)`` field on
    Trend metrics by default; only ``avg/min/med/max/p(90)/p(95)`` are
    serialised. The p99 value is enforced via a ``thresholds`` entry
    (e.g. ``p(99)<400``) whose boolean result IS present on the metric.

    We therefore look in this order:
      1) explicit numeric ``p(99)`` / ``p99`` field (rare),
      2) the ``thresholds`` map for any ``p(99)<NNN`` expression,
      3) ``None`` if neither is present.
    """
    p99 = metric.get("p(99)") or metric.get("p99")
    if p99 is not None:
        try:
            v = float(p99)
        except (TypeError, ValueError):
            v = None
        if v is not None:
            return (v, v <= float(floor_ms))
    thresholds = metric.get("thresholds") or {}
    if isinstance(thresholds, dict):
        for expr, result in thresholds.items():
            if "p(99)" in expr or "p99" in expr:
                # result is bool in summary-export. True == threshold met.
                if isinstance(result, dict):
                    ok = bool(result.get("ok"))
                else:
                    ok = bool(result)
                return (None, ok)
    return (None, None)


def _stdout_threshold_breach_routes(stdout_log: Path) -> set[str]:
    """Parse ``k6-stdout.log`` for the final ``thresholds … have been
    crossed`` line. Returns the set of route metric keys that k6
    reported as breached. Empty set if no breach line is present.

    This is the source of truth for k6 threshold pass/fail in
    ``--summary-export`` because the per-metric ``thresholds`` map can
    contain stale intermediate booleans.
    """
    if not stdout_log.exists():
        return set()
    breached: set[str] = set()
    try:
        text = stdout_log.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return set()
    for line in text.splitlines():
        if "thresholds on metrics" in line and "have been crossed" in line:
            # Format: ...thresholds on metrics 'route_latency{route:...}, route_latency{...}' have been crossed
            try:
                inner = line.split("thresholds on metrics", 1)[1]
                inner = inner.split("have been crossed", 1)[0]
                inner = inner.strip().strip("'\"")
                for part in inner.split(","):
                    p = part.strip().strip("'\"")
                    if p:
                        breached.add(p)
            except (IndexError, ValueError):
                continue
    return breached


def _gate_g1(
    summary: dict[str, Any] | None,
    stdout_breaches: set[str] | None = None,
) -> tuple[str, list[str]]:
    stdout_breaches = stdout_breaches or set()
    if not summary:
        return ("UNKNOWN", ["G1: no k6-summary.json present"])
    if summary.get("preflight") == "unreachable":
        return ("UNKNOWN", ["G1: preflight=unreachable (smoke skipped routes)"])
    if summary.get("k6") == "missing":
        return ("UNKNOWN", ["G1: k6 binary missing on runner; latency unmeasured"])
    notes: list[str] = []
    verdict = "GREEN"
    for route, floor_ms in ROUTE_FLOORS_MS_P99.items():
        m = _route_metric(summary, route)
        if m is None:
            verdict = "UNKNOWN" if verdict == "GREEN" else verdict
            notes.append(f"G1[{route}]: metric absent (no samples).")
            continue
        # k6-stdout final summary is authoritative for threshold pass/fail.
        sub_key = f"route_latency{{route:{route}}}"
        if sub_key in stdout_breaches:
            notes.append(
                f"G1[{route}]: k6-stdout reports threshold breach "
                f"(p(99) > {floor_ms}ms floor) -> RED"
            )
            verdict = "RED"
            continue
        p99_val, threshold_ok = _extract_p99(m, floor_ms)
        if p99_val is None and threshold_ok is None:
            # Neither numeric nor threshold available — cannot judge.
            verdict = "UNKNOWN" if verdict == "GREEN" else verdict
            notes.append(
                f"G1[{route}]: p99 unavailable (numeric absent AND no "
                f"p(99) threshold present). Treating as UNKNOWN."
            )
            continue
        if p99_val is not None:
            ok = float(p99_val) <= float(floor_ms)
            notes.append(
                f"G1[{route}]: p99={p99_val:.1f}ms floor={floor_ms}ms "
                f"-> {'GREEN' if ok else 'RED'}"
            )
            if not ok:
                verdict = "RED"
        else:  # threshold_ok is known
            ok = bool(threshold_ok)
            notes.append(
                f"G1[{route}]: numeric p99 absent; threshold "
                f"p(99)<{floor_ms} -> {'GREEN' if ok else 'RED'} (from "
                f"k6 threshold evaluation)"
            )
            if not ok:
                verdict = "RED"
    return (verdict, notes)


def _gate_g2(meta: dict[str, Any]) -> tuple[str, list[str]]:
    delta = meta.get("audit_chain_integrity_violation_delta")
    if delta is None:
        return ("UNKNOWN", ["G2: audit_chain_integrity_violation_delta not provided"])
    if int(delta) == 0:
        return ("GREEN", [f"G2: audit chain integrity violations delta=0 over run"])
    return ("RED", [f"G2: audit chain integrity violations delta={delta} (must be 0)"])


def _gate_g3(meta: dict[str, Any]) -> tuple[str, list[str]]:
    sev = meta.get("sev_incidents_during_run")
    if sev is None:
        return ("UNKNOWN", ["G3: sev_incidents_during_run not provided"])
    if int(sev) == 0:
        return ("GREEN", ["G3: SEV-0/1 incidents during run = 0"])
    return ("RED", [f"G3: SEV-0/1 incidents during run = {sev} (must be 0)"])


def _gate_g4(meta: dict[str, Any]) -> tuple[str, list[str]]:
    acks = meta.get("pilot_tenant_acks_count")
    if acks is None:
        return ("UNKNOWN", ["G4: pilot_tenant_acks_count not provided (manual)"])
    if int(acks) >= 5:
        return ("GREEN", [f"G4: pilot tenant acks={acks} (>= 5)"])
    return ("RED", [f"G4: pilot tenant acks={acks} (< 5)"])


def _gate_g5(meta: dict[str, Any]) -> tuple[str, list[str]]:
    lag = meta.get("neon_shadow_lag_p99_seconds")
    if lag is None:
        return ("UNKNOWN", ["G5: neon_shadow_lag_p99_seconds not provided"])
    if float(lag) <= 300:
        return ("GREEN", [f"G5: Neon shadow lag p99={lag}s (<=300)"])
    return ("RED", [f"G5: Neon shadow lag p99={lag}s (>300)"])


def _gate_g6(meta: dict[str, Any]) -> tuple[str, list[str]]:
    pct = meta.get("dsr_24h_success_pct")
    if pct is None:
        return ("UNKNOWN", ["G6: dsr_24h_success_pct not provided"])
    if float(pct) >= 100.0:
        return ("GREEN", [f"G6: DSR cron 24h success rate={pct}% (=100)"])
    return ("RED", [f"G6: DSR cron 24h success rate={pct}% (<100)"])


def _verdict(states: list[str]) -> str:
    if any(s == "RED" for s in states):
        return "BLOCK"
    if any(s == "UNKNOWN" for s in states):
        return "MANUAL-REVIEW"
    return "GREENLIGHT"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run-dir", required=True, type=Path)
    ap.add_argument("--out", required=False, type=Path)
    args = ap.parse_args(argv)

    run_dir: Path = args.run_dir
    out: Path = args.out or (run_dir / "analysis.md")

    meta = _read_json(run_dir / "run-meta.json") or {}
    summary = _read_json(run_dir / "k6-summary.json") or {}
    stdout_breaches = _stdout_threshold_breach_routes(run_dir / "k6-stdout.log")

    g1 = _gate_g1(summary, stdout_breaches)
    g2 = _gate_g2(meta)
    g3 = _gate_g3(meta)
    g4 = _gate_g4(meta)
    g5 = _gate_g5(meta)
    g6 = _gate_g6(meta)

    states = [g1[0], g2[0], g3[0], g4[0], g5[0], g6[0]]
    verdict = _verdict(states)

    lines: list[str] = []
    lines.append(f"# Wave-22 Endurance Analysis — {verdict}")
    lines.append("")
    lines.append(f"- Generated: {datetime.now(timezone.utc).isoformat()}")
    lines.append(f"- Run dir: `{run_dir}`")
    lines.append(f"- Mode: `{meta.get('mode', 'unknown')}`")
    lines.append(f"- Duration: `{meta.get('duration', 'unknown')}`")
    lines.append(f"- Target: `{meta.get('target_host', 'unknown')}`")
    lines.append(f"- Operator: `{meta.get('operator', 'unknown')}`")
    lines.append("")
    lines.append("## Greenlight gates (RB-GA-CUTOVER §4 G1–G6)")
    lines.append("")
    lines.append("| Gate | State | Notes |")
    lines.append("|------|-------|-------|")
    for name, (state, notes) in [
        ("G1 latency", g1),
        ("G2 audit-chain integrity", g2),
        ("G3 SEV-0/1 zero", g3),
        ("G4 pilot acks", g4),
        ("G5 Neon shadow lag", g5),
        ("G6 DSR cron success", g6),
    ]:
        first = notes[0] if notes else ""
        lines.append(f"| {name} | {state} | {first} |")
    lines.append("")
    lines.append("## Detailed notes")
    lines.append("")
    for gate_label, (_, notes) in [
        ("G1", g1),
        ("G2", g2),
        ("G3", g3),
        ("G4", g4),
        ("G5", g5),
        ("G6", g6),
    ]:
        lines.append(f"### {gate_label}")
        for n in notes:
            lines.append(f"- {n}")
        lines.append("")

    lines.append("## What to do next")
    lines.append("")
    if verdict == "GREENLIGHT":
        lines.append(
            "All gates GREEN. Attach this report + raw artefacts to the GA "
            "evidence pack (`specs/_compliance/GA-GATE-CRITERIA.md` row R-6)."
        )
    elif verdict == "MANUAL-REVIEW":
        lines.append(
            "Operator must populate the `UNKNOWN` gates in `run-meta.json` "
            "(pilot acks, Neon lag, DSR cron pct, audit-chain delta, SEV "
            "incident count) and re-run this analyser. See "
            "`specs/_runbooks/RB-24H-ENDURANCE-LOAD.md` §5."
        )
    else:  # BLOCK
        lines.append(
            "At least one gate is RED. Cutover is BLOCKED. Open a SEV-2 "
            "incident, follow `specs/_runbooks/RB-PERF-REGRESSION.md` for "
            "latency-driven regressions, and re-run the campaign once the "
            "root cause is fixed."
        )

    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"[analyze_endurance_run] verdict={verdict} -> {out}")

    # Exit code: 0 for GREENLIGHT/MANUAL-REVIEW; 10 for BLOCK so CI can fail closed.
    return 10 if verdict == "BLOCK" else 0


if __name__ == "__main__":
    sys.exit(main())
