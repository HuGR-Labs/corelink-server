#!/usr/bin/env python3
"""Validate the owner supplied staging identity receipt for the load lane.

The receipt is an environment secret because it is issued by the staging
owner.  This command writes only its public, non-secret fields to the run
artifact and refuses every target except the canonical staging origin.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
from pathlib import Path
from urllib.parse import urlsplit


CANONICAL_TARGET = "https://staging.corelink.humangr.com"
RECEIPT_SCHEMA = 1
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
TENANT_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
)
ISO_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
REQUIRED = {
    "schema",
    "environment",
    "target",
    "tenant_id",
    "deployment_sha",
    "issued_at",
    "expires_at",
}
WORKFLOW_REQUIREMENTS = (
    "workflow_dispatch:",
    "Type \"run-bounded-load\"",
    "K6_TARGET_IDENTITY_RECEIPT",
    "scripts/validate_load_target_receipt.py",
    "timeout-minutes: 45",
    "timeout-minutes: 1",
    "_internal/load-test/teardown",
    "if: steps.filter.outputs.enabled == 'true' && steps.target_host.outcome == 'success' && always()",
)


class ReceiptError(ValueError):
    pass


def tenant_sha256(tenant_id: str) -> str:
    """Return the stable, non-identifier reference used in B-103 evidence."""
    return hashlib.sha256(tenant_id.encode("ascii")).hexdigest()


def require_tenant_binding(receipt: dict[str, object], configured_tenant: str) -> str:
    """Fail closed unless the protected tenant variable matches the receipt."""
    tenant_id = receipt.get("tenant_id")
    if not isinstance(tenant_id, str) or not configured_tenant or configured_tenant != tenant_id:
        raise ReceiptError("configured tenant does not match the validated target receipt")
    return tenant_sha256(tenant_id)


def workflow_gaps(workflow: str) -> list[str]:
    """Check the load lane's static safety boundary for contract tests."""
    gaps = [f"missing:{needle}" for needle in WORKFLOW_REQUIREMENTS if needle not in workflow]
    if "  schedule:" in workflow or "- cron:" in workflow:
        gaps.append("unattended schedule")
    if "${{ secrets.K6_TARGET_HOST }}" not in workflow:
        gaps.append("target must come from the protected staging secret")
    if "K6_TARGET_HOST}/_internal/load-test/teardown" not in workflow:
        gaps.append("teardown must derive from validated target")
    return gaps


def _timestamp(value: object, field: str) -> dt.datetime:
    if not isinstance(value, str) or not ISO_RE.fullmatch(value):
        raise ReceiptError(f"{field} must be an UTC timestamp (YYYY-MM-DDTHH:MM:SSZ)")
    try:
        return dt.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.UTC)
    except ValueError as exc:
        raise ReceiptError(f"{field} is not a valid UTC timestamp") from exc


def validate(receipt: object, target: str, *, now: dt.datetime | None = None) -> dict[str, object]:
    if not isinstance(receipt, dict):
        raise ReceiptError("receipt must be a JSON object")
    if set(receipt) != REQUIRED:
        raise ReceiptError("receipt fields are not the exact required set")
    if receipt.get("schema") != RECEIPT_SCHEMA:
        raise ReceiptError("unsupported receipt schema")
    if receipt.get("environment") != "staging":
        raise ReceiptError("receipt environment must be staging")

    def canonical(value: object, field: str) -> str:
        if not isinstance(value, str) or value.rstrip("/") != CANONICAL_TARGET:
            raise ReceiptError(f"{field} must identify the canonical staging target")
        parsed = urlsplit(value)
        if parsed.scheme != "https" or parsed.netloc != "staging.corelink.humangr.com" or parsed.path not in ("", "/") or parsed.query or parsed.fragment:
            raise ReceiptError(f"{field} must be an origin without a path or query")
        return CANONICAL_TARGET

    canonical(receipt.get("target"), "receipt target")
    canonical(target, "workflow target")
    deployment_sha = receipt.get("deployment_sha")
    if not isinstance(deployment_sha, str) or not SHA_RE.fullmatch(deployment_sha):
        raise ReceiptError("deployment_sha must be a 40 character lowercase commit SHA")
    tenant_id = receipt.get("tenant_id")
    if not isinstance(tenant_id, str) or not TENANT_RE.fullmatch(tenant_id):
        raise ReceiptError("tenant_id must be a lowercase canonical UUID")
    issued = _timestamp(receipt.get("issued_at"), "issued_at")
    expires = _timestamp(receipt.get("expires_at"), "expires_at")
    current = now or dt.datetime.now(dt.UTC)
    if expires <= issued or expires <= current:
        raise ReceiptError("receipt is expired or has no positive lifetime")
    if expires - issued > dt.timedelta(hours=24):
        raise ReceiptError("receipt lifetime exceeds the 24 hour staging lease")
    return {
        "schema": RECEIPT_SCHEMA,
        "environment": "staging",
        "target": CANONICAL_TARGET,
        "tenant_id": tenant_id,
        "deployment_sha": deployment_sha,
        "issued_at": receipt["issued_at"],
        "expires_at": receipt["expires_at"],
    }


def artifact_receipt(receipt: dict[str, object], *, redact_tenant: bool) -> dict[str, object]:
    """Return the receipt representation safe to retain as a CI artifact.

    A staging identity receipt is an input authority, not evidence that needs
    to disclose its tenant UUID.  Bounded-load artifacts retain a stable
    one-way reference so their wire evidence can still be tied to the
    validated receipt without publishing tenant identity.
    """
    if not redact_tenant:
        return receipt
    tenant = receipt["tenant_id"]
    assert isinstance(tenant, str)
    redacted = dict(receipt)
    redacted.pop("target", None)
    redacted["tenant_id"] = "[REDACTED]"
    redacted["tenant_id_sha256"] = "sha256:" + hashlib.sha256(tenant.encode("ascii")).hexdigest()
    return redacted


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--receipt-env", default="K6_TARGET_IDENTITY_RECEIPT")
    parser.add_argument("--tenant-env", help="require this environment variable to match the receipt tenant")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--github-output", type=Path)
    parser.add_argument(
        "--redact-tenant",
        action="store_true",
        help="replace the tenant UUID in the output artifact with a stable hash reference",
    )
    args = parser.parse_args(argv)
    raw = os.environ.get(args.receipt_env, "")
    try:
        receipt = validate(json.loads(raw), args.target)
        tenant_hash: str | None = None
        if args.tenant_env:
            tenant_hash = require_tenant_binding(receipt, os.environ.get(args.tenant_env, ""))
        elif args.redact_tenant:
            raise ReceiptError("--redact-tenant requires --tenant-env for a bound B-103 receipt")
    except (json.JSONDecodeError, ReceiptError) as exc:
        print(f"::error::target identity receipt rejected: {exc}")
        return 1
    args.output.write_text(
        json.dumps(artifact_receipt(receipt, redact_tenant=args.redact_tenant), sort_keys=True, indent=2) + "\n",
        encoding="utf-8",
    )
    if args.github_output:
        with args.github_output.open("a", encoding="utf-8") as output:
            output.write(f"deployment_sha={receipt['deployment_sha']}\n")
            if args.redact_tenant:
                output.write(f"tenant_sha256={tenant_hash}\n")
    print(f"target receipt accepted: target={CANONICAL_TARGET} deployment_sha={receipt['deployment_sha']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
