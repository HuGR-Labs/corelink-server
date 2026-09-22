#!/usr/bin/env python3
"""Validate the credentialless usage acknowledgement contract for issue #1635.

The runner consumer must settle only records whose server outcome is accepted or
an exact dedupe. Conflicts, rejects, and ambiguous outcomes remain retryable or
quarantined and must never be included in settlement markers.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

OUTCOMES = {"accepted", "deduped", "rejected", "conflicting", "ambiguous"}
SETTLEABLE = {"accepted", "deduped"}
REASONS = {
    "conflicting": {"payload_mismatch", "existing_fingerprint_unverifiable"},
    "rejected": {"invalid_record", "backend_failure"},
    "ambiguous": {"transport_unknown", "concurrent_outcome_unknown"},
}
TOP_KEYS = {"schema_version", "batch_id", "outcomes", "settlement_event_ids"}
OUTCOME_KEYS = {"event_id", "status", "reason"}


def fail(message: str) -> None:
    raise SystemExit(f"FAIL: {message}")


def main(path: str) -> None:
    payload = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(payload, dict) or set(payload) != TOP_KEYS:
        fail("acknowledgement must contain exactly the version, batch, outcomes, and settlement fields")
    if type(payload["schema_version"]) is not int or payload["schema_version"] != 1:
        fail("unsupported acknowledgement schema version")
    if not isinstance(payload["batch_id"], str) or not payload["batch_id"]:
        fail("batch_id must be a non-empty string")
    outcomes = payload["outcomes"]
    settlement = payload["settlement_event_ids"]
    if not isinstance(outcomes, list) or not outcomes:
        fail("outcomes must be a non-empty list")
    if not isinstance(settlement, list) or any(not isinstance(x, str) for x in settlement):
        fail("settlement_event_ids must be a list of strings")

    seen: set[str] = set()
    expected_settlement: list[str] = []
    for index, item in enumerate(outcomes):
        if not isinstance(item, dict) or set(item) != OUTCOME_KEYS:
            fail(f"outcomes[{index}] has an unknown or missing field")
        event_id = item["event_id"]
        status = item["status"]
        reason = item["reason"]
        if not isinstance(event_id, str) or not event_id:
            fail(f"outcomes[{index}].event_id must be a non-empty string")
        if event_id in seen:
            fail(f"duplicate event id: {event_id}")
        seen.add(event_id)
        if status not in OUTCOMES:
            fail(f"{event_id} has unsupported status: {status}")
        if not isinstance(reason, str) or not reason:
            fail(f"{event_id}.reason must be a non-empty string")
        if status in REASONS and reason not in REASONS[status]:
            fail(f"{event_id} has invalid reason for {status}: {reason}")
        if status in SETTLEABLE:
            if reason not in {"stored", "exact_replay"}:
                fail(f"{event_id} settleable status requires stored or exact_replay reason")
            expected_settlement.append(event_id)

    if settlement != expected_settlement:
        fail("settlement_event_ids must preserve outcome order and include only accepted/deduped events")
    print(f"PASS: validated {len(outcomes)} per-record outcomes; {len(settlement)} settlement markers")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: verify_i1635_consumer_contract.py ACK.json")
    main(sys.argv[1])
