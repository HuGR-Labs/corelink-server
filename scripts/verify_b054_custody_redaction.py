#!/usr/bin/env python3
"""Fail closed if synthetic B-054 material appears in source or publishable output."""

from __future__ import annotations

import base64
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
    ".txt",
    ".yml",
    ".yaml",
}


class RedactionError(RuntimeError):
    pass


def protected_values() -> list[bytes]:
    values: list[bytes] = []
    for name in SECRET_NAMES:
        value = os.environ.get(name, "")
        if len(value) != 64 or any(char not in "0123456789abcdef" for char in value):
            raise RedactionError(f"protected value {name} is absent or malformed")
        raw = bytes.fromhex(value)
        values.extend(
            (
                value.encode("ascii"),
                value.upper().encode("ascii"),
                raw,
                base64.b64encode(raw),
                base64.b64encode(raw).rstrip(b"="),
                base64.urlsafe_b64encode(raw),
                base64.urlsafe_b64encode(raw).rstrip(b"="),
            )
        )
    return values


def tracked_text() -> list[tuple[str, bytes]]:
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    files: list[tuple[str, bytes]] = []
    for encoded_path in result.stdout.split(b"\0"):
        if not encoded_path:
            continue
        path = Path(encoded_path.decode("utf-8", errors="strict"))
        if path.suffix.lower() not in TEXT_SUFFIXES or not path.is_file():
            continue
        try:
            files.append(("tracked source", path.read_bytes()))
        except OSError:
            raise RedactionError("tracked source could not be scanned") from None
    return files


def main() -> int:
    try:
        if len(sys.argv) != 3:
            raise RedactionError("expected a transcript and redacted receipt path")
        transcript_path, receipt_path = map(Path, sys.argv[1:])
        approval_path = Path(os.environ.get("B054_APPROVAL_RECEIPT_PATH", ""))
        if not transcript_path.is_file() or not receipt_path.is_file() or not approval_path.is_file():
            raise RedactionError("transcript or receipt is missing")
        content = tracked_text() + [
            ("test transcript", transcript_path.read_bytes()),
            ("approval receipt", approval_path.read_bytes()),
            ("redacted receipt", receipt_path.read_bytes()),
        ]
        values = protected_values()
        if any(secret and secret in blob for _, blob in content for secret in values):
            raise RedactionError("protected material matched a scanned source or output boundary")
    except (OSError, subprocess.CalledProcessError, UnicodeError, RedactionError) as error:
        print(f"B-054 output withheld; redaction check failed ({error})", file=sys.stderr)
        return 1

    print("B-054 redaction: PASS (tracked source, transcript, approval, and receipt scanned)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
