#!/usr/bin/env python3
"""Reject only clippy diagnostics whose primary span is #2576 code."""

import json
import sys
from pathlib import Path


TARGET = "crates/corelink-container/src/storage/staging_load_test_admission.rs"


def main() -> int:
    if len(sys.argv) != 2:
        return 2
    records = [json.loads(line) for line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines() if line]
    if not any(record.get("reason") == "build-finished" for record in records):
        print("cargo clippy emitted no build-finished record", file=sys.stderr)
        return 2
    findings = []
    for record in records:
        if record.get("reason") != "compiler-message":
            continue
        message = record["message"]
        if any(span.get("is_primary") and span.get("file_name", "").replace("\\", "/").endswith(TARGET) for span in message.get("spans", [])):
            findings.append(message.get("message", "candidate diagnostic"))
    if findings:
        print("#2576 candidate clippy diagnostics:\n" + "\n".join(findings), file=sys.stderr)
        return 1
    print("#2576 admission module has no primary clippy diagnostics")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
