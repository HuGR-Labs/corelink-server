#!/usr/bin/env python3
"""quickstart_list — List all PATs via GET /v1/pats.

Customer concept: server-side authoritative credential inventory.
Plaintext tokens are never returned (only metadata + last-4 prefix).

Run:
    CORELINK_PAT=$PAT python3 examples/quickstart_list.py
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request


def main() -> int:
    api = os.environ.get("CORELINK_API_URL", "https://sandbox.corelink.humangr.com")
    pat = os.environ.get("CORELINK_PAT")
    if not pat:
        print("error: CORELINK_PAT env var is required", file=sys.stderr)
        return 2

    req = urllib.request.Request(
        url=f"{api}/v1/pats",
        method="GET",
        headers={"Authorization": f"Bearer {pat}", "Accept": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
            items = payload.get("items") if isinstance(payload, dict) else None
            count = len(items) if isinstance(items, list) else 0
            print(f"status={resp.status} pat_count={count}")
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
