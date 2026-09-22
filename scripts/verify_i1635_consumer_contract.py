#!/usr/bin/env python3
"""Validate a server-shaped #1635 billing ingest acknowledgement."""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any

TOP_KEYS = {"outcomes", "accepted", "deduped", "rejected", "total"}
OUTCOME_KEYS = {"index", "idem_key", "outcome"}
WITH_REASON = {"rejected", "conflict"}
OUTCOMES = {"accepted", "deduped", "rejected", "conflict"}
CONFLICT_REASONS = {"payload_mismatch", "existing_fingerprint_unverifiable"}
REJECTION_REASONS = {
    "bad_tenant_id",
    "invalid_record",
    "bad_billing_period",
    "qty_out_of_storage_range",
    "bad_region",
    "bad_idem_key",
    "empty_source",
    "source_too_long",
    "time_ms_out_of_storage_range",
}
IDEM_KEY = re.compile(r"^[0-9a-f]{64}$")


class ContractError(ValueError):
    """A response violates the server acknowledgement contract."""


def fail(message: str) -> None:
    raise ContractError(message)


def canonical_idem_key(record: Any) -> str | None:
    if not isinstance(record, dict):
        return None
    value = record.get("idem_key")
    if not isinstance(value, str):
        return None
    canonical = value.lower()
    return canonical if IDEM_KEY.fullmatch(canonical) else None


def validate_no_body(status: int, body: str | None) -> None:
    if status != 503:
        fail("a missing acknowledgement is valid only for HTTP 503")
    if body not in (None, ""):
        fail("HTTP 503 persistence uncertainty must have no acknowledgement body")


def validate(status: int, response: Any, request: Any) -> None:
    if status == 503:
        fail("HTTP 503 must be represented as a bodyless ambiguous response")
    if status not in {202, 409, 422}:
        fail(f"unsupported acknowledgement HTTP status: {status}")
    if not isinstance(request, list) or not request:
        fail("submitted request must be a non-empty JSON array")
    if not isinstance(response, dict) or set(response) != TOP_KEYS:
        fail("response must contain exactly outcomes, accepted, deduped, rejected, and total")

    outcomes = response["outcomes"]
    if not isinstance(outcomes, list) or len(outcomes) != len(request) or not outcomes:
        fail("outcomes must cover every submitted record exactly once")

    counts = {name: 0 for name in OUTCOMES}
    for index, (item, submitted) in enumerate(zip(outcomes, request, strict=True)):
        if not isinstance(item, dict):
            fail(f"outcomes[{index}] must be an object")
        outcome = item.get("outcome")
        if not isinstance(outcome, str) or outcome not in OUTCOMES:
            fail(f"outcomes[{index}] has unsupported outcome: {outcome}")
        expected_keys = OUTCOME_KEYS | ({"reason"} if outcome in WITH_REASON else set())
        if set(item) != expected_keys:
            fail(f"outcomes[{index}] has an unknown field or missing/extra reason")
        if type(item["index"]) is not int or item["index"] != index:
            fail(f"outcomes[{index}].index must equal its zero-based request position")
        if item["idem_key"] != canonical_idem_key(submitted):
            fail(f"outcomes[{index}].idem_key does not match the submitted canonical key")
        if outcome in WITH_REASON:
            reason = item["reason"]
            if not isinstance(reason, str) or not reason.strip():
                fail(f"outcomes[{index}].reason must be a non-empty string")
            if outcome == "conflict" and reason not in CONFLICT_REASONS:
                fail(f"outcomes[{index}] has unsupported conflict reason: {reason}")
            if outcome == "rejected" and reason not in REJECTION_REASONS:
                fail(f"outcomes[{index}] has unsupported rejection reason: {reason}")
        counts[outcome] += 1

    for name in ("accepted", "deduped", "rejected", "total"):
        if type(response[name]) is not int or response[name] < 0:
            fail(f"{name} must be a non-negative integer")
    if response["accepted"] != counts["accepted"]:
        fail("accepted counter does not match per-record outcomes")
    if response["deduped"] != counts["deduped"]:
        fail("deduped counter does not match per-record outcomes")
    if response["rejected"] != counts["rejected"]:
        fail("rejected counter does not match per-record outcomes")
    if response["total"] != response["accepted"] + response["deduped"]:
        fail("total must equal accepted + deduped")
    if response["total"] + response["rejected"] + counts["conflict"] != len(outcomes):
        fail("per-record outcome counts do not cover the response")

    expected_status = (
        409 if counts["conflict"] else
        422 if response["total"] == 0 and response["rejected"] > 0 else
        202
    )
    if status != expected_status:
        fail(f"HTTP status {status} does not match outcome-derived status {expected_status}")


def load_json(path: str) -> Any:
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read JSON fixture {path}: {exc}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--status", required=True, type=int, help="server HTTP status")
    parser.add_argument("--response", help="response JSON fixture")
    parser.add_argument("--request", help="submitted request array fixture")
    parser.add_argument("--no-body", action="store_true", help="bodyless 503 receipt")
    args = parser.parse_args()

    try:
        if args.no_body:
            if args.response or args.request:
                fail("--no-body cannot be combined with JSON fixture paths")
            validate_no_body(args.status, None)
            print("PASS: bodyless HTTP 503 remains ambiguous and retryable")
            return
        if not args.response or not args.request:
            fail("--response and --request are required for record acknowledgements")
        validate(args.status, load_json(args.response), load_json(args.request))
    except ContractError as exc:
        raise SystemExit(f"FAIL: {exc}") from exc
    print(f"PASS: validated {args.status} acknowledgement")


if __name__ == "__main__":
    main()
