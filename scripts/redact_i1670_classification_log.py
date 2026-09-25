#!/usr/bin/env python3
"""Remove absolute filesystem paths from B-128 hosted failure log artifacts."""
from __future__ import annotations

import argparse
import re
from pathlib import Path


# Linux runner diagnostics can include a fixture location or the absolute linker
# path. Keep classification text, exit codes, and annotations while removing
# any absolute path token from the published log.
ABSOLUTE_PATH_RE = re.compile(r"(?<![A-Za-z0-9_./:])/(?:[A-Za-z0-9_.@+-]+/)*[A-Za-z0-9_.@+-]+")
REDACTED_PATH = "<redacted-path>"


def redact_paths(text: str) -> str:
    """Replace absolute Linux path tokens without changing diagnostic meaning."""
    return ABSOLUTE_PATH_RE.sub(REDACTED_PATH, text)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    raw = args.input.read_text(encoding="utf-8")
    redacted = redact_paths(raw)
    if ABSOLUTE_PATH_RE.search(redacted):
        raise SystemExit("B-128 log redaction left an absolute filesystem path")
    args.output.write_text(redacted, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
