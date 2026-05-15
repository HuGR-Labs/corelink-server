#!/usr/bin/env python3
"""quickstart_team_invite — File a team-invite op via POST /v1/admin/ops.

Customer concept: adding tenant members flows through the admin-ops
dual-approval gate (CTRL-DUAL-APPROVAL-001). On approval an invite
email is dispatched via the verified-sender pipeline.

Run:
    CORELINK_PAT=$ADMIN_PAT INVITEE_EMAIL=alice@example.com \\
        python3 examples/quickstart_team_invite.py
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
    invitee = os.environ.get("INVITEE_EMAIL", "newmember@example.com")
    idem = f"idem-{uuid.uuid4()}"
    body = json.dumps(
        {
            "op_type": "team_invite",
            "reason": "onboard new engineer",
            "params": {"email": invitee, "role": "developer"},
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
    except urllib.error.HTTPError as exc:
        print(f"http_error status={exc.code} body={exc.read().decode('utf-8', 'replace')}", file=sys.stderr)
        return 1
    except urllib.error.URLError as exc:
        print(f"network_error: {exc.reason}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
