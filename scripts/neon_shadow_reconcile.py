#!/usr/bin/env python3
"""
neon_shadow_reconcile.py — daily reconciliation harness for the Neon
analytics shadow vs the canonical R2 NDJSON audit archive.

Driven by `.github/workflows/neon-shadow-reconcile-daily.yml`. For each
region in `--regions`:

  1. Looks up the per-region Neon DSN at `NEON_DB_URL_<REGION_UPPER>`
     (per `corelink-audit-chain::neon_shadow::real::EnvVarResolver`).
     A missing DSN is logged + the region is SKIPPED (not a failure
     — bring-up friendly).
  2. Queries the shadow row count for `--target-date` via the
     canonical SQL constant pinned by the Rust driver
     (`SQL_RECONCILE_COUNT`).
  3. Lists the R2 NDJSON archive for the same window via paginated
     Cloudflare API v4 (same harness as
     `.github/workflows/audit-chain-daily-verify.yml`).
  4. Compares the two counts. Non-zero diff fires a PagerDuty Events
     API v2 trigger with dedup_key=`neon-shadow-drift-<region>-<date>`.

Exit code:
  0  — every region either had clean reconciliation OR was skipped
       (no DSN configured).
  1  — at least one region surfaced a drift OR a connection error.

CLI:
  --regions REGIONS         Comma-separated region codes (lowercase
                            3-letter colocode; e.g. `iad,fra,gru`).
  --target-date YYYY-MM-DD  UTC date to reconcile.
  --bucket BUCKET           R2 bucket name (audit archive).
  --emit-pd                 When set, fire PagerDuty pages on drift /
                            error. Off in dry-run / local CLI use.
  --output-json PATH        Write a structured per-region report to
                            this path.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import sys
from dataclasses import asdict, dataclass
from typing import Optional


# ---------------------------------------------------------------------------
# SQL constant — single source of truth lives in the Rust driver
# (`crates/corelink-audit-chain/src/neon_shadow/real.rs::SQL_RECONCILE_COUNT`).
# Drift between this string + the Rust constant is caught by the unit
# test `sql_constants_pin_to_migration_schema` (Rust-side) + the
# `scripts/check_migrations_additive.py` validator (Python-side).
# ---------------------------------------------------------------------------
SQL_RECONCILE_COUNT = (
    "SELECT COUNT(*)::bigint AS cnt FROM audit_events_shadow "
    "WHERE event_time >= to_timestamp($1::double precision / 1000.0) "
    "  AND event_time <  to_timestamp($2::double precision / 1000.0)"
)


@dataclass
class RegionResult:
    """One region's reconcile outcome."""

    region: str
    skipped: bool
    skipped_reason: Optional[str]
    shadow_count: Optional[int]
    r2_count: Optional[int]
    drift: Optional[int]
    error: Optional[str]


def env_var_name(region: str) -> str:
    """Map a 3-letter region code to its canonical env var name."""
    return f"NEON_DB_URL_{region.upper()}"


def utc_day_window_ms(target_date: str) -> tuple[int, int]:
    """Return `[from_ms, to_ms)` UTC window for the given YYYY-MM-DD."""
    d = dt.datetime.strptime(target_date, "%Y-%m-%d").replace(
        tzinfo=dt.timezone.utc
    )
    from_ms = int(d.timestamp() * 1000)
    to_ms = int((d + dt.timedelta(days=1)).timestamp() * 1000)
    return from_ms, to_ms


def query_shadow_count(
    dsn: str, from_ms: int, to_ms: int
) -> int:
    """Run `SQL_RECONCILE_COUNT` against the per-region Neon project.

    Wraps the query in BEGIN / SELECT set_config('app.current_tenant', ...) /
    COMMIT mirrors `RealNeonShadowSink::reconcile_count` — BUT the
    reconcile cron operates as a service-role tenant that bypasses RLS
    for the cross-tenant aggregate (controlled by a `corelink_audit_reconcile`
    role pinned to the migration). The cron is the only consumer that
    legitimately sees aggregate row counts across tenants per region.
    """
    import psycopg  # type: ignore

    with psycopg.connect(dsn, autocommit=False) as conn:
        with conn.cursor() as cur:
            # The reconcile role is a strictly-read aggregator; the
            # tenant GUC is omitted by design (the role bypasses RLS
            # for `SELECT COUNT(*)` only). See migration §RLS notes.
            cur.execute(SQL_RECONCILE_COUNT, (from_ms, to_ms))
            row = cur.fetchone()
            if row is None or row[0] is None:
                raise RuntimeError("reconcile count returned no rows")
            return int(row[0])


def list_r2_audit_count(
    bucket: str, account_id: str, api_token: str, target_date: str
) -> int:
    """Count R2 NDJSON keys under `audit/.../<YYYY-MM-DD>/` via the
    paginated Cloudflare API v4. Mirrors the pagination loop in
    `.github/workflows/audit-chain-daily-verify.yml`.
    """
    import requests  # type: ignore

    yyyy, mm, dd = target_date.split("-")
    prefix = f"audit/{yyyy}/{mm}/{dd}/"
    url = (
        f"https://api.cloudflare.com/client/v4/accounts/{account_id}/"
        f"r2/buckets/{bucket}/objects"
    )
    headers = {"Authorization": f"Bearer {api_token}"}
    cursor: Optional[str] = None
    total = 0
    pages = 0
    while True:
        pages += 1
        if pages > 100:
            raise RuntimeError(
                "R2 list pagination exceeded 100 pages (>100k keys)"
                " — fanout guard"
            )
        params: dict[str, object] = {"prefix": prefix, "per_page": 1000}
        if cursor is not None:
            params["cursor"] = cursor
        resp = requests.get(url, headers=headers, params=params, timeout=30)
        if resp.status_code != 200:
            raise RuntimeError(
                f"R2 list non-2xx: {resp.status_code} {resp.text[:200]}"
            )
        body = resp.json()
        if not body.get("success", False):
            raise RuntimeError(f"R2 list success=false: {body.get('errors')}")
        result = body.get("result", []) or []
        total += len(result)
        info = body.get("result_info", {}) or {}
        cursor = info.get("cursor") or None
        if not cursor:
            break
    return total


