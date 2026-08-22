#!/usr/bin/env python3
"""quickstart_get — Fetch the authenticated principal via GET /v1/users/me.

Customer concept: the canonical "who am I" probe. Use it to verify PAT
validity, scopes, and tenant binding before issuing follow-up calls.

Run:
    CORELINK_PAT=$PAT python3 examples/quickstart_get.py
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request


def main() -> int:
    api = os.environ.get("CORELINK_API_URL", "https://corelink-api.humangr.com")
    pat = os.environ.get("CORELINK_PAT")
    if not pat:
        print("error: CORELINK_PAT env var is required", file=sys.stderr)
        return 2

    req = urllib.request.Request(
        url=f"{api}/v1/users/me",
        method="GET",
        headers={"Authorization": f"Bearer {pat}", "Accept": "application/json"},
    )
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
