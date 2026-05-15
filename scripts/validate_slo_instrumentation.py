#!/usr/bin/env python3
"""
validate_slo_instrumentation.py — SLO catalog ↔ code instrumentation gate.

Companion to `specs/_audits/2026-05-14-slo-instrumentation-gaps.md`.

Enforces:

  1. **Every SLO declared in `slo_catalog.md §4.x` is either:**
       - Bound to an `Sli` enum variant in
         `crates/corelink-slo/src/definition.rs` whose `slug()` matches
         the SLO ID (or its canonical `SLI-*` cognate), OR
       - Allowlisted as deferred ("Phase 2" / "S-13" / "S-17" /
         "internal") in `DEFERRED_SLOS` below. Each deferred entry
         records the owning sprint / WI so a stale allowlist is a
         visible review smell.

  2. **No orphan `Sli` variants**: every `slug()` returned by
     `Sli::*::slug()` must map back to an SLO ID declared in
     `slo_catalog.md`. Catches drift where a code SLI is added but
     the catalog is not amended.

  3. **Audit document presence**: `specs/_audits/2026-05-14-slo-instrumentation-gaps.md`
     exists (load-bearing reference for the deferred-SLO rationale).

Exit code:
  0 on full coverage; non-zero on any gap.

Usage:
  python3 scripts/validate_slo_instrumentation.py
  python3 scripts/validate_slo_instrumentation.py --verbose
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

SLO_CATALOG = REPO / "specs" / "03_architecture" / "slo_catalog.md"
DEFINITION_RS = (
    REPO / "crates" / "corelink-slo" / "src" / "definition.rs"
)
AUDIT_DOC = (
    REPO
    / "specs"
    / "_audits"
    / "2026-05-14-slo-instrumentation-gaps.md"
)

# SLOs deliberately deferred to their owning sprint / Phase 2; each
# entry MUST be reviewed at the matching audit checkpoint.
DEFERRED_SLOS: dict[str, str] = {
    "SLO-AVAIL-EXEC": "Phase 2 (Remote Execution); roadmap post-GA",
    "SLO-FRESH-BILLING": "S-10 follow-on; needs `_age_seconds` histogram in `RedMetricKind` (canonical-15 expansion ADR pending)",
    "SLO-FRESH-DSR-ERASURE": "S-11 follow-on; long-window freshness emitter",
    "SLO-DEPLOY-SAFE": "S-12 (rollout subsystem); deploy_attempts table",
    "SLO-ADMIN-CONFIG-PROPAGATION": "S-13 (admin plane); WI-S13-001",
    "SLO-ADMIN-DUAL-APPROVAL-LATENCY": "S-13 (admin plane); WI-S13-002",
    "SLO-ADMIN-ROTATION-OVERLAP": "S-13 (admin plane); WI-S13-003 + key_management.md",
    "SLO-ADMIN-ROLLBACK-RECOVERY": "S-13 (admin plane); WI-S13-005",
    "SLO-RTO-REGION-FAILOVER": "S-17 (DR drill); WI-S17-002",
    "SLO-RPO-REGION": "S-17 (DR drill); WI-S17-002",
    "SLO-ONCALL-MTTA-SEV1": "S-17 (PagerDuty ingest); WI-S17-005",
    "SLO-ONCALL-MTTR-SEV1": "S-17 (PagerDuty ingest); WI-S17-005",
}

# Some SLOs map to an SLI slug that differs in the prefix (SLO-AVAIL-X
# in catalog vs `SLI-AVAIL-X` in the `Sli::slug()` taxonomy). This map
# normalizes the catalog ID to the canonical SLI slug that the alert
# evaluator binds against.
SLO_TO_SLI_ALIASES: dict[str, str] = {
    "SLO-AVAIL-CP": "SLI-AVAIL-CP",
    "SLO-AVAIL-CAS-GET": "SLI-AVAIL-CAS-GET",
    "SLO-AVAIL-CAS-PUT": "SLI-AVAIL-CAS-PUT",
    "SLO-AVAIL-AC": "SLI-AVAIL-AC-LOOKUP",
    "SLO-LAT-CAS-GET": "SLI-LATENCY-CAS-GET-P99",
    "SLO-LAT-CAS-PUT": "SLI-LATENCY-CAS-PUT-P99",
    "SLO-LAT-AC-HIT": "SLI-LATENCY-AC-HIT-P99",
}

# SLI slugs that historically pre-date this audit and reference SLO
# language outside `slo_catalog.md §4.x` headings (the catalog text
# discusses them in prose without dedicated subsections). These are
# considered already-instrumented and not orphans.
LEGACY_PRE_AUDIT_SLI_SLUGS: set[str] = {
    "SLI-AVAIL-AUTH",  # cognate of SLO-AVAIL-CP §4.1 ("auth + admin")
    "SLI-RATE-LIMIT-WITHIN-QUOTA",  # S-08 inheritance, slo_catalog.md §3 + §4
}


def parse_slo_ids_from_catalog(path: Path) -> list[str]:
    """Extract every `**SLO-*` ID from §4.x of `slo_catalog.md`."""
    text = path.read_text(encoding="utf-8")
    # Match e.g. `**SLO-AVAIL-CP**` (with optional trailing colon).
    rx = re.compile(r"\*\*(SLO-[A-Z][A-Z0-9_-]+)\*\*")
    seen: list[str] = []
    seen_set: set[str] = set()
    for m in rx.finditer(text):
        slo = m.group(1)
        if slo not in seen_set:
            seen.append(slo)
            seen_set.add(slo)
    return seen


def parse_sli_slugs_from_definition(path: Path) -> list[str]:
    """Extract every slug emitted by `Sli::*::slug()`."""
    text = path.read_text(encoding="utf-8")
    # Match `=> "SLI-..."` or `=> "SLO-..."`.
    rx = re.compile(r'=>\s*"((?:SLI|SLO)-[A-Z][A-Z0-9_-]+)"')
    seen: list[str] = []
    seen_set: set[str] = set()
    for m in rx.finditer(text):
        slug = m.group(1)
        if slug not in seen_set:
            seen.append(slug)
            seen_set.add(slug)
    return seen


def classify_slo(slo_id: str, sli_slugs: set[str]) -> str:
    """
    Return one of: "BOUND" / "DEFERRED" / "MISSING".

    BOUND  — SLO maps (via alias) to an `Sli::slug()` in code.
    DEFERRED — listed in `DEFERRED_SLOS` allowlist.
    MISSING — declared in spec, no code binding, no allowlist entry.
    """
    canonical = SLO_TO_SLI_ALIASES.get(slo_id, slo_id)
    if canonical in sli_slugs:
        return "BOUND"
    if slo_id in DEFERRED_SLOS:
        return "DEFERRED"
    return "MISSING"


def find_orphan_slis(
    sli_slugs: list[str], catalog_slos: list[str]
) -> list[str]:
    """Return Sli slugs that have no SLO in the catalog (or alias)."""
    valid: set[str] = set()
    for slo in catalog_slos:
        valid.add(slo)
        if slo in SLO_TO_SLI_ALIASES:
            valid.add(SLO_TO_SLI_ALIASES[slo])
    valid |= LEGACY_PRE_AUDIT_SLI_SLUGS
    return [s for s in sli_slugs if s not in valid]


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--verbose", action="store_true", help="print every SLO row"
    )
    args = ap.parse_args(argv)

    if not SLO_CATALOG.is_file():
        print(
            f"[FAIL] missing SLO catalog: {SLO_CATALOG}",
            file=sys.stderr,
        )
        return 2
    if not DEFINITION_RS.is_file():
        print(
            f"[FAIL] missing Sli definition: {DEFINITION_RS}",
            file=sys.stderr,
        )
        return 2
    if not AUDIT_DOC.is_file():
        print(
            "[FAIL] missing audit doc: "
            f"{AUDIT_DOC} (load-bearing reference)",
            file=sys.stderr,
        )
        return 2

    catalog = parse_slo_ids_from_catalog(SLO_CATALOG)
    if not catalog:
        print("[FAIL] parsed zero SLOs from catalog", file=sys.stderr)
        return 2
    sli_slugs = parse_sli_slugs_from_definition(DEFINITION_RS)
    if not sli_slugs:
        print(
            "[FAIL] parsed zero Sli slugs from definition.rs",
            file=sys.stderr,
        )
        return 2

    sli_slug_set = set(sli_slugs)
    bound: list[str] = []
    deferred: list[tuple[str, str]] = []
    missing: list[str] = []

    for slo in catalog:
        cls = classify_slo(slo, sli_slug_set)
        if cls == "BOUND":
            bound.append(slo)
        elif cls == "DEFERRED":
            deferred.append((slo, DEFERRED_SLOS[slo]))
        else:
            missing.append(slo)

    orphans = find_orphan_slis(sli_slugs, catalog)

    print("== SLO Instrumentation Validator ==")
    print(f"  catalog SLOs declared : {len(catalog)}")
    print(f"  Sli enum variants     : {len(sli_slugs)}")
    print(f"  BOUND                 : {len(bound)}")
    print(f"  DEFERRED (allowlisted): {len(deferred)}")
    print(f"  MISSING               : {len(missing)}")
    print(f"  orphan SLI slugs      : {len(orphans)}")

    if args.verbose:
        print("\n-- BOUND --")
        for slo in bound:
            print(
                f"  {slo} -> {SLO_TO_SLI_ALIASES.get(slo, slo)}"
            )
        print("\n-- DEFERRED --")
        for slo, why in deferred:
            print(f"  {slo}: {why}")

    if missing:
        print("\n[FAIL] uninstrumented SLOs (no Sli binding, no defer):")
        for slo in missing:
            print(f"  - {slo}")
        return 1

    if orphans:
        print(
            "\n[FAIL] orphan SLI slugs (no catalog SLO + no alias):"
        )
        for s in orphans:
            print(f"  - {s}")
        return 1

    print("\n[OK] every declared SLO has a binding or allowlist entry.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
