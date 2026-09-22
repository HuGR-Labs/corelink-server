#!/usr/bin/env python3
"""Fail closed validation for B-083's isolated AWS KMS probe receipt.

The receipt indexes only hashes of CloudTrail event IDs.  It cannot certify a
CoreLink runtime, nor can a partial provider probe be promoted to lifecycle
completion.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_EVIDENCE = ROOT / "evidence/owner-actions/B-083/aws-kms-isolated-probe.json"
UTC_TIMESTAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
RECEIPT = re.compile(r"^audit://cloudtrail/sha256:[0-9a-f]{64}$")
SUSPICIOUS_VALUE = (
    re.compile(r"-----BEGIN [A-Z ]+PRIVATE KEY-----"),
    re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
    re.compile(r"\b(?:gh[pousr]_|github_pat_|sk_live_|whsec_|xox[baprs]-)\S+", re.IGNORECASE),
    re.compile(r"\b[A-Za-z0-9/+=]{40}\b"),
)
PASS_STEPS = frozenset({"customer_create", "grant_boundary", "wrap_unwrap", "context_rejection", "disabled_rejection", "propagation_retry"})
INCOMPLETE_STEPS = frozenset({"rotation", "revocation"})
OBSERVATION_STEPS = PASS_STEPS | INCOMPLETE_STEPS | {"deletion_schedule"}


class EvidenceError(ValueError):
    """Raised when the partial provider probe could overstate its result."""


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, child in pairs:
        if key in value:
            raise EvidenceError(f"duplicate JSON key: {key}")
        value[key] = child
    return value


def _exact(value: Any, keys: frozenset[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != keys:
        actual = sorted(value) if isinstance(value, dict) else type(value).__name__
        raise EvidenceError(f"{label} keys must be exactly {sorted(keys)}, got {actual}")
    return value


def _text(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > 512:
        raise EvidenceError(f"{label} must be a non-empty receipt-safe string")
    if any(pattern.search(value) for pattern in SUSPICIOUS_VALUE):
        raise EvidenceError(f"{label} contains material or a credential-shaped value")
    return value


def _receipts(value: Any, label: str, required: bool) -> list[str]:
    if not isinstance(value, list) or (required and not value):
        raise EvidenceError(f"{label} must {'not be empty' if required else 'be a list'}")
    if len(value) != len(set(value)):
        raise EvidenceError(f"{label} must not repeat a receipt")
    for receipt in value:
        if not isinstance(receipt, str) or not RECEIPT.fullmatch(receipt):
            raise EvidenceError(f"{label} must contain only redacted CloudTrail receipt hashes")
    return value


def _observation(name: str, value: Any, seen_receipts: set[str]) -> None:
    item = _exact(value, frozenset({"status", "audit_receipts", "detail", "blocker"}), f"observations.{name}")
    status = item["status"]
    _text(item["detail"], f"observations.{name}.detail")
    if name in PASS_STEPS:
        if status != "PASS" or item["blocker"] is not None:
            raise EvidenceError(f"observations.{name} must be receipt-backed PASS without a blocker")
        receipts = _receipts(item["audit_receipts"], f"observations.{name}.audit_receipts", required=True)
        reused = set(receipts).intersection(seen_receipts)
        if reused:
            raise EvidenceError(f"observations.{name} reuses a receipt from another observation")
        seen_receipts.update(receipts)
        return
    if name in INCOMPLETE_STEPS:
        if status != "NOT_COMPLETED" or item["audit_receipts"] != []:
            raise EvidenceError(f"observations.{name} must remain NOT_COMPLETED without a receipt")
        _text(item["blocker"], f"observations.{name}.blocker")
        return
    if status != "OBSERVED_WITHOUT_REDACTED_RECEIPT" or item["audit_receipts"] != []:
        raise EvidenceError("observations.deletion_schedule must not turn uncorrelated readback into a receipt-backed PASS")
    _text(item["blocker"], "observations.deletion_schedule.blocker")


def validate_record(record: Any) -> None:
    root = _exact(record, frozenset({"schema_version", "captured_at", "provider", "scope", "identity_boundary", "observations", "runtime_binding", "conclusion"}), "AWS KMS probe")
    if root["schema_version"] != 1:
        raise EvidenceError("schema_version must be 1")
    if not isinstance(root["captured_at"], str) or not UTC_TIMESTAMP.fullmatch(root["captured_at"]):
        raise EvidenceError("captured_at must be a UTC second-precision timestamp")
    if root["provider"] != "aws-kms":
        raise EvidenceError("provider must be aws-kms")
    scope = _exact(root["scope"], frozenset({"environment", "region", "resource_identifiers"}), "scope")
    if scope["environment"] != "isolated_nonproduction" or scope["resource_identifiers"] != "NONE_RETAINED":
        raise EvidenceError("scope must be isolated nonproduction with no retained resource identifier")
    _text(scope["region"], "scope.region")
    identity = _exact(root["identity_boundary"], frozenset({"grant_operations", "persistent_credentials", "temporary_role"}), "identity_boundary")
    if identity != {"grant_operations": ["Decrypt", "DescribeKey", "Encrypt"], "persistent_credentials": "NOT_CREATED", "temporary_role": "DELETED"}:
        raise EvidenceError("identity boundary must be the deleted grant-only role with no persistent credentials")
    observations = _exact(root["observations"], OBSERVATION_STEPS, "observations")
    seen_receipts: set[str] = set()
    for name in sorted(OBSERVATION_STEPS):
        _observation(name, observations[name], seen_receipts)
    runtime = _exact(root["runtime_binding"], frozenset({"status", "blocker"}), "runtime_binding")
    if runtime["status"] != "NOT_PROVISIONED":
        raise EvidenceError("an AWS CLI probe cannot claim a protected CoreLink runtime")
    _text(runtime["blocker"], "runtime_binding.blocker")
    conclusion = _exact(root["conclusion"], frozenset({"state", "closure_eligibility", "reason"}), "conclusion")
    if conclusion["state"] != "PARTIAL_PROVIDER_PROBE" or conclusion["closure_eligibility"] != "INELIGIBLE":
        raise EvidenceError("partial provider evidence is never eligible to close B-083")
    _text(conclusion["reason"], "conclusion.reason")


def _read_record(path: Path) -> Any:
    if not path.is_file() or path.is_symlink():
        raise EvidenceError(f"evidence path must be a regular file: {path}")
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=_reject_duplicate_keys)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise EvidenceError(f"invalid JSON evidence: {exc}") from exc


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path, default=DEFAULT_EVIDENCE)
    args = parser.parse_args(argv)
    try:
        validate_record(_read_record(args.evidence))
    except EvidenceError as exc:
        print(f"B-083 AWS KMS probe: FAIL: {exc}", file=sys.stderr)
        return 1
    print("B-083 AWS KMS probe: PASS (partial provider boundary; no runtime completion claim)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
