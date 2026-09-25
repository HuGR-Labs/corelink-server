#!/usr/bin/env python3
"""Static contract checks for #2576's authenticated atomic admission seam."""

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
MODULE = ROOT / "crates/corelink-container/src/storage/staging_load_test_admission.rs"
FORBIDDEN_INDEX = re.compile(r"\b(?:nonce|nonce_bytes|input|output|digest|encoded)\s*\[")


def main() -> int:
    source = MODULE.read_text(encoding="utf-8")
    required = (
        'const AUTH_DOMAIN: &[u8] = b"corelink/staging-load-admission-auth/v1\\0";',
        'const NONCE_DOMAIN: &[u8] = b"corelink/staging-load-admission-nonce/v1\\0";',
        "mac.verify_slice(&tag)",
        "fn decode_nonce(",
        "fn nonce_digest_hex(",
        "for byte in nonce_bytes.iter().copied()",
        "for byte in bytes",
        'write!(&mut encoded, "{byte:02x}")',
        ".batch(vec![",
        "D1BatchStatement::new(SQL_INSERT_RUN, run_params)",
        "D1BatchStatement::new(SQL_INSERT_NONCE, nonce_params)",
        '.field("key", &"[REDACTED]")',
        '.field("nonce_digest", &"[REDACTED]")',
    )
    missing = [fragment for fragment in required if fragment not in source]
    if missing:
        print(f"missing #2576 contract fragments: {missing!r}", file=sys.stderr)
        return 1
    if FORBIDDEN_INDEX.search(source):
        print("nonce/digest indexing or slicing is forbidden", file=sys.stderr)
        return 1
    mutated = source.replace(
        'for byte in bytes {\n        write!(&mut encoded, "{byte:02x}")',
        'for byte in bytes {\n        let byte = digest[0];\n        write!(&mut encoded, "{byte:02x}")',
        1,
    )
    if mutated == source or not FORBIDDEN_INDEX.search(mutated):
        print("indexing negative fixture is ineffective", file=sys.stderr)
        return 1
    print("#2576 verifier contract passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