def fire_pagerduty(
    routing_key: str,
    region: str,
    target_date: str,
    shadow_count: int,
    r2_count: int,
    drift: int,
) -> None:
    """Dispatch PD Events API v2 trigger with the canonical dedup_key."""
    import requests  # type: ignore

    payload = {
        "routing_key": routing_key,
        "event_action": "trigger",
        "dedup_key": f"neon-shadow-drift-{region}-{target_date}",
        "payload": {
            "summary": (
                f"Neon shadow drift detected — region={region} "
                f"date={target_date} drift={drift}"
            ),
            "severity": "warning",
            "source": f"neon-shadow-reconcile-daily/{region}",
            "component": "neon-analytics-shadow",
            "group": "corelink",
            "class": "analytics-reconciliation",
            "custom_details": {
                "region": region,
                "target_date": target_date,
                "shadow_count": shadow_count,
                "r2_count": r2_count,
                "drift": drift,
                "runbook": "specs/_runbooks/RB-NEON-SHADOW-LAG.md",
            },
        },
    }
    resp = requests.post(
        "https://events.pagerduty.com/v2/enqueue",
        json=payload,
        timeout=30,
    )
    if resp.status_code not in (200, 202):
        print(
            f"::warning::PagerDuty enqueue non-2xx: "
            f"{resp.status_code} {resp.text[:200]}",
            file=sys.stderr,
        )


def reconcile_region(
    region: str,
    target_date: str,
    bucket: str,
    account_id: Optional[str],
    api_token: Optional[str],
    pd_routing_key: Optional[str],
    emit_pd: bool,
) -> RegionResult:
    """Reconcile one region. Returns a structured result."""
    var = env_var_name(region)
    dsn = os.environ.get(var)
    if not dsn:
        return RegionResult(
            region=region,
            skipped=True,
            skipped_reason=f"{var} not set",
            shadow_count=None,
            r2_count=None,
            drift=None,
            error=None,
        )
    from_ms, to_ms = utc_day_window_ms(target_date)
    try:
        shadow_count = query_shadow_count(dsn, from_ms, to_ms)
    except Exception as e:  # noqa: BLE001 — surface every backend err
        return RegionResult(
            region=region,
            skipped=False,
            skipped_reason=None,
            shadow_count=None,
            r2_count=None,
            drift=None,
            error=f"neon query failed: {e}",
        )
    try:
        if not (account_id and api_token):
            raise RuntimeError("CF_API_TOKEN / CF_ACCOUNT_ID unset")
        r2_count = list_r2_audit_count(
            bucket, account_id, api_token, target_date
        )
    except Exception as e:  # noqa: BLE001
        return RegionResult(
            region=region,
            skipped=False,
            skipped_reason=None,
            shadow_count=shadow_count,
            r2_count=None,
            drift=None,
            error=f"r2 list failed: {e}",
        )
    drift = r2_count - shadow_count
    result = RegionResult(
        region=region,
        skipped=False,
        skipped_reason=None,
        shadow_count=shadow_count,
        r2_count=r2_count,
        drift=drift,
        error=None,
    )
    if drift != 0 and emit_pd and pd_routing_key:
        fire_pagerduty(
            pd_routing_key, region, target_date, shadow_count, r2_count, drift
        )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--regions", required=True)
    parser.add_argument("--target-date", required=True)
    parser.add_argument("--bucket", required=True)
    parser.add_argument("--emit-pd", action="store_true")
    parser.add_argument("--output-json", default=None)
    args = parser.parse_args()

    account_id = os.environ.get("CF_ACCOUNT_ID")
    api_token = os.environ.get("CF_API_TOKEN")
    pd_routing_key = os.environ.get("PAGERDUTY_ROUTING_KEY")

    regions = [r.strip() for r in args.regions.split(",") if r.strip()]
    results: list[RegionResult] = []
    for region in regions:
        r = reconcile_region(
            region=region,
            target_date=args.target_date,
            bucket=args.bucket,
            account_id=account_id,
            api_token=api_token,
            pd_routing_key=pd_routing_key,
            emit_pd=args.emit_pd,
        )
        results.append(r)
        if r.skipped:
            print(f"[skip ] region={region} ({r.skipped_reason})")
        elif r.error:
            print(f"[ERROR] region={region} err={r.error}")
        elif r.drift == 0:
            print(
                f"[ok   ] region={region} shadow={r.shadow_count} "
                f"r2={r.r2_count} drift=0"
            )
        else:
            print(
                f"[DRIFT] region={region} shadow={r.shadow_count} "
                f"r2={r.r2_count} drift={r.drift} — PD fired={args.emit_pd}"
            )

    report = {
        "target_date": args.target_date,
        "bucket": args.bucket,
        "regions": [asdict(r) for r in results],
    }
    if args.output_json:
        with open(args.output_json, "w", encoding="utf-8") as fh:
            json.dump(report, fh, indent=2)
        print(f"Wrote report to {args.output_json}")

    # Exit non-zero IFF any region surfaced drift or error.
    any_bad = any(
        (r.error is not None) or (r.drift is not None and r.drift != 0)
        for r in results
    )
    return 1 if any_bad else 0


if __name__ == "__main__":
    sys.exit(main())
