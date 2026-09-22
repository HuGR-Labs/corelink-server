#!/usr/bin/env python3
"""Offline mutation checks for the B-129 live probe contract."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("probe", ROOT / "scripts/probe_i1671_b129.py")
assert spec and spec.loader
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)

SHA = "0123456789abcdef0123456789abcdef01234567"
REGION = "Sam"
BASE_HEADERS = {
    "x-corelink-deployed-sha": SHA,
    "x-corelink-server-timing-wdb-detail": "on",
    "x-corelink-deployed-region": REGION,
    "x-request-id": "req-1",
    "cf-ray": "ray-1",
}
VALUES = {name: 1.0 for name in (*probe.Q, "ohop", *probe.ORIGIN)}
VALUES.update({"wdb": 5.0, "origin": 11.0, "total": 17.0})


def rejects(mutated_headers=None, mutated_values=None):
    try:
        probe.validate_sample(200, mutated_headers or BASE_HEADERS, mutated_values or VALUES, 0.02, SHA, REGION)
    except RuntimeError:
        return
    raise AssertionError("B-129 probe accepted an incomplete identity or phase row")


assert probe.validate_sample(200, BASE_HEADERS, VALUES, 0.02, SHA, REGION) < 10
for name in ("ostore", "oaccounting", "ohandler"):
    missing = dict(VALUES)
    del missing[name]
    rejects(mutated_values=missing)
rejects(mutated_headers={**BASE_HEADERS, "x-corelink-deployed-region": "Enam"})
rejects(mutated_headers={key: value for key, value in BASE_HEADERS.items() if key != "x-corelink-deployed-sha"})
