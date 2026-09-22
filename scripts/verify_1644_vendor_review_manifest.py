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
REQUIRED_REFRESH_FIELD_ORDER = [
    "scope",
    "subprocessors",
    "residency",
    "security_changes",
    "dpa_changes",
    "renewal_date",
    "human_decision",
    "signer",
]
REQUIRED_REFRESH_FIELDS = set(REQUIRED_REFRESH_FIELD_ORDER)
EXPECTED_SOURCE_REGISTER = "specs/_compliance/VENDOR-RISK-REGISTER.md"
EXPECTED_REFRESH_SCHEMA = {
    "required_fields": REQUIRED_REFRESH_FIELD_ORDER,
    "field_rule": (
        "Each field must be completed from dated evidence before the review can move "
        "out of pending_external; null means owner action required."
    ),
    "approval_rule": (
        "A named human reviewer and signer must record the decision; repository "
        "automation cannot approve legal or contractual terms."
    ),
}
EXPECTED_NON_CLAIMS = [
    "No review, signature, approval, SOC 2 report, ISO certificate, or Drata export is claimed.",
    "Repository templates are structural packets only and are not completed reviews.",
    "Public vendor pages do not close the Critical review cadence item.",
    "The manifest does not authorize editing effective legal or contractual text.",
]
EXPECTED_VENDOR_METADATA = {vendor_id: ("repository_assessment", "evidence_insufficient_not_approved") for vendor_id in EXPECTED}


def verify() -> dict[str, object]:
    packet = json.loads(MANIFEST.read_text())
    if packet.get("schema_version") != 1 or packet.get("issue") != "#1644":
        raise ValueError("manifest identity/schema drifted")
    if packet.get("backlog_id") != "B-032" or packet.get("status") != "pending_external":
        raise ValueError("manifest must remain pending external")
    if packet.get("source_register") != EXPECTED_SOURCE_REGISTER or not (
        ROOT / EXPECTED_SOURCE_REGISTER
    ).is_file():
        raise ValueError("source register is missing or drifted")
    if packet.get("refresh_packet_schema") != EXPECTED_REFRESH_SCHEMA:
        raise ValueError("refresh packet schema drifted")
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
        if (
            vendor.get("record_kind"),
            vendor.get("evidence_status"),
        ) != EXPECTED_VENDOR_METADATA[vendor["id"]]:
            raise ValueError(f"evidence boundary drifted: {vendor.get('id')}")
        if vendor.get("owner") != "VP-Sec":
            raise ValueError(f"owner drifted: {vendor.get('id')}")
        if vendor.get("last_review") != "2026-05-15" or vendor.get("next_review") != "2026-08-15":
            raise ValueError(f"review dates drifted: {vendor.get('id')}")
        record = vendor.get("canonical_record")
        if record is not None and not re.fullmatch(r"docs/compliance/vendor-reviews/[a-z0-9-]+\.md", record):
            raise ValueError(f"unsafe canonical record path: {vendor.get('id')}")
        evidence = vendor.get("current_repo_evidence")
        if not isinstance(evidence, list) or not evidence or any(
            not isinstance(path, str)
            or path.startswith("/")
            or ".." in path.split("/")
            or not (ROOT / path).is_file()
            for path in evidence
        ):
            raise ValueError(f"current repository evidence is missing or unsafe: {vendor.get('id')}")
        if record is None or not (ROOT / record).is_file():
            raise ValueError(f"canonical assessment packet is missing: {vendor.get('id')}")
        assessment_text = (ROOT / record).read_text()
        required_markers = (
            "Review date | `2026-09-22`",
            "Reviewer | `Automated public-source assessment (not Legal Counsel)`",
            "Review outcome | `Evidence insufficient — not approved`",
            "Next review due | `2026-11-20",
            "First-party source consulted on 2026-09-22",
        )
        if any(marker not in assessment_text for marker in required_markers):
            raise ValueError(f"assessment completeness drifted: {vendor.get('id')}")
        vendor_packet = vendor.get("refresh_packet")
        if not isinstance(vendor_packet, dict) or set(vendor_packet) != REQUIRED_REFRESH_FIELDS:
            raise ValueError(f"refresh packet fields drifted: {vendor.get('id')}")
        for field in REQUIRED_REFRESH_FIELDS:
            expected_status = "pending_human_review" if field in {"human_decision", "signer"} else "pending_external"
            if (
                not isinstance(vendor_packet[field], dict)
                or vendor_packet[field].get("status") != expected_status
            ):
                raise ValueError(f"refresh packet status must remain pending: {vendor.get('id')} {field}")
        if vendor_packet["human_decision"].get("decision") is not None:
            raise ValueError(f"unapproved decision claimed: {vendor.get('id')}")
        signer = vendor_packet["signer"]
        if signer.get("name") is not None or signer.get("signed_at") is not None:
            raise ValueError(f"unapproved signature claimed: {vendor.get('id')}")
    if packet.get("non_claims") != EXPECTED_NON_CLAIMS:
        raise ValueError("non-claim boundary drifted")
    return {"status": "pending_external", "critical_vendors": len(vendors), "days_overdue": 38}


if __name__ == "__main__":
    print(json.dumps(verify(), sort_keys=True))
