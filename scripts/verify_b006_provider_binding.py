#!/usr/bin/env python3
"""Fail-closed validator for the authenticated B-006 Wrangler binding receipt."""

from __future__ import annotations

import argparse
import json
import re
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

SOURCE = "https://corelink-spawn-worker.gmhelmold.workers.dev/internal/v1/metrics"
PROVIDER = "cloudflare-wrangler"
WORKER = "corelink-spawn-worker"
EXPECTED_DEPLOYMENT_ID = "e95c9cf1-e3d7-46d8-b84f-c245f6ed6628"
EXPECTED_VERSION_ID = "f712de57-997c-4f4a-9fbf-03f8d0b25b86"
EXPECTED_VERSION_NUMBER = 171
EXPECTED_SCRIPT_ETAG = "0c1146f115c4a99d0a8b3bb4d1842130dad63c950a01880218f07c77ed4690e0"
EXPECTED_ROLLOUT_PERCENTAGE = 100
EXPECTED_ACCOUNT_ID = "6a1fc1c626fc2628823e60b9db01f5cd"
EXPECTED_AUTH_EMAIL = "gmhelmold@gmail.com"
EXPECTED_AUTH_TYPE = "OAuth Token"
EXPECTED_WRANGLER_VERSION = "4.111.0"
LOCK_SOURCE = "pnpm-lock.yaml:wrangler@4.111.0"
MAX_RECEIPT_AGE = timedelta(hours=24)
MAX_RECEIPT_FUTURE = timedelta(minutes=5)
PROVIDER_KEYS = frozenset(
    {
        "schema", "provider", "evidence_source", "authenticated", "credential_values_printed",
        "auth_identity", "wrangler_version", "wrangler_lock_source", "wrangler_package_integrity",
        "deployment_api_success", "version_api_success",
        "worker", "metrics_source", "deployment_id", "version_id", "version_number",
        "script_etag", "rollout_percentage", "captured_at",
    }
)
class ProviderBindingError(ValueError):
    """The receipt does not bind the metrics read to the deployed Worker."""


def read_lock_integrity() -> str:
    lockfile = Path(__file__).resolve().parents[1] / "pnpm-lock.yaml"
    try:
        text = lockfile.read_text(encoding="utf-8")
    except OSError as exc:
        raise ProviderBindingError(f"cannot read pinned Wrangler lockfile: {exc}") from exc
    matches = re.findall(
        r"^  wrangler@4\.111\.0:\n    resolution: \{integrity: ([^,}]+)(?:,|\})",
        text,
        re.MULTILINE,
    )
    if len(matches) != 1 or not matches[0].startswith("sha512-"):
        raise ProviderBindingError("pinned Wrangler lockfile integrity is missing or ambiguous")
    return matches[0]


def _load(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ProviderBindingError(f"cannot read provider evidence: {exc}") from exc
    if not isinstance(value, dict):
        raise ProviderBindingError("provider evidence root must be an object")
    return value


def validate_provider_binding(binding: dict[str, Any]) -> None:
    if set(binding) != PROVIDER_KEYS:
        missing = sorted(PROVIDER_KEYS - set(binding))
        extra = sorted(set(binding) - PROVIDER_KEYS)
        raise ProviderBindingError(f"provider schema drift: missing={missing}, extra={extra}")
    if binding["schema"] != "corelink-b006-provider-binding-v3":
        raise ProviderBindingError("unsupported B-006 provider-binding schema")
    if binding["provider"] != PROVIDER or binding["evidence_source"] != "Cloudflare Wrangler deployments/versions API":
        raise ProviderBindingError("provider evidence is not authenticated Wrangler/API evidence")
    if binding["authenticated"] is not True or binding["credential_values_printed"] is not False:
        raise ProviderBindingError("provider evidence must be authenticated and redacted")
    identity = binding["auth_identity"]
    if not isinstance(identity, dict) or set(identity) != {"account_id", "email", "auth_type", "logged_in"}:
        raise ProviderBindingError("provider authentication identity is missing or ambiguous")
    if identity != {
        "account_id": EXPECTED_ACCOUNT_ID,
        "email": EXPECTED_AUTH_EMAIL,
        "auth_type": EXPECTED_AUTH_TYPE,
        "logged_in": True,
    }:
        raise ProviderBindingError("provider authentication identity drifted")
    if binding["wrangler_version"] != EXPECTED_WRANGLER_VERSION or binding["wrangler_lock_source"] != LOCK_SOURCE:
        raise ProviderBindingError("Wrangler binary version/integrity is not pinned")
    if binding["wrangler_package_integrity"] != read_lock_integrity():
        raise ProviderBindingError("computed Wrangler package integrity does not match the committed lockfile")
    if binding["deployment_api_success"] is not True or binding["version_api_success"] is not True:
        raise ProviderBindingError("provider deployment/version API result was not successful")
    if binding["worker"] != WORKER or binding["metrics_source"] != SOURCE:
        raise ProviderBindingError("provider binding is not for the pinned metrics Worker")
    if binding["deployment_id"] != EXPECTED_DEPLOYMENT_ID or binding["version_id"] != EXPECTED_VERSION_ID:
        raise ProviderBindingError("deployment/version binding drifted")
    if binding["version_number"] != EXPECTED_VERSION_NUMBER:
        raise ProviderBindingError("deployed version number drifted")
    if not isinstance(binding["script_etag"], str) or not re.fullmatch(r"[0-9a-f]{64}", binding["script_etag"]):
        raise ProviderBindingError("provider script content digest is malformed or absent")
    if binding["script_etag"] != EXPECTED_SCRIPT_ETAG:
        raise ProviderBindingError("provider script content digest drifted")
    if binding["rollout_percentage"] != EXPECTED_ROLLOUT_PERCENTAGE:
        raise ProviderBindingError("provider rollout is not the pinned 100% serving rollout")
    captured = binding["captured_at"]
    if not isinstance(captured, str) or not re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", captured):
        raise ProviderBindingError("provider timestamp must be whole-second UTC")
    try:
        captured_at = datetime.strptime(captured, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    except ValueError as exc:
        raise ProviderBindingError("provider timestamp is invalid") from exc
    now = datetime.now(timezone.utc)
    if captured_at < now - MAX_RECEIPT_AGE or captured_at > now + MAX_RECEIPT_FUTURE:
        raise ProviderBindingError("provider evidence is stale or from the future")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        validate_provider_binding(_load(args.input))
    except ProviderBindingError as exc:
        print(f"B-006 provider evidence FAIL: {exc}")
        return 1
    print("B-006 provider evidence PASS: authenticated Wrangler binding is structurally fail-closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
