#!/usr/bin/env python3
"""Redact sensitive values from an unfiltered Wrangler tail JSONL stream.

The stream is not searched or event-filtered: each JSON event is preserved.
Only credential, tenant, identity, and direct personal-data fields are masked
before the evidence artifact is written.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections.abc import Mapping


SENSITIVE_KEY = re.compile(
    r"(?:authorization|bearer|cookie|set.?cookie|token|api.?key|credential|"
    r"secret|password|email|client.?ip|remote.?ip|"
    r"source.?ip|ip.?address|ip|"
    r"tenant.?id|user.?id|customer.?id|account.?id)$",
    re.IGNORECASE,
)
BEARER = re.compile(r"(?i)\bBearer\s+[^\s,;\"']+")
CORELINK_PAT = re.compile(r"\bcorelink_pat_[A-Za-z0-9_-]+\b")
EMAIL = re.compile(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b")


def redact(value: object, tenant: str) -> object:
    if isinstance(value, Mapping):
        result: dict[str, object] = {}
        for key, item in value.items():
            name = str(key)
            result[name] = "[REDACTED]" if SENSITIVE_KEY.search(name) else redact(item, tenant)
        return result
    if isinstance(value, list):
        return [redact(item, tenant) for item in value]
    if isinstance(value, str):
        safe = value.replace(tenant, "[TENANT]") if tenant else value
        safe = BEARER.sub("Bearer [REDACTED]", safe)
        safe = CORELINK_PAT.sub("[REDACTED]", safe)
        return EMAIL.sub("[EMAIL]", safe)
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tenant", default="")
    args = parser.parse_args()
    count = 0
    for line_number, line in enumerate(sys.stdin, 1):
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError as exc:
            raise SystemExit(f"unfiltered Wrangler tail line {line_number} is not JSON") from exc
        if not isinstance(event, dict):
            raise SystemExit(f"unfiltered Wrangler tail line {line_number} is not an object")
        print(json.dumps(redact(event, args.tenant), sort_keys=True, separators=(",", ":")))
        count += 1
    if count == 0:
        raise SystemExit("unfiltered Wrangler tail captured zero events")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
