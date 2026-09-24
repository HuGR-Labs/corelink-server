#!/usr/bin/env python3
"""Fail-closed validator for the redacted B-006 metrics receipt.

This validator deliberately accepts no raw response body or credential.  A
production metrics read can close B-006 only when the receipt proves a 200
response authenticated with the dedicated observability key and contains the
aggregate counter.  Transport, authentication, schema, or redaction failures
remain indeterminate.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Literal

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.verify_b006_provider_binding import ProviderBindingError, validate_provider_binding


SOURCE = "https://corelink-spawn-worker.gmhelmold.workers.dev/internal/v1/metrics"
KEYCHAIN_SERVICE = "CoreLink/METRICS_OBSERVABILITY_KEY"
KEYCHAIN_ACCOUNT = "corelink-ops"
COUNTER = "capability_claim_unserved"
MAX_RECEIPT_AGE = timedelta(hours=24)
MAX_RECEIPT_FUTURE = timedelta(minutes=5)
MAX_CLOSURE_SKEW = timedelta(minutes=15)
RECEIPT_KEYS = frozenset(
    {
        "schema", "verdict", "reason", "source", "captured_at", "http_status",
        "authenticated", "aggregate_only", "labels_included", COUNTER,
        "reopen_threshold", "request_attempted", "credential_values_printed",
        "keychain_service", "keychain_account", "auth_header",
    }
)
ALLOWED_REASONS = frozenset(
    {
        "dedicated observability credential was rejected; no counter can be inferred",
        "production metrics response was not HTTP 200; no counter can be inferred",
        "bounded production request failed; no counter can be inferred",
        "Keychain item unavailable; no production request was attempted",
        "Keychain item empty; no production request was attempted",
        "Keychain identity mismatch; no production request was attempted",
        "pinned source URL mismatch; no production request was attempted",
        "pinned source URL mismatch; no counter can be inferred",
        "redirect rejected; no counter can be inferred",
        "authenticated aggregate snapshot retained; separate Wrangler deployment evidence is required for closure",
        "production metrics response exceeded the bounded body limit",
        "authenticated response did not contain the expected aggregate counter",
        "aggregate counter was missing or malformed",
    }
)


class EvidenceError(ValueError):
    """The receipt cannot support a B-006 disposition."""


def _load(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise EvidenceError(f"cannot read evidence: {exc}") from exc
    if not isinstance(value, dict):
        raise EvidenceError("evidence root must be an object")
    return value


ReceiptMode = Literal["live", "historical"]


def validate_receipt(receipt: dict[str, Any], *, mode: ReceiptMode = "live") -> None:
    """Validate receipt truth; enforce age for live/operator evidence.

    Historical mode is for validating committed fixtures whose contents remain
    meaningful after the live observation window expires. It does not relax
    schema, timestamp syntax, future-time, authentication, redaction, or
    counter validation. Runtime and closure callers use the strict live default.
    """

    if mode not in ("live", "historical"):
        raise EvidenceError("receipt validation mode must be live or historical")

    if set(receipt) != RECEIPT_KEYS:
        missing = sorted(RECEIPT_KEYS - set(receipt))
        extra = sorted(set(receipt) - RECEIPT_KEYS)
        raise EvidenceError(f"receipt schema drift: missing={missing}, extra={extra}")
    if receipt["schema"] != "corelink-b006-capability-metrics-v2":
        raise EvidenceError("unsupported B-006 evidence schema")
    if receipt["source"] != SOURCE:
        raise EvidenceError("metrics source is not the pinned production surface")
    if receipt["keychain_service"] != KEYCHAIN_SERVICE or receipt["keychain_account"] != KEYCHAIN_ACCOUNT:
        raise EvidenceError("wrong Keychain service/account")
    if receipt["auth_header"] != "X-Corelink-Internal-Auth":
        raise EvidenceError("metrics request did not use the dedicated auth header")
    if receipt["reason"] not in ALLOWED_REASONS:
        raise EvidenceError("receipt reason is not an approved redacted reason")
    if receipt["reason"].endswith("no production request was attempted"):
        raise EvidenceError("an evidence receipt must not claim no request was attempted")
    if not isinstance(receipt["captured_at"], str) or not re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", receipt["captured_at"]):
        raise EvidenceError("receipt timestamp must be whole-second UTC")
    try:
        captured_at = datetime.strptime(receipt["captured_at"], "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    except ValueError as exc:
        raise EvidenceError("receipt timestamp is invalid") from exc
    now = datetime.now(timezone.utc)
    is_stale_live_receipt = mode == "live" and captured_at < now - MAX_RECEIPT_AGE
    is_from_future = captured_at > now + MAX_RECEIPT_FUTURE
    if is_stale_live_receipt or is_from_future:
        raise EvidenceError("receipt timestamp is stale or from the future")
    if receipt["credential_values_printed"] is not False:
        raise EvidenceError("credential output must remain redacted")
    if receipt["request_attempted"] is not True:
        raise EvidenceError("receipt does not prove a production request was attempted")
    if receipt["reopen_threshold"] != {"counter": COUNTER, "condition": ">0"}:
        raise EvidenceError("reopen threshold drifted")
    if receipt["labels_included"] is not False:
        raise EvidenceError("tenant/customer labels must not be retained")

    status = receipt["http_status"]
    authenticated = receipt["authenticated"]
    aggregate_only = receipt["aggregate_only"]
    counter = receipt[COUNTER]
    if not isinstance(status, int) or isinstance(status, bool) or not 100 <= status <= 599:
        raise EvidenceError("invalid HTTP status")
    if status == 403 and receipt["reason"] != "dedicated observability credential was rejected; no counter can be inferred":
        raise EvidenceError("HTTP 403 receipt has an inconsistent reason")
    if status != 403 and receipt["reason"] == "dedicated observability credential was rejected; no counter can be inferred":
        raise EvidenceError("credential-rejected reason requires HTTP 403")
    if status != 200 or authenticated is not True:
        # A failed/authentication-rejected response can never be interpreted as
        # a zero counter, even if a stale artifact or prose says otherwise.
        if authenticated is not False:
            raise EvidenceError("non-200 receipt cannot claim authentication")
        if counter is not None or aggregate_only is not False:
            raise EvidenceError("unauthenticated/non-200 receipt carries counter data")
        if receipt["verdict"] != "INDETERMINATE":
            raise EvidenceError("failed production read must remain indeterminate")
        return
    if aggregate_only is not True or not isinstance(counter, int) or isinstance(counter, bool) or counter < 0:
        raise EvidenceError("authenticated 200 receipt lacks a non-negative aggregate counter")
    expected_verdict = "REOPEN" if counter > 0 else "INDETERMINATE"
    if receipt["verdict"] != expected_verdict:
        raise EvidenceError("authenticated counter does not match fail-closed receipt verdict")
    if counter == 0 and receipt["reason"] != "authenticated aggregate snapshot retained; separate Wrangler deployment evidence is required for closure":
        raise EvidenceError("zero receipt must remain indeterminate without Wrangler evidence")


def validate_closure(metrics: dict[str, Any], provider: dict[str, Any]) -> None:
    """Require a fresh authenticated zero and its separately authenticated binding."""

    validate_receipt(metrics)
    try:
        validate_provider_binding(provider)
    except ProviderBindingError as exc:
        raise EvidenceError(f"provider binding is not valid: {exc}") from exc
    if metrics["http_status"] != 200 or metrics["authenticated"] is not True or metrics[COUNTER] != 0:
        raise EvidenceError("closure requires an authenticated HTTP 200 aggregate-only zero")
    metrics_at = datetime.strptime(metrics["captured_at"], "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    provider_at = datetime.strptime(provider["captured_at"], "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    if abs(metrics_at - provider_at) > MAX_CLOSURE_SKEW:
        raise EvidenceError("metrics and provider evidence are not from the same fresh observation window")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        validate_receipt(_load(args.input))
    except EvidenceError as exc:
        print(f"B-006 evidence FAIL: {exc}")
        return 1
    print("B-006 evidence PASS: redacted receipt is structurally fail-closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
