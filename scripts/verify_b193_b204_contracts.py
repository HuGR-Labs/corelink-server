#!/usr/bin/env python3
"""Fail-closed static contract verifier for B-193..B-204.

This verifier is intentionally local and read-only.  It checks the source
ordering and wiring that unit tests cannot prove after a careless mutation;
missing evidence is an error, never an implicit pass.
"""

from __future__ import annotations

import sys
import re
from pathlib import Path


def require(text: str, needle: str, label: str, errors: list[str]) -> None:
    if needle not in text:
        errors.append(f"{label}: missing {needle!r}")


def forbid(text: str, needle: str, label: str, errors: list[str]) -> None:
    if needle in text:
        errors.append(f"{label}: stale/unqualified claim {needle!r}")


def provisioned_set(text: str, label: str, errors: list[str]) -> set[str]:
    """Extract the executable PROVISIONED_MACROS initializer, fail closed."""
    match = re.search(r"PROVISIONED_MACROS.{0,500}?=\s*(?:new Set[^[]*)?\[([^]]*)\]", text, re.S)
    if not match:
        errors.append(f"{label}: executable PROVISIONED_MACROS initializer missing")
        return set()
    values = set(re.findall(r'"([a-z]+)"', match.group(1)))
    if not values:
        errors.append(f"{label}: executable PROVISIONED_MACROS initializer is empty")
    return values


