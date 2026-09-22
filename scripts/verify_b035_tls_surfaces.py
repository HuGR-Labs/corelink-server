#!/usr/bin/env python3
"""Verify the B-035 contract inventory without mutating legal or edge state.

B-035 cannot be closed by changing the versioned, counsel-approved instruments
in place.  The eight files therefore remain an explicit, fail-closed inventory
until counsel publishes a superseding version or the owner raises the external
Cloudflare floor.  ``--live`` performs one read-only GET of the Cloudflare zone
setting; it never sends a write request and never prints credentials.

The legal lines do not name individual hostnames.  The verifier therefore
reports the configured ingress/custom-domain and egress surfaces as an
explicit audit inventory, while the only live value it can safely check is
the Cloudflare zone mapping recorded by ADR-0072 and ``check_tls_floor.py``.
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

# Keep the contract audit honest about the traffic surfaces it covers.  These
# are the ingress routes and explicit egress URLs found in the Worker/Terraform
# manifests, plus the R2 endpoints used by the container.  Cloudflare's zone
# setting applies to proxied hostnames in this zone; it does not configure TLS
# on an R2 origin or prove a provider-managed egress policy.  Every row carries
# its source and proof status so a repository declaration cannot be mistaken
# for live provider evidence.
NAMED_SURFACES = (
    {"name": "corelink-api", "kind": "ingress", "hostname": "corelink-api.humangr.com/*", "enforcement": "Cloudflare zone min_tls_version", "source": "wrangler.toml:340-342", "status": "production_route_declared"},
    {"name": "corelink-oci", "kind": "ingress", "hostname": "corelink-oci.humangr.com/*", "enforcement": "Cloudflare zone min_tls_version", "source": "wrangler.toml:349-351", "status": "production_route_declared"},
    {"name": "regional-api-sam", "kind": "ingress", "hostname": "sam.corelink-api.humangr.com", "enforcement": "Cloudflare zone min_tls_version", "source": "wrangler.toml:665-667", "status": "production_custom_domain_declared"},
    {"name": "regional-api-lhr", "kind": "ingress", "hostname": "lhr.corelink-api.humangr.com", "enforcement": "Cloudflare zone min_tls_version", "source": "wrangler.toml:837-839", "status": "production_custom_domain_declared"},
    {"name": "regional-api-nrt", "kind": "ingress", "hostname": "nrt.corelink-api.humangr.com", "enforcement": "Cloudflare zone min_tls_version", "source": "wrangler.toml:1013-1015", "status": "production_custom_domain_declared"},
    {"name": "regional-api-syd", "kind": "ingress", "hostname": "syd.corelink-api.humangr.com", "enforcement": "Cloudflare zone min_tls_version", "source": "wrangler.toml:1180-1182", "status": "production_custom_domain_declared"},
    {"name": "signup-worker", "kind": "ingress", "hostname": "corelink-signup.humangr.com/*", "enforcement": "Cloudflare zone min_tls_version", "source": "apps/signup-worker/wrangler.toml:91-93", "status": "production_route_declared"},
    {"name": "get-worker", "kind": "ingress", "hostname": "corelink-get.humangr.com/*", "enforcement": "Cloudflare zone min_tls_version", "source": "apps/get-corelink-worker/wrangler.toml:80-82", "status": "production_route_declared"},
    {"name": "analytics-worker", "kind": "ingress", "hostname": "corelink-analytics.humangr.com", "enforcement": "Cloudflare zone min_tls_version", "source": "apps/analytics-worker/wrangler.toml:85-87", "status": "production_custom_domain_declared"},
    {"name": "docs-route", "kind": "ingress", "hostname": "corelink-docs.humangr.com/*", "enforcement": "Cloudflare zone min_tls_version", "source": "apps/docs/wrangler.toml:54-56", "status": "production_route_declared"},
    {"name": "docs-apex-bare", "kind": "ingress", "hostname": "humangr.com/corelink/docs", "enforcement": "Cloudflare zone min_tls_version", "source": "apps/docs/wrangler.toml:41-43", "status": "production_route_declared"},
    {"name": "docs-apex-wildcard", "kind": "ingress", "hostname": "humangr.com/corelink/docs/*", "enforcement": "Cloudflare zone min_tls_version", "source": "apps/docs/wrangler.toml:45-47", "status": "production_route_declared"},
    {"name": "admin-apex", "kind": "ingress", "hostname": "humangr.com/corelink/*", "enforcement": "Cloudflare zone min_tls_version", "source": "apps/admin-ui/wrangler.toml:68-79", "status": "operator_managed_route_not_live_verified"},
    {"name": "synthetic-pager-staging", "kind": "ingress", "hostname": "staging.corelink.humangr.com/v1/webhooks/pagerduty", "enforcement": "Cloudflare zone min_tls_version", "source": "apps/synthetic-pager-worker/wrangler.toml:35-36", "status": "staging_route_activation_gated"},
    {"name": "api-apex-terraform", "kind": "ingress", "hostname": "api.humangr.com/*", "enforcement": "proxied Cloudflare route; zone min_tls_version", "source": "infra/terraform/modules/cloudflare-base/main.tf:68-87", "status": "terraform_desired_state_not_live_verified"},
    {"name": "regional-api-terraform", "kind": "ingress", "hostname": "<region>.api.humangr.com/*", "enforcement": "proxied Cloudflare route; zone min_tls_version", "source": "infra/terraform/modules/corelink-region/main.tf:131-149", "status": "terraform_desired_state_not_live_verified"},
    {"name": "clerk-issuer", "kind": "egress", "hostname": "clerk.corelink-app.humangr.com", "enforcement": "provider-managed Clerk TLS; no repo floor", "source": "wrangler.toml:317; apps/admin-ui/wrangler.toml:76-78", "status": "provider_managed_not_live_verified"},
    {"name": "fabric-authority", "kind": "egress", "hostname": "corelink-fabricd.gmhelmold.workers.dev", "enforcement": "provider-managed Cloudflare Worker TLS; no repo floor", "source": "wrangler.toml:317,633,802,979,1150", "status": "provider_managed_not_live_verified"},
    {"name": "pagerduty-events", "kind": "egress", "hostname": "events.pagerduty.com/v2/enqueue", "enforcement": "provider-managed PagerDuty TLS; no repo floor", "source": "apps/synthetic-pager-worker/wrangler.toml:19,35,49", "status": "explicit_external_endpoint_not_live_verified"},
    {"name": "github-release-origin", "kind": "egress", "hostname": "github.com/HuGR-Labs/corelink-cli/releases/latest/download", "enforcement": "provider-managed GitHub TLS; no repo floor", "source": "apps/get-corelink-worker/wrangler.toml:63,69", "status": "production_config_declared_not_live_verified"},
    {"name": "r2-global", "kind": "egress", "hostname": "*.r2.cloudflarestorage.com", "enforcement": "provider-managed R2 TLS; no repo setting", "source": "wrangler.toml:317,653,998,1169", "status": "provider_managed_not_live_verified"},
    {"name": "r2-eu", "kind": "egress", "hostname": "*.eu.r2.cloudflarestorage.com", "enforcement": "provider-managed R2 TLS; no repo setting", "source": "wrangler.toml:826", "status": "provider_managed_not_live_verified"},
)

PROVIDER_BOUNDARIES = (
    {"name": "cloudflare-bindings", "kind": "internal_provider", "surface": "Worker service bindings, D1, KV, Durable Objects", "source": "wrangler.toml", "status": "provider_managed_no_public_handshake"},
    {"name": "configurable-egress", "kind": "egress", "surface": "Neon, Stripe, GitHub, PagerDuty, Resend, Sentry and analytics URLs", "source": "wrangler.toml + application secrets/config", "status": "host_or_setting_not_repo_declared"},
    {"name": "future-byok", "kind": "provider", "surface": "AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault", "source": "legal/dpa-residency-amendment.md", "status": "future_only_not_active"},
)


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
            "hostname_mapping": "repository route/config inventory only; legal instruments name no hosts",
            "protocol_floor": f"TLS {PROTOCOL_FLOOR}",
            "cipher_policy": CIPHER_POLICY,
            "source": "ADR-0072 + scripts/check_tls_floor.py",
            "named_surfaces": [dict(surface) for surface in NAMED_SURFACES],
            "provider_boundaries": [dict(boundary) for boundary in PROVIDER_BOUNDARIES],
            "downgrade_probe": {
                "status": "provider_read_only_setting_only",
                "scope": "Cloudflare edge zone setting; no production handshake mutation",
                "result": "TLS 1.2 is the configured minimum when --live reports match",
                "limitation": "Cloudflare does not expose a repo-owned per-host or R2-origin minimum in this repository; R2 TLS policy is provider-managed",
            },
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
