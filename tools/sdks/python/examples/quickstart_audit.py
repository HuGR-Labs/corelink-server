#!/usr/bin/env python3
"""quickstart_audit — Stream audit events via GET /v1/admin/audit-events.

Customer concept: every privileged operation is Merkle-chained into the
audit log (INV-AUDIT-APPEND-ONLY). The response includes chain_head_hash
for client-side verification (CTRL-AUDIT-002).

Run:
    CORELINK_PAT=$ADMIN_PAT python3 examples/quickstart_audit.py
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request


def main() -> int:
    api = os.environ.get("CORELINK_API_URL", "https://corelink-api.humangr.com")
    pat = os.environ.get("CORELINK_PAT")
    if not pat:
        print("error: CORELINK_PAT env var is required", file=sys.stderr)
        return 2

    qs = urllib.parse.urlencode({"limit": "50"})
    req = urllib.request.Request(
        url=f"{api}/v1/admin/audit-events?{qs}",
        method="GET",
        headers={"Authorization": f"Bearer {pat}", "Accept": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
            events = payload.get("events") if isinstance(payload, dict) else None
            count = len(events) if isinstance(events, list) else 0
            head = payload.get("chain_head_hash", "<none>") if isinstance(payload, dict) else "<none>"
            print(f"status={resp.status} events={count} chain_head={head}")
    except urllib.error.HTTPError as exc:
        print(f"http_error status={exc.code} body={exc.read().decode('utf-8', 'replace')}", file=sys.stderr)
        return 1
    except urllib.error.URLError as exc:
        print(f"network_error: {exc.reason}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
