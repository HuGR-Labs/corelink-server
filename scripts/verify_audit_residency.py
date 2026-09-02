#!/usr/bin/env python3
"""Fail-closed auditor for the audit-outbox residency predicate (B-127).

The old ``INNER JOIN`` check answered only for rows that still had a tenant.
This tool instead reads one aggregate over the *full* ``audit_outbox``
population and partitions every row as satisfied, violated, or unevaluable.
An unevaluable row (including a retained DSR-erasure orphan) is evidence that
the predicate cannot be proven; it is a failing result, never a clean result.

The command is read-only.  Use ``--input`` to validate a saved Cloudflare D1
JSON response, or provide credentials plus ``--database-id`` for one live
read.  It never loads, writes, or prints credentials.

Exit status: 0 compliant; 1 violated or unevaluable; 2 indeterminate.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, NoReturn


CANONICAL_REGIONS = ("wnam", "enam", "weur", "sam", "apac", "afr")
_REGIONS_SQL = ", ".join(f"'{region}'" for region in CANONICAL_REGIONS)

# ``EXISTS`` is intentional: an erased tenant can have one row per erased
# backend, and joining that table would multiply audit rows and corrupt the
# denominator this control is protecting.
RESIDENCY_SQL = f"""
WITH classified AS (
    SELECT
        a.tenant_id,
        a.region AS audit_region,
        t.tenant_id AS joined_tenant_id,
        CASE
            WHEN t.tenant_id IS NULL
              OR a.region IS NULL
              OR t.primary_region IS NULL
              OR a.region NOT IN ({_REGIONS_SQL})
              OR t.primary_region NOT IN ({_REGIONS_SQL})
                THEN 'unevaluable'
            WHEN a.region = t.primary_region THEN 'satisfied'
            ELSE 'violated'
        END AS residency_state,
        CASE WHEN t.tenant_id IS NULL
                  AND EXISTS (
                      SELECT 1 FROM dsr_erasure_log AS d
                      WHERE d.tenant_id = a.tenant_id
                  )
             THEN 1 ELSE 0 END AS erased_orphan
    FROM audit_outbox AS a
    LEFT JOIN tenant AS t ON t.tenant_id = a.tenant_id
)
SELECT
    COUNT(*) AS total_rows,
    SUM(residency_state = 'satisfied') AS satisfied_rows,
    SUM(residency_state = 'violated') AS violated_rows,
    SUM(residency_state = 'unevaluable') AS unevaluable_rows,
    SUM(joined_tenant_id IS NULL) AS orphan_rows,
    COUNT(DISTINCT CASE WHEN joined_tenant_id IS NULL THEN tenant_id END)
        AS orphan_tenants,
    SUM(erased_orphan) AS erased_orphan_rows,
    COUNT(DISTINCT CASE WHEN erased_orphan = 1 THEN tenant_id END)
        AS erased_orphan_tenants,
    SUM(joined_tenant_id IS NULL) - SUM(erased_orphan)
        AS unexplained_orphan_rows,
    COUNT(DISTINCT CASE WHEN joined_tenant_id IS NULL AND erased_orphan = 0
                        THEN tenant_id END) AS unexplained_orphan_tenants,
    SUM(audit_region = 'weur') AS weur_audit_rows,
    SUM(audit_region = 'weur' AND joined_tenant_id IS NULL) AS weur_orphan_rows,
    (SELECT COUNT(*) FROM tenant WHERE primary_region = 'weur') AS weur_tenants,
    (SELECT COUNT(*) FROM dsr_erasure_log) AS erasure_log_rows
