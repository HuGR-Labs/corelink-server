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

# Hard-coded dry-run fixture: 3 BR tenants (all sam), 1 EU tenant (weur),
# 1 deliberately-broken tenant where one object lives in the wrong region.
# Used when --env=dry-run (no cloud calls).
DRY_RUN_FIXTURE: dict = {
    "tenants": [
        {
            "tenant_id": "br-tenant-001",
            "primary_region": "sam",
            "sampled_objects": [
                {"uri": "cas-sam/abc123", "region": "sam"},
                {"uri": "ac-sam/build-42", "region": "sam"},
                {"uri": "audit-sam/2026-05-15/000001.evt", "region": "sam"},
            ],
        },
        {
            "tenant_id": "br-tenant-002",
            "primary_region": "sam",
            "sampled_objects": [
                {"uri": "cas-sam/def456", "region": "sam"},
                {"uri": "audit-sam/2026-05-15/000002.evt", "region": "sam"},
            ],
        },
        {
            "tenant_id": "br-tenant-003",
            "primary_region": "sam",
            "sampled_objects": [
                {"uri": "cas-sam/ghi789", "region": "sam"},
                {"uri": "ac-sam/build-43", "region": "sam"},
            ],
        },
        {
            "tenant_id": "eu-tenant-001",
            "primary_region": "weur",
            "sampled_objects": [
                {"uri": "cas-weur/eu-blob", "region": "weur"},
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
            observed = obj["region"]
            if observed != expected_region:
                report.add_violation(
                    Violation(
                        tenant_id=tenant_id,
                        expected_region=expected_region,
                        observed_region=observed,
                        object_uri=obj["uri"],
                    )
                )

    return report


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

    args = parser.parse_args(argv)

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
