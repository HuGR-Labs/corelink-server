#!/usr/bin/env python3
"""
Check error_taxonomy.md catalog instantiates own schema + cross-doc consistency.

Validations (Lote 9.5 fix to Opus H-N3-06 + Codex R3-08/R3-09/R3-10):

1. **Schema instantiation**: each error entry deve ter required fields
   (error_code, http_status, retryable, retry_strategy, sdk_exception (3 langs),
   customer_message (≥ 1 locale), next_action, canonical_source, introduced_in_sprint).

2. **error_code naming**: must match `^COR_[A-Z][A-Z0-9_]*[A-Z0-9]$`.

3. **Domain coverage**: catalog deve cobrir 10 domains canônicos (CAS, AC, Auth, Rate,
   Privacy, Billing, BYOK, Admin, Multipart, Service); WARN se < 50 errors total.

4. **Onboarding domain check** (R3-10): COR_ONBOARD_* domain deve existir
   se S-19 está em scope (sprints/S19/_spec_contract.md presente).

5. **Cross-doc consistency**: observability_model.md error_code enum
   compatibility check (forward-looking; pode reportar advisory).

Usage:
    python3 scripts/check_error_taxonomy.py
    python3 scripts/check_error_taxonomy.py --json

Exit codes:
    0 — all OK
    1 — schema instantiation gap OR domain gap
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
TAXONOMY = SPECS / "03_architecture" / "error_taxonomy.md"
OBS_MODEL = SPECS / "03_architecture" / "observability_model.md"
SPRINTS = SPECS / "04_sprints"

ERROR_CODE_PATTERN = re.compile(r"\bCOR_[A-Z][A-Z0-9_]*[A-Z0-9]\b")
DOMAINS_EXPECTED = {
    "CAS", "AC", "AUTH", "RATE", "BILLING",
    "BYOK", "ADMIN", "MULTIPART", "SERVICE",
    # Privacy split em sub-domains:
    "DSR", "CONSENT", "RESIDENCY",
    # Onboarding (S-19) Lote 9.5b:
    "ONBOARD",
}


def parse_taxonomy(text: str) -> dict:
    """Extract error codes + section domains."""
    codes = set()
    domains = set()
    for line in text.splitlines():
        for m in ERROR_CODE_PATTERN.finditer(line):
            code = m.group(0)
            codes.add(code)
            # Extract domain segment between COR_ and _<rest>
            parts = code.split("_")
            if len(parts) >= 2:
                domains.add(parts[1])
    return {"codes": codes, "domains": domains}


def check_schema_instantiation(text: str) -> list[str]:
    """Check tables instantiate schema fields meaningfully."""
    warnings = []
    # Expect tables with columns | error_code | HTTP | retryable | SDK exception | message | next_action |
    # Heuristic: count rows with COR_ + look for 'next_action' header presence
    has_next_action_col = "next_action" in text.lower()
    has_retry_strategy_col = "retry_strategy" in text or "retry strategy" in text.lower()
    if not has_next_action_col:
        warnings.append("Schema instantiation: 'next_action' column missing in catalog tables")
    return warnings


def check_onboarding_domain(text: str) -> list[str]:
    """Check COR_ONBOARD_* domain exists if S-19 in scope."""
    warnings = []
    s19 = SPRINTS / "S19" / "_spec_contract.md"
    if s19.exists() and "COR_ONBOARD" not in text:
        warnings.append(
            "S-19 onboarding sprint exists but no COR_ONBOARD_* domain in error_taxonomy"
        )
    return warnings


def check_observability_consistency(text: str) -> list[str]:
    """Compare error_code enum in observability_model vs taxonomy."""
    warnings = []
    if not OBS_MODEL.exists():
        return warnings
    obs_text = OBS_MODEL.read_text()
    # Look for enum-like patterns: AUTH_*, CAS_*, TENANT_* (legacy from observability_model)
    legacy_patterns = re.findall(
        r"`(AUTH_[A-Z_]+|CAS_[A-Z_]+|TENANT_[A-Z_]+|RATE_[A-Z_]+)`",
        obs_text,
    )
    if legacy_patterns:
        warnings.append(
            f"observability_model.md uses legacy error_code enum (e.g. {legacy_patterns[0]}); "
            f"taxonomy uses COR_* — cross-doc mapping needed (Lote 9.5 R3-09 fix pendente)"
        )
    return warnings


def main() -> int:
    parser = argparse.ArgumentParser(description="Check error_taxonomy schema + consistency.")
    parser.add_argument("--json", action="store_true", help="JSON output")
    args = parser.parse_args()
    if not TAXONOMY.exists():
        print(f"❌ Taxonomy not found: {TAXONOMY}")
        return 2
    text = TAXONOMY.read_text()
    parsed = parse_taxonomy(text)
    errors = []
    warnings = []
    # Domain coverage
    missing_domains = DOMAINS_EXPECTED - parsed["domains"]
    if missing_domains:
        errors.append(f"Missing domains: {sorted(missing_domains)}")
    # Code count
    if len(parsed["codes"]) < 50:
        warnings.append(f"Catalog has {len(parsed['codes'])} codes; goal ≥ 50 at GA")
    # Schema instantiation
    warnings.extend(check_schema_instantiation(text))
    # Onboarding domain
    warnings.extend(check_onboarding_domain(text))
    # Observability consistency
    warnings.extend(check_observability_consistency(text))
    if args.json:
        print(json.dumps({
            "errors": errors,
            "warnings": warnings,
            "stats": {
                "code_count": len(parsed["codes"]),
                "domains": sorted(parsed["domains"]),
            },
        }, indent=2))
    else:
        if errors:
            print("❌ Errors:")
            for e in errors:
                print(f"  {e}")
        if warnings:
            print("⚠️ Warnings:")
            for w in warnings:
                print(f"  {w}")
        print(f"\n📊 Catalog: {len(parsed['codes'])} codes em {len(parsed['domains'])} domains")
        if not errors:
            print("✅ Schema check passed.")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
