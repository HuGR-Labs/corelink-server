#!/usr/bin/env python3
"""Fail-closed verifier for the #1644 Critical-vendor public-source packets."""

from __future__ import annotations

import json
import re
from datetime import date
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
EXPECTED_RECORDS = {
    "cloudflare": "docs/compliance/vendor-reviews/cloudflare-dpa-review-2026-04.md",
    "stripe": "docs/compliance/vendor-reviews/stripe-dpa-review-2026-04.md",
    "clerk": "docs/compliance/vendor-reviews/clerk-dpa-review-2026-04.md",
    "aws-kms": "docs/compliance/vendor-reviews/aws-kms-review-2026-09.md",
    "gcp-kms": "docs/compliance/vendor-reviews/gcp-kms-review-2026-09.md",
    "azure-key-vault": "docs/compliance/vendor-reviews/azure-key-vault-review-2026-09.md",
    "drata": "docs/compliance/vendor-reviews/drata-review-2026-09.md",
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
    "No executed CoreLink agreement, human approval, signature, SOC 2 report, ISO certificate, or Drata workspace export is claimed.",
    "Vendor-public statements are context only and are not CoreLink-specific completed reviews.",
    "Public vendor pages do not close the Critical review cadence item.",
    "The manifest does not authorize editing effective legal or contractual text.",
]
EXPECTED_PACKET_KIND = ("public_source_assessment_only", "evidence_insufficient_not_approved")


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
    capture_date = date.fromisoformat(packet.get("captured_at", ""))
    last_review = date.fromisoformat(cadence["baseline_last_review"])
    formal_due = date.fromisoformat(cadence["scheduled_next_review"])
    if capture_date != date(2026, 9, 22):
        raise ValueError("capture date drifted")
    if (capture_date - formal_due).days != cadence["days_overdue_at_capture"]:
        raise ValueError("overdue-day arithmetic drifted")
    if (capture_date - last_review).days != 130:
        raise ValueError("baseline elapsed-day arithmetic drifted")

    vendors = packet.get("vendors")
    if not isinstance(vendors, list) or {v.get("id") for v in vendors} != set(EXPECTED):
        raise ValueError("Critical vendor population is not exact")
    if len(vendors) != len(EXPECTED):
        raise ValueError("duplicate Critical vendor id")
    for vendor in vendors:
        vendor_id = vendor.get("id")
        if vendor.get("name") != EXPECTED[vendor_id] or vendor.get("category") != "Critical":
            raise ValueError(f"vendor identity/category drifted: {vendor_id}")
        if (vendor.get("record_kind"), vendor.get("evidence_status")) != EXPECTED_PACKET_KIND:
            raise ValueError(f"public assessment boundary drifted: {vendor_id}")
        if vendor.get("owner") != "VP-Sec":
            raise ValueError(f"owner drifted: {vendor_id}")
        if vendor.get("last_review") != "2026-05-15" or vendor.get("next_review") != "2026-08-15":
            raise ValueError(f"formal review dates drifted: {vendor_id}")
        record = vendor.get("canonical_record")
        if record != EXPECTED_RECORDS[vendor_id]:
            raise ValueError(f"canonical packet missing or changed: {vendor_id}")
        evidence = vendor.get("current_repo_evidence")
        if not isinstance(evidence, list) or record not in evidence or any(
            not isinstance(path, str)
            or path.startswith("/")
            or ".." in path.split("/")
            or not (ROOT / path).is_file()
            for path in evidence
        ):
            raise ValueError(f"current repository evidence is missing or unsafe: {vendor_id}")
        vendor_packet = vendor.get("refresh_packet")
        if not isinstance(vendor_packet, dict) or set(vendor_packet) != set(REQUIRED_REFRESH_FIELD_ORDER):
            raise ValueError(f"refresh packet fields drifted: {vendor_id}")
        for field in REQUIRED_REFRESH_FIELD_ORDER:
            expected_status = (
                "pending_human_review" if field in {"human_decision", "signer"} else "pending_external"
            )
            if (
                not isinstance(vendor_packet[field], dict)
                or vendor_packet[field].get("status") != expected_status
            ):
                raise ValueError(f"refresh packet status must remain pending: {vendor_id} {field}")
        if vendor_packet["human_decision"].get("decision") is not None:
            raise ValueError(f"unapproved decision claimed: {vendor_id}")
        signer = vendor_packet["signer"]
        if signer.get("name") is not None or signer.get("signed_at") is not None:
            raise ValueError(f"unapproved signature claimed: {vendor_id}")

        text = (ROOT / record).read_text()
        required_text = [
            "STATUS: TEMPLATE — formal Legal review pending",
            "2026-09-22",
            "| Reviewer | `TBD (named Legal Counsel / Privacy Officer)`; no formal reviewer or decision is recorded. |",
            "| Review outcome | `TBD (formal Legal outcome pending)`; public-source assessment: **Evidence insufficient — not approved**. |",
            "| Human decision | Pending formal human review; no approval decision is recorded. |",
            "| Signer | `TBD (named human signer pending)`; no signature is recorded. |",
            "| Next review due | `2026-08-15`",
            "38 calendar days overdue",
            "does not satisfy the formal review",
            "alter its due date",
            "### Official vendor sources consulted on 2026-09-22",
            "| Scope |",
            "| Subprocessors |",
            "| Residency |",
            "| Security changes / current assurance |",
            "| DPA changes |",
            "| Renewal date |",
        ]
        if any(value not in text for value in required_text):
            raise ValueError(f"assessment is incomplete or claims approval: {vendor_id}")
        if "2026-11-20" in text or "next review due | `2026-09-22`" in text.lower():
            raise ValueError(f"public assessment reset or invented a formal due date: {vendor_id}")
        if len(re.findall(r"https://", text)) < 2:
            raise ValueError(f"primary-source references are missing: {vendor_id}")
        for field in ("Reviewer", "Review outcome", "Human decision", "Signer", "Next review due"):
            if len(re.findall(rf"(?m)^\| {re.escape(field)} \|", text)) != 1:
                raise ValueError(f"duplicate or missing approval-boundary field: {vendor_id} {field}")
        if re.search(r"(?im)^.*\b(?:approved|signed|accepted)\s+by\s+[A-Z][A-Za-z .'-]+", text):
            raise ValueError(f"fabricated named approval or signature: {vendor_id}")

    if packet.get("non_claims") != EXPECTED_NON_CLAIMS:
        raise ValueError("non-claim boundary drifted")
    return {
        "status": "pending_external",
        "critical_vendors": len(vendors),
        "formal_due_date": "2026-08-15",
        "days_overdue": 38,
        "public_assessments_not_approved": len(vendors),
    }


if __name__ == "__main__":
    print(json.dumps(verify(), sort_keys=True))
