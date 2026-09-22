#!/usr/bin/env python3
"""Fail closed if protected B-054 key material appears in tracked text or output."""

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
TEXT_SUFFIXES = {
    ".json",
    ".lock",
    ".md",
    ".mjs",
    ".py",
    ".rs",
    ".sh",
    ".sql",
    ".toml",
    ".ts",
    ".tsx",
    ".yaml",
    ".yml",
}


def protected_needles() -> list[bytes]:
    needles: list[bytes] = []
    for name in SECRET_NAMES:
        value = os.environ.get(name, "")
        try:
            decoded = bytes.fromhex(value)
        except ValueError:
            raise ValueError("a protected value is malformed") from None
        if len(value) != 64 or len(decoded) != 32:
            raise ValueError("a protected value has an invalid length")
        needles.extend((value.encode("ascii"), value.upper().encode("ascii"), decoded))
    return needles


def tracked_source_paths() -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z"], check=True, capture_output=True
    )
    return [
        Path(os.fsdecode(raw))
        for raw in result.stdout.split(b"\0")
        if raw and Path(os.fsdecode(raw)).suffix.lower() in TEXT_SUFFIXES
    ]


def main() -> int:
    if len(sys.argv) != 3:
        print(
            "usage: verify_b054_custody_redaction.py TRANSCRIPT RECEIPT",
            file=sys.stderr,
        )
        return 2

    try:
        needles = protected_needles()
        source_paths = tracked_source_paths()
    except (OSError, subprocess.CalledProcessError, ValueError):
        print("B-054 redaction check failed: protected inputs unavailable or invalid", file=sys.stderr)
        return 1

    paths = source_paths + [Path(arg) for arg in sys.argv[1:]]
    for path in paths:
        try:
            payload = path.read_bytes()
        except OSError:
            print("B-054 redaction check failed: an expected file could not be read", file=sys.stderr)
            return 1
        if any(needle in payload for needle in needles):
            print("B-054 redaction check failed: protected bytes found; output withheld", file=sys.stderr)
            return 1

    print("B-054 redaction check passed: tracked text, transcript, and receipt are clear")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
