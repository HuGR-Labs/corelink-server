#!/usr/bin/env python3
"""Fail #2546 candidate clippy only for diagnostics owned by its new module."""

from __future__ import annotations

import json
import sys
from pathlib import Path


TARGET = "crates/corelink-container/src/storage/staging_load_test_admission.rs"


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: check_issue_2546_clippy.py CARGO_JSONL", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    try:
        records = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    except (OSError, json.JSONDecodeError) as error:
        print(f"cannot read cargo clippy JSON: {error}", file=sys.stderr)
        return 2
    messages = [record["message"] for record in records if record.get("reason") == "compiler-message"]
    if not any(record.get("reason") == "build-finished" for record in records):
        print("cargo clippy emitted no build-finished record", file=sys.stderr)
        return 2
    findings: list[str] = []
    for message in messages:
        primary_files = {
            span.get("file_name", "").replace("\\", "/").removeprefix("./")
            for span in message.get("spans", [])
            if span.get("is_primary")
        }
        if not any(file_name.endswith(TARGET) for file_name in primary_files):
            continue
        code = message.get("code") or {}
        findings.append(
            f"{code.get('code', 'compiler diagnostic')}: {message.get('message', '<no message>')}"
        )
    if findings:
        print("#2546 admission module clippy diagnostics:", file=sys.stderr)
        for finding in findings:
            print(f"- {finding}", file=sys.stderr)
        return 1
    print("#2546 admission module has no primary clippy diagnostics")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
