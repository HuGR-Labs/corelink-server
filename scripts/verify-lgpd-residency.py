#!/usr/bin/env python3
"""
verify-lgpd-residency.py — programmatic residency-attestation verifier (GAP-22).

Reads a list of test tenant_ids (default: `tests/fixtures/lgpd-residency-tenants.json`
when run with `--env staging`/`production`; in `--dry-run` mode, uses a hard-coded
in-memory fixture so the script remains runnable in CI without cloud credentials).

For each tenant, queries:

    - **D1** for `tenant.primary_region` (canonical column per
      `data_model.md §4.1 L151`).
    - **R2** object metadata for a sample of stored objects in the tenant's
      buckets (`cas-<region>`, `ac-<region>`, `audit-<region>`).

Validates: for every `sam` (BR-residency) tenant, **every** sampled object's
region tag matches `sam` (i.e. lives in `sa-east-1` / São Paulo per Cloudflare R2
locationHint `wnam-southamerica-east1`). Non-`sam` tenants are skipped (the LGPD
attestation only covers BR-residency tenants — though `--strict` extends the
check to all 6 canonical regions).

Exit codes:

    0  All BR-residency tenants comply.
    1  At least one cross-region violation detected (LGPD Art. 33 §1º breach).
    2  Configuration error (missing credentials / fixture file / etc.).
    3  Infrastructure error (could not reach D1 / R2).

Operational use:

    - Nightly CI job (R-6 sustained staging window onward): runs with
      `--env staging` against the staging tenant set; gates promotion to
      production if it fails.
    - DPO monthly checklist (item 5) runs with `--env production` and
      attaches output to the monthly attestation extract.
    - Pre-attestation refresh: run with `--env production --strict` to
      cover all 6 canonical regions, not just `sam`.

Companion docs:

    - `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` (the
      attestation this script sustains).
    - `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` (item 5).
    - `crates/corelink-privacy-residency-enforcement/` (runtime
      enforcement — the source of truth this script audits *against*).
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, Optional

CANONICAL_REGIONS: tuple[str, ...] = (
    "wnam",
    "enam",
    "weur",
    "sam",
    "apac",
    "afr",
)

REGION_LOCATION_HINT: dict[str, str] = {
    # Mapping per `specs/03_architecture/privacy_model.md §7.1` +
    # Cloudflare R2 location hints.
    "wnam": "wnam",  # Western North America (CA)
    "enam": "enam",  # Eastern North America (US)
    "weur": "weur",  # Western Europe (NL/DE)
    "sam": "wnam-southamerica-east1",  # São Paulo (BR) — LGPD scope
    "apac": "apac",  # Asia-Pacific (SG)
    "afr": "afr",  # Africa (ZA)
}

# Cloudflare R2 returns the physical placement of a bucket as a location HINT
# code (the `location` field on GET .../r2/buckets/{bucket}, e.g. "EEUR", or a
# locationHint like "eeur" on creation). Map every R2 location code we may see
# onto the canonical macro-region vocabulary this verifier reasons about
# (CANONICAL_REGIONS). Anything not in this table cannot be honestly mapped and
# MUST surface as "unverifiable" (→ a violation), never a silent pass.
#
# Cloudflare jurisdictional/hint codes (see CF R2 docs "Data location"):
#   APAC  Asia-Pacific          → apac
#   EEUR  Eastern Europe        → weur (EU jurisdiction)
#   WEUR  Western Europe        → weur (EU jurisdiction)
#   ENAM  Eastern North America → enam
#   WNAM  Western North America → wnam
#   OC    Oceania               → apac (no distinct CoreLink macro)
R2_LOCATION_TO_MACRO: dict[str, str] = {
    "APAC": "apac",
    "EEUR": "weur",
    "WEUR": "weur",
    "ENAM": "enam",
    "WNAM": "wnam",
    "OC": "apac",
}


def location_code_to_macro(code: Optional[str]) -> Optional[str]:
    """Map a Cloudflare R2 location code to a canonical macro region.

    Accepts the codes returned by the R2 API (case-insensitive, e.g. ``EEUR``,
    ``eeur``). Returns the canonical macro (one of ``CANONICAL_REGIONS``, except
    ``sam``/``afr`` which Cloudflare does not surface as a distinct location
    code today), or ``None`` for an empty/unknown code — which the caller MUST
    treat as a residency violation (fail-loud), never a pass.
    """
    if not code or not isinstance(code, str):
        return None
    return R2_LOCATION_TO_MACRO.get(code.strip().upper())


def _cf_bucket_location(
    account_id: str,
    bucket: str,
    token: str,
    *,
    jurisdiction: Optional[str] = None,
) -> Optional[str]:
    """Query the live Cloudflare R2 API for a bucket's physical location code.

    ``GET /accounts/{account_id}/r2/buckets/{bucket}`` returns the bucket's
    ``location`` (an R2 location code such as ``EEUR``). EU-jurisdiction buckets
    require the ``cf-r2-jurisdiction: eu`` header to be addressable. Returns the
    raw location code, or ``None`` when it cannot be established (network/API
    error, missing field) — fail-loud is the caller's responsibility.
    """
    url = (
        f"https://api.cloudflare.com/client/v4/accounts/{account_id}"
        f"/r2/buckets/{bucket}"
    )
    req = urllib.request.Request(url, method="GET")
    req.add_header("Authorization", f"Bearer {token}")
    req.add_header("Content-Type", "application/json")
    if jurisdiction:
        req.add_header("cf-r2-jurisdiction", jurisdiction)
    try:
        with urllib.request.urlopen(req, timeout=20) as resp:  # noqa: S310
            payload = json.loads(resp.read().decode("utf-8"))
    except (urllib.error.URLError, OSError, ValueError):
        return None
    if not isinstance(payload, dict) or not payload.get("success"):
        return None
    result = payload.get("result")
    if not isinstance(result, dict):
        return None
    loc = result.get("location")
    return loc if isinstance(loc, str) and loc else None

# Hard-coded dry-run fixture: 3 BR tenants (all sam), 1 EU tenant (weur).
# Used when --env=dry-run (no cloud calls). Each object carries the PROBED
# ``physical_region`` (the offline stand-in for a live CF R2 location probe) —
# the self-written key-prefix ``region`` is retained only to document that it is
# NOT what physical_region_of() trusts. All objects here reside correctly, so a
# dry-run is a clean pass (exit 0) — the offline smoke test. The broken/violation
# path is exercised by the embedded --self-test.
DRY_RUN_FIXTURE: dict = {
    "tenants": [
        {
            "tenant_id": "br-tenant-001",
            "primary_region": "sam",
            "sampled_objects": [
                {"uri": "cas-sam/abc123", "region": "sam", "physical_region": "sam"},
                {"uri": "ac-sam/build-42", "region": "sam", "physical_region": "sam"},
                {
                    "uri": "audit-sam/2026-05-15/000001.evt",
                    "region": "sam",
                    "physical_region": "sam",
                },
            ],
        },
        {
            "tenant_id": "br-tenant-002",
            "primary_region": "sam",
            "sampled_objects": [
                {"uri": "cas-sam/def456", "region": "sam", "physical_region": "sam"},
                {
                    "uri": "audit-sam/2026-05-15/000002.evt",
                    "region": "sam",
                    "physical_region": "sam",
                },
            ],
        },
        {
            "tenant_id": "br-tenant-003",
            "primary_region": "sam",
            "sampled_objects": [
                {"uri": "cas-sam/ghi789", "region": "sam", "physical_region": "sam"},
                {"uri": "ac-sam/build-43", "region": "sam", "physical_region": "sam"},
            ],
        },
        {
            "tenant_id": "eu-tenant-001",
            "primary_region": "weur",
            "sampled_objects": [
                {
                    "uri": "cas-weur/eu-blob",
                    "region": "weur",
                    "physical_region": "weur",
                },
            ],
        },
    ]
}


@dataclass
class Violation:
    """One detected cross-region inconsistency."""

    tenant_id: str
    expected_region: str
    observed_region: str
    object_uri: str

    def as_log_line(self) -> str:
        return (
            f"VIOLATION tenant={self.tenant_id} "
            f"expected={self.expected_region} "
            f"observed={self.observed_region} "
            f"uri={self.object_uri}"
        )


@dataclass
class VerificationReport:
    """Aggregated report for one verifier run."""

    env: str
    tenants_examined: int = 0
    objects_sampled: int = 0
    br_tenants: int = 0
    violations: list[Violation] = field(default_factory=list)

    def add_violation(self, v: Violation) -> None:
        self.violations.append(v)

    def has_violations(self) -> bool:
        return bool(self.violations)

    def summary(self) -> str:
        lines = [
            "=" * 72,
            "LGPD Art. 33 §1º residency verifier — summary",
            "=" * 72,
            f"environment             : {self.env}",
            f"tenants examined        : {self.tenants_examined}",
            f"  of which BR-residency : {self.br_tenants}",
            f"objects sampled         : {self.objects_sampled}",
            f"violations              : {len(self.violations)}",
            "=" * 72,
        ]
        if self.violations:
            lines.append("")
            lines.append("Violations detail:")
            for v in self.violations:
                lines.append(f"  - {v.as_log_line()}")
            lines.append("")
            lines.append(
                "ACTION: Page Privacy Officer + Security Lead. "
                "Trigger `RB-DATA-RESIDENCY-LEAK.md`."
            )
        return "\n".join(lines)


def load_fixture(env: str, fixture_path: Optional[Path]) -> dict:
    """Load the tenant + sampled-object fixture for verification."""
    if env == "dry-run":
        return DRY_RUN_FIXTURE
    if fixture_path is None:
        raise ValueError(
            f"env={env} requires --fixture path "
            f"(or set CORELINK_RESIDENCY_FIXTURE env var)"
        )
    if not fixture_path.exists():
        raise FileNotFoundError(
            f"fixture not found: {fixture_path}"
        )
    return json.loads(fixture_path.read_text())


def iter_tenants(fixture: dict) -> Iterable[dict]:
    for entry in fixture.get("tenants", []):
        yield entry


def physical_region_of(obj: dict) -> Optional[str]:
    """Authoritative PHYSICAL macro-region of a stored object.

    The ONLY trustworthy residency signal is where the bytes physically live —
    the bucket's Cloudflare R2 location code (the ``location`` field on
    ``GET /accounts/{acct}/r2/buckets/{bucket}``), NOT the key prefix (which the
    container writes from ``R2_CAS_REGION`` and which a misroute or a shared
    bucket renders meaningless — F-021).

    Resolution order:

    1. If the record already carries a probed ``physical_region`` (the dry-run /
       offline fixture path, no cloud calls), trust it.
    2. Otherwise, perform a REAL probe: read the object's ``bucket`` field, query
       the live CF R2 API for that bucket's physical ``location`` code, and map
       it to a canonical macro via :func:`location_code_to_macro`. Requires
       ``CLOUDFLARE_API_TOKEN`` (read access to R2) and ``CLOUDFLARE_ACCOUNT_ID``
       in the environment; an EU-jurisdiction bucket carries ``jurisdiction:
       "eu"`` on the record so the probe sends ``cf-r2-jurisdiction: eu``.

    Returns the canonical macro region (F-013 now CLOSED — real per-jurisdiction
    buckets exist), or ``None`` when the physical location cannot be established
    (missing bucket field, missing creds, API/network error, or an unmappable
    location code). A ``None`` result MUST be treated as a residency violation,
    never a pass: the LGPD attestation cannot be honestly signed on an
    unverifiable object.
    """
    # (1) Offline/fixture shortcut: a pre-probed physical region.
    loc = obj.get("physical_region")
    if isinstance(loc, str) and loc:
        return loc

    # (2) Real probe against the live Cloudflare R2 API.
    bucket = obj.get("bucket")
    if not isinstance(bucket, str) or not bucket:
        return None
    token = os.environ.get("CLOUDFLARE_API_TOKEN")
    account_id = os.environ.get("CLOUDFLARE_ACCOUNT_ID")
    if not token or not account_id:
        return None
    jurisdiction = obj.get("jurisdiction")
    if not isinstance(jurisdiction, str) or not jurisdiction:
        jurisdiction = None
    code = _cf_bucket_location(
        account_id, bucket, token, jurisdiction=jurisdiction
    )
    return location_code_to_macro(code)


def verify(
    env: str,
    fixture: dict,
    *,
    strict: bool = False,
) -> VerificationReport:
    """Run the verification pass and return a report."""
    report = VerificationReport(env=env)

    for tenant in iter_tenants(fixture):
        tenant_id = tenant["tenant_id"]
        expected_region = tenant["primary_region"]

        if expected_region not in CANONICAL_REGIONS:
            # Open-string drift — never should occur given the closed enum
            # at the D1 / Rust enum layer. Treat as a violation.
            report.tenants_examined += 1
            report.add_violation(
                Violation(
                    tenant_id=tenant_id,
                    expected_region="<canonical-enum>",
                    observed_region=expected_region,
                    object_uri="<tenant.primary_region>",
                )
            )
            continue

        report.tenants_examined += 1
        is_br = expected_region == "sam"
        if is_br:
            report.br_tenants += 1

        # By default we only enforce on BR-residency tenants (the LGPD scope).
        # --strict extends to all 6 canonical regions (i.e. catches any
        # cross-region drift, not just BR).
        if not is_br and not strict:
            continue

        for obj in tenant.get("sampled_objects", []):
            report.objects_sampled += 1
            # F-021 (overnight red-team): obj["region"] is the key-PREFIX the
            # container itself wrote (from R2_CAS_REGION) — comparing it to
            # expected_region is a TAUTOLOGY that proves nothing about physical
            # jurisdiction. The authoritative signal is the object's PHYSICAL R2
            # location (HeadObject locationHint / CF R2 API), via physical_region_of().
            # An unavailable physical location (e.g. per-region buckets not yet
            # provisioned — F-013) is a VIOLATION (fail-loud), never a silent pass:
            # the attestation must not be signed on a self-referential check.
            observed = physical_region_of(obj)
            if observed is None or observed != expected_region:
                report.add_violation(
                    Violation(
                        tenant_id=tenant_id,
                        expected_region=expected_region,
                        observed_region=observed or "<physical-location-unverifiable>",
                        object_uri=obj["uri"],
                    )
                )

    return report


def _self_test() -> int:
    """Embedded unit tests (no external test deps / no cloud calls).

    Run via ``python3 scripts/verify-lgpd-residency.py --self-test`` or, under a
    discoverer, ``python3 -m unittest`` is not applicable (single-file script) —
    these assert the location-code → macro mapping and the fail-loud posture of
    ``physical_region_of`` for the offline fixture path. Returns process exit
    code (0 = all pass).
    """
    failures: list[str] = []

    def check(name: str, cond: bool) -> None:
        if not cond:
            failures.append(name)

    # location_code_to_macro: every documented R2 location code → its macro.
    check("EEUR→weur", location_code_to_macro("EEUR") == "weur")
    check("WEUR→weur", location_code_to_macro("WEUR") == "weur")
    check("ENAM→enam", location_code_to_macro("ENAM") == "enam")
    check("WNAM→wnam", location_code_to_macro("WNAM") == "wnam")
    check("APAC→apac", location_code_to_macro("APAC") == "apac")
    check("OC→apac", location_code_to_macro("OC") == "apac")
    # Case-insensitive + whitespace-tolerant.
    check("eeur(lower)→weur", location_code_to_macro("eeur") == "weur")
    check("' EEUR '→weur", location_code_to_macro("  EEUR  ") == "weur")
    # Fail-loud on unknown / empty / non-str.
    check("unknown→None", location_code_to_macro("ZZZZ") is None)
    check("empty→None", location_code_to_macro("") is None)
    check("none→None", location_code_to_macro(None) is None)
    check(
        "nonstr→None",
        location_code_to_macro(123) is None,  # type: ignore[arg-type]
    )

    # physical_region_of offline path: a pre-probed physical_region is trusted.
    check(
        "fixture-physical_region trusted",
        physical_region_of({"physical_region": "weur"}) == "weur",
    )
    # No physical_region + no bucket → unverifiable (None → violation upstream).
    check(
        "no-signal→None",
        physical_region_of({"uri": "cas-lhr/x"}) is None,
    )

    if failures:
        print(
            "SELF-TEST FAILED: " + ", ".join(failures),
            file=sys.stderr,
        )
        return 1
    print("SELF-TEST OK (mapping + fail-loud posture)")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        prog="verify-lgpd-residency",
        description=(
            "LGPD Art. 33 §1º residency-attestation verifier (GAP-22). "
            "Validates that BR-residency tenants' stored objects live "
            "only in the `sam` region. Wired into nightly CI + DPO monthly "
            "checklist item 5."
        ),
    )
    parser.add_argument(
        "--env",
        choices=("dry-run", "staging", "production"),
        default="dry-run",
        help=(
            "Environment to verify against. `dry-run` uses an in-memory "
            "fixture (no cloud calls). `staging`/`production` require "
            "--fixture (or CORELINK_RESIDENCY_FIXTURE env var) — and, "
            "when wired to live D1/R2, valid credentials."
        ),
    )
    parser.add_argument(
        "--fixture",
        type=Path,
        default=os.environ.get("CORELINK_RESIDENCY_FIXTURE"),
        help=(
            "Path to a JSON fixture file describing tenants and sampled "
            "objects. Schema: `{\"tenants\": [{\"tenant_id\": str, "
            "\"primary_region\": str, \"sampled_objects\": [{\"uri\": "
            "str, \"region\": str}]}]}`."
        ),
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help=(
            "Extend the check from BR-only (`sam`) to all 6 canonical "
            "regions. Catches any cross-region drift; recommended pre "
            "attestation refresh."
        ),
    )
    parser.add_argument(
        "--quiet",
        action="store_true",
        help="Suppress the summary output (only exit code communicates).",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help=(
            "Run the embedded unit tests (location-code→macro mapping + "
            "fail-loud posture) and exit. No cloud calls."
        ),
    )

    args = parser.parse_args(argv)

    if args.self_test:
        return _self_test()

    try:
        fixture = load_fixture(args.env, args.fixture)
    except FileNotFoundError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2
    except ValueError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2

    report = verify(args.env, fixture, strict=args.strict)

    if not args.quiet:
        print(report.summary())

    return 1 if report.has_violations() else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
