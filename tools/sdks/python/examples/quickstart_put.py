#!/usr/bin/env python3
"""quickstart_put — Issue a new PAT via POST /v1/pats.

Customer concept: PATs are the canonical credential for CoreLink APIs.
The plaintext token is shown-once (CTRL-CRED-001) — store it immediately.

Run:
    CORELINK_PAT=$BOOTSTRAP_PAT python3 examples/quickstart_put.py
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request
import uuid


def main() -> int:
    api = os.environ.get("CORELINK_API_URL", "https://corelink-api.humangr.com")
    pat = os.environ.get("CORELINK_PAT")
    if not pat:
        print("error: CORELINK_PAT env var is required", file=sys.stderr)
        return 2

    idem = f"idem-{uuid.uuid4()}"
    body = json.dumps({"label": "ci-cache-rw", "scopes": ["cache:r", "cache:w"], "ttl_hours": 24}).encode("utf-8")
    req = urllib.request.Request(
        url=f"{api}/v1/pats",
        method="POST",
        data=body,
        headers={
            "Authorization": f"Bearer {pat}",
            "Content-Type": "application/json",
            "Idempotency-Key": idem,
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            print(f"status={resp.status}")
            print(f"response={resp.read().decode('utf-8')}")
            print("NOTE: plaintext token shown once — store it now.")
    except urllib.error.HTTPError as exc:
        print(f"http_error status={exc.code} body={exc.read().decode('utf-8', 'replace')}", file=sys.stderr)
        return 1
    except urllib.error.URLError as exc:
        print(f"network_error: {exc.reason}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