def verify(root: Path) -> list[str]:
    errors: list[str] = []
    def read(rel: str) -> str:
        path = root / rel
        if not path.is_file():
            errors.append(f"missing evidence file: {rel}")
            return ""
        return path.read_text(encoding="utf-8")

    # B126 split the OCI upload implementation and its focal tests into part2
    # fragments. Read the executable fragment, not the facade prefix.
    oci = read("crates/corelink-container/src/routes/oci/b126_m2_impl_01_part2.rs")
    oci_tests = read("crates/corelink-container/src/routes/oci/b126_m2_test_1_1_part2.rs")
    finalize = oci[oci.find("fn finalize_upload") :]
    if not finalize:
        errors.append("B-193: finalize_upload implementation missing")
    elif (
        finalize.find("d.verify_against_bytes(&session.buf)") < 0
        or finalize.find("d.verify_against_bytes(&session.buf)") > finalize.find("self.moat")
    ):
        errors.append("B-193: digest verification is not before the R2 persist")
    require(oci_tests, "finalize_rejects_digest_lie_and_persists_nothing", "B-193", errors)

    status = read("crates/corelink-adapter-host/src/oci/server/core.rs")
    require(status, "OciAdapterError::Cas(_) | OciAdapterError::Kv(_) => StatusCode::SERVICE_UNAVAILABLE", "B-194", errors)
    require(status, "live_storage_failures_are_retryable_503s", "B-194", errors)

    migrations = sorted((root / "migrations/d1").glob("0044_*.sql"))
    names = {p.name for p in migrations}
    if names != {"0044_drata_evidence_sent.sql", "0044_stripe_webhook_events_processed.sql"}:
        errors.append("B-195: historical 0044 set changed or is missing")
    checker = read("scripts/check_migration_prefixes.py")
    require(checker, "GRANDFATHERED", "B-195", errors)
    require(checker, "actual == allowed", "B-195", errors)

    cron = read("apps/signup-worker/src/webhooks/dsr_verify_cron.ts")
    if cron.count("LIMIT ?3") < 2:
        errors.append("B-196: both DSR source scans must have LIMIT ?3")
    require(cron, "requested_at >= ?2", "B-200", errors)
    require(cron, "SWEEP_BATCH_LIMIT", "B-196", errors)
    require(cron, "invalid/expired requested anchor", "B-200", errors)

    residency = read("crates/corelink-container/src/routes/residency.rs")
    require(residency, 'container_colo == "iad"', "B-197", errors)
    require(residency, "colo_matches_macro", "B-199", errors)
    require(residency, "decision_absent_header_only_allows_on_iad", "B-197", errors)

    worker_map = read("worker/src/region-map.ts")
    clerk = read("apps/signup-worker/src/webhooks/clerk_identity.ts")
    rust_map = read("crates/corelink-container/src/storage/region_map.rs")
    # Signup now imports the canonical set through clerk_identity.ts; the
    # split facade clerk.ts is only a re-export and must not grow a second set.
    clerk_identity = clerk
    for label, text in (("B-198", worker_map), ("B-198", clerk_identity), ("B-203", rust_map)):
        require(text, '"apac"', label, errors)
        require(text, '"weur"', label, errors)
        actual = provisioned_set(text, label, errors)
        if actual != {"wnam", "enam", "weur", "apac"}:
            errors.append(
                f"{label}: provisioned set must be {{wnam,enam,weur,apac}}, got {sorted(actual)}"
            )
        require(text, "nrt", label, errors)
    for text, label in ((worker_map, "B-198"), (rust_map, "B-203"), (clerk_identity, "B-198")):
        for stale in (
            "apac → nrt   (Asia-Pacific — Tokyo; NOT provisioned",
            "Provisioned = { wnam, enam, weur }",
            "Provisioned = `{wnam, enam, weur}`",
            "PROVISIONED_MACROS = {wnam, enam, weur}",
        ):
            forbid(text, stale, label, errors)
    require(worker_map, "coloMatchesMacro", "B-199", errors)
    require(rust_map, "colo_matches_macro", "B-199", errors)
    for stale in (
        "provisioned macro (wnam/enam/weur/sam)",
        "REJECT unprovisioned macro regions (apac/afr today)",
    ):
        forbid(clerk, stale, "B-198", errors)

    wp4 = read("docs/design/2026-08-17-wp4-apac-physical-plan.md")
    require(wp4, "Status: EXECUTED", "B-198", errors)
    require(wp4, "current provisioned contract is `{wnam, enam, weur, apac}`", "B-198", errors)
    require(wp4, "`corelink-cas-apac` is APAC-located and bound", "B-198", errors)
    for stale in (
        "Status: PLAN (self-reviewed; ready to execute",
        "PROVISIONED_MACROS = {wnam, enam, weur}",
        "routable-but-NOT-provisioned (signup",
        "exist, are `location=ENAM`, EMPTY (0 objects), and are",
        "Add `apac` to `PROVISIONED_MACROS`",
        "Verdict: APPROVED to execute",
    ):
        forbid(wp4, stale, "B-198", errors)

    okf_adr = read("docs/knowledge/adr/adr-s14-002-region-pinning-enforcement.md")
    require(okf_adr, "current provisioned contract is `{wnam, enam, weur, apac}`", "B-198", errors)
    require(okf_adr, "APAC-located `nrt` bucket", "B-198", errors)
    for stale in (
        "At launch CAS is a **single US bucket**",
        "physical at-rest residency is single-bucket-US",
        "`apac/afr` regions are valid",
    ):
        forbid(okf_adr, stale, "B-198", errors)

    # B-198's customer-facing legal corpus and its generated OKF mirror must
    # describe the same provisioned topology.  Checking only executable maps
    # is insufficient: stale legal prose can still make an unavailable SAM
    # region a contractual promise.  The exact positive sentences also make
    # a missing/stale generated index fail closed rather than silently drift.
    source_adr = read("specs/03_architecture/adrs/ADR-S14-008-dpa-amendment-schrems-ii-tia-legal-externo.md")
    require(source_adr, "the 4 provisioned regions `{wnam, enam, weur, apac}`", "B-198", errors)
    require(source_adr, "APAC (`apac`) is pinned to Tokyo (`nrt`)", "B-198", errors)
    forbid(source_adr, "4 enumerated regions (WNAM/ENAM/WEUR/SAM)", "B-198", errors)

    adr_s14_001 = read("docs/knowledge/adr/adr-s14-001-multi-region-terraform-module.md")
    require(adr_s14_001, "current provisioned contract is `{wnam, enam, weur, apac}`", "B-198", errors)
    require(adr_s14_001, "with `apac → nrt`", "B-198", errors)
    require(adr_s14_001, "`sam` and `afr` are not provisioned", "B-198", errors)
    for stale in (
        "PROVISIONED_MACROS = {wnam, enam, weur}",
        "four production regions stood up (WNAM/ENAM/WEUR/SAM)",
        "US-only-deployed",
    ):
        forbid(adr_s14_001, stale, "B-198", errors)

    tia = read("legal/tia-template.md")
    require(tia, "4 provisioned regions (WNAM/ENAM/WEUR/APAC)", "B-198", errors)
    require(tia, "APAC tenants are pinned to Tokyo (`nrt`)", "B-198", errors)
    require(tia, "`SAM` is not provisioned and is not promised", "B-198", errors)
    for stale in (
        "Brazil (SAM Region — LGPD Context)",
        "Brazilian data stored in sa-east infrastructure",
        "Supplementary measures in Section 4 apply equally to SAM region transfers",
        "4 enumerated regions (WNAM/ENAM/WEUR/SAM)",
    ):
        forbid(tia, stale, "B-198", errors)

    sub_processors = read("legal/dpa/SUB-PROCESSOR-COMMITMENTS.md")
    require(sub_processors, "(WNAM / ENAM / WEUR / APAC)", "B-198", errors)
    require(sub_processors, "APAC tenants are pinned to Tokyo (`nrt`)", "B-198", errors)
    require(sub_processors, "`SAM` is not provisioned and is not promised", "B-198", errors)
    forbid(sub_processors, "(WNAM / ENAM / WEUR / SAM)", "B-198", errors)

    canonical_okf_claim = (
        "The current provisioned residency contract is `{wnam, enam, weur, apac}`, "
        "with `apac → nrt`; `sam` and `afr` are not provisioned and are rejected at signup."
    )
    okf_source = read("docs/knowledge/adr/adr-s14-008-dpa-amendment-schrems-ii-tia-legal-externo.md")
    require(okf_source, canonical_okf_claim, "B-198", errors)
    okf_index = read("docs/okf-wiki-site/index.html")
    require(okf_index, canonical_okf_claim, "B-198", errors)
    require(okf_index, "current provisioned contract is `{wnam, enam, weur, apac}` with `apac → nrt`", "B-198", errors)
    require(okf_index, "`sam` and `afr` are not provisioned", "B-198", errors)

    ac = read("crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs")
    require(ac, "list_objects_v2", "B-201", errors)
    require(ac, "count_ac_remaining", "B-201", errors)
    require(ac, "no `ac_meta` D1 index row", "B-201", errors)

    # The storage facade is an include! composition point; the physical
    # bucket mapping lives in the CAS builder fragment.
    r2 = read("crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs")
    for mapping in ("\"iad\" => \"corelink-cas-prod\"", "\"lhr\" => \"corelink-cas-eu\"", "\"nrt\" => \"corelink-cas-apac\""):
        require(r2, mapping, "B-202", errors)
    require(r2, "validate_cas_bucket_for_region", "B-202", errors)

    fips = root / "docs/compliance/byok-fips-evidence.md"
    if not fips.is_file():
        errors.append("B-204: DPA-referenced FIPS evidence document is missing")
    else:
        fips_text = fips.read_text(encoding="utf-8")
        require(fips_text, "not yet available", "B-204", errors)
        require(fips_text, "no FIPS-validated", "B-204", errors)
    dpa = read("legal/dpa-residency-amendment.md")
    require(dpa, "CURRENT LAUNCH BOUNDARY (DD-051 / B-204)", "B-204", errors)
    require(dpa, "BYOK is not enabled or provisioned", "B-204", errors)
    require(dpa, "four-provider reference below is a **future-state design requirement**", "B-204", errors)
    for stale in (
        "Customer may choose any of the following FIPS 140-2 / FIPS 140-3 validated KMS providers",
        "FIPS-validated BYOK (4 providers)",
        "Customer revokes CMK (BYOK kill switch)",
        "BYOK crypto-erase mechanism (Section 9.5 / Section 10).",
        "Concludes that BYOK customer-controlled CMK",
        "CoreLink cannot decrypt blob content without Customer-provided CMK",
    ):
        forbid(dpa, stale, "B-204", errors)
    return errors


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    errors = verify(root)
    if errors:
        print("FAIL: B-193..B-204 contract evidence is incomplete", file=sys.stderr)
        print("\n".join(f"- {e}" for e in errors), file=sys.stderr)
        return 1
    print("OK: B-193..B-204 source contracts and fail-closed evidence verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
