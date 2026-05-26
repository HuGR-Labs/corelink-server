#!/usr/bin/env python3
"""quickstart_portal_session — Bootstrap an enterprise-portal session
via POST /v1/enterprise/inquire.

Customer concept: enterprise prospects request contracted-tier
provisioning (DPA + BAA + custom SLA). Returns a portal URL +
short-lived session token; the prospect completes the questionnaire
in-browser.

Run:
    CORELINK_PAT=$PAT python3 examples/quickstart_portal_session.py
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request
import uuid


def main() -> int:
    api = os.environ.get("CORELINK_API_URL", "https://sandbox.corelink.humangr.com")
    pat = os.environ.get("CORELINK_PAT")
    if not pat:
        print("error: CORELINK_PAT env var is required", file=sys.stderr)
        return 2
    idem = f"idem-{uuid.uuid4()}"
    body = json.dumps(
        {
            "company": "Acme Corp",
            "contact_email_hash": "0" * 64,
            "expected_seats": 250,
            "use_case": "monorepo build cache for 80 engineers",
        }
    ).encode("utf-8")
    req = urllib.request.Request(
        url=f"{api}/v1/enterprise/inquire",
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
