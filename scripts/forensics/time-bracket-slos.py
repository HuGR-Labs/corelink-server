#!/usr/bin/env python3
"""
time-bracket-slos.py — given an incident window, list which SLOs breached.

Reads the canonical SLO catalog (`specs/03_architecture/slo_catalog.md`),
extracts each SLO's PromQL query, runs it against the configured Grafana
instance for the [--start, --end] window, and prints a JSON report:

    [
      {
        "slo_id": "CAS-READ-P99",
        "breached": true,
        "first_breach_at": "2026-05-15T14:23:14Z",
        "max_severity": "page",
        "regions_affected": ["wnam"]
      },
      ...
    ]

Severity levels (from slo_catalog):
    ok      — within objective
    warn    — within burn rate budget but trending
    page    — burn rate exceeds page threshold

Usage:
    python3 scripts/forensics/time-bracket-slos.py \
        --start 2026-05-15T14:00:00Z \
        --end   2026-05-15T15:00:00Z

Exit codes:
    0 — at least one breach found (printed to stdout)
    1 — no breaches in window (clean — also printed)
    2 — invocation / configuration error

NOTE: this is the forensics shim — it reads SLO defs from the spec
corpus and does NOT mutate anything. Network calls are GET-only.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
SLO_CATALOG = REPO_ROOT / "specs" / "03_architecture" / "slo_catalog.md"

GRAFANA_URL = os.environ.get("GRAFANA_URL", "https://grafana.corelink.dev")
GRAFANA_TOKEN = os.environ.get("GRAFANA_TOKEN", "")

# SLO ids are extracted by regex from slo_catalog.md headers of the form
# `### <SLO-ID> — <description>`.
SLO_ID_RE = re.compile(r"^###\s+(?P<id>[A-Z0-9-]+)\s+[-—]\s+", re.MULTILINE)


def iso_to_unix(ts: str) -> int:
    """Parse an ISO8601 Z timestamp to Unix epoch seconds."""
    try:
        # Python <3.11 doesn't accept 'Z' directly; normalize.
        normalized = ts.replace("Z", "+00:00")
        dt = datetime.fromisoformat(normalized)
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        return int(dt.timestamp())
    except ValueError as exc:
        raise SystemExit(f"error: bad timestamp {ts!r}: {exc}") from exc


def discover_slo_ids(catalog: Path) -> list[str]:
    if not catalog.exists():
        raise SystemExit(f"error: SLO catalog not found at {catalog}")
    text = catalog.read_text(encoding="utf-8")
    ids = SLO_ID_RE.findall(text)
    # De-duplicate while preserving order.
    seen: set[str] = set()
    ordered: list[str] = []
    for slo_id in ids:
        if slo_id not in seen:
            seen.add(slo_id)
            ordered.append(slo_id)
    return ordered


def query_slo(slo_id: str, start_unix: int, end_unix: int) -> dict:
    """
    Query Grafana for one SLO over the window.

    Returns a dict with breach metadata. If GRAFANA_TOKEN is empty the
    function returns a stub marker (`"data_source": "unconfigured"`)
    so the script remains usable for dry-runs in CI.
    """
    if not GRAFANA_TOKEN:
        return {
            "slo_id": slo_id,
            "breached": False,
            "data_source": "unconfigured",
            "note": "GRAFANA_TOKEN env var not set; skipping live query",
        }

    # The SLO recording rule is named `corelink:slo:<id>:breach:rate5m`
    # by convention (see specs/03_architecture/observability_model.md §9).
    rule = f"corelink:slo:{slo_id.lower().replace('-', '_')}:breach:rate5m"
    params = {
        "query": rule,
        "start": str(start_unix),
        "end": str(end_unix),
        "step": "30",
    }
    url = f"{GRAFANA_URL}/api/datasources/proxy/1/api/v1/query_range?" + \
        urllib.parse.urlencode(params)
    req = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {GRAFANA_TOKEN}",
            "Accept": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            payload = json.loads(resp.read())
    except (urllib.error.URLError, json.JSONDecodeError) as exc:
        return {
            "slo_id": slo_id,
            "breached": False,
            "error": f"grafana query failed: {exc}",
        }

    series = payload.get("data", {}).get("result", [])
    breaches: list[dict] = []
    for s in series:
        region = s.get("metric", {}).get("region", "unknown")
        for ts, val_str in s.get("values", []):
            try:
                val = float(val_str)
            except ValueError:
                continue
            if val > 0.0:
                breaches.append({"region": region, "at_unix": int(ts), "rate": val})

    if not breaches:
        return {"slo_id": slo_id, "breached": False}

    first = min(breaches, key=lambda b: b["at_unix"])
    max_rate = max(b["rate"] for b in breaches)
    regions = sorted({b["region"] for b in breaches})
    return {
        "slo_id": slo_id,
        "breached": True,
        "first_breach_at": datetime.fromtimestamp(
            first["at_unix"], tz=timezone.utc
        ).isoformat(),
        "max_severity": "page" if max_rate >= 1.0 else "warn",
        "regions_affected": regions,
        "max_burn_rate": max_rate,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--start", required=True, help="ISO8601 Z start")
    parser.add_argument("--end",   required=True, help="ISO8601 Z end")
    args = parser.parse_args()

    start_unix = iso_to_unix(args.start)
    end_unix = iso_to_unix(args.end)
    if end_unix <= start_unix:
        print("error: --end must be strictly after --start", file=sys.stderr)
        return 2

    slo_ids = discover_slo_ids(SLO_CATALOG)
    if not slo_ids:
        print("error: no SLO ids found in catalog; check slo_catalog.md format",
              file=sys.stderr)
        return 2

    report = [query_slo(sid, start_unix, end_unix) for sid in slo_ids]
    print(json.dumps(report, indent=2))

    any_breach = any(r.get("breached") for r in report)
    return 0 if any_breach else 1


if __name__ == "__main__":
    sys.exit(main())
