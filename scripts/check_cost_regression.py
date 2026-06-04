#!/usr/bin/env python3
"""
Check cost regression gate adoption per meta-contract §14.10.

§14.10 universal cost regression gate: cada sprint que toca hot path
(CAS/AC/billing/observability/region) DEVE incluir benchmark com per-op
cost estimate em $USD/million ops; PR > 10% regression bloqueia merge.

Aplicação obrigatória em S-07/S-08/S-09/S-10/S-14 (audits R1+R2 flag
S-07/8/9/10 ainda gap em R3).

Usage:
    python3 scripts/check_cost_regression.py
    python3 scripts/check_cost_regression.py --json

Exit codes:
    0 — all required sprints have cost gate documented
    1 — cost gate missing em sprint required
    2 — file/parse error
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SPECS = REPO_ROOT / "specs"
SPRINTS = SPECS / "04_sprints"
META = SPRINTS / "_sprint_creation_contract.md"

# Sprints onde cost regression gate e obrigatorio (HIGH_RISK hot path, per meta-contract §14.10).
# S07 and S09 are real sealed sprints: their dirs live under _sealed/ after reaching SEALED
# status (verified in specs/04_sprints/_sealed/S07/ and _sealed/S09/).  The gate still
# applies to them -- find_sprint_contract() checks both paths.
# Active sprint dirs as of 2026-06-03: S00 S02 S06 S08 S10 S11 S13 S14.
# Sealed sprint dirs:  specs/04_sprints/_sealed/{S01,S03,S04,S05,S06,S07,S09,...}.
REQUIRED = {"S07", "S08", "S09", "S10", "S14"}
# Optional but recommended
RECOMMENDED = {"S01", "S02", "S04", "S05", "S06", "S13"}


def find_sprint_contract(sprint: str) -> "Path | None":
    """Locate _spec_contract.md for a sprint, checking active path then _sealed/."""
    direct = SPRINTS / sprint / "_spec_contract.md"
    if direct.exists():
        return direct
    sealed = SPRINTS / "_sealed" / sprint / "_spec_contract.md"
    if sealed.exists():
        return sealed
    return None


def has_cost_gate(text: str) -> bool:
    """Check if sprint contract mentions cost regression gate."""
    patterns = [
        r"cost regression gate",
        r"§14\.10",
        r"cost regression",
        r"per-op cost",
        r"USD per million",
        r"USD/million",
    ]
    for p in patterns:
        if re.search(p, text, re.IGNORECASE):
            return True
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description="Check cost regression gate adoption.")
    parser.add_argument("--json", action="store_true", help="JSON output")
    args = parser.parse_args()
    errors = []
    warnings = []
    summary = {"required": {}, "recommended": {}}
    # Check meta-contract has §14.10
    if META.exists() and not has_cost_gate(META.read_text()):
        errors.append("Meta-contract missing §14.10 cost regression gate definition")
    # Check each required sprint (active dir OR _sealed/ dir)
    for sprint in sorted(REQUIRED):
        contract = find_sprint_contract(sprint)
        if contract is None:
            errors.append(f"{sprint}: _spec_contract.md not found")
            continue
        text = contract.read_text()
        has_gate = has_cost_gate(text)
        summary["required"][sprint] = has_gate
        if not has_gate:
            errors.append(
                f"{sprint}: cost regression gate not documented (§14.10 required for hot path)"
            )
    # Check recommended (warn only; also checks _sealed/)
    for sprint in sorted(RECOMMENDED):
        contract = find_sprint_contract(sprint)
        if contract is None:
            continue
        text = contract.read_text()
        has_gate = has_cost_gate(text)
        summary["recommended"][sprint] = has_gate
        if not has_gate:
            warnings.append(f"{sprint}: cost regression gate recommended but not documented")
    if args.json:
        print(json.dumps({
            "errors": errors,
            "warnings": warnings,
            "summary": summary,
        }, indent=2))
    else:
        if errors:
            print("❌ Cost regression gate errors:")
            for e in errors:
                print(f"  {e}")
        if warnings:
            print("⚠️ Recommendations:")
            for w in warnings:
                print(f"  {w}")
        print("\n📊 Required sprints status:")
        for sprint, ok in sorted(summary["required"].items()):
            mark = "✅" if ok else "❌"
            print(f"  {mark} {sprint}")
        if not errors:
            print("\n✅ All required sprints have cost regression gate documented.")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
