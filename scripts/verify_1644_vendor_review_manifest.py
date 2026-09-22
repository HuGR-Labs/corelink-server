#!/usr/bin/env python3
"""Fail-closed structural verifier for the #1644 Critical-vendor census."""

from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "docs/handoff/2026-09-22-1644-critical-vendor-review-manifest.json"
EXPECTED = {
    "cloudflare": "Cloudflare, Inc.",
    "stripe": "Stripe, Inc.",
    "clerk": "Clerk, Inc.",
    "aws-kms": "Amazon Web Services, Inc.",
    "gcp-kms": "Google LLC (Google Cloud)",
    "azure-key-vault": "Microsoft Corporation (Azure)",
    "drata": "Drata, Inc.",
}


def verify() -> dict[str, object]:
    packet = json.loads(MANIFEST.read_text())
    if packet.get("schema_version") != 1 or packet.get("issue") != "#1644":
        raise ValueError("manifest identity/schema drifted")
    if packet.get("backlog_id") != "B-032" or packet.get("status") != "pending_external":
        raise ValueError("manifest must remain pending external")
    cadence = packet.get("cadence")
    if cadence != {
        "critical": "quarterly",
        "cadence_days": 90,
        "baseline_last_review": "2026-05-15",
        "scheduled_next_review": "2026-08-15",
        "days_overdue_at_capture": 38,
    }:
        raise ValueError("cadence facts drifted")
    vendors = packet.get("vendors")
    if not isinstance(vendors, list) or {v.get("id") for v in vendors} != set(EXPECTED):
        raise ValueError("Critical vendor population is not exact")
    if len(vendors) != len(EXPECTED):
        raise ValueError("duplicate Critical vendor id")
    for vendor in vendors:
        if vendor.get("name") != EXPECTED[vendor["id"]] or vendor.get("category") != "Critical":
            raise ValueError(f"vendor identity/category drifted: {vendor.get('id')}")
        if vendor.get("owner") != "VP-Sec":
            raise ValueError(f"owner drifted: {vendor.get('id')}")
        if vendor.get("last_review") != "2026-05-15" or vendor.get("next_review") != "2026-08-15":
            raise ValueError(f"review dates drifted: {vendor.get('id')}")
        record = vendor.get("canonical_record")
        if record is not None and not re.fullmatch(r"docs/compliance/vendor-reviews/[a-z0-9-]+\.md", record):
            raise ValueError(f"unsafe canonical record path: {vendor.get('id')}")
        if vendor.get("id") in {"aws-kms", "gcp-kms", "azure-key-vault", "drata"} and record is not None:
            raise ValueError(f"external-only record unexpectedly claimed local path: {vendor.get('id')}")
    non_claims = packet.get("non_claims")
    if not isinstance(non_claims, list) or len(non_claims) != 4:
        raise ValueError("non-claim boundary drifted")
    return {"status": "pending_external", "critical_vendors": len(vendors), "days_overdue": 38}


if __name__ == "__main__":
    print(json.dumps(verify(), sort_keys=True))
