#!/usr/bin/env python3
"""Build the redacted, target-bound receipt for the protected B-046 proof."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from datetime import datetime
from pathlib import Path
from typing import Mapping


SCHEMA = "corelink.b046.s3-object-lock-protected-proof.v1"
EXPECTED_REPOSITORY_ID = "1232040291"
EXPECTED_REPOSITORY = "HuGR-dev/corelink-server"
MAX_COST_USD_MICROS = 5_000_000
DIGEST_SUMMARY = re.compile(r"^([0-9]+)/([0-9]+) digest files valid$")
LOG_SUMMARY = re.compile(r"^([0-9]+)/([0-9]+) log files valid$")


class ReceiptError(ValueError):
    """Protected proof inputs do not establish the approved target contract."""


def _required(values: Mapping[str, str], key: str) -> str:
    value = values.get(key, "")
    if not value or value != value.strip() or "\n" in value or "\r" in value:
        raise ReceiptError(f"required proof input {key} is missing or malformed")
    return value


def _sha256(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def _utc_timestamp(value: str) -> str:
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ReceiptError("retention expiry is not an ISO-8601 timestamp") from exc
    if parsed.tzinfo is None or parsed.utcoffset().total_seconds() != 0:
        raise ReceiptError("retention expiry must be timezone-aware UTC")
    return parsed.isoformat().replace("+00:00", "Z")


def validate_digest_output(output: str) -> None:
    """Require nonempty, fully valid CloudTrail digest and log summaries."""
    if "INVALID" in output:
        raise ReceiptError("CloudTrail validation output contains an invalid file")
    summaries = [line.strip() for line in output.splitlines()]
    for expression, label in ((DIGEST_SUMMARY, "digest"), (LOG_SUMMARY, "log")):
        matches = [match for line in summaries if (match := expression.fullmatch(line))]
        if len(matches) != 1:
            raise ReceiptError(f"CloudTrail validation output must contain one {label} summary")
        valid, total = (int(part) for part in matches[0].groups())
        if total < 1 or valid != total:
            raise ReceiptError(f"CloudTrail {label} files are missing or not all valid")


def build_receipt(
    values: Mapping[str, str], *, digest_validation_output: str
) -> dict[str, object]:
    validate_digest_output(digest_validation_output)
    repository = _required(values, "GITHUB_REPOSITORY")
    repository_id = _required(values, "GITHUB_REPOSITORY_ID")
    if repository != EXPECTED_REPOSITORY or repository_id != EXPECTED_REPOSITORY_ID:
        raise ReceiptError("proof must come from the canonical protected repository")

    run_id = _required(values, "GITHUB_RUN_ID")
    attempt = _required(values, "GITHUB_RUN_ATTEMPT")
    commit = _required(values, "GITHUB_SHA")
    if not run_id.isdigit() or not attempt.isdigit() or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ReceiptError("workflow run identity is malformed")
    if int(run_id) < 1 or int(attempt) < 1 or _required(values, "B046_CHECKED_OUT_SHA") != commit:
        raise ReceiptError("receipt commit must equal the workflow's checked-out SHA")
    server_url = _required(values, "GITHUB_SERVER_URL")
    if server_url != "https://github.com":
        raise ReceiptError("proof workflow server is not canonical GitHub")
    workflow_url = f"{server_url}/{repository}/actions/runs/{run_id}"

    configured_account = _required(values, "AWS_ACCOUNT_ID")
    actual_account = _required(values, "B046_ACTUAL_ACCOUNT_ID")
    region = _required(values, "AWS_REGION")
    actual_region = _required(values, "B046_ACTUAL_REGION")
    jurisdiction = _required(values, "JURISDICTION")
    if not re.fullmatch(r"[0-9]{12}", configured_account) or actual_account != configured_account:
        raise ReceiptError("observed account does not match the approved account")
    if actual_region != region or not re.fullmatch(r"[a-z]{2}(-gov)?-[a-z]+-[0-9]+", region):
        raise ReceiptError("observed region does not match the approved region")
    if not re.fullmatch(r"[A-Z][A-Z0-9-]{1,31}", jurisdiction):
        raise ReceiptError("approved jurisdiction is malformed")

    bucket_prefix = _required(values, "AWS_BUCKET_PREFIX")
    bucket = _required(values, "B046_BUCKET")
    expected_bucket = f"{bucket_prefix}-{run_id}-{attempt}"
    key = f"audit/probe-{run_id}/synthetic.txt"
    if bucket != expected_bucket or len(bucket) > 63:
        raise ReceiptError("probe bucket does not match the approved run-bound prefix")
    if _required(values, "B046_KEY") != key:
        raise ReceiptError("probe object key does not match the expected synthetic key")

    cost_text = _required(values, "COST_CEILING_USD_MICROS")
    if not re.fullmatch(r"[1-9][0-9]*", cost_text):
        raise ReceiptError("approved cost ceiling is malformed")
    cost_ceiling = int(cost_text)
    if cost_ceiling > MAX_COST_USD_MICROS:
        raise ReceiptError("approved cost ceiling exceeds the repository limit")

    receipt: dict[str, object] = {
        "schema_version": 1,
        "schema": SCHEMA,
        "workflow_url": workflow_url,
        "commit": commit,
        "run_id": run_id,
        "run_attempt": attempt,
        "repository_id": repository_id,
        "approval_reference": _required(values, "APPROVAL_REFERENCE"),
        "target": {
            "account_id_sha256": _sha256(actual_account),
            "region": region,
            "jurisdiction": jurisdiction,
            "bucket_name_sha256": _sha256(bucket),
            "object_key_sha256": _sha256(key),
            "object_version_sha256": _sha256(_required(values, "B046_VERSION")),
            "region_matches_configured_target": True,
        },
        "bucket": {
            "versioning_enabled": True,
            "object_lock_enabled": True,
            "default_retention_mode": "COMPLIANCE",
            "default_retention_days": 1,
        },
        "object": {
            "put_request_id": _required(values, "B046_PUT_REQUEST_ID"),
            "put_data_event_id": _required(values, "B046_CLOUDTRAIL_PUT_EVENT_ID"),
            "delete_denial_data_event_id": _required(values, "B046_CLOUDTRAIL_DELETE_EVENT_ID"),
            "retention_mode": "COMPLIANCE",
            "retain_until": _utc_timestamp(_required(values, "B046_RETAIN_UNTIL")),
            "legal_hold": "ON",
        },
        "delete_control": {
            "action": "s3:DeleteObjectVersion",
            "effective_permission": "allowed",
            "same_version_pre_expiry_denied": True,
        },
        "cloudtrail_digest_validation": "passed",
        "cloudtrail_digest_output_sha256": _sha256(digest_validation_output),
        "cost_ceiling_usd_micros": cost_ceiling,
        "cost_owner": _required(values, "COST_OWNER"),
        "cleanup_owner": _required(values, "CLEANUP_OWNER"),
        "cleanup_due": "after_retention_and_legal_hold_release",
        "synthetic_only": True,
    }
    receipt["receipt_sha256"] = hashlib.sha256(_canonical(receipt)).hexdigest()
    return receipt


def _canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--digest-validation-output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.is_symlink() or (args.output.exists() and not args.output.is_file()):
        raise SystemExit("receipt output must be a regular non-symlink file")
    try:
        digest_output = args.digest_validation_output.read_text(encoding="utf-8")
        receipt = build_receipt(os.environ, digest_validation_output=digest_output)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except (OSError, ReceiptError) as exc:
        raise SystemExit(f"B-046 proof receipt refused: {exc}") from exc
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
