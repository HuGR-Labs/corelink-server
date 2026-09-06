#!/usr/bin/env python3
"""Static and mutation guard for the B246 WebAuthn recovery-OTP tests.

The 100-cycle property is allowed to use only its explicitly test-local cheap
PHC seam. Production minting remains OWASP-2024 Argon2id and is exercised by a
real-cost end-to-end smoke test.
"""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROPERTY = ROOT / "crates/corelink-auth/tests/webauthn_prop_webauthn.rs"
PRODUCTION = ROOT / "crates/corelink-auth/src/webauthn/recovery.rs"


def _rust_code(source: str) -> str:
    """Erase comments/literal contents while preserving Rust token shape.

    This is intentionally a small lexer rather than a regex: a marker in a
    comment or string must not satisfy the guard, and nested Rust block
    comments must not hide the live code that follows them.
    """
    out: list[str] = []
    i = 0
    block_depth = 0
    length = len(source)
    while i < length:
        if block_depth:
            if source.startswith("/*", i):
                block_depth += 1
                out.extend("  ")
                i += 2
            elif source.startswith("*/", i):
                block_depth -= 1
                out.extend("  ")
                i += 2
            else:
                out.append("\n" if source[i] == "\n" else " ")
                i += 1
            continue
        if source.startswith("//", i):
            i += 2
            while i < length and source[i] != "\n":
                out.append(" ")
                i += 1
            continue
        if source.startswith("/*", i):
            block_depth = 1
            out.extend("  ")
            i += 2
            continue
        # Preserve a neutral token for literals. Their contents cannot then
        # spoof an invariant marker, while calls such as get_decimal(STR)
        # remain structurally visible.
        raw_match = re.match(r'r(#+)"', source[i:])
        if raw_match:
            hashes = raw_match.group(1)
            terminator = f'"{hashes}'
            out.append("STR")
            i += len(raw_match.group(0))
            end = source.find(terminator, i)
            if end < 0:
                return ""  # unterminated literal: fail closed
            i = end + len(terminator)
            continue
        if source[i] == '"':
            out.append("STR")
            i += 1
            escaped = False
            while i < length:
                char = source[i]
                i += 1
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    break
            else:
                return ""  # unterminated literal: fail closed
            continue
        out.append(source[i])
        i += 1
    return "".join(out)


def _function_body(code: str, name: str) -> str:
    """Return one live function body, or empty on malformed source."""
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*\(", code)
    if match is None:
        return ""
    opening = code.find("{", match.end())
    if opening < 0:
        return ""
    depth = 0
    for position in range(opening, len(code)):
        if code[position] == "{":
            depth += 1
        elif code[position] == "}":
            depth -= 1
            if depth == 0:
                return code[opening + 1 : position]
    return ""


def secure_shape(property_source: str, production_source: str) -> bool:
    """Require live, function-scoped 100-cycle/smoke and production floor."""
    property_code = _rust_code(property_source)
    production_code = _rust_code(production_source)
    property_loop = _function_body(property_code, "prop_recovery_otp_single_use_100")
    property_helper = _function_body(property_code, "mint_property_otp")
    smoke = _function_body(property_code, "recovery_otp_production_cost_owasp_smoke")
    property_markers = (
        "const PROPERTY_ARGON2_M_COST_KIB: u32 = 8_192;",
        "const PROPERTY_ARGON2_T_COST: u32 = 1;",
        "const PROPERTY_ARGON2_P_COST: u32 = 1;",
    )
    production_markers = (
        "const ARGON2_M_COST_KIB: u32 = 65_536;",
        "const ARGON2_T_COST: u32 = 3;",
        "const ARGON2_P_COST: u32 = 4;",
        "Params::new(\n        ARGON2_M_COST_KIB,\n        ARGON2_T_COST,\n        ARGON2_P_COST,",
    )
    if any(marker not in property_code for marker in property_markers):
        return False
    if any(marker not in production_code for marker in production_markers):
        return False
    # A weak test seam must never become a production override or weaken the
    # canonical constants by textual substitution.
    if "PROPERTY_ARGON2_" in production_code:
        return False
    if not property_helper or "PROPERTY_ARGON2_M_COST_KIB" not in property_code:
        return False
    if (
        "for sequence in 0..100" not in property_loop
        or "mint_property_otp(user, now_ms, sequence)" not in property_loop
        or "RecoveryOtpAlreadyConsumed" not in property_loop
    ):
        return False
    if (
        "corelink_auth::webauthn::recovery::mint_otp(" not in smoke
        or smoke.count("get_decimal(STR).unwrap_or(0)") != 3
        or ", 65_536" not in smoke
        or ", 3" not in smoke
        or ", 4" not in smoke
        or "RecoveryOtpAlreadyConsumed" not in smoke
    ):
        return False
    return True


def mutation_self_test(property_source: str, production_source: str) -> None:
    """Ensure the guard rejects cycle-count and production-cost regressions."""
    live_99_with_comment = property_source.replace(
        "for sequence in 0..100", "for sequence in 0..99", 1
    ) + "\n// for sequence in 0..100\n"
    if secure_shape(live_99_with_comment, production_source):
        raise SystemExit("B246 verifier accepted a reduced 100-cycle property")
    weak_property_with_comment = property_source.replace(
        "const PROPERTY_ARGON2_M_COST_KIB: u32 = 8_192;",
        "// const PROPERTY_ARGON2_M_COST_KIB: u32 = 8_192;\n"
        "const PROPERTY_ARGON2_M_COST_KIB: u32 = 1;",
        1,
    )
    if secure_shape(weak_property_with_comment, production_source):
        raise SystemExit("B246 verifier accepted a commented weak property marker")
    weak_production_with_comment = production_source.replace(
        "const ARGON2_M_COST_KIB: u32 = 65_536;",
        "const ARGON2_M_COST_KIB: u32 = 8_192;\n"
        "// const ARGON2_M_COST_KIB: u32 = 65_536;",
        1,
    )
    if secure_shape(property_source, weak_production_with_comment):
        raise SystemExit("B246 verifier accepted weakened production Argon2id memory cost")
    if secure_shape(property_source.replace(
        'get_decimal(\"p\").unwrap_or(0), 4',
        'get_decimal(\"p\").unwrap_or(0), 1',
        1,
    ), production_source):
        raise SystemExit("B246 verifier accepted weakened production-cost smoke")


def main() -> int:
    property_source = PROPERTY.read_text(encoding="utf-8")
    production_source = PRODUCTION.read_text(encoding="utf-8")
    if not secure_shape(property_source, production_source):
        raise SystemExit("B246 verifier: OTP test/production security shape is incomplete")
    mutation_self_test(property_source, production_source)
    print("B246 verifier: 100-cycle single-use seam + OWASP production-cost smoke PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
