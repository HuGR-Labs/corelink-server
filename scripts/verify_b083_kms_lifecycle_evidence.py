#!/usr/bin/env python3
"""Fail-closed contract for B-083's real-KMS lifecycle evidence.

This verifier validates a redacted receipt index, never a credential, KMS
ciphertext, plaintext TCS, CMK identifier, token, or provider response body.
It distinguishes the current exact external blocker from a completed lifecycle.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_EVIDENCE = ROOT / "evidence/owner-actions/B-083/byok-real-kms-lifecycle.json"
SCHEMA_VERSION = 2
PROVIDERS = frozenset({"aws", "gcp", "azure", "vault"})
PREREQUISITES = frozenset({"isolated_test_tenant", "customer_controlled_cmk", "least_privilege_runtime_identity", "protected_operator_authorization", "durable_audit_receipt_sink"})
LIFECYCLE_STEPS = frozenset({"customer_create_or_import", "provider_access", "wrap_unwrap", "revoke_restore", "rotate", "deletion_schedule", "audit_receipts", "tenant_isolation", "failure_retry", "residency"})
RECEIPT_KEYS = frozenset({"status", "receipt_reference", "completed_at", "blocker"})
SECRET_FIELD_MARKERS = ("ciphertext", "plaintext", "secret", "token", "credential", "key_material")
SECRET_VALUE_PATTERNS = (re.compile(r"-----BEGIN [A-Z ]+PRIVATE KEY-----"), re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"), re.compile(r"\b(?:gh[pousr]_|github_pat_|sk_live_|whsec_)\S+", re.IGNORECASE))
SHA256 = re.compile(r"^sha256:[0-9a-f]{64}$")
REDACTED_RECEIPT = re.compile(r"^audit://[a-z0-9][a-z0-9._/-]{2,180}$")
UTC_TIMESTAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")


class EvidenceError(ValueError):
    """Raised when a B-083 evidence record could overstate KMS proof."""


def _exact_keys(value: Any, expected: frozenset[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        actual = sorted(value) if isinstance(value, dict) else type(value).__name__
        raise EvidenceError(f"{label} keys must be exactly {sorted(expected)}, got {actual}")
    return value


def _text(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise EvidenceError(f"{label} must be a non-empty string")
    return value


def _reject_secret_material(value: Any, label: str = "record") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if any(marker in key.lower() for marker in SECRET_FIELD_MARKERS):
                raise EvidenceError(f"{label}.{key} is forbidden: evidence indexes receipts, never material")
            _reject_secret_material(child, f"{label}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _reject_secret_material(child, f"{label}[{index}]")
    elif isinstance(value, str):
        if len(value) > 512:
            raise EvidenceError(f"{label} is too long for a redacted receipt index")
        if any(pattern.search(value) for pattern in SECRET_VALUE_PATTERNS):
            raise EvidenceError(f"{label} contains credential or private-key material")


def _validate_receipt(name: str, receipt: Any, state: str) -> None:
    item = _exact_keys(receipt, RECEIPT_KEYS, f"lifecycle.{name}")
    status = item["status"]
    if status not in {"NOT_EXECUTED", "PASS"}:
        raise EvidenceError(f"lifecycle.{name}.status must be NOT_EXECUTED or PASS")
    reference, completed_at, blocker = item["receipt_reference"], item["completed_at"], item["blocker"]
    if status == "NOT_EXECUTED":
        if reference is not None or completed_at is not None:
            raise EvidenceError(f"lifecycle.{name} cannot carry a receipt before execution")
        _text(blocker, f"lifecycle.{name}.blocker")
        if state != "BLOCKED":
            raise EvidenceError(f"lifecycle.{name} is incomplete while evidence_state is {state}")
        return
    if state != "VERIFIED":
        raise EvidenceError(f"lifecycle.{name} cannot PASS while evidence_state is {state}")
    if not isinstance(reference, str) or not REDACTED_RECEIPT.fullmatch(reference):
        raise EvidenceError(f"lifecycle.{name} PASS requires a redacted audit:// receipt_reference")
    if not isinstance(completed_at, str) or not UTC_TIMESTAMP.fullmatch(completed_at):
        raise EvidenceError(f"lifecycle.{name} PASS requires a UTC completed_at timestamp")
    if blocker is not None:
        raise EvidenceError(f"lifecycle.{name} PASS cannot retain a blocker")


def validate_record(record: Any) -> None:
    """Validate the provider-neutral B-083 record without contacting a provider."""
    root = _exact_keys(record, frozenset({"schema_version", "captured_at", "evidence_state", "tenant_redacted", "runtime", "external_prerequisites", "custody_policy", "lifecycle", "operator", "repository_checks"}), "B-083 evidence")
    _reject_secret_material(root)
    if root["schema_version"] != SCHEMA_VERSION:
        raise EvidenceError(f"schema_version must be {SCHEMA_VERSION}")
    if not isinstance(root["captured_at"], str) or not UTC_TIMESTAMP.fullmatch(root["captured_at"]):
        raise EvidenceError("captured_at must be a UTC second-precision timestamp")
    state = root["evidence_state"]
    if state not in {"BLOCKED", "VERIFIED"}:
        raise EvidenceError("evidence_state must be BLOCKED or VERIFIED")
    _text(root["tenant_redacted"], "tenant_redacted")
    _text(root["operator"], "operator")
    runtime = _exact_keys(root["runtime"], frozenset({"provider", "image_digest", "execution_region"}), "runtime")
    if runtime["provider"] not in PROVIDERS:
        raise EvidenceError("runtime.provider is not a supported provider label")
    prerequisites = _exact_keys(root["external_prerequisites"], PREREQUISITES, "external_prerequisites")
    custody = _exact_keys(root["custody_policy"], frozenset({"create_or_import", "deletion_scheduling", "service_permissions"}), "custody_policy")
    if custody != {"create_or_import": "customer_controlled", "deletion_scheduling": "customer_controlled", "service_permissions": "wrap_unwrap_check_access_only"}:
        raise EvidenceError("custody_policy must keep CMK create/import/deletion customer controlled")
    lifecycle = _exact_keys(root["lifecycle"], LIFECYCLE_STEPS, "lifecycle")
    for name in sorted(LIFECYCLE_STEPS):
        _validate_receipt(name, lifecycle[name], state)
    checks = root["repository_checks"]
    if not isinstance(checks, list) or not checks:
        raise EvidenceError("repository_checks must be a non-empty list")
    for index, check in enumerate(checks):
        item = _exact_keys(check, frozenset({"command", "status", "detail"}), f"repository_checks[{index}]")
        if item["status"] != "PASS":
            raise EvidenceError(f"repository_checks[{index}] must be PASS")
        _text(item["command"], f"repository_checks[{index}].command")
        _text(item["detail"], f"repository_checks[{index}].detail")
    if state == "BLOCKED":
        if root["tenant_redacted"] != "NOT_PROVISIONED":
            raise EvidenceError("BLOCKED evidence must not name a tenant")
        if runtime["image_digest"] is not None or runtime["execution_region"] is not None:
            raise EvidenceError("BLOCKED evidence cannot claim a protected runtime")
        if set(prerequisites.values()) != {"MISSING"}:
            raise EvidenceError("BLOCKED evidence must list every exact external prerequisite as MISSING")
        return
    if not SHA256.fullmatch(runtime["image_digest"] or ""):
        raise EvidenceError("VERIFIED evidence requires the shipped image sha256 digest")
    if not isinstance(runtime["execution_region"], str) or not runtime["execution_region"].strip():
        raise EvidenceError("VERIFIED evidence requires the protected execution region")
    if set(prerequisites.values()) != {"PROVISIONED"}:
        raise EvidenceError("VERIFIED evidence requires every external prerequisite")
    if root["tenant_redacted"] == "NOT_PROVISIONED":
        raise EvidenceError("VERIFIED evidence requires a redacted protected test tenant reference")


def _read_record(path: Path) -> Any:
    if not path.is_file() or path.is_symlink():
        raise EvidenceError(f"evidence path must be a regular file: {path}")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise EvidenceError(f"invalid JSON evidence: {exc}") from exc


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path, default=DEFAULT_EVIDENCE)
    args = parser.parse_args(argv)
    try:
        validate_record(_read_record(args.evidence))
    except EvidenceError as exc:
        print(f"B-083 KMS lifecycle evidence: FAIL: {exc}", file=sys.stderr)
        return 1
    print("B-083 KMS lifecycle evidence: PASS (typed external blocker; no KMS claim)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
