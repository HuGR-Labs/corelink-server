#!/usr/bin/env python3
"""
Valida que todo INV declarado em WI sob specs/04_sprints/SXX/work_items/
exists nesta seção do `invariant_registry.md`.

Endereça gap persistente flagged em S-01/S-02/S-03/S-04 R4 reviews:
"INVs declared in WIs never actually land in invariant_registry.md post-SEAL —
no CI gate."

Lote 10.4bis: introduz CI gate enforcing WI INV declarations exist em registry.

Uso:
    python3 scripts/validate_inv_promotion.py            # report normal
    python3 scripts/validate_inv_promotion.py --strict   # exit 1 if drift

Exit codes:
    0 — todas INVs declaradas em WIs existem em registry §3.X
    1 — drift detected (alguma INV declarada em WI ausente do registry)
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SPECS_DIR = REPO_ROOT / "specs"
SPRINTS_DIR = SPECS_DIR / "04_sprints"
REGISTRY = SPECS_DIR / "03_architecture" / "invariant_registry.md"

# Pattern: INV-* IDs em texto
INV_PATTERN = re.compile(r"\bINV-[A-Za-z][A-Za-z0-9_-]+\b")

# Names que NÃO são reais INVs (plural-form, placeholders, framework-internal):
EXCLUDE_INV = {
    "INV-AC",
    "INV-AUTH",
    "INV-AUTH-PAT",
    "INV-CAS-SIDE-CHANNEL",
    "INV-DATA-AC-REFS-EXIST",
    "INV-LIFECYCLE-001",
    "INV-GC",
    "INV-SUPPLY",
    "INV-SCOPE-DISCIPLINE",
    "INV-DATA-CLASSIFICATION",
    "INV-XXX",
    "INV-XXX-",
    "INV-XXX-name",
    "INV-AAA",
    "INV-BBB",
    "INV-YYY",
    "INV-ZZZ",
    # Forward-looking (declared mas not yet shipped; OK):
    "INV-BILLING-RECONCILE-3-LAYER",
    "INV-BYOK-CRYPTO-SOVEREIGNTY",
    "INV-REGION-NO-CROSS-LEAK",
    "INV-ONBOARD-DPA-FIRST",
    "INV-KEY-NO-SKIP",
    # Aliases legacy CamelCase:
    "INV-TenantIsolation",
    "INV-AuditLogImmutability",
    "INV-CASIdempotency",
    "INV-QuotaEnforcement",
    "INV-DigestVerification",
    "INV-DataResidency",
}


def extract_invs_from_file(path: Path) -> set[str]:
    """Extract all INV-* mentions from a markdown file."""
    text = path.read_text()
    return set(INV_PATTERN.findall(text)) - EXCLUDE_INV


def extract_invs_from_registry() -> set[str]:
    """Extract INVs from registry that are CANONICALLY DEFINED (table rows)."""
    text = REGISTRY.read_text()
    # Match table rows starting with `| **INV-X**` or `| INV-X |`
    pattern = re.compile(r"^\|\s*\*?\*?(INV-[A-Z][A-Z0-9_-]+)\*?\*?\s*\|", re.MULTILINE)
    return set(pattern.findall(text))


def main() -> int:
    if not REGISTRY.exists():
        print(f"ERROR: registry not found at {REGISTRY}", file=sys.stderr)
        return 2

    registry_invs = extract_invs_from_registry()
    print(f"Registry contains {len(registry_invs)} canonically-defined INVs")

    drift: dict[str, set[str]] = {}
    total_wi_invs: set[str] = set()

    for wi_path in sorted(SPRINTS_DIR.glob("S*/work_items/WI-*.md")):
        wi_invs = extract_invs_from_file(wi_path)
        total_wi_invs |= wi_invs
        missing = wi_invs - registry_invs
        if missing:
            drift[str(wi_path.relative_to(REPO_ROOT))] = missing

    print(f"WIs reference {len(total_wi_invs)} distinct INVs")
    print(f"Registry coverage: {len(total_wi_invs & registry_invs)}/{len(total_wi_invs)}")

    if drift:
        print("\n❌ Drift detected: INVs declared in WIs but not in registry §3.X:")
        for wi, missing in sorted(drift.items()):
            for inv in sorted(missing):
                print(f"  {inv}")
                print(f"    in {wi}")
        print(f"\nTotal drift: {sum(len(v) for v in drift.values())} INV references not in registry.")
        print("Fix: add missing INVs to invariant_registry.md (in appropriate §3.X domain section).")
        return 1

    print("\n✅ All WI-declared INVs are present in invariant_registry.md.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
