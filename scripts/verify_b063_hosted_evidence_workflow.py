#!/usr/bin/env python3
"""Fail-closed contract checks for the B-063 hosted evidence lane."""

from __future__ import annotations

import re
from pathlib import Path


WORKFLOW = Path(".github/workflows/issue-1648-b063-read-only-evidence.yml")


def verify(source: str) -> None:
    required = (
        "runs-on: ubuntu-24.04",
        "permissions:\n  contents: read",
        "workflow_dispatch:",
        "pull_request:",
        "READ_ONLY",
        "CF_ACCOUNT_ID",
        "CF_API_TOKEN",
        "AUDIT_D1_DATABASE_ID",
        "SELECT o.tenant_id",
        "tenant_prefix",
        "rows_written",
        "actions/upload-artifact@",
    )
    for marker in required:
        if marker not in source:
            raise AssertionError(f"missing B-063 hosted-lane contract: {marker}")

    if re.search(r"(?im)^\s*(?:UPDATE|INSERT|DELETE|REPLACE|ALTER|DROP|CREATE)\b", source):
        raise AssertionError("workflow contains a mutating SQL statement")
    for forbidden in ("PAGERDUTY", "pagerduty", "events.pagerduty.com", "event_action"):
        if forbidden in source:
            raise AssertionError(f"workflow must not contain PagerDuty mutation: {forbidden}")

    actions = re.findall(r"uses:\s*([^\s#]+)", source)
    unpinned = [action for action in actions if not re.search(r"@[0-9a-f]{40}$", action)]
    if unpinned:
        raise AssertionError(f"unpinned action references: {unpinned}")

    if '[[ "${READ_ONLY}" == "true" ]]' not in source:
        raise AssertionError("live lane must fail closed unless READ_ONLY=true")
    if "str(row.get(\"tenant_id\") or \"\")[:8]" not in source:
        raise AssertionError("receipt must retain only the eight-character tenant prefix")


def main() -> int:
    verify(WORKFLOW.read_text(encoding="utf-8"))
    print("B-063 hosted read-only evidence workflow contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
