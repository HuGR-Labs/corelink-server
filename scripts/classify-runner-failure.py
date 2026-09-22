#!/usr/bin/env python3
"""Classify a failed Linux runner gate without changing its exit status.

The detector consumes text only.  A caller supplies the command status because
an output line mentioning ENOSPC in a successful test must not turn green work
into an infrastructure incident.  The JSON schema is intentionally tiny so it
can be uploaded as a workflow artifact and parsed without scraping prose.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


CODE = re.compile(r"error\s*\[E\d+\]|assertion failed|panicked at|tests? .*FAILED", re.I)
DISK = re.compile(
    r"no space left on device|\bENOSPC\b|os error 28|disk\s+(?:is\s+)?full|"
    r"write(?:\s+error)?\s*:\s*(?:no space|os error 28)", re.I
)
LINKER = re.compile(
    r"(?:collect2|\bld(?:\.exe)?\b|linker).{0,120}(?:bus error|signal\s+7)|"
    r"(?:bus error|signal\s+7).{0,120}(?:collect2|\bld(?:\.exe)?\b|linker)", re.I
)
CANCEL = re.compile(
    r"runner\s+(?:lost|disconnect(?:ed)?|shutdown|unreachable)|"
    r"(?:job|workflow)\s+(?:was\s+)?cancel(?:led|ed)\b|"
    r"the operation was canceled|received\s+termination|\bSIGTERM\b", re.I
)
TIMEOUT = re.compile(r"^GATE_TIMEOUT\b", re.I | re.M)


def classify(text: str, status: int) -> tuple[str, str]:
    if status == 0:
        return "SUCCESS", "command exited zero"
    if status == 124 and TIMEOUT.search(text):
        return "TIMEOUT", "bounded gate timeout"
    # A real test/compiler diagnostic wins over incidental infrastructure words
    # in the same log.  This preserves the existing mixed-failure contract.
    if CODE.search(text):
        return "TEST_FAILURE", "test or compiler diagnostic"
    if DISK.search(text):
        return "ENOSPC", "runner storage exhausted"
    if LINKER.search(text):
        return "LINKER_FAILURE", "runner linker terminated"
    if CANCEL.search(text):
        return "CANCELLED", "runner cancellation or termination"
    return "FAILURE", "unclassified non-zero gate"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--status", type=int, default=1)
    parser.add_argument("--json-output", type=Path)
    args = parser.parse_args()
    classification, reason = classify(sys.stdin.read(), args.status)
    payload = {
        "schema": "corelink.runner-classification/v1",
        "classification": classification,
        "exit_code": args.status,
        "reason": reason,
        "original_exit_code": args.status,
        "original_status_preserved": True,
    }
    if args.json_output:
        try:
            args.json_output.parent.mkdir(parents=True, exist_ok=True)
            args.json_output.write_text(json.dumps(payload, sort_keys=True) + "\n", encoding="utf-8")
        except OSError as exc:
            print(f"classification artifact unavailable: {exc}", file=sys.stderr)
    print(f"classification: {classification}")
    print(f"summary: {reason}; original exit {args.status} is preserved")
    if classification in {"ENOSPC", "LINKER_FAILURE"}:
        return 42
    if classification == "CANCELLED":
        return 43
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
