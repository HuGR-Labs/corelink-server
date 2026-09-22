#!/usr/bin/env python3
"""Credentialless, read-only contract verifier for issue #1641.

This checks the repository-owned PagerDuty event path, D1 receipt schema and
escalation-policy references. It never opens a socket, reads a secret, or
sends/acknowledges/resolves an incident. A PASS is only repository evidence;
the PagerDuty incident timeline and human receipt remain an owner artifact.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "evidence/i1641/pagerduty-contract-manifest.json"

EXPECTED_RECEIPT_CHAIN = [
    "scheduler acceptance",
    "receiver D1 triggered row",
    "PagerDuty incident id and timestamp",
    "signed acknowledgement or escalation webhook",
    "D1 outcome and mtta_ms",
]
EXPECTED_PROHIBITED = [
    "routing keys or webhook secrets in git",
    "direct PagerDuty calls from the scheduler",
    "production synthetic-drill activation without an explicit reviewed deployment",
]


def fail(message: str) -> int:
    print(f"I-1641 FAIL: {message}", file=sys.stderr)
    return 1


def require(text: str, fragment: str, label: str) -> str | None:
    if fragment not in text:
        return f"missing {label}: {fragment}"
    return None


def validate_manifest(manifest: dict) -> str | None:
    """Keep the credentialless gate bound to the live B-008 closure contract."""
    if not isinstance(manifest, dict):
        return "manifest root must be an object"
    if manifest.get("schema") != "corelink.pagerduty.contract-manifest.v1":
        return "manifest schema is not the PagerDuty contract version"
    if manifest.get("issue") != 1641 or manifest.get("backlog_id") != "B-008":
        return "manifest must identify issue 1641 and backlog item B-008"
    if manifest.get("credentialless") is not True:
        return "manifest must remain credentialless"
    if manifest.get("network_calls") is not False or manifest.get("mutating_actions") is not False:
        return "manifest verifier posture must be read-only and network-free"
    if manifest.get("status") != "repository_contract_ready_external_receipt_pending":
        return "manifest must keep the external PagerDuty receipt explicitly pending"
    if manifest.get("github_hosted_command") != "python3 scripts/verify_i1641_pagerduty_contract.py":
        return "manifest must name the hosted verifier command"

    receipts = manifest.get("receipts")
    if not isinstance(receipts, dict):
        return "manifest receipts contract is missing"
    if receipts.get("required_chain") != EXPECTED_RECEIPT_CHAIN:
        return "manifest receipt chain must include the external incident and human receipt"
    if receipts.get("external_owner_artifact") != (
        "PagerDuty incident timeline export plus matching D1 drill row for one staging synthetic drill."
    ):
        return "manifest must name the canonical external owner artifact"
    if receipts.get("external_owner") != "SRE Lead":
        return "manifest must keep the SRE Lead as the external evidence owner"
    blocker = receipts.get("blocker")
    if not isinstance(blocker, str) or "PagerDuty" not in blocker or "human acknowledgement" not in blocker:
        return "manifest blocker must explain why repository evidence cannot close human delivery"
    if manifest.get("prohibited") != EXPECTED_PROHIBITED:
        return "manifest prohibited actions must retain the PagerDuty safety boundary"
    return None


def verify(root: Path = ROOT) -> int:
    try:
        manifest = json.loads((root / MANIFEST.relative_to(ROOT)).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return fail(f"manifest unreadable: {error}")

    manifest_error = validate_manifest(manifest)
    if manifest_error:
        return fail(manifest_error)

    paths = manifest.get("path")
    if not isinstance(paths, dict):
        return fail("manifest source paths are missing")
    try:
        source = {name: (root / value).read_text(encoding="utf-8") for name, value in paths.items()}
    except (OSError, UnicodeDecodeError) as error:
        return fail(f"contract source unreadable: {error}")

    checks = (
        (source["scheduler_contract"], '"0 14 * * 1"', "Monday synthetic cron"),
        (source["scheduler"], 'const deliveryId = `SP-${controller.scheduledTime}`', "stable dedup key"),
        (source["scheduler"], 'correlation_id: `PAT-CORRELATION-ID-001:${deliveryId}`', "correlation id"),
        (source["scheduler"], 'delivery_mode: ((week % 4) + 4) % 4 === 3 ? "deferred" : "immediate"', "boundary deferral"),
        (source["receiver_contract"], 'export const PAGERDUTY_EVENTS_URL = "https://events.pagerduty.com/v2/enqueue"', "canonical Events API endpoint"),
        (source["receiver_contract"], 'export const SYNTHETIC_SERVICE = "synthetic-drill"', "synthetic service isolation"),
        (source["receiver_contract"], 'export const SYNTHETIC_EVENT_SEVERITY = "info"', "synthetic wire severity"),
        (source["receiver"], "await persistDelivery(env, envelope)", "D1 receipt before page"),
        (source["receiver"], "if (response === null || !response.ok)", "non-2xx delivery failure"),
        (source["receiver"], "verifyPagerDutySignature", "signed webhook receipt"),
        (source["receiver"], "SET outcome = ?, engineer_slug = ?, ack_ts_ms = ?, mtta_ms = ?, ack_vector = ?", "ack outcome receipt"),
        (source["lifecycle_migration"], "CREATE TABLE IF NOT EXISTS synthetic_page_audit_events", "append-only audit receipts"),
        (source["lifecycle_migration"], "event_type      TEXT    NOT NULL CHECK (event_type IN ('triggered', 'delivered', 'acked', 'escalated'))", "receipt event taxonomy"),
        (source["policy"], "tier_1_to_tier_2_after_seconds: 300", "tier 1 escalation delay"),
        (source["policy"], "tier_2_to_tier_3_after_seconds: 600", "tier 2 escalation delay"),
        (source["policy"], "Tier 3 → manual escalation", "tier 3 escalation boundary"),
        (source["runbook"], "Confirm PagerDuty incident created on the `synthetic-drill`", "operator incident receipt step"),
        (source["runbook"], "Confirm ack within 5 min", "operator acknowledgement receipt step"),
        (source["runbook"], "Confirm PagerDuty incident created", "external incident evidence requirement"),
    )
    for text, fragment, label in checks:
        error = require(text, fragment, label)
        if error:
            return fail(error)

    pagerduty_config = source["pagerduty_config"]
    if not re.search(r"(?m)^\s*- name: corelink-incident-response\s*$", pagerduty_config):
        return fail("canonical escalation policy is missing")
    for schedule in ("corelink-oncall-tier-1", "corelink-oncall-tier-2", "corelink-oncall-tier-3"):
        if schedule not in pagerduty_config:
            return fail(f"escalation policy target is missing: {schedule}")

    # Secrets must remain external. Detect assignments, while permitting the
    # documented variable names and secret-binding instructions.
    secret_literals = re.compile(r"(?im)^\s*(?:PAGERDUTY_(?:SYNTHETIC_)?ROUTING_KEY|PAGERDUTY_WEBHOOK_SECRET)\s*=\s*[^$<{\s][^\n#]*$")
    for name, text in source.items():
        if secret_literals.search(text):
            return fail(f"possible literal PagerDuty secret in {name}")

    print("I-1641 PASS: credentialless PagerDuty event/receipt/escalation contract verified; external human receipt remains pending")
    return 0


if __name__ == "__main__":
    raise SystemExit(verify())
