#!/usr/bin/env python3
"""Verify the #2546 nonce codec contract, including a negative mutation fixture."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODULE = ROOT / "crates/corelink-container/src/storage/staging_load_test_admission.rs"
INDEXING = re.compile(r"\b(?:nonce|nonce_bytes|input|bytes|digest|encoded)\s*\[")


class VerificationError(RuntimeError):
    pass


def verify_source(source: str) -> None:
    required = (
        "const NONCE_DOMAIN: &[u8] = b\"corelink/staging-load-admission-nonce/v1\\0\";",
        "fn decode_nonce(",
        "fn nonce_digest_hex(",
        "const MAX_CLAIM_LIFETIME_MS: i64 = 15 * 60 * 1_000;",
        "const SQL_INSERT_RUN:",
        "const SQL_INSERT_NONCE:",
        "hasher.update(NONCE_DOMAIN);",
        "for byte in nonce_bytes.iter().copied()",
        "for byte in digest",
        'write!(&mut encoded, "{byte:02x}")',
        ".batch(vec![",
        "D1BatchStatement::new(SQL_INSERT_RUN, run_params)",
        "D1BatchStatement::new(SQL_INSERT_NONCE, nonce_params)",
        '.field("nonce", &"[REDACTED]")',
    )
    missing = [fragment for fragment in required if fragment not in source]
    if missing:
        raise VerificationError(f"missing nonce codec contract fragments: {missing!r}")
    if INDEXING.search(source):
        match = INDEXING.search(source)
        raise VerificationError(f"indexing or slicing is forbidden in nonce codec: {match.group(0)!r}")


def check_negative_fixture(source: str) -> None:
    mutation = source.replace(
        "for byte in digest {\n        write!(&mut encoded, \"{byte:02x}\")",
        "for byte in digest {\n        let byte = digest[0];\n        write!(&mut encoded, \"{byte:02x}\")",
        1,
    )
    if mutation == source:
        raise VerificationError("indexing negative fixture did not mutate the digest encoder")
    try:
        verify_source(mutation)
    except VerificationError:
        return
    raise VerificationError("indexing-based digest encoder was accepted by the verifier")


def main() -> int:
    try:
        source = MODULE.read_text(encoding="utf-8")
        verify_source(source)
        check_negative_fixture(source)
    except (OSError, VerificationError) as error:
        print(f"#2546 admission verifier: {error}", file=sys.stderr)
        return 1
    print("#2546 admission verifier passed (indexing mutation rejected)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
