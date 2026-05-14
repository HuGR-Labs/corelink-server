#!/usr/bin/env python3
"""notice_text_hash_canonical.py — Canonical deterministic notice text hash.

Algorithm (per WI-S11-004 AC-006 + corelink-privacy-notice-emit::event::notice_text_hash):
  1. CRLF → LF normalization.
  2. CR-only → LF normalization.
  3. Trim trailing whitespace per line.
  4. Rejoin lines with '\\n'.
  5. UTF-8 NFC normalization (unicodedata.normalize).
  6. SHA-256 hex64 of UTF-8 bytes.

Cross-validates with Rust crate corelink-privacy-notice-emit::notice_text_hash.
Property test verifies 100% parity for ASCII/Latin-supplement corpus.

Usage:
    python3 scripts/notice_text_hash_canonical.py legal/privacy-notice/v1.0.0/pt-BR.md
    python3 scripts/notice_text_hash_canonical.py --all legal/privacy-notice/v1.0.0/
"""

import hashlib
import pathlib
import sys
import unicodedata


def notice_text_hash(content: str) -> str:
    """Compute the canonical deterministic SHA-256 hex64 hash of notice content.

    Steps:
      1. CRLF → LF.
      2. CR-only → LF.
      3. Trim trailing whitespace per line.
      4. Rejoin with '\\n'.
      5. UTF-8 NFC normalize.
      6. SHA-256 hex64.
    """
    # Step 1+2: normalize line endings.
    normalized = content.replace("\r\n", "\n").replace("\r", "\n")
    # Step 3+4: trim trailing whitespace per line and rejoin.
    trimmed = "\n".join(line.rstrip() for line in normalized.split("\n"))
    # Step 5: UTF-8 NFC normalization.
    nfc = unicodedata.normalize("NFC", trimmed)
    # Step 6: SHA-256.
    return hashlib.sha256(nfc.encode("utf-8")).hexdigest()


def hash_file(path: pathlib.Path) -> str:
    """Hash a single notice file."""
    content = path.read_text(encoding="utf-8")
    return notice_text_hash(content)


def main() -> None:
    args = sys.argv[1:]
    if not args:
        print("Usage: notice_text_hash_canonical.py [--all <dir>] <file>", file=sys.stderr)
        sys.exit(1)

    if args[0] == "--all":
        if len(args) < 2:
            print("--all requires a directory argument", file=sys.stderr)
            sys.exit(1)
        directory = pathlib.Path(args[1])
        for locale_file in sorted(directory.glob("*.md")):
            h = hash_file(locale_file)
            print(f"{h}  {locale_file.name}")
    else:
        file_path = pathlib.Path(args[0])
        if not file_path.exists():
            print(f"File not found: {file_path}", file=sys.stderr)
            sys.exit(1)
        print(hash_file(file_path))


if __name__ == "__main__":
    main()
