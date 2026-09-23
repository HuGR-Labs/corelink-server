#!/usr/bin/env python3
"""Project an unfiltered B-103 Wrangler tail to correlated safe records.

The probe's operation IDs are intentionally non-secret correlation handles.
Everything else that can identify a tenant, person, client, or credential is
removed before an artifact is written.  The command fails closed when the
tail is empty, malformed, or lacks an operation ID emitted by the probe.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections.abc import Mapping
from pathlib import Path


OPERATION_ID = re.compile(r"\bb103-[0-9a-f]{32}\b")
UUID = re.compile(r"\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b", re.IGNORECASE)
BEARER = re.compile(r"(?i)\b(?:bearer|basic)\s+[^\s,;\"']+")
CORELINK_PAT = re.compile(r"\bcorelink_pat_[A-Za-z0-9_-]+\b")
EMAIL = re.compile(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b")
IPV4 = re.compile(r"\b(?:25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})(?:\.(?:25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})){3}\b")
IPV6 = re.compile(r"(?i)(?<![\w:])(?:[0-9a-f]{1,4}:){2,}[0-9a-f:]+(?![\w:])")


def sensitive_key(name: object) -> bool:
    """Recognize sensitive field names despite case and separator changes."""
    compact = re.sub(r"[^a-z0-9]", "", str(name).lower())
    exact = {
        "authorization",
        "proxyauthorization",
        "cookie",
        "setcookie",
        "email",
        "ip",
        "clientip",
        "remoteip",
        "sourceip",
        "ipaddress",
        "tenant",
        "tenantid",
        "userid",
        "customerid",
        "accountid",
        "session",
        "jwt",
    }
    return compact in exact or compact.endswith(("token", "secret", "password", "apikey", "credential", "privatekey"))


def redact_string(value: str, values: tuple[str, ...]) -> str:
    redacted = value
    for sensitive in values:
        redacted = redacted.replace(sensitive, "[REDACTED]")
    redacted = BEARER.sub("[REDACTED]", redacted)
    redacted = CORELINK_PAT.sub("[REDACTED]", redacted)
    redacted = EMAIL.sub("[EMAIL]", redacted)
    redacted = IPV4.sub("[IP]", redacted)
    redacted = IPV6.sub("[IP]", redacted)
    # A Worker tail can render a tenant under an unexpected nested key or in a
    # message string.  UUIDs are not needed for B-103 correlation (the probe
    # uses b103-* operation IDs), so remove every UUID rather than betting on
    # a fixed JSON shape.
    return UUID.sub("[UUID]", redacted)


def redact(value: object, values: tuple[str, ...]) -> object:
    if isinstance(value, Mapping):
        return {
            str(key): "[REDACTED]" if sensitive_key(key) else redact(item, values)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [redact(item, values) for item in value]
    if isinstance(value, str):
        return redact_string(value, values)
    return value


def operation_ids(value: object) -> set[str]:
    if isinstance(value, Mapping):
        return set().union(*(operation_ids(item) for item in value.values()))
    if isinstance(value, list):
        return set().union(*(operation_ids(item) for item in value))
    if isinstance(value, str):
        return set(OPERATION_ID.findall(value))
    return set()


def safe_tail_record(event: Mapping[str, object]) -> dict[str, object] | None:
    """Project a B-103 event; discard unrelated events without retaining data."""
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
    if operation is None or not isinstance(operation, str) or not operation.startswith("b103-"):
        return None
    if not OPERATION_ID.fullmatch(operation):
        raise ValueError("tail event has no valid B-103 operation identifier")
    method = request.get("method") if isinstance(request, Mapping) else None
    outcome = event.get("outcome")
    timestamp = event.get("eventTimestamp")
    return {
        "operation_id": operation,
        "method": method if isinstance(method, str) and method in {"GET", "HEAD", "PROPFIND", "MKCOL", "PUT", "DELETE"} else "other",
        "outcome": outcome if isinstance(outcome, str) and re.fullmatch(r"[A-Za-z_-]{1,32}", outcome) else "other",
        "event_timestamp": timestamp if isinstance(timestamp, int) and not isinstance(timestamp, bool) else None,
    }


def required_operation_ids(path: Path) -> set[str]:
    try:
        evidence = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"wire evidence is unreadable: {path}") from exc
    found = operation_ids(evidence)
    if not found:
        raise ValueError("wire evidence contains no B-103 operation IDs")
    return found


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-operation-ids-from", type=Path, required=True)
    args = parser.parse_args(argv)

    try:
        required = required_operation_ids(args.require_operation_ids_from)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc

    count = 0
    observed: set[str] = set()
    for line_number, line in enumerate(sys.stdin, 1):
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError as exc:
            raise SystemExit(f"unfiltered Wrangler tail line {line_number} is not JSON") from exc
        if not isinstance(event, dict):
            raise SystemExit(f"unfiltered Wrangler tail line {line_number} is not an object")
        # Only the correlation handle and bounded event metadata survive.  Do
        # not retain URLs, request headers, logs, exceptions, or other tenants'
        # event payloads even if a heuristic redactor misses a secret shape.
        try:
            safe = safe_tail_record(event)
        except ValueError as exc:
            raise SystemExit(f"unfiltered Wrangler tail line {line_number} cannot be correlated: {exc}") from exc
        if safe is None:
            continue
        observed.update(operation_ids(safe))
        print(json.dumps(safe, sort_keys=True, separators=(",", ":")))
        count += 1
    if count == 0:
        raise SystemExit("unfiltered Wrangler tail captured zero events")
    missing = sorted(required - observed)
    if missing:
        raise SystemExit(f"tail is missing {len(missing)} B-103 operation IDs needed for response correlation")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
