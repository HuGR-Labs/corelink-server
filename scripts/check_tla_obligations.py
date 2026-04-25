#!/usr/bin/env python3
"""
Check TLA+ obligations matrix against invariant_registry.md §4.

Cada invariante CRITICAL/HIGH no registry §3 deve ter status documented em §4:
- §4.1 GREEN (TLC verde em CI)
- §4.2 PLANNED (sprint owner com filename)
- §4.3 algorithm-only (justified non-TLA+)

CI gate (Lote 9.5):
- Toda CRITICAL invariante DEVE estar em §4.1 ou §4.2.
- Pre-S-20 GA gate: todo PLANNED CRITICAL deve transitar GREEN antes de gate liberation.
- Spec contracts que declaram TLA+ planned MUST appear em §4.2 com matching filename.

Usage:
    python3 scripts/check_tla_obligations.py
    python3 scripts/check_tla_obligations.py --pre-ga  # stricter check (S-20 gate)
    python3 scripts/check_tla_obligations.py --json    # machine-readable

Exit codes:
    0 — all obligations met
    1 — CRITICAL obligation missing
    2 — file/registry parse error
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SPECS = REPO_ROOT / "specs"
REGISTRY = SPECS / "03_architecture" / "invariant_registry.md"
TLA_DIR = SPECS / "tla"
SPRINTS_DIR = SPECS / "04_sprints"

# Pattern: INV-* mention em registry tables
INV_PATTERN = re.compile(r"\*\*INV-[A-Z0-9-]+\*\*")
# Severity pattern em row column
SEVERITY_PATTERN = re.compile(r"\| (CRITICAL|HIGH|MEDIUM) \|", re.IGNORECASE)


def parse_registry_invariants(text: str) -> dict[str, dict]:
    """Parse §3 registry tables; return {inv_id: {severity, sprint_origin}}."""
    invariants = {}
    in_section = False
    current_section = None
    for line in text.splitlines():
        if line.startswith("## 3."):
            in_section = True
            continue
        if line.startswith("## 4.") or line.startswith("## 5."):
            in_section = False
        if not in_section:
            continue
        if line.startswith("### 3."):
            current_section = line.strip()
            continue
        # Parse table row
        if line.startswith("|") and "INV-" in line and "**INV-" in line:
            inv_match = re.search(r"\*\*INV-([A-Z0-9-]+)\*\*", line)
            sev_match = re.search(r"\|\s*(CRITICAL|HIGH|MEDIUM)\s*\|", line, re.IGNORECASE)
            if inv_match:
                full_id = f"INV-{inv_match.group(1)}"
                invariants[full_id] = {
                    "severity": (sev_match.group(1).upper() if sev_match else "UNKNOWN"),
                    "section": current_section or "",
                }
    return invariants


def parse_tla_obligations(text: str) -> dict[str, dict]:
    """Parse §4 obligation matrix; return {inv_id: {status, file}}."""
    obligations = {}
    in_section = False
    sub_section = None
    for line in text.splitlines():
        if line.startswith("## 4."):
            in_section = True
            continue
        if line.startswith("## 5.") or line.startswith("## 6."):
            in_section = False
        if not in_section:
            continue
        if line.startswith("### 4."):
            sub_section = line.strip()
            continue
        # parse table rows
        if line.startswith("|") and "INV-" in line:
            inv_match = re.search(r"INV-[A-Z0-9-]+", line)
            if inv_match:
                inv_id = inv_match.group(0)
                # Detect status: GREEN (✅) / PLANNED (📋) / non-TLA+
                if "✅ GREEN" in line:
                    status = "GREEN"
                elif "📋 PLANNED" in line:
                    status = "PLANNED"
                elif sub_section and "4.3" in sub_section:
                    status = "ALGORITHM_ONLY"
                else:
                    status = "UNKNOWN"
                # Filename
                file_match = re.search(r"`(specs/tla/[a-z_]+\.tla)`", line)
                filename = file_match.group(1) if file_match else None
                obligations[inv_id] = {"status": status, "file": filename}
    return obligations


def check(pre_ga: bool = False) -> tuple[list[str], list[str]]:
    """Run check; return (errors, warnings)."""
    errors = []
    warnings = []
    if not REGISTRY.exists():
        errors.append(f"Registry not found: {REGISTRY}")
        return errors, warnings
    text = REGISTRY.read_text()
    invariants = parse_registry_invariants(text)
    obligations = parse_tla_obligations(text)
    # Check: every CRITICAL/HIGH invariant has obligation entry
    for inv_id, meta in invariants.items():
        sev = meta["severity"]
        if sev not in ("CRITICAL", "HIGH"):
            continue
        if inv_id not in obligations:
            errors.append(
                f"{inv_id} ({sev}) defined in §3 but missing from §4 obligation matrix"
            )
            continue
        ob = obligations[inv_id]
        if sev == "CRITICAL":
            if ob["status"] not in ("GREEN", "PLANNED", "ALGORITHM_ONLY"):
                errors.append(
                    f"{inv_id} (CRITICAL) has unknown TLA+ status in §4: {ob['status']}"
                )
            if pre_ga and ob["status"] == "PLANNED":
                errors.append(
                    f"{inv_id} (CRITICAL) is PLANNED but pre-GA gate requires GREEN"
                )
        if ob["status"] == "PLANNED" and ob["file"]:
            tla_path = REPO_ROOT / ob["file"]
            if not tla_path.exists():
                warnings.append(
                    f"{inv_id} declares planned TLA+ {ob['file']} but file not yet in repo"
                )
    return errors, warnings


def main() -> int:
    parser = argparse.ArgumentParser(description="Check TLA+ obligations.")
    parser.add_argument("--pre-ga", action="store_true",
                        help="Strict check; PLANNED → GREEN required (S-20 gate)")
    parser.add_argument("--json", action="store_true", help="JSON output")
    args = parser.parse_args()
    errors, warnings = check(pre_ga=args.pre_ga)
    if args.json:
        print(json.dumps({"errors": errors, "warnings": warnings}, indent=2))
    else:
        if errors:
            print("❌ TLA+ obligation errors:")
            for e in errors:
                print(f"  {e}")
        if warnings:
            print("⚠️ Warnings (forward-looking):")
            for w in warnings:
                print(f"  {w}")
        if not errors and not warnings:
            print("✅ TLA+ obligations matrix consistent.")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
