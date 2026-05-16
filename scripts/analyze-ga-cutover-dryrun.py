#!/usr/bin/env python3
"""analyze-ga-cutover-dryrun.py — Wave-24 RB-GA-CUTOVER dry-run analyser.

Reads the JSON evidence emitted by ``scripts/ga-cutover-dryrun.sh`` and
emits a Markdown summary keyed to the 6 greenlight criteria
(G1..G6 + composite) declared in:

  - ``specs/_runbooks/RB-GA-CUTOVER.md`` §4
  - ``dashboards/alerts/dash-ga-greenlight.yml``

Verdict mapping (exit code):
  - 0 — all 6 greenlights GREEN AND every §3 step PASS  -> ``GREEN``.
  - 1 — any greenlight RED OR any step FAIL              -> ``RED``.
  - 2 — input file missing / malformed / schema error.

This script intentionally has zero third-party dependencies so it runs
in CI containers and on operator laptops without provisioning.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


REQUIRED_GREENLIGHTS = (
    "G1_p99_latency_regions_ok",
    "G2_audit_chain_integrity",
    "G3_sev01_zero_72h_ok",
    "G4_pilot_attestations_ok",
    "G5_neon_shadow_lag_ok",
    "G6_dsr_cron_24h_success",
)


def _safe_load(path: Path) -> dict[str, Any]:
    try:
        with path.open("r", encoding="utf-8") as fh:
            payload = json.load(fh)
    except FileNotFoundError:
        sys.stderr.write(f"ERROR: evidence file not found: {path}\n")
        sys.exit(2)
    except json.JSONDecodeError as exc:
        sys.stderr.write(f"ERROR: evidence file is not valid JSON: {exc}\n")
        sys.exit(2)
    if not isinstance(payload, dict):
        sys.stderr.write("ERROR: evidence root must be a JSON object\n")
        sys.exit(2)
    return payload


def _emit_markdown(payload: dict[str, Any]) -> str:
    gl = payload.get("greenlights", {})
    composite = int(gl.get("composite_ok", 0))
    verdict = payload.get("verdict", "UNKNOWN")
    steps = payload.get("steps", [])
    triggers = payload.get("rollback_triggers", [])
    pre = payload.get("pre_cutover_checklist", [])
    metrics = payload.get("greenlight_metrics_snapshot", {})

    n_pass = sum(1 for s in steps if s.get("outcome") == "PASS")
    n_fail = sum(1 for s in steps if s.get("outcome") == "FAIL")
    triggers_fired = [t for t in triggers if t.get("fired") == "YES"]

    lines: list[str] = []
    lines.append(f"# GA Cutover Dry-Run Analysis — {payload.get('run_date', 'NA')}")
    lines.append("")
    lines.append(f"- **Verdict**: `{verdict}` (composite_ok={composite})")
    lines.append(f"- **Base SHA**: `{payload.get('base_sha', 'NA')[:12]}`")
    lines.append(f"- **Branch**: `{payload.get('base_branch', 'NA')}`")
    lines.append(f"- **Environment**: {payload.get('environment', 'NA')}")
    lines.append(f"- **Run started**: {payload.get('run_started_at', 'NA')}")
    lines.append("")
    lines.append("## Greenlight criteria (G1..G6)")
    lines.append("")
    lines.append("| Gate | Value | Status |")
    lines.append("|---|---|---|")
    for key in REQUIRED_GREENLIGHTS:
        val = int(gl.get(key, 0))
        status = "GREEN" if val == 1 else "RED"
        lines.append(f"| `{key}` | {val} | {status} |")
    lines.append(f"| `composite_ok` | {composite} | {'GREEN' if composite == 1 else 'RED'} |")
    lines.append("")
    lines.append("## Greenlight metric snapshot")
    lines.append("")
    for k, v in sorted(metrics.items()):
        lines.append(f"- `{k}` = `{v}`")
    lines.append("")
    lines.append("## §3 step execution")
    lines.append("")
    lines.append(f"- Total steps: {len(steps)}")
    lines.append(f"- PASS: {n_pass}  /  FAIL: {n_fail}")
    lines.append("")
    lines.append("| Step | Label | Outcome | Duration (ms) |")
    lines.append("|---|---|---|---|")
    for s in steps:
        lines.append(
            f"| {s.get('id', '?')} | {s.get('label', '?')} | "
            f"{s.get('outcome', '?')} | {s.get('duration_ms', 0)} |"
        )
    lines.append("")
    lines.append("## §0 pre-cutover checklist (T-72h)")
    lines.append("")
    for chk in pre:
        lines.append(f"- `{chk.get('check', '?')}` -> {chk.get('status', '?')}")
    lines.append("")
    lines.append("## §5 rollback trigger evaluation")
    lines.append("")
    lines.append("| Trigger | Fired? |")
    lines.append("|---|---|")
    for t in triggers:
        lines.append(f"| `{t.get('id', '?')}` | {t.get('fired', '?')} |")
    lines.append("")
    lines.append(
        f"Triggers fired: {len(triggers_fired)} "
        f"({'NONE' if not triggers_fired else ', '.join(t['id'] for t in triggers_fired)})"
    )
    lines.append("")
    lines.append("## Caveats")
    lines.append("")
    for c in payload.get("caveats", []):
        lines.append(f"- {c}")
    lines.append("")
    return "\n".join(lines)


def _verdict_exit_code(payload: dict[str, Any]) -> int:
    gl = payload.get("greenlights", {})
    if int(gl.get("composite_ok", 0)) != 1:
        return 1
    for key in REQUIRED_GREENLIGHTS:
        if int(gl.get(key, 0)) != 1:
            return 1
    for step in payload.get("steps", []):
        if step.get("outcome") != "PASS":
            return 1
    return 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(
        description="Analyse RB-GA-CUTOVER dry-run evidence and emit verdict."
    )
    ap.add_argument("evidence", help="Path to the ga-cutover-dryrun-*.json file")
    ap.add_argument(
        "--out",
        default=None,
        help="Optional Markdown output path (default: stdout).",
    )
    args = ap.parse_args(argv)

    path = Path(args.evidence)
    payload = _safe_load(path)
    md = _emit_markdown(payload)

    if args.out:
        Path(args.out).write_text(md, encoding="utf-8")
    else:
        sys.stdout.write(md)
        if not md.endswith("\n"):
            sys.stdout.write("\n")

    return _verdict_exit_code(payload)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
