#!/usr/bin/env python3
"""Static, mutation-backed contract for the Clerk property-test hot path."""
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/corelink-clerk/tests/prop_validate.rs"
MAX_SOURCE_BYTES = 200_000


class VerificationError(RuntimeError):
    pass


def read_source() -> str:
    if not SOURCE.is_file():
        raise VerificationError(f"missing Clerk property test: {SOURCE}")
    source = SOURCE.read_text(encoding="utf-8")
    if len(source.encode("utf-8")) > MAX_SOURCE_BYTES:
        raise VerificationError("Clerk property test exceeds bounded size")
    return source


def function_region(source: str, name: str) -> str:
    marker = f"fn {name}"
    starts = [match.start() for match in re.finditer(re.escape(marker), source)]
    if len(starts) != 1:
        raise VerificationError(f"expected exactly one function {name}, found {len(starts)}")
    start = starts[0]
    following = re.search(r"\n(?:fn |#\[tokio::test|proptest!)", source[start + len(marker) :])
    end = start + len(marker) + following.start() if following else len(source)
    if end - start > 50_000:
        raise VerificationError(f"function {name} boundary is missing or too large")
    return source[start:end]


def validate_source(source: str) -> None:
    if "PROPTEST_CASES" not in source or "unwrap_or(10_000)" not in source:
        raise VerificationError("PROPTEST_CASES default 10_000 contract is missing")
    if "cases: proptest_cases()" not in source:
        raise VerificationError("proptest case count is not wired to the runtime knob")
    required = (
        "wrong_signature_is_rejected",
        "expired_token_is_rejected",
        "issuer_mismatch_is_rejected",
        "audience_mismatch_is_rejected",
        "random_bytes_never_panic",
        "Algorithm::RS256",
        "AuthError::SignatureInvalid",
        "AuthError::Expired",
        "AuthError::IssuerMismatch",
        "AuthError::AudienceMismatch",
    )
    for marker in required:
        if marker not in source:
            raise VerificationError(f"security invariant marker missing: {marker}")

    shared_key = function_region(source, "shared_key")
    if "OnceLock<TestRsaKey>" not in shared_key or "TestRsaKey::generate(\"kid_v1\")" not in shared_key:
        raise VerificationError("RSA keypair is not process-shared through OnceLock")

    cached = function_region(source, "shared_encoding_key")
    if "OnceLock<EncodingKey>" not in cached or cached.count("EncodingKey::from_rsa_pem") != 1:
        raise VerificationError("EncodingKey PEM parse is not isolated to the OnceLock cache")

    sign = function_region(source, "sign")
    if "shared_encoding_key()" not in sign or "EncodingKey::from_rsa_pem" in sign:
        raise VerificationError("sign still parses EncodingKey per property case")
    if "encode(&header, claims, shared_encoding_key())" not in sign:
        raise VerificationError("sign does not use the cached EncodingKey")

    if source.count("EncodingKey::from_rsa_pem") != 1:
        raise VerificationError("EncodingKey::from_rsa_pem must occur exactly once")
    if "sign(shared_key()" in source:
        raise VerificationError("property cases still pass a key requiring per-case parsing")


def mutation_checks(source: str) -> None:
    validate_source(source)
    parse = "EncodingKey::from_rsa_pem(shared_key().private_pem.as_bytes())"
    mutant = source.replace("encode(&header, claims, shared_encoding_key())", parse, 1)
    if mutant == source:
        raise VerificationError("per-case parse mutation fixture did not change source")
    try:
        validate_source(mutant)
    except VerificationError:
        pass
    else:
        raise VerificationError("per-case EncodingKey parse mutation was accepted")
    validate_source(source)

    mutant = source.replace("unwrap_or(10_000)", "unwrap_or(100)", 1)
    if mutant == source:
        raise VerificationError("proptest-density mutation fixture did not change source")
    try:
        validate_source(mutant)
    except VerificationError:
        pass
    else:
        raise VerificationError("reduced PROPTEST_CASES mutation was accepted")
    validate_source(source)


def main() -> int:
    try:
        source = read_source()
        mutation_checks(source)
    except (OSError, VerificationError) as error:
        print(f"B247 DRIFTED: {error}", file=sys.stderr)
        return 1
    print("B247 candidate confirmed: cached EncodingKey; 2/2 weakening mutations rejected")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
