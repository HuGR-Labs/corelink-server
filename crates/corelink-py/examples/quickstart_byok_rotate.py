#!/usr/bin/env python3
"""quickstart_byok_rotate — File a BYOK rotation op via POST /v1/admin/ops.

Customer concept: privileged operations are dual-approval gated
(CTRL-DUAL-APPROVAL-001). The op is PENDING until a second admin
approves via POST /v1/admin/ops/{op_id}/approve.

Run:
    CORELINK_PAT=$ADMIN_PAT python3 examples/quickstart_byok_rotate.py
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request
import uuid


def main() -> int:
    api = os.environ.get("CORELINK_API_URL", "https://sandbox.corelink.dev")
    pat = os.environ.get("CORELINK_PAT")
    if not pat:
        print("error: CORELINK_PAT env var is required", file=sys.stderr)
        return 2

    idem = f"idem-{uuid.uuid4()}"
    body = json.dumps(
        {
            "op_type": "byok_rotate",
            "reason": "scheduled quarterly rotation",
            "params": {"key_alias": "primary", "target_region": "eu-west-1"},
        }
    ).encode("utf-8")
    req = urllib.request.Request(
        url=f"{api}/v1/admin/ops",
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
            print(f"status={resp.status} body={resp.read().decode('utf-8')}")
            print("NOTE: op is PENDING until a second admin approves it.")
    except urllib.error.HTTPError as exc:
        print(f"http_error status={exc.code} body={exc.read().decode('utf-8', 'replace')}", file=sys.stderr)
        return 1
    except urllib.error.URLError as exc:
        print(f"network_error: {exc.reason}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
