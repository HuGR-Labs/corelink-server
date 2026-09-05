#!/usr/bin/env python3
"""Static, mutation-backed contract for the Clerk property-test hot path."""
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/corelink-clerk/tests/prop_validate.rs"
FAKE_SOURCE = ROOT / "crates/corelink-clerk/src/fakes.rs"
MAX_SOURCE_BYTES = 200_000
STRUCTURAL_INVARIANTS = {
    "wrong_signature_is_rejected": (
        "sig_bytes[idx] = new_byte;",
        "adapter.validate(&mutated)",
        "AuthError::SignatureInvalid",
    ),
    "expired_token_is_rejected": (
        "claims.exp =",
        "adapter.validate(&jwt)",
        "AuthError::Expired",
    ),
    "issuer_mismatch_is_rejected": (
        "claims.iss = bogus.clone();",
        "adapter.validate(&jwt)",
        "AuthError::IssuerMismatch",
        "prop_assert_eq!(&got, &bogus)",
    ),
    "audience_mismatch_is_rejected": (
        "claims.aud = bogus;",
        "adapter.validate(&jwt)",
        "AuthError::AudienceMismatch",
    ),
    "random_bytes_never_panic": (
        "String::from_utf8_lossy(&blob)",
        "adapter.validate(&s)",
        "result.is_err()",
    ),
}


class VerificationError(RuntimeError):
    pass


def read_source() -> str:
    if not SOURCE.is_file():
        raise VerificationError(f"missing Clerk property test: {SOURCE}")
    source = SOURCE.read_text(encoding="utf-8")
    if len(source.encode("utf-8")) > MAX_SOURCE_BYTES:
        raise VerificationError("Clerk property test exceeds bounded size")
    return source


def read_fake_source() -> str:
    if not FAKE_SOURCE.is_file():
        raise VerificationError(f"missing test-only RSA fake source: {FAKE_SOURCE}")
    source = FAKE_SOURCE.read_text(encoding="utf-8")
    if len(source.encode("utf-8")) > MAX_SOURCE_BYTES:
        raise VerificationError("test-only RSA fake source exceeds bounded size")
    return source


def strip_rust_comments(source: str) -> str:
    """Blank comments while preserving strings, code, and line positions."""
    out: list[str] = []
    index = 0
    block_depth = 0
    while index < len(source):
        if block_depth:
            if source.startswith("/*", index):
                block_depth += 1
                out.extend("  ")
                index += 2
            elif source.startswith("*/", index):
                block_depth -= 1
                out.extend("  ")
                index += 2
            else:
                out.append("\n" if source[index] == "\n" else " ")
                index += 1
            continue
        if source.startswith("//", index):
            out.extend("  ")
            index += 2
            while index < len(source) and source[index] != "\n":
                out.append(" ")
                index += 1
            continue
        if source.startswith("/*", index):
            block_depth = 1
            out.extend("  ")
            index += 2
            continue
        if source[index] == '"':
            out.append(source[index])
            index += 1
            while index < len(source):
                out.append(source[index])
                if source[index] == "\\" and index + 1 < len(source):
                    index += 1
                    out.append(source[index])
                elif source[index] == '"':
                    index += 1
                    break
                index += 1
            continue
        out.append(source[index])
        index += 1
    if block_depth:
        raise VerificationError("unterminated Rust block comment")
    return "".join(out)


def function_region(source: str, name: str) -> str:
    marker = f"fn {name}"
    starts = [match.start() for match in re.finditer(rf"\b{re.escape(marker)}\b", source)]
    if len(starts) != 1:
        raise VerificationError(f"expected exactly one function {name}, found {len(starts)}")
    start = starts[0]
    following = re.search(
        r"\n[ \t]*(?:fn |#\[tokio::test|proptest!)", source[start + len(marker) :]
    )
    end = start + len(marker) + following.start() if following else len(source)
    if end - start > 50_000:
        raise VerificationError(f"function {name} boundary is missing or too large")
    return source[start:end]


