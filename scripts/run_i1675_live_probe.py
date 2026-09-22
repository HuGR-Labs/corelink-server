#!/usr/bin/env python3
"""Run one owner-provisioned #1675 arm and classify only typed evidence.

The staging service owns the probe controls.  This client never turns a curl
timeout, exit 124, elapsed time, or missing response into a causal receipt.
The control must return a JSON object containing an observation and its signed
marker; malformed or unsigned responses fail closed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

from classify_timeout_receipt import ReceiptError, classify


PATHS = {
    "timeout": "/_internal/i1675/timeout",
    "runner_communication_loss": "/_internal/i1675/runner-communication-loss",
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--arm", choices=sorted(PATHS), required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--run-id", type=int, required=True)
    parser.add_argument("--job-id", type=int, required=True)
    parser.add_argument("--timeout-seconds", type=int, default=30)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    token = os.environ.get("K6_STAGING_PAT", "")
    if not token:
        print("::error::K6_STAGING_PAT is unavailable", file=sys.stderr)
        return 2
    payload = {
        "arm": args.arm,
        "run_id": args.run_id,
        "job_id": args.job_id,
        "declared_timeout_seconds": args.timeout_seconds,
        "request_id": f"i1675-{args.run_id}-{args.job_id}-{args.arm}",
    }
    request = urllib.request.Request(
        args.target.rstrip("/") + PATHS[args.arm],
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
            "X-CoreLink-I1675-Arm": args.arm,
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=args.timeout_seconds + 10) as response:
            body = json.load(response)
    except (OSError, urllib.error.URLError, json.JSONDecodeError) as exc:
        print(f"::error::{args.arm} control returned no typed evidence: {exc}", file=sys.stderr)
        return 1

    if not isinstance(body, dict) or body.get("marker") != "corelink.i1675.signed-observation.v1":
        print(f"::error::{args.arm} control marker is missing or invalid", file=sys.stderr)
        return 1
    observation = body.get("observation")
    if not isinstance(observation, dict):
        print(f"::error::{args.arm} control did not return an observation", file=sys.stderr)
        return 1
    if observation.get("run_id") != args.run_id or observation.get("job_id") != args.job_id:
        print(f"::error::{args.arm} observation identity does not match hosted job", file=sys.stderr)
        return 1
    if body.get("signature_sha256") != hashlib.sha256(
        json.dumps(observation, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest():
        print(f"::error::{args.arm} observation signature does not verify", file=sys.stderr)
        return 1
    try:
        receipt = classify(observation)
    except ReceiptError as exc:
        print(f"::error::{args.arm} observation rejected: {exc}", file=sys.stderr)
        return 1
    expected = "declared_timeout" if args.arm == "timeout" else "runner_communication_loss"
    if receipt["classification"] != expected:
        print(
            f"::error::{args.arm} classified as {receipt['classification']}; expected {expected}",
            file=sys.stderr,
        )
        return 1
    args.output.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(f"i1675 {args.arm}: {receipt['classification']} run={args.run_id} job={args.job_id}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
