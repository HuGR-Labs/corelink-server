#!/usr/bin/env python3
"""Fail closed if synthetic custody keys appear in source, logs, or receipt."""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys


SECRET_NAMES = (
    "B054_E1_SIGNING_SEED_HEX",
    "B054_E1_LINK_KEY_HEX",
    "B054_E2_SIGNING_SEED_HEX",
    "B054_E2_LINK_KEY_HEX",
)
SOURCE_SUFFIXES = {
    ".rs",
    ".toml",
    ".yml",
    ".yaml",
    ".json",
    ".md",
    ".sh",
    ".py",
    ".ts",
    ".tsx",
    ".js",
    ".mjs",
    ".sql",
    ".lock",
}


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: verify_b054_custody_redaction.py TRANSCRIPT RECEIPT", file=sys.stderr)
        return 2

    needles: list[bytes] = []
    for name in SECRET_NAMES:
        value = os.environ.get(name, "")
        try:
            decoded = bytes.fromhex(value)
        except ValueError:
            print("B-054 redaction check: a protected value is malformed", file=sys.stderr)
            return 1
        if len(decoded) != 32 or len(value) != 64:
            print("B-054 redaction check: a protected value has an invalid length", file=sys.stderr)
            return 1
        needles.extend((value.encode("ascii"), decoded))

    try:
        tracked = subprocess.run(
            ["git", "ls-files", "-z"], check=True, capture_output=True
        ).stdout.split(b"\0")
    except (OSError, subprocess.CalledProcessError):
        print("B-054 redaction check: tracked source inventory unavailable", file=sys.stderr)
        return 1

    paths = [Path(arg) for arg in sys.argv[1:]]
    for rel in tracked:
        if not rel:
            continue
        path = Path(os.fsdecode(rel))
        if path.suffix.lower() in SOURCE_SUFFIXES:
            paths.append(path)

    for path in paths:
        try:
            payload = path.read_bytes()
        except OSError:
            print("B-054 redaction check: a source/output file could not be read", file=sys.stderr)
            return 1
        if any(needle in payload for needle in needles):
            print("B-054 redaction check: raw or encoded key material detected", file=sys.stderr)
            return 1

    print("B-054 redaction check passed: no key bytes in tracked source, test output, or receipt")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
