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
    EXPECTED_DEPLOYMENT_ID,
    EXPECTED_SOURCE_SHA,
    EXPECTED_VERSION_ID,
    EXPECTED_VERSION_NUMBER,
    EXPECTED_ROLLOUT_PERCENTAGE,
    PROVIDER,
    SOURCE,
    WORKER,
    validate_provider_binding,
)


def _wrangler_json(*args: str) -> Any:
    result = subprocess.run(
        ["npx", "wrangler", *args, "--json"],
        check=True,
        capture_output=True,
        text=True,
        timeout=60,
    )
    return json.loads(result.stdout)


def collect(source_sha: str) -> dict[str, Any]:
    if source_sha != EXPECTED_SOURCE_SHA:
        raise ValueError("source SHA is not the pinned deployment source")
    deployments = _wrangler_json("deployments", "list", "--name", WORKER)
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
    version = _wrangler_json("versions", "view", EXPECTED_VERSION_ID, "--name", WORKER)
    if not isinstance(version, dict) or version.get("id") != EXPECTED_VERSION_ID or version.get("number") != EXPECTED_VERSION_NUMBER:
        raise ValueError("pinned version metadata drifted")
    receipt = {
        "schema": "corelink-b006-provider-binding-v1",
        "provider": PROVIDER,
        "evidence_source": "Cloudflare Wrangler deployments/versions API",
        "authenticated": True,
        "credential_values_printed": False,
        "worker": WORKER,
        "metrics_source": SOURCE,
        "deployment_id": EXPECTED_DEPLOYMENT_ID,
        "version_id": EXPECTED_VERSION_ID,
        "version_number": EXPECTED_VERSION_NUMBER,
        "source_sha": source_sha,
        "rollout_percentage": EXPECTED_ROLLOUT_PERCENTAGE,
        "captured_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    }
    validate_provider_binding(receipt)
    return receipt


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        receipt = collect(args.source_sha)
    except (OSError, subprocess.SubprocessError, ValueError, json.JSONDecodeError) as exc:
        print(f"B-006 provider collection FAIL: {exc}")
        return 2
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"provider": receipt["provider"], "version_id": receipt["version_id"], "rollout_percentage": receipt["rollout_percentage"]}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
