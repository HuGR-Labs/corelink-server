#!/usr/bin/env python3
"""quickstart_stats — Probe service health/stats via GET /api/health.

Customer concept: liveness + readiness probe. Returns version, region,
and per-dependency status. Anonymous (no PAT required).

Run:
    python3 examples/quickstart_stats.py
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request


def main() -> int:
    api = os.environ.get("CORELINK_API_URL", "https://sandbox.corelink.dev")
    req = urllib.request.Request(url=f"{api}/api/health", method="GET", headers={"Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
            print(f"status={resp.status}")
            print(json.dumps(payload, indent=2))
    except urllib.error.HTTPError as exc:
        print(f"http_error status={exc.code} body={exc.read().decode('utf-8', 'replace')}", file=sys.stderr)
        return 1
    except urllib.error.URLError as exc:
        print(f"network_error: {exc.reason}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
