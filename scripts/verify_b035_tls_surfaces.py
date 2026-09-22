#!/usr/bin/env python3
"""Verify the B-035 contract inventory without mutating legal or edge state.

B-035 cannot be closed by changing the versioned, counsel-approved instruments
in place.  The eight files therefore remain an explicit, fail-closed inventory
until counsel publishes a superseding version or the owner raises the external
Cloudflare floor.  ``--live`` performs one read-only GET of the Cloudflare zone
setting; it never sends a write request and never prints credentials.

The legal lines do not name individual hostnames.  This verifier consequently
does not infer eight host mappings: the only external surface it can safely
check is the zone mapping recorded by ADR-0072 and ``check_tls_floor.py``.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
ZONE_NAME = "humangr.com"
ZONE_ID = "73f57f6d508beea67e2f78bea11d3c25"
API = "https://api.cloudflare.com/client/v4/zones/{zone}/settings/min_tls_version"

# This is a protocol floor, not a cipher-suite promise.  ADR-0072 explicitly
# records that the Cloudflare cipher list was not audited or pinned.
PROTOCOL_FLOOR = "1.2"
CIPHER_POLICY = "external Cloudflare policy; no cipher-suite floor is claimed or verified"


class VerificationError(RuntimeError):
    """Raised when a repository contract input is missing or ambiguous."""


class Instrument:
    __slots__ = ("path", "claim")

    def __init__(self, path: str, claim: str) -> None:
        self.path = path
        self.claim = claim


INSTRUMENTS = (
    Instrument("legal/dpa/v1.0.0.en-US.md", "TLS 1.3+"),
    Instrument("legal/dpa/v1.0.0.pt-BR.md", "TLS 1.3+"),
    Instrument("legal/dpa/v1.0.0.es-419.md", "TLS 1.3+"),
    Instrument("legal/dpa/STANDARD-CONTRACTUAL-CLAUSES-EU.md", "TLS 1.3+"),
    Instrument("legal/dpa/SUB-PROCESSOR-COMMITMENTS.md", "TLS 1.3+"),
    Instrument("legal/privacy-notice/v1.0.0/en-US.md", "TLS 1.3"),
    Instrument("legal/privacy-notice/v1.0.0/pt-BR.md", "TLS 1.3"),
    Instrument("legal/privacy-notice/v1.0.0/es-MX.md", "TLS 1.3"),
)

# The claim is intentionally anchored to the exact protocol token.  A broad
# ``TLS 1.3`` grep would count a corrected ``TLS 1.2 minimum; 1.3 preferred``
# line as if it still promised 1.3-only.
CLAIM_RE = re.compile(r"TLS 1\.3(?:\+|(?=[).,;\s]))")


def _read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise VerificationError(f"missing or non-regular instrument: {relative}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise VerificationError(f"cannot read instrument {relative}: {exc}") from exc


def _line_number(text: str, match: re.Match[str]) -> int:
    return text.count("\n", 0, match.start()) + 1


def inventory(root: Path = ROOT) -> dict[str, Any]:
    """Return the complete eight-line inventory and its fail-closed status."""
    rows: list[dict[str, Any]] = []
    for instrument in INSTRUMENTS:
        text = _read(root, instrument.path)
        matches = list(CLAIM_RE.finditer(text))
        lines = [_line_number(text, match) for match in matches]
        rows.append(
            {
                "path": instrument.path,
                "promised_claim": instrument.claim,
                "claim_count": len(matches),
                "claim_lines": lines,
                "claim_matches_expected": len(matches) == 1
                and matches[0].group(0) == instrument.claim,
            }
        )

    complete = len(rows) == 8 and all(
        row["claim_count"] == 1 and row["claim_matches_expected"] for row in rows
    )
    return {
        "instrument_count": len(rows),
        "instruments": rows,
        "status": "open_exact_inventory" if complete else "drift_or_incomplete",
        "external_surface": {
            "zone": ZONE_NAME,
            "zone_id": f"{ZONE_ID[:8]}…{ZONE_ID[-4:]}",
            "hostname_mapping": "not present in instruments; no hostnames inferred",
            "protocol_floor": f"TLS {PROTOCOL_FLOOR}",
            "cipher_policy": CIPHER_POLICY,
            "source": "ADR-0072 + scripts/check_tls_floor.py",
        },
    }


def _token() -> tuple[str | None, str]:
    for name in ("CLOUDFLARE_API_TOKEN", "CF_API_TOKEN"):
        value = os.environ.get(name)
        if value:
            return value, name
    return None, "none"


def read_live_floor(token: str) -> dict[str, Any]:
    """Read Cloudflare's setting with GET only; redact all error bodies."""
    request = urllib.request.Request(
        API.format(zone=ZONE_ID),
        headers={"Authorization": f"Bearer {token}"},
        method="GET",
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as exc:
        return {"status": "api_error", "http_status": exc.code}
    except urllib.error.URLError:
        return {"status": "network_error"}
    if not isinstance(payload, dict) or not payload.get("success"):
        return {"status": "api_rejected"}
    result = payload.get("result")
    value = result.get("value") if isinstance(result, dict) else None
    if not isinstance(value, str):
        return {"status": "malformed_response"}
    return {
        "status": "match" if value == PROTOCOL_FLOOR else "drift",
        "value": value,
        "expected": PROTOCOL_FLOOR,
    }


def verify(root: Path, *, live: bool, expect_open: bool) -> tuple[int, dict[str, Any]]:
    try:
        report = inventory(root)
    except VerificationError as exc:
        return 1, {"status": "verification_error", "error": str(exc)}

    token, source = _token()
    report["credential"] = {"available": bool(token), "source": source}
    report["mutation"] = {
        "performed": False,
        "write_methods": [],
        "legal_files_modified": False,
    }
    report["live_floor"] = {"status": "not_requested"}
    if live:
        report["live_floor"] = (
            read_live_floor(token) if token else {"status": "credential_unavailable"}
        )

    # The repository contract is exact in either mode.  ``--expect-open``
    # documents why the eight stale claims are currently accepted by this gate;
    # omitting it must not turn a partial correction or missing claim green.
    static_ok = report["status"] == "open_exact_inventory"
    live_result = report["live_floor"]
    live_ok = not live or live_result.get("status") == "match"
    # Missing credentials are a hard failure for a requested live proof.  A
    # static hosted contract run deliberately omits --live and needs no token.
    code = 0 if static_ok and live_ok else (2 if live and live_result.get("status") == "credential_unavailable" else 1)
    return code, report


def _self_test(root: Path) -> None:
    """Exercise the fail-closed inventory logic without network or mutation."""
    report = inventory(root)
    if report["status"] != "open_exact_inventory" or report["instrument_count"] != 8:
        raise VerificationError("current B-035 inventory is not the expected eight-line open state")

    target = root / INSTRUMENTS[0].path
    original = target.read_text(encoding="utf-8")
    mutant = original.replace("TLS 1.3+", "TLS 1.2 minimum (TLS 1.3 preferred)", 1)
    if mutant == original:
        raise VerificationError("self-test mutation did not alter a contract claim")
    target_report = inventory_from_overrides(root, {INSTRUMENTS[0].path: mutant})
    if target_report["status"] != "drift_or_incomplete":
        raise VerificationError("corrected contract claim did not fail closed")


def inventory_from_overrides(root: Path, overrides: dict[str, str]) -> dict[str, Any]:
    """Inventory helper used only by the local mutation self-test."""
    rows: list[dict[str, Any]] = []
    for instrument in INSTRUMENTS:
        text = overrides.get(instrument.path, _read(root, instrument.path))
        matches = list(CLAIM_RE.finditer(text))
        rows.append(
            {
                "path": instrument.path,
                "promised_claim": instrument.claim,
                "claim_count": len(matches),
                "claim_lines": [_line_number(text, match) for match in matches],
                "claim_matches_expected": len(matches) == 1
                and matches[0].group(0) == instrument.claim,
            }
        )
    complete = len(rows) == 8 and all(
        row["claim_count"] == 1 and row["claim_matches_expected"] for row in rows
    )
    return {"instrument_count": len(rows), "instruments": rows, "status": "open_exact_inventory" if complete else "drift_or_incomplete"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--live", action="store_true", help="GET Cloudflare floor; never writes")
    parser.add_argument("--expect-open", action="store_true", help="require all eight stale claims")
    parser.add_argument("--self-test", action="store_true", help="run bounded mutation checks")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        try:
            _self_test(args.root)
        except VerificationError as exc:
            print(f"B-035 self-test failed: {exc}", file=sys.stderr)
            return 1
        print("B-035 self-test passed (read-only, fail-closed mutation check)")
        return 0

    code, report = verify(args.root, live=args.live, expect_open=args.expect_open)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(f"B-035 inventory: {report['status']}")
        print(f"instruments: {report.get('instrument_count', 0)}/8")
        print(f"protocol floor: TLS {PROTOCOL_FLOOR}")
        print(f"cipher policy: {CIPHER_POLICY}")
        print(f"live floor: {report['live_floor']['status']}")
        print("mutation: not performed")
    return code


if __name__ == "__main__":
    raise SystemExit(main())
