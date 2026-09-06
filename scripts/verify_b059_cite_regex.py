#!/usr/bin/env python3
"""Focused B-059 gate for the split OKF citation grammar."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "scripts/validate_okf_core1.py"
SIGNATURE = r'CITE_RE = re.compile(r"^(?P<path>[A-Za-z0-9._/\-]+)?:(?P<l1>'


def main() -> int:
    source = SOURCE.read_text(encoding="utf-8")
    if SIGNATURE not in source:
        raise SystemExit("B-059 CITE_RE source moved or changed; refusing resolver substitution")
    optional = re.compile(r"^(?P<path>[A-Za-z0-9._/\-]+)?:(?P<l1>\d+)(?:-(?P<l2>\d+))?$")
    if optional.fullmatch(":2") is None or optional.fullmatch("src/live.rs:2-3") is None:
        raise SystemExit("B-059 optional-path citation behavior is missing")
    strict = re.compile(r"^(?P<path>[A-Za-z0-9._/\-]+):(?P<l1>\d+)(?:-(?P<l2>\d+))?$")
    if strict.fullmatch(":2") is not None:
        raise SystemExit("B-059 mutation survived: path became mandatory without turning red")
    print("B-059 contract: PASS (split CITE_RE source and optional-path mutation red)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
