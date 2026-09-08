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
from pathlib import Path
from typing import Any


SOURCE = "https://corelink-spawn-worker.gmhelmold.workers.dev/internal/v1/metrics"
EXPECTED_VERSION = "122e166c-b7d4-421a-a66c-44b57690af00"
EXPECTED_SOURCE_SHA = "0ce4070989fe5d02e92c4acd5b0932fe58e4316d"
EXPECTED_WORKER = "corelink-spawn-worker"
COUNTER = "capability_claim_unserved"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")


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


def validate_receipt(receipt: dict[str, Any]) -> None:
    """Validate a receipt without ever accepting a guessed zero."""

    required = {
        "schema",
        "verdict",
        "reason",
        "source",
        "captured_at",
        "http_status",
        "authenticated",
        "aggregate_only",
        "labels_included",
        "capability_claim_unserved",
        "reopen_threshold",
        "request_attempted",
        "credential_values_printed",
        "credential_item",
        "auth_header",
        "deployed_worker_version",
        "deployed_worker",
        "deployed_version_number",
        "deployment_percentage",
        "deployed_source_sha",
        "deployment_id",
    }
    missing = required - set(receipt)
    if missing:
        raise EvidenceError(f"missing receipt fields: {sorted(missing)}")
    if receipt["schema"] != "corelink-b006-capability-metrics-v2":
        raise EvidenceError("unsupported B-006 evidence schema")
    if receipt["source"] != SOURCE:
        raise EvidenceError("metrics source is not the pinned production surface")
    if receipt["credential_item"] != "CoreLink/METRICS_OBSERVABILITY_KEY":
        raise EvidenceError("wrong credential class or service")
    if receipt["auth_header"] != "X-Corelink-Internal-Auth":
        raise EvidenceError("metrics request did not use the dedicated auth header")
    if receipt["credential_values_printed"] is not False:
        raise EvidenceError("credential output must remain redacted")
    if receipt["request_attempted"] is not True:
        raise EvidenceError("receipt does not prove a production request was attempted")
    if receipt["reopen_threshold"] != {"counter": COUNTER, "condition": ">0"}:
        raise EvidenceError("reopen threshold drifted")
    if receipt["deployed_worker_version"] != EXPECTED_VERSION:
        raise EvidenceError("receipt is not bound to the deployed Worker version")
    if receipt["deployed_worker"] != EXPECTED_WORKER or receipt["deployed_version_number"] != 133:
        raise EvidenceError("receipt is not bound to the deployed Worker/version metadata")
    if receipt["deployment_percentage"] != 100:
        raise EvidenceError("receipt is not bound to the serving rollout")
    source_sha = receipt["deployed_source_sha"]
    if source_sha != EXPECTED_SOURCE_SHA or not SHA_RE.fullmatch(source_sha):
        raise EvidenceError("receipt is not bound to the exact deployed source SHA")
    if not isinstance(receipt["deployment_id"], str) or not receipt["deployment_id"]:
        raise EvidenceError("deployment id is missing")
    if receipt["labels_included"] is not False:
        raise EvidenceError("tenant/customer labels must not be retained")

    status = receipt["http_status"]
    authenticated = receipt["authenticated"]
    aggregate_only = receipt["aggregate_only"]
    counter = receipt[COUNTER]
    if not isinstance(status, int) or isinstance(status, bool) or not 100 <= status <= 599:
        raise EvidenceError("invalid HTTP status")
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
    if receipt["verdict"] != ("REOPEN" if counter > 0 else "DONE"):
        raise EvidenceError("authenticated counter does not match receipt verdict")


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
