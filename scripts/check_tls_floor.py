#!/usr/bin/env python3
"""Assert the live Cloudflare TLS floor still matches what the docs claim.

`min_tls_version` is a Cloudflare zone setting. Anyone with zone-edit access can
change it from the dashboard, and until this script existed nothing in the repo
would have noticed (ADR-0072, "Drift risk"). Both directions are a real
incident:

- raised back to **1.3**: `sccache` and every other native-tls /
  SecureTransport client fails its handshake against the live product — the
  exact outage ADR-0072 was written to prevent a second time;
- lowered below **1.2**: the control table in `security_model.md`
  (CTRL-CRYPTO-001) and every compliance crosswalk that cites it become false
  in the weaker direction, which is the one auditors care about.

So this asserts equality with the documented value, not "at least".

Auth: `CLOUDFLARE_API_TOKEN` (or `CF_API_TOKEN`) with Zone Settings: Read. A
token that cannot authenticate exits non-zero — a drift check that cannot read
the value must never report "no drift".
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request

# The zone that terminates TLS for every CoreLink hostname, and the floor
# ADR-0072 decided on. Changing either here is a documentation change: update
# ADR-0072 and security_model.md's CTRL-CRYPTO-001 row in the same commit.
ZONES = {
    "humangr.com": ("73f57f6d508beea67e2f78bea11d3c25", "1.2"),
}

API = "https://api.cloudflare.com/client/v4/zones/{zone}/settings/min_tls_version"


def token() -> str:
    for name in ("CLOUDFLARE_API_TOKEN", "CF_API_TOKEN"):
        value = os.environ.get(name)
        if value:
            return value
    print("FAIL: no CLOUDFLARE_API_TOKEN / CF_API_TOKEN in the environment — "
          "cannot read the live TLS floor, so cannot claim it has not drifted.",
          file=sys.stderr)
    raise SystemExit(2)


def live_floor(zone_id: str, bearer: str) -> str:
    request = urllib.request.Request(
        API.format(zone=zone_id),
        headers={"Authorization": f"Bearer {bearer}"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as exc:
        print(f"FAIL: Cloudflare answered HTTP {exc.code} for zone {zone_id}. "
              "The token likely lacks Zone Settings: Read.", file=sys.stderr)
        raise SystemExit(2)
    except urllib.error.URLError as exc:
        print(f"FAIL: could not reach the Cloudflare API ({exc.reason}).",
              file=sys.stderr)
        raise SystemExit(2)

    if not payload.get("success"):
        print(f"FAIL: Cloudflare rejected the request: "
              f"{json.dumps(payload.get('errors'))}", file=sys.stderr)
        raise SystemExit(2)
    return payload["result"]["value"]


def main() -> int:
    bearer = token()
    drifted = False
    for name, (zone_id, expected) in ZONES.items():
        actual = live_floor(zone_id, bearer)
        if actual == expected:
            print(f"ok: {name} min_tls_version = {actual} (as documented)")
            continue
        drifted = True
        print(f"DRIFT: {name} min_tls_version is {actual}, but ADR-0072 and "
              f"security_model.md's CTRL-CRYPTO-001 document {expected}.",
              file=sys.stderr)
        if actual > expected:
            print("       Raised floor: native-tls/SecureTransport clients "
                  "(sccache) can no longer complete a handshake. Read "
                  "ADR-0072's exit condition before treating this as an "
                  "improvement.", file=sys.stderr)
        else:
            print("       Lowered floor: the security model and every "
                  "compliance crosswalk citing CTRL-CRYPTO-001 now overstate "
                  "the control.", file=sys.stderr)
    if drifted:
        print("Fix the zone or fix the documents — do not delete the check.",
              file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