def validate_source(source: str, fake_source: str | None = None) -> None:
    code = strip_rust_comments(source)
    fake_code = strip_rust_comments(fake_source if fake_source is not None else read_fake_source())
    if "PROPTEST_CASES" not in code or "unwrap_or(10_000)" not in code:
        raise VerificationError("PROPTEST_CASES default 10_000 contract is missing")
    if "cases: proptest_cases()" not in code:
        raise VerificationError("proptest case count is not wired to the runtime knob")
    if "const PROPERTY_RSA_BITS: usize = 512;" not in code:
        raise VerificationError("property loop must pin its test-only RSA key to 512 bits")
    if "const SMOKE_RSA_BITS: usize = 2048;" not in code:
        raise VerificationError("RSA-2048 smoke key size is not pinned explicitly")
    for name, markers in STRUCTURAL_INVARIANTS.items():
        region = function_region(code, name)
        for marker in markers:
            if marker not in region:
                raise VerificationError(f"{name} missing structural marker: {marker}")

    sign_code = function_region(code, "sign")
    if "Algorithm::RS256" not in sign_code:
        raise VerificationError("sign no longer fixes Algorithm::RS256")

    sign = function_region(code, "sign")
    if "shared_property_encoding_key()" not in sign or "EncodingKey::from_rsa_pem" in sign:
        raise VerificationError("sign still parses EncodingKey per property case")
    if "encode(&header, claims, shared_property_encoding_key())" not in sign:
        raise VerificationError("sign does not use the cached EncodingKey")

    if code.count("EncodingKey::from_rsa_pem") != 2:
        raise VerificationError("EncodingKey::from_rsa_pem must occur only in the two caches")
    if "sign(shared_property_key()" in code:
        raise VerificationError("property cases still pass a key requiring per-case parsing")

    property_key = function_region(code, "shared_property_key")
    if "OnceLock<TestRsaKey>" not in property_key or "generate_with_bits(\"kid_property\", PROPERTY_RSA_BITS)" not in property_key:
        raise VerificationError("property key is not test-only and OnceLock-cached")
    smoke_key = function_region(code, "shared_smoke_key")
    if "generate_with_bits(\"kid_smoke_2048\", SMOKE_RSA_BITS)" not in smoke_key:
        raise VerificationError("RSA-2048 smoke key is not generated explicitly")
    smoke_encoding = function_region(code, "shared_smoke_encoding_key")
    if "OnceLock<EncodingKey>" not in smoke_encoding or "EncodingKey::from_rsa_pem" not in smoke_encoding:
        raise VerificationError("RSA-2048 smoke key is not cached before signing")
    smoke = function_region(code, "rsa_2048_smoke_validates")
    for marker in ("let key = shared_smoke_key();", "build_adapter_for(key)", "sign_2048_smoke", "RSA-2048 smoke"):
        if marker not in smoke:
            raise VerificationError(f"RSA-2048 smoke is missing structural marker: {marker}")

    fake_key = function_region(fake_code, "generate_with_bits")
    if '#[cfg(feature = "test-utils")]' not in fake_code or "pub mod test_keys" not in fake_code:
        raise VerificationError("variable-size RSA helper escaped the test-utils feature boundary")
    if "RsaPrivateKey::new(&mut rng, bits)" not in fake_key:
        raise VerificationError("test-only RSA fake does not honor explicit key size")
    if "pub fn generate(kid: &str)" not in fake_code or "generate_with_bits(kid, 2048)" not in fake_code:
        raise VerificationError("default test RSA helper no longer pins RSA-2048")


def mutation_checks(source: str) -> None:
    fake_source = read_fake_source()
    validate_source(source, fake_source)
    parse = "EncodingKey::from_rsa_pem(shared_property_key().private_pem.as_bytes())"
    mutant = source.replace("encode(&header, claims, shared_property_encoding_key())", parse, 1)
    if mutant == source:
        raise VerificationError("per-case parse mutation fixture did not change source")
    try:
        validate_source(mutant, fake_source)
    except VerificationError:
        pass
    else:
        raise VerificationError("per-case EncodingKey parse mutation was accepted")
    validate_source(source, fake_source)

    mutant = source.replace("unwrap_or(10_000)", "unwrap_or(100)", 1)
    if mutant == source:
        raise VerificationError("proptest-density mutation fixture did not change source")
    try:
        validate_source(mutant, fake_source)
    except VerificationError:
        pass
    else:
        raise VerificationError("reduced PROPTEST_CASES mutation was accepted")
    validate_source(source, fake_source)

    for old, new, message in (
        (
            "const PROPERTY_RSA_BITS: usize = 512;",
            "const PROPERTY_RSA_BITS: usize = 2048;",
            "property weak-key boundary",
        ),
        (
            "const SMOKE_RSA_BITS: usize = 2048;",
            "const SMOKE_RSA_BITS: usize = 512;",
            "RSA-2048 smoke boundary",
        ),
    ):
        mutant = source.replace(old, new, 1)
        if mutant == source:
            raise VerificationError(f"{message} mutation fixture did not change source")
        try:
            validate_source(mutant, fake_source)
        except VerificationError:
            pass
        else:
            raise VerificationError(f"{message} mutation was accepted")
        validate_source(source, fake_source)

    fake_mutant = fake_source.replace('#[cfg(feature = "test-utils")]', "#[cfg(feature = \"production\")]", 1)
    if fake_mutant == fake_source:
        raise VerificationError("test-utils boundary mutation fixture did not change source")
    try:
        validate_source(source, fake_mutant)
    except VerificationError:
        pass
    else:
        raise VerificationError("variable-size RSA helper boundary mutation was accepted")
    validate_source(source, fake_source)

    mutant = source.replace("async fn rsa_2048_smoke_validates()", "async fn rsa_smoke_validates()", 1)
    if mutant == source:
        raise VerificationError("RSA-2048 smoke removal mutation fixture did not change source")
    try:
        validate_source(mutant, fake_source)
    except VerificationError:
        pass
    else:
        raise VerificationError("RSA-2048 smoke removal mutation was accepted")
    validate_source(source, fake_source)

    for name, markers in STRUCTURAL_INVARIANTS.items():
        marker = markers[0]
        mutant = source.replace(marker, f"// removed {name}: {marker}", 1)
        if mutant == source:
            raise VerificationError(f"{name} mutation fixture did not change source")
        try:
            validate_source(mutant, fake_source)
        except VerificationError:
            pass
        else:
            raise VerificationError(f"{name} weakening mutation was accepted")
        validate_source(source, fake_source)


def main() -> int:
    try:
        source = read_source()
        mutation_checks(source)
    except (OSError, VerificationError) as error:
        print(f"B247 DRIFTED: {error}", file=sys.stderr)
        return 1
    print("B247 candidate confirmed: test-only 512-bit property key + 2048-bit smoke; 11/11 mutations rejected")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
