#!/usr/bin/env python3
"""Credentialless verifier for the B-072 owner evidence artifact.

The verifier reads one redacted JSON artifact and performs schema, timestamp,
correlation, and secret-safety checks. It never contacts Cloudflare or
PagerDuty and never changes an incident. A passing result confirms only that
the supplied artifact is structurally admissible; the referenced runtime
events still require owner review.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from datetime import datetime
from pathlib import Path
from typing import Any


REQUIRED_FIELDS = {
    "schema_version",
    "captured_at",
    "cron",
    "correlation_id",
    "receiver_run",
    "pagerduty_incident",
    "d1_row",
    "terminal_outcome",
    "terminal_at",
    "operator",
}
TERMINAL_OUTCOMES = {"acked", "escalated"}
EXPECTED_CRON = "0 14 * * 1"
CORRELATION_RE = re.compile(r"^PAT-CORRELATION-ID-001:SP-[0-9]{13}$")
SECRET_RE = re.compile(
    r"(?i)(routing[_-]?key|integration[_-]?key|api[_-]?key|webhook[_-]?secret|"
    r"authorization\s*:\s*bearer|token\s*[:=])"
)
REDACTION_RE = re.compile(r"(?i)(redacted|restricted|owner[-_ ]held|external[-_ ]ref)")


def fail(message: str) -> int:
    print(f"B-072 EVIDENCE FAIL: {message}", file=sys.stderr)
    return 1


def utc_timestamp(value: Any, field: str) -> str | None:
    if not isinstance(value, str) or not value.endswith("Z"):
        return f"{field} must be an RFC3339 UTC timestamp ending in Z"
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError:
        return f"{field} is not an RFC3339 timestamp"
    if parsed.utcoffset() is None:
        return f"{field} must include a UTC offset"
    return None


def verify(path: Path) -> int:
    try:
        artifact = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        return fail(f"artifact unreadable: {error}")
    if not isinstance(artifact, dict):
        return fail("artifact root must be an object")

    missing = sorted(REQUIRED_FIELDS - artifact.keys())
    if missing:
        return fail("missing required fields: " + ", ".join(missing))
    if artifact.get("schema_version") != 1:
        return fail("schema_version must be 1")
    if artifact.get("terminal_outcome") not in TERMINAL_OUTCOMES:
        return fail("terminal_outcome must be 'acked' or 'escalated'")
    if artifact.get("cron") != EXPECTED_CRON:
        return fail(f"cron must be {EXPECTED_CRON!r}")
    if not isinstance(artifact.get("correlation_id"), str) or not CORRELATION_RE.fullmatch(artifact["correlation_id"]):
        return fail("correlation_id must use the canonical scheduled-time format")

    for field in ("captured_at", "terminal_at"):
        error = utc_timestamp(artifact[field], field)
        if error:
            return fail(error)
    terminal_at = datetime.fromisoformat(artifact["terminal_at"][:-1] + "+00:00")
    captured_at = datetime.fromisoformat(artifact["captured_at"][:-1] + "+00:00")
    if terminal_at > captured_at:
        return fail("terminal_at must not be later than captured_at")

    for field in ("receiver_run", "pagerduty_incident", "d1_row"):
        value = artifact[field]
        if not isinstance(value, str) or not value.strip():
            return fail(f"{field} must be a non-empty redacted reference")
        if SECRET_RE.search(value) or not REDACTION_RE.search(value):
            return fail(f"{field} must be an explicitly redacted external reference")
    operator = artifact["operator"]
    if not isinstance(operator, str) or not operator.strip() or SECRET_RE.search(operator):
        return fail("operator must be a non-empty non-secret identifier")

    serialized = json.dumps(artifact, ensure_ascii=False)
    if SECRET_RE.search(serialized):
        return fail("artifact contains a credential-like field or value")
    print("B-072 EVIDENCE PASS: redacted cron-to-receiver-PagerDuty-D1 terminal-outcome artifact is admissible")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("artifact", type=Path)
    args = parser.parse_args()
    return verify(args.artifact)


if __name__ == "__main__":
    raise SystemExit(main())
