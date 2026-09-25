#!/usr/bin/env python3
"""Adversarial checks for the credentialless B-072 evidence verifier."""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

from verify_b072_external_evidence import verify


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests/fixtures/b072_evidence_valid.json"


def run_case(value: object) -> int:
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "evidence.json"
        path.write_text(json.dumps(value), encoding="utf-8")
        return verify(path)


def main() -> int:
    valid = json.loads(FIXTURE.read_text(encoding="utf-8"))
    if verify(FIXTURE) != 0:
        raise AssertionError("valid B-072 fixture rejected")
    for outcome in ("acked", "escalated"):
        if run_case({**valid, "terminal_outcome": outcome}) != 0:
            raise AssertionError(f"valid B-072 terminal outcome rejected: {outcome}")

    mutations = {
        "missing correlation": {**valid, "correlation_id": "drifted"},
        "wrong cron": {**valid, "cron": "0 6 * * 1"},
        "credential": {**valid, "pagerduty_incident": "routing_key=secret"},
        "unredacted reference": {**valid, "d1_row": "row-123"},
        "unknown terminal outcome": {**valid, "terminal_outcome": "unacked"},
        "terminal event after capture": {**valid, "terminal_at": "2026-09-22T12:11:00Z"},
        "fractional event after capture": {
            **valid,
            "captured_at": "2026-09-22T12:10:00Z",
            "terminal_at": "2026-09-22T12:10:00.1Z",
        },
    }
    for name, mutation in mutations.items():
        if run_case(mutation) == 0:
            raise AssertionError(f"B-072 mutation unexpectedly accepted: {name}")
    print(f"B-072 evidence mutation PASS: {len(mutations)} unsafe artifacts rejected")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
