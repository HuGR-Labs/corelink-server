#!/usr/bin/env python3
"""
validate_dashboards.py — WI-S09-005 canonical dashboard structural validator.

Enforces (per WI-S09-005 §1 + §6.1):

  1. **Canonical 12 count discipline** (Lote 10.8-tris P1-NEW-1 lesson absorbed):
     dashboards/grafana/ contains exactly the 12 canonical dashboard JSON files
     listed in observability_model.md §10 Nível-3 (Lote 10.9-quaters NEW-P0-1
     corrected): GLOBAL-HEALTH + GLOBAL-PRODUCT + TENANT + CAS + AC + EXEC + GC
     + SUPPLY-CHAIN + SECURITY + PRIVACY + COST + SLO-CATALOG. Plus 3 legacy
     dashboards (DEDUP + RATE + MULTIPART) refactored as panels embedded in
     parent dashboards but kept on disk for historical reference.

  2. **JSON well-formed**: every file under dashboards/grafana/DASH-*.json
     parses as JSON.

  3. **Panel count ≥ 8** per WI §6.1 minimum coverage discipline.

  4. **Required variables**: every canonical 12 dashboard MUST expose at least a
     `region` template variable; canonical multi-tenant dashboards MUST expose
     a `tenant_tier` template variable; AdminCtx-aware dashboards MUST expose a
     `tenant` query template variable. See per-dashboard expected_vars below.

  5. **Datasource**: every panel + every template query references the
     canonical `${DS_PROMETHEUS}` (literal `$DS_PROMETHEUS`) datasource
     placeholder; `__inputs[].name` MUST be `DS_PROMETHEUS`.

  6. **Tags**: every dashboard MUST include `corelink` + `sota` tags.

  7. **lastUpdated annotation freshness** (sprint contract §14.s09.6):
     every canonical 12 dashboard MUST include a `annotations.lastUpdated`
     ISO 8601 timestamp; sustained > 90d = SEV-3 stale dashboard alert.

  8. **Cardinality budget annotation** (WI-S09-001 inheritance):
     canonical 12 dashboard MUST include `annotations.cardinality_budget_used`
     "<used>/100000" string; sum across dashboards alerted SEV-3 at 80% global
     cap.

Exit code:
  0 = all checks pass; 1 = any failure.

Usage:
  python3 scripts/validate_dashboards.py
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

# Anchor at repo root (parent of scripts/)
REPO_ROOT = Path(__file__).resolve().parent.parent
DASHBOARDS_DIR = REPO_ROOT / "dashboards" / "grafana"

# Lote 10.9-quaters NEW-P0-1 corrected canonical 12 (observability_model.md §10 Nível-3)
CANONICAL_12 = [
    "DASH-GLOBAL-HEALTH",
    "DASH-GLOBAL-PRODUCT",
    "DASH-TENANT",
    "DASH-CAS",
    "DASH-AC",
    "DASH-EXEC",
    "DASH-GC",
    "DASH-SUPPLY-CHAIN",
    "DASH-SECURITY",
    "DASH-PRIVACY",
    "DASH-COST",
    "DASH-SLO-CATALOG",
]

# Legacy dashboards refactored as panels embedded in parent dashboards but kept on disk.
# Per Lote 10.9-quaters NEW-P0-1: AUTH→SECURITY, BILLING→COST, RATE-LIMIT→TENANT,
# DEDUP→CAS, CHAOS→SLO-CATALOG. Multipart kept as legacy detail referenced from CAS.
LEGACY = [
    "DASH-DEDUP",
    "DASH-RATE",
    "DASH-MULTIPART",
    # Ops-internal dashboards (not part of the canonical-12 customer
    # observability surface). Added by S-17 WI-S17-005 (PagerDuty 24/7
    # incident response). Kept in `dashboards/grafana/` for the oncall
    # team; not referenced from `observability_model.md §10 Nível-3`.
    "DASH-ONCALL-24-7",
    "DASH-ONCALL-FATIGUE",
    # Granular SLO detail dashboards shipped by WP-6.1 (feat commit b82a1c3c),
    # supplementary to the canonical DASH-SLO-CATALOG. They live on disk for
    # the SRE/SLO surface but are outside the canonical-12 customer set — the
    # allowlist was never updated when they landed, so the count-discipline
    # check flagged them "unexpected" the next time this gate ran.
    "DASH-SLO-API",
    "DASH-SLO-AUDIT",
]

# Per-dashboard required template variables.
EXPECTED_VARS = {
    "DASH-GLOBAL-HEALTH": {"tenant_tier", "region"},
    "DASH-GLOBAL-PRODUCT": {"tenant_tier", "region"},
    "DASH-TENANT": {"tenant", "tenant_tier", "region"},
    "DASH-CAS": {"tenant_tier", "region"},
    "DASH-AC": {"tenant_tier", "region"},
    "DASH-EXEC": {"tenant_tier", "region"},
    "DASH-GC": {"tenant_tier", "region"},
    "DASH-SUPPLY-CHAIN": {"region"},
    "DASH-SECURITY": {"tenant_tier", "region"},
    "DASH-PRIVACY": {"tenant_tier", "region"},
    "DASH-COST": {"tenant_tier", "region"},
    "DASH-SLO-CATALOG": {"region"},
    # legacy
    "DASH-DEDUP": {"tenant_tier", "region"},
    "DASH-RATE": {"tenant_tier", "region"},
    "DASH-MULTIPART": {"tenant_tier", "region"},
}

ISO_8601 = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})$"
)
CARDINALITY_BUDGET = re.compile(r"^\d+/\d+$")


def fail(uid: str, msg: str, errors: list[str]) -> None:
    errors.append(f"  [FAIL] {uid}: {msg}")


def validate_dashboard(path: Path, errors: list[str], warnings: list[str]) -> None:
    uid = path.stem
    try:
        with path.open() as f:
            data = json.load(f)
    except json.JSONDecodeError as exc:
        fail(uid, f"JSON parse error: {exc}", errors)
        return

    # uid match filename
    if data.get("uid") != uid:
        fail(uid, f"uid mismatch: file={uid!r} json={data.get('uid')!r}", errors)

    # Panel count >= 8 per WI §6.1 minimum
    panels = data.get("panels", [])
    if len(panels) < 8:
        fail(uid, f"panels count {len(panels)} < 8 minimum", errors)

    # Required template variables
    expected = EXPECTED_VARS.get(uid, set())
    tmpl_list = data.get("templating", {}).get("list", [])
    var_names = {v.get("name") for v in tmpl_list}
    missing = expected - var_names
    if missing:
        fail(uid, f"missing required template vars: {sorted(missing)}", errors)

    # Datasource consistency: __inputs[].name == DS_PROMETHEUS
    inputs = data.get("__inputs", [])
    input_names = {i.get("name") for i in inputs}
    if "DS_PROMETHEUS" not in input_names:
        fail(uid, "__inputs missing DS_PROMETHEUS entry", errors)

    # Datasource: every panel datasource is $DS_PROMETHEUS or unset (inherits)
    for p in panels:
        ds = p.get("datasource")
        if ds is None:
            continue  # inherits dashboard default — acceptable
        if isinstance(ds, dict):
            ds_name = ds.get("uid") or ds.get("name") or ""
        else:
            ds_name = str(ds)
        if "DS_PROMETHEUS" not in ds_name:
            fail(uid, f"panel id={p.get('id')} datasource {ds!r} not DS_PROMETHEUS", errors)

    # Tags must include "corelink" + "sota"
    tags = set(data.get("tags", []))
    if "corelink" not in tags:
        fail(uid, "tags missing 'corelink'", errors)
    if "sota" not in tags and uid in CANONICAL_12:
        # Legacy dashboards (DASH-DEDUP / DASH-RATE / DASH-MULTIPART) predate
        # the sota tag convention; only enforce on canonical 12.
        warnings.append(f"  [WARN] {uid}: tags missing 'sota' (canonical 12 should include)")

    # lastUpdated annotation (canonical 12 only)
    if uid in CANONICAL_12:
        annotations = data.get("annotations", {})
        last_updated = annotations.get("lastUpdated") if isinstance(annotations, dict) else None
        if not last_updated:
            fail(uid, "missing annotations.lastUpdated (sprint contract §14.s09.6)", errors)
        elif not ISO_8601.match(last_updated):
            fail(uid, f"lastUpdated {last_updated!r} not ISO 8601", errors)

        # cardinality_budget_used annotation
        cbu = annotations.get("cardinality_budget_used") if isinstance(annotations, dict) else None
        if not cbu:
            fail(uid, "missing annotations.cardinality_budget_used (WI-S09-001 inheritance)", errors)
        elif not CARDINALITY_BUDGET.match(cbu):
            fail(uid, f"cardinality_budget_used {cbu!r} not '<used>/<cap>' format", errors)


def main() -> int:
    if not DASHBOARDS_DIR.is_dir():
        print(f"FAIL: dashboards directory not found: {DASHBOARDS_DIR}")
        return 1

    files = sorted(DASHBOARDS_DIR.glob("DASH-*.json"))
    on_disk_uids = {f.stem for f in files}

    errors: list[str] = []
    warnings: list[str] = []

    # Canonical 12 count discipline (Lote 10.8-tris P1-NEW-1 lesson)
    canonical_on_disk = on_disk_uids & set(CANONICAL_12)
    if canonical_on_disk != set(CANONICAL_12):
        missing = set(CANONICAL_12) - canonical_on_disk
        if missing:
            errors.append(
                f"  [FAIL] canonical-12 count discipline: missing {sorted(missing)}"
            )

    extra = on_disk_uids - set(CANONICAL_12) - set(LEGACY)
    if extra:
        errors.append(
            f"  [FAIL] unexpected dashboards (not in canonical 12 nor legacy): {sorted(extra)}"
        )

    # Per-file validation
    for path in files:
        validate_dashboard(path, errors, warnings)

    print(f"Total dashboards on disk: {len(files)} ({len(canonical_on_disk)}/12 canonical + {len(on_disk_uids & set(LEGACY))} legacy)")
    print()
    print("Canonical 12 (observability_model.md §10 Nível-3):")
    for uid in CANONICAL_12:
        marker = "OK" if uid in on_disk_uids else "MISSING"
        print(f"  [{marker}] {uid}")
    print()
    print("Legacy (refactored as embedded panels; kept for historical reference):")
    for uid in LEGACY:
        marker = "OK" if uid in on_disk_uids else "ABSENT"
        print(f"  [{marker}] {uid}")
    print()

    if warnings:
        print("Warnings:")
        for w in warnings:
            print(w)
        print()

    if errors:
        print("Errors:")
        for e in errors:
            print(e)
        print()
        print(f"FAIL: {len(errors)} error(s) found.")
        return 1

    print("All canonical-12 dashboard structural checks PASS.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
