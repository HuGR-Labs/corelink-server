#!/usr/bin/env python3
"""Collect a bounded, aggregate-only B-006 production metrics receipt.

The observability key is read from macOS Keychain into process memory and is
never printed, written to an artifact, or passed in a command-line argument.
The response body is parsed in memory and only the named aggregate counter is
retained.  Authentication, transport, size, and schema failures produce an
indeterminate receipt rather than a guessed zero.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

SOURCE = "https://corelink-spawn-worker.gmhelmold.workers.dev/internal/v1/metrics"
WORKER_VERSION = "122e166c-b7d4-421a-a66c-44b57690af00"
SOURCE_SHA = "0ce4070989fe5d02e92c4acd5b0932fe58e4316d"
DEPLOYMENT_ID = "55a021ee-0530-4729-a9c9-a01e012d92f6"
WORKER = "corelink-spawn-worker"
VERSION_NUMBER = 133
DEPLOYMENT_PERCENTAGE = 100
COUNTER = "capability_claim_unserved"


def _base(reason: str, *, attempted: bool, status: int | None) -> dict[str, Any]:
    return {
        "schema": "corelink-b006-capability-metrics-v2",
        "verdict": "INDETERMINATE",
        "reason": reason,
        "source": SOURCE,
        "captured_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "http_status": status,
        "authenticated": False,
        "aggregate_only": False,
        "labels_included": False,
        "capability_claim_unserved": None,
        "reopen_threshold": {"counter": COUNTER, "condition": ">0"},
        "request_attempted": attempted,
        "credential_values_printed": False,
        "credential_item": "CoreLink/METRICS_OBSERVABILITY_KEY",
        "auth_header": "X-Corelink-Internal-Auth",
        "deployed_worker_version": WORKER_VERSION,
        "deployed_worker": WORKER,
        "deployed_version_number": VERSION_NUMBER,
        "deployment_percentage": DEPLOYMENT_PERCENTAGE,
        "deployed_source_sha": SOURCE_SHA,
        "deployment_id": DEPLOYMENT_ID,
    }


def collect(url: str, service: str, account: str, timeout: float, max_bytes: int) -> dict[str, Any]:
    try:
        key = subprocess.run(
            ["security", "find-generic-password", "-s", service, "-a", account, "-w"],
            check=True,
            capture_output=True,
            text=True,
            timeout=min(timeout, 5),
        ).stdout.rstrip("\r\n")
    except (OSError, subprocess.SubprocessError):
        return _base("Keychain item unavailable; no production request was attempted", attempted=False, status=None)
    if not key:
        return _base("Keychain item empty; no production request was attempted", attempted=False, status=None)

    request = urllib.request.Request(url, headers={"X-Corelink-Internal-Auth": key}, method="GET")
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            status = response.status
            body = response.read(max_bytes + 1)
    except urllib.error.HTTPError as exc:
        return _base("dedicated observability credential was rejected; no counter can be inferred", attempted=True, status=exc.code)
    except (OSError, urllib.error.URLError, TimeoutError):
        return _base("bounded production request failed; no counter can be inferred", attempted=True, status=None)

    if status != 200:
        return _base("production metrics response was not HTTP 200; no counter can be inferred", attempted=True, status=status)
    if len(body) > max_bytes:
        return _base("production metrics response exceeded the bounded body limit", attempted=True, status=status)
    try:
        payload = json.loads(body)
        counters = payload["counters"]
        counter = counters[COUNTER]
    except (KeyError, TypeError, ValueError, json.JSONDecodeError):
        return _base("authenticated response did not contain the expected aggregate counter", attempted=True, status=status)
    if not isinstance(counter, int) or isinstance(counter, bool) or counter < 0:
        return _base("aggregate counter was missing or malformed", attempted=True, status=status)

    receipt = _base("authenticated aggregate snapshot retained", attempted=True, status=status)
    receipt["authenticated"] = True
    receipt["aggregate_only"] = True
    receipt[COUNTER] = counter
    receipt["verdict"] = "REOPEN" if counter > 0 else "DONE"
    return receipt


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default=SOURCE)
    parser.add_argument("--keychain-service", default="CoreLink/METRICS_OBSERVABILITY_KEY")
    parser.add_argument("--keychain-account", default="corelink-ops")
    parser.add_argument("--auth-header", default="X-Corelink-Internal-Auth")
    parser.add_argument("--timeout", type=float, default=10)
    parser.add_argument("--max-bytes", type=int, default=1_048_576)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.url != SOURCE or args.auth_header != "X-Corelink-Internal-Auth" or not 1 <= args.timeout <= 30 or not 1 <= args.max_bytes <= 1_048_576:
        parser.error("URL, timeout, or body bound is outside the pinned B-006 contract")
    receipt = collect(args.url, args.keychain_service, args.keychain_account, args.timeout, args.max_bytes)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"verdict": receipt["verdict"], "http_status": receipt["http_status"], "request_attempted": receipt["request_attempted"]}, sort_keys=True))
    return 0 if receipt["verdict"] == "DONE" else 2


if __name__ == "__main__":
    raise SystemExit(main())