FROM classified
""".strip()

COUNT_FIELDS = (
    "total_rows",
    "satisfied_rows",
    "violated_rows",
    "unevaluable_rows",
    "orphan_rows",
    "orphan_tenants",
    "erased_orphan_rows",
    "erased_orphan_tenants",
    "unexplained_orphan_rows",
    "unexplained_orphan_tenants",
    "weur_audit_rows",
    "weur_orphan_rows",
    "weur_tenants",
    "erasure_log_rows",
)


class Indeterminate(ValueError):
    """Evidence is absent, malformed, partial, or otherwise not trustworthy."""


@dataclass(frozen=True)
class Counts:
    total_rows: int
    satisfied_rows: int
    violated_rows: int
    unevaluable_rows: int
    orphan_rows: int
    orphan_tenants: int
    erased_orphan_rows: int
    erased_orphan_tenants: int
    unexplained_orphan_rows: int
    unexplained_orphan_tenants: int
    weur_audit_rows: int
    weur_orphan_rows: int
    weur_tenants: int
    erasure_log_rows: int


def _count(row: dict[str, Any], field: str) -> int:
    value = row.get(field)
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise Indeterminate(f"D1 result field {field!r} is missing or not a non-negative integer")
    return value


def parse_d1_response(payload: object) -> Counts:
    """Parse exactly one successful D1 query result; never default missing counts."""
    if not isinstance(payload, dict) or payload.get("success") is not True:
        raise Indeterminate("D1 response is missing success=true")
    result = payload.get("result")
    if not isinstance(result, list) or len(result) != 1 or not isinstance(result[0], dict):
        raise Indeterminate("D1 response must contain exactly one result set")
    result_set = result[0]
    if result_set.get("success") is False:
        raise Indeterminate("D1 reported an unsuccessful result set")
    rows = result_set.get("results")
    if not isinstance(rows, list) or len(rows) != 1 or not isinstance(rows[0], dict):
        raise Indeterminate("D1 response must contain exactly one aggregate row")
    row = rows[0]
    return Counts(**{field: _count(row, field) for field in COUNT_FIELDS})


def assess(counts: Counts, *, environment: str) -> tuple[str, str]:
    """Return the explicit state and reason after checking partition invariants."""
    if counts.total_rows == 0:
        raise Indeterminate("audit_outbox population is empty; no residency claim is proven")
    if counts.satisfied_rows + counts.violated_rows + counts.unevaluable_rows != counts.total_rows:
        raise Indeterminate("three-state partition does not equal the full audit_outbox population")
    if counts.orphan_rows > counts.unevaluable_rows:
        raise Indeterminate("orphan rows escaped the unevaluable bucket")
    if counts.erased_orphan_rows > counts.orphan_rows:
        raise Indeterminate("erased-orphan rows exceed the orphan denominator")
    if counts.unexplained_orphan_rows != counts.orphan_rows - counts.erased_orphan_rows:
        raise Indeterminate("unexplained-orphan count does not conserve the orphan denominator")
    if counts.weur_orphan_rows > counts.weur_audit_rows:
        raise Indeterminate("weur orphan count exceeds the weur audit population")
    if environment == "production" and counts.erasure_log_rows == 0:
        raise Indeterminate("production DSR-erasure control is empty; orphan classification is unproven")
    if counts.violated_rows or counts.unevaluable_rows:
        return (
            "FAILED",
            "known violations or unevaluable rows exist; neither may be reported compliant",
        )
    return ("COMPLIANT", "all rows are in the satisfied bucket")


def _live_payload(account_id: str, token: str, database_id: str, timeout: int) -> object:
    request = urllib.request.Request(
        f"https://api.cloudflare.com/client/v4/accounts/{account_id}/d1/database/{database_id}/query",
        data=json.dumps({"sql": RESIDENCY_SQL}).encode("utf-8"),
        method="POST",
        headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:  # noqa: S310
            return json.loads(response.read().decode("utf-8"))
    except (urllib.error.URLError, urllib.error.HTTPError, OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise Indeterminate(f"live D1 query unavailable or malformed: {exc}") from exc


def _indeterminate(message: str) -> NoReturn:
    print(f"status=INDETERMINATE reason={message}", file=sys.stderr)
    raise SystemExit(2)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--environment", choices=("production", "staging", "test"), required=True)
    parser.add_argument("--input", type=Path, help="saved raw Cloudflare D1 JSON response")
    parser.add_argument("--database-id", help="D1 database UUID for a read-only live query")
    parser.add_argument("--account-id", default=os.environ.get("CLOUDFLARE_ACCOUNT_ID"))
    parser.add_argument("--api-token", default=os.environ.get("CLOUDFLARE_API_TOKEN"))
    parser.add_argument("--timeout-seconds", type=int, default=20)
    args = parser.parse_args(argv)

    if not 1 <= args.timeout_seconds <= 60:
        _indeterminate("timeout must be between 1 and 60 seconds")
    if args.input and args.database_id:
        _indeterminate("choose exactly one evidence source: --input or --database-id")
    if not args.input and not args.database_id:
        _indeterminate("missing evidence source: provide --input or --database-id")

    try:
        if args.input:
            payload = json.loads(args.input.read_text(encoding="utf-8"))
        else:
            if not args.account_id or not args.api_token:
                raise Indeterminate("live query requires account ID and API token")
            payload = _live_payload(args.account_id, args.api_token, args.database_id, args.timeout_seconds)
        counts = parse_d1_response(payload)
        state, reason = assess(counts, environment=args.environment)
    except (OSError, json.JSONDecodeError, Indeterminate) as exc:
        _indeterminate(str(exc))

    print(json.dumps({"environment": args.environment, "status": state, "reason": reason, "counts": asdict(counts)}, sort_keys=True))
    return 0 if state == "COMPLIANT" else 1


if __name__ == "__main__":
    raise SystemExit(main())
