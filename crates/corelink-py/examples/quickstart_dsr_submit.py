#!/usr/bin/env python3
"""quickstart_dsr_submit — Submit a DSR via POST /v1/privacy/dsr/{action}.

Customer concept: GDPR/CCPA data-subject requests. Returns a request_id;
poll GET /v1/privacy/dsr/{request_id}/status for progress. Erasure
produces a signed attestation (CTRL-ERASURE-ATTEST-001).

Run:
    CORELINK_PAT=$PAT DSR_ACTION=export python3 examples/quickstart_dsr_submit.py
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
    action = os.environ.get("DSR_ACTION", "export")
    idem = f"idem-{uuid.uuid4()}"
    body = json.dumps(
        {
            "subject_email_hash": "0" * 64,
            "verification_token": "demo-verification-token",
            "scope": ["profile", "audit_events"],
        }
    ).encode("utf-8")
    req = urllib.request.Request(
        url=f"{api}/v1/privacy/dsr/{action}",
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
            print(f"action={action} status={resp.status} body={resp.read().decode('utf-8')}")
    except urllib.error.HTTPError as exc:
        print(f"http_error status={exc.code} body={exc.read().decode('utf-8', 'replace')}", file=sys.stderr)
        return 1
    except urllib.error.URLError as exc:
        print(f"network_error: {exc.reason}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
