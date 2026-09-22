#!/usr/bin/env python3
"""Replay one redacted B-063 archive partition without contacting production.

The input is a JSON object with ``partition`` (``tenant_id`` and ``region``)
and ``rows``.  Each row is the read-only D1 projection containing ``id``,
``sequence_number``, ``prev_hash``, ``chain_hash``, ``enqueued_at`` and
``canonical_jcs`` plus the three epoch metadata columns.  Tenant IDs may be
redacted, but every row must use the same value as the partition selector.

The verifier applies the archive reader's exact ordering ``(sequence_number,
id)`` and replays the legacy chain rule.  It never writes D1/R2 and never
prints row payloads.  A valid prefix followed by a break is reported as
``quarantine_required``; malformed, mixed-tenant, or hash-invalid input is
``fail_closed``.  Exit 0 means the supplied rows are fully drainable; exit 1
means the evidence describes a chain break; exit 2 means the evidence itself
cannot be trusted.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
from pathlib import Path
from typing import Any

try:
    import blake3
except ImportError as exc:  # pragma: no cover - exercised by the hosted gate
    raise SystemExit("B-063 replay requires the pinned blake3 Python package") from exc


REQUIRED_ROW_FIELDS = {
    "id",
    "tenant_id",
    "region",
    "sequence_number",
    "prev_hash",
    "chain_hash",
    "enqueued_at",
    "canonical_jcs",
    "algorithm_id",
    "epoch_id",
    "link_key_id",
}
HEX_LENGTH = 64
REGIONS = {"wnam", "enam", "weur", "sam", "apac", "afr"}


class ReplayError(ValueError):
    """Input is not safe to interpret as archive evidence."""


def _hash(value: str, field: str) -> bytes:
    if (
        not isinstance(value, str)
        or len(value) != HEX_LENGTH
        or value != value.lower()
        or any(char not in "0123456789abcdef" for char in value)
    ):
        raise ReplayError(f"{field} is not canonical lowercase blake3 hex")
    return bytes.fromhex(value)


def _legacy_metadata(row: dict[str, Any]) -> bool:
    values = (row["algorithm_id"], row["epoch_id"], row["link_key_id"])
    return values in ((None, None, None), (0, 0, None))


def _archive_key(row: dict[str, Any], tenant: str, region: str) -> str:
    try:
        timestamp = int(row["enqueued_at"])
        day = dt.datetime.fromtimestamp(timestamp / 1000, dt.timezone.utc)
    except (TypeError, ValueError, OverflowError, OSError) as exc:
        raise ReplayError("enqueued_at is not a usable epoch-millisecond value") from exc
    return f"audit/{day:%Y/%m/%d}/{tenant}/{region}/{int(row['sequence_number']):08d}.ndjson"


def replay(document: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(document, dict):
        raise ReplayError("document must be an object")
    partition = document.get("partition")
    rows = document.get("rows")
    if not isinstance(partition, dict) or not isinstance(rows, list):
        raise ReplayError("document requires partition object and rows array")
    tenant = partition.get("tenant_id")
    region = partition.get("region")
    if not isinstance(tenant, str) or not tenant:
        raise ReplayError("partition.tenant_id must be a non-empty string")
    if not isinstance(region, str) or region not in REGIONS:
        raise ReplayError("partition.region is outside the canonical region set")
    if not rows:
        raise ReplayError("empty population cannot prove recovery")

    normalized: list[dict[str, Any]] = []
    seen_ids: set[str] = set()
    for index, raw in enumerate(rows):
        if not isinstance(raw, dict) or set(raw) < REQUIRED_ROW_FIELDS:
            raise ReplayError(f"row {index} is missing required fields")
        row = dict(raw)
        row_id = row["id"]
        if not isinstance(row_id, str) or not row_id or row_id in seen_ids:
            raise ReplayError(f"row {index} has a missing or duplicate id")
        seen_ids.add(row_id)
        if row["tenant_id"] != tenant or row["region"] != region:
            raise ReplayError(f"row {index} crosses the selected tenant partition")
        if not _legacy_metadata(row):
            raise ReplayError(f"row {index} has partial or keyed metadata in a legacy replay")
        if not isinstance(row["sequence_number"], int) or row["sequence_number"] < 0:
            raise ReplayError(f"row {index} has an invalid sequence number")
        _hash(row["prev_hash"], f"row {index}.prev_hash")
        _hash(row["chain_hash"], f"row {index}.chain_hash")
        if not isinstance(row["canonical_jcs"], str):
            raise ReplayError(f"row {index}.canonical_jcs is not text")
        normalized.append(row)

    ordered = sorted(normalized, key=lambda row: (row["sequence_number"], row["id"]))
    if ordered != normalized:
        raise ReplayError("rows are not in deterministic (sequence_number, id) order")

    prefix = 0
    break_reason: str | None = None
    previous_sequence: int | None = None
    previous_hash: str | None = None
    for index, row in enumerate(ordered):
        sequence = row["sequence_number"]
        if previous_sequence is not None:
            if sequence != previous_sequence + 1:
                break_reason = f"sequence_gap:expected={previous_sequence + 1},found={sequence}"
                break
            if row["prev_hash"] != previous_hash:
                break_reason = f"chain_head_discontinuity:seq={sequence}"
                break
        expected = blake3.blake3(
            bytes.fromhex(row["prev_hash"]) + row["canonical_jcs"].encode("utf-8")
        ).hexdigest()
        if expected != row["chain_hash"]:
            raise ReplayError(f"row {index} chain_hash does not match canonical_jcs")
        prefix = index + 1
        previous_sequence = sequence
        previous_hash = row["chain_hash"]

    if break_reason is None:
        keys = sorted({_archive_key(row, "<tenant-redacted>", region) for row in ordered})
        return {
            "verdict": "drainable",
            "rows": len(ordered),
            "verifying_prefix_rows": len(ordered),
            "break": None,
            "archive_keys": keys,
            "writes": {"d1": 0, "r2": 0},
        }

    return {
        "verdict": "quarantine_required",
        "rows": len(ordered),
        "verifying_prefix_rows": prefix,
        "break": {"index": prefix, "reason": break_reason},
        "archive_keys": sorted(
            {_archive_key(row, "<tenant-redacted>", region) for row in ordered[:prefix]}
        ),
        "writes": {"d1": 0, "r2": 0},
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True, help="redacted replay JSON")
    args = parser.parse_args(argv)
    try:
        document = json.loads(args.input.read_text(encoding="utf-8"))
        result = replay(document)
    except (OSError, UnicodeError, json.JSONDecodeError, ReplayError) as exc:
        print(json.dumps({"verdict": "fail_closed", "error": str(exc)}))
        return 2
    print(json.dumps(result, sort_keys=True))
    return 0 if result["verdict"] == "drainable" else 1


if __name__ == "__main__":
    raise SystemExit(main())
