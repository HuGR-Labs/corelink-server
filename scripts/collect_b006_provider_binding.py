#!/usr/bin/env python3
"""Collect a redacted, read-only Wrangler deployment/version binding receipt."""

from __future__ import annotations

import argparse
import json
import sys
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.verify_b006_provider_binding import (
    EXPECTED_ACCOUNT_ID,
    EXPECTED_AUTH_EMAIL,
    EXPECTED_AUTH_TYPE,
    EXPECTED_DEPLOYMENT_ID,
    EXPECTED_SCRIPT_ETAG,
    EXPECTED_VERSION_ID,
    EXPECTED_VERSION_NUMBER,
    EXPECTED_ROLLOUT_PERCENTAGE,
    EXPECTED_WRANGLER_INTEGRITY,
    EXPECTED_WRANGLER_VERSION,
    PROVIDER,
    SOURCE,
    WORKER,
    validate_provider_binding,
)

WRANGLER_COMMAND = ("npx", "--yes", "--package", f"wrangler@{EXPECTED_WRANGLER_VERSION}", "wrangler")


def _wrangler_json(*args: str) -> tuple[Any, bool]:
    result = subprocess.run(
        [*WRANGLER_COMMAND, *args, "--json"],
        check=True,
        capture_output=True,
        text=True,
        timeout=60,
    )
    payload = json.loads(result.stdout)
    return payload, result.returncode == 0 and payload is not None


def _validate_whoami(identity: Any) -> dict[str, Any]:
    if not isinstance(identity, dict) or identity.get("loggedIn") is not True or identity.get("authType") != EXPECTED_AUTH_TYPE:
        raise ValueError("Wrangler whoami is not an authenticated expected identity")
    if identity.get("email") != EXPECTED_AUTH_EMAIL:
        raise ValueError("Wrangler whoami account email drifted")
    accounts = identity.get("accounts")
    if not isinstance(accounts, list) or len(accounts) != 1 or not isinstance(accounts[0], dict) or accounts[0].get("id") != EXPECTED_ACCOUNT_ID:
        raise ValueError("Wrangler whoami account binding drifted")
    return {
        "account_id": EXPECTED_ACCOUNT_ID,
        "email": EXPECTED_AUTH_EMAIL,
        "auth_type": EXPECTED_AUTH_TYPE,
        "logged_in": True,
    }


def collect() -> dict[str, Any]:
    identity_response, identity_authenticated = _wrangler_json("whoami")
    identity = _validate_whoami(identity_response)
    if not identity_authenticated:
        raise ValueError("Wrangler whoami authentication evidence is unavailable")
    deployments, deployments_authenticated = _wrangler_json("deployments", "list", "--name", WORKER)
    deployment = next((item for item in deployments if item.get("id") == EXPECTED_DEPLOYMENT_ID), None)
    if not isinstance(deployment, dict):
        raise ValueError("pinned deployment was not returned by Wrangler")
    versions = deployment.get("versions")
    if not isinstance(versions, list) or not any(
        item.get("version_id") == EXPECTED_VERSION_ID and item.get("percentage") == EXPECTED_ROLLOUT_PERCENTAGE
        for item in versions
        if isinstance(item, dict)
    ):
        raise ValueError("pinned deployment does not serve the pinned version at 100%")
    version, version_authenticated = _wrangler_json("versions", "view", EXPECTED_VERSION_ID, "--name", WORKER)
    if not isinstance(version, dict) or version.get("id") != EXPECTED_VERSION_ID or version.get("number") != EXPECTED_VERSION_NUMBER:
        raise ValueError("pinned version metadata drifted")
    script = version.get("resources", {}).get("script", {})
    script_etag = script.get("etag") if isinstance(script, dict) else None
    if script_etag != EXPECTED_SCRIPT_ETAG:
        raise ValueError("provider response lacks the pinned script content digest")
    authenticated = identity_authenticated and deployments_authenticated and version_authenticated
    if not authenticated:
        raise ValueError("Wrangler/API authentication evidence is unavailable")
    receipt = {
        "schema": "corelink-b006-provider-binding-v3",
        "provider": PROVIDER,
        "evidence_source": "Cloudflare Wrangler deployments/versions API",
        "authenticated": authenticated,
        "credential_values_printed": False,
        "auth_identity": identity,
        "wrangler_version": EXPECTED_WRANGLER_VERSION,
        "wrangler_package_integrity": EXPECTED_WRANGLER_INTEGRITY,
        "deployment_api_success": deployments_authenticated,
        "version_api_success": version_authenticated,
        "worker": WORKER,
        "metrics_source": SOURCE,
        "deployment_id": EXPECTED_DEPLOYMENT_ID,
        "version_id": EXPECTED_VERSION_ID,
        "version_number": EXPECTED_VERSION_NUMBER,
        "script_etag": script_etag,
        "rollout_percentage": EXPECTED_ROLLOUT_PERCENTAGE,
        "captured_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    }
    validate_provider_binding(receipt)
    return receipt


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        receipt = collect()
    except (OSError, subprocess.SubprocessError, ValueError, json.JSONDecodeError) as exc:
        print(f"B-006 provider collection FAIL: {exc}")
        return 2
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"provider": receipt["provider"], "version_id": receipt["version_id"], "rollout_percentage": receipt["rollout_percentage"]}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
