#!/usr/bin/env python3
"""Static and mutation guard for the B246 WebAuthn recovery-OTP tests.

The 100-cycle property is allowed to use only its explicitly test-local cheap
PHC seam. Production minting remains OWASP-2024 Argon2id and is exercised by a
real-cost end-to-end smoke test.
"""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROPERTY = ROOT / "crates/corelink-auth/tests/webauthn_prop_webauthn.rs"
PRODUCTION = ROOT / "crates/corelink-auth/src/webauthn/recovery.rs"


def secure_shape(property_source: str, production_source: str) -> bool:
    """Require the 100-cycle seam, real-cost smoke, and production floor."""
    property_markers = (
        "// This is deliberately a test-file seam, never a production configuration.",
        "const PROPERTY_ARGON2_M_COST_KIB: u32 = 8_192;",
        "const PROPERTY_ARGON2_T_COST: u32 = 1;",
        "const PROPERTY_ARGON2_P_COST: u32 = 1;",
        "for sequence in 0..100",
        "fn mint_property_otp(",
        "RecoveryOtpAlreadyConsumed",
        "fn recovery_otp_production_cost_owasp_smoke()",
        "corelink_auth::webauthn::recovery::mint_otp(",
        'get_decimal("m").unwrap_or(0), 65_536',
        'get_decimal("t").unwrap_or(0), 3',
        'get_decimal("p").unwrap_or(0), 4',
    )
    production_markers = (
        "const ARGON2_M_COST_KIB: u32 = 65_536;",
        "const ARGON2_T_COST: u32 = 3;",
        "const ARGON2_P_COST: u32 = 4;",
        "Params::new(\n        ARGON2_M_COST_KIB,\n        ARGON2_T_COST,\n        ARGON2_P_COST,",
    )
    if any(marker not in property_source for marker in property_markers):
        return False
    if any(marker not in production_source for marker in production_markers):
        return False
    # A weak test seam must never become a production override or weaken the
    # canonical constants by textual substitution.
    if "PROPERTY_ARGON2_" in production_source:
        return False
    return True


def mutation_self_test(property_source: str, production_source: str) -> None:
    """Ensure the guard rejects cycle-count and production-cost regressions."""
    if secure_shape(property_source.replace("for sequence in 0..100", "for sequence in 0..99", 1), production_source):
        raise SystemExit("B246 verifier accepted a reduced 100-cycle property")
    if secure_shape(property_source, production_source.replace(
        "const ARGON2_M_COST_KIB: u32 = 65_536;",
        "const ARGON2_M_COST_KIB: u32 = 8_192;",
        1,
    )):
        raise SystemExit("B246 verifier accepted weakened production Argon2id memory cost")
    if secure_shape(property_source.replace(
        'get_decimal("p").unwrap_or(0), 4',
        'get_decimal("p").unwrap_or(0), 1',
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
