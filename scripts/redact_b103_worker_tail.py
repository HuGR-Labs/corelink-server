#!/usr/bin/env python3
"""Write a minimal B-103 tail artifact only after complete correlation."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import sys
from collections.abc import Mapping
from pathlib import Path


OPERATION_ID = re.compile(r"b103-[0-9a-f]{32}")
METHODS = {"GET", "HEAD", "PROPFIND", "MKCOL", "PUT", "DELETE"}


def operation_ids(value: object) -> list[str]:
    if isinstance(value, Mapping):
        return [op for item in value.values() for op in operation_ids(item)]
    if isinstance(value, list):
        return [op for item in value for op in operation_ids(item)]
    if isinstance(value, str) and OPERATION_ID.fullmatch(value):
        return [value]
    return []


def required_operation_ids(path: Path) -> set[str]:
    try:
        evidence = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError("wire evidence is unreadable") from exc
    ids = operation_ids(evidence)
    if not ids:
        raise ValueError("wire evidence contains no B-103 operation IDs")
    if len(ids) != len(set(ids)):
        raise ValueError("wire evidence contains duplicate B-103 operation IDs")
    return set(ids)


def safe_tail_record(event: Mapping[str, object]) -> dict[str, object] | None:
    payload = event.get("event")
    request = payload.get("request") if isinstance(payload, Mapping) else None
    headers = request.get("headers") if isinstance(request, Mapping) else None
    if not isinstance(headers, Mapping):
        return None
    operation = next(
        (
            value
            for name, value in headers.items()
            if str(name).lower() == "x-corelink-operation" and isinstance(value, str)
        ),
        None,
    )
    if operation is None or not operation.startswith("b103-"):
        return None
    if not OPERATION_ID.fullmatch(operation):
        raise ValueError("tail event has an invalid B-103 operation identifier")
    method = request.get("method")
    outcome = event.get("outcome")
    timestamp = event.get("eventTimestamp")
    if isinstance(timestamp, int) and not isinstance(timestamp, bool):
        if not 0 < timestamp < 100_000_000_000_000:
            raise ValueError("tail event has an invalid timestamp")
    elif isinstance(timestamp, str) and re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,6})?Z", timestamp):
        try:
            dt.datetime.fromisoformat(timestamp.replace("Z", "+00:00"))
        except ValueError as exc:
            raise ValueError("tail event has an invalid timestamp") from exc
    else:
        raise ValueError("tail event has no valid timestamp")
    return {
        "operation_id": operation,
        "method": method if isinstance(method, str) and method in METHODS else "other",
        "outcome": outcome if isinstance(outcome, str) and re.fullmatch(r"[A-Za-z_-]{1,32}", outcome) else "other",
        "event_timestamp": timestamp,
    }


def redact_lines(lines: object, required: set[str]) -> list[str]:
    records: list[dict[str, object]] = []
    observed: set[str] = set()
    for line_number, line in enumerate(lines, 1):
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except (json.JSONDecodeError, TypeError) as exc:
            raise ValueError(f"unfiltered Worker tail line {line_number} is not JSON") from exc
        if not isinstance(event, dict):
            raise ValueError(f"unfiltered Worker tail line {line_number} is not an object")
        try:
            record = safe_tail_record(event)
        except ValueError as exc:
            raise ValueError(f"unfiltered Worker tail line {line_number} cannot be correlated") from exc
        if record is None:
            continue
        operation = str(record["operation_id"])
        if operation not in required:
            continue
        if operation in observed:
            continue
        records.append(record)
        observed.add(operation)
    if not records:
        raise ValueError("unfiltered Worker tail captured zero B-103 events")
    missing = required - observed
    if missing:
        raise ValueError(f"Worker tail is missing {len(missing)} emitted B-103 operation IDs")
    return [json.dumps(record, sort_keys=True, separators=(",", ":")) for record in records]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wire-evidence", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)

    try:
        required = required_operation_ids(args.wire_evidence)
        records = redact_lines(sys.stdin, required)
    except ValueError as exc:
        args.output.unlink(missing_ok=True)
        raise SystemExit(str(exc)) from exc

    args.output.write_text("\n".join(records) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
