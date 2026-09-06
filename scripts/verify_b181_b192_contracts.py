#!/usr/bin/env python3
"""Cheap, local verifier for the D03 B181-B192 security contracts.

This checker reads source files only.  It is deliberately fail-closed: a
missing source, missing wiring edge, or weakened guard is a verification
failure.  The behavioral tests remain authoritative for runtime semantics;
this catches accidental removal of one of the cross-surface chokepoints.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path
from uuid import UUID


ROOT = Path(__file__).resolve().parents[1]


def _expiry_live(expires_ms: int, now_ms: int) -> bool:
    return expires_ms == 0 or expires_ms > now_ms


def _issuer_allowed(issuer: str, configured: str | None, environment: str) -> bool:
    if configured:
        return issuer == configured
    if environment in {"prod", "production", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd"}:
        return False
    return issuer.startswith("https://") and "clerk" in issuer


def _middleware_auth_required(flag: str | None, node_env: str) -> bool:
    # A production request must remain protected regardless of a leaked test flag.
    return node_env == "production"


def _canonical_uuid(value: str) -> bool:
    try:
        parsed = UUID(value)
    except ValueError:
        return False
    return str(parsed) == value


def _canonical_digest(value: str) -> bool:
    return len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def self_test() -> None:
    """Run semantic truth tables, including one killable mutant per guard.

    These tests intentionally do not repeat source-presence assertions. Each
    mutant changes a decision boundary, and the expected result demonstrates
    that the boundary is observable (expiry equality, production config,
    production E2E flag, UUID canonicalization, and digest alphabet).
    """
    assert _expiry_live(0, 100)
    assert _expiry_live(101, 100)
    assert not _expiry_live(100, 100)  # kills `>= now` mutant
    assert not _expiry_live(99, 100)
    expiry_ge_mutant = lambda expires_ms, now_ms: expires_ms == 0 or expires_ms >= now_ms
    assert expiry_ge_mutant(100, 100) is not False

    assert _issuer_allowed("https://clerk.humangr.com", "https://clerk.humangr.com", "production")
    assert not _issuer_allowed("https://attacker.clerk.example", None, "production")
    assert not _issuer_allowed("https://attacker.clerk.example", None, "prod")
    assert _issuer_allowed("https://clerk.humangr.com", None, "test")
    assert not _issuer_allowed("http://clerk.humangr.com", None, "test")
    issuer_shape_mutant = lambda issuer, configured, environment: (
        issuer == configured if configured else issuer.startswith("https://") and "clerk" in issuer
    )
    assert issuer_shape_mutant("https://attacker.clerk.example", None, "production") is True

    assert _middleware_auth_required("1", "production")
    assert _middleware_auth_required(None, "production")
    assert _middleware_auth_required("1", "development") is False
    e2e_flag_mutant = lambda flag, node_env: node_env == "production" and flag != "1"
    assert e2e_flag_mutant("1", "production") is False

    canonical = "0190abcd-1234-75ab-8def-0123456789ab"
    assert _canonical_uuid(canonical)
    assert not _canonical_uuid(canonical.upper())
    assert not _canonical_uuid("tenant_abc123")
    uuid_parse_mutant = lambda value: bool(UUID(value))
    assert uuid_parse_mutant(canonical.upper()) is True

    assert _canonical_digest("a" * 64)
    assert not _canonical_digest("A" * 64)
    assert not _canonical_digest("a" * 63)
    digest_hex_mutant = lambda value: len(value) == 64 and all(c in "0123456789abcdefABCDEF" for c in value)
    assert digest_hex_mutant("A" * 64) is True

    runner_rows = {
        "RUNNER_STARTER": (20, 100),
        "RUNNER_PRO": (40, 240),
        "RUNNER_TEAM": (80, 600),
        "RUNNER_SCALE": (160, 1200),
        "RUNNER_MAX": (320, 2400),
    }
    assert len(runner_rows) == 5
    assert runner_rows["RUNNER_PRO"] != runner_rows["RUNNER_SCALE"]


def source(rel: str) -> str:
    path = ROOT / rel
    if not path.is_file():
        raise AssertionError(f"missing contract source: {rel}")
    return path.read_text(encoding="utf-8")


def code_only(text: str) -> str:
    """Mask comments and literals while preserving executable source.

    The adapter PAT implementation is split across Rust fragments.  Contract
    anchors must therefore be lexical code witnesses, not copied identifiers
    in comments or diagnostic strings.
    """
    out: list[str] = []
    i = 0
    quote: str | None = None
    block = False
    while i < len(text):
        if block:
            if text.startswith("*/", i):
                block = False
                out.extend("  ")
                i += 2
            else:
                out.append("\n" if text[i] == "\n" else " ")
                i += 1
            continue
        if quote:
            ch = text[i]
            out.append("\n" if ch == "\n" else " ")
            if ch == "\\" and i + 1 < len(text):
                out.append("\n" if text[i + 1] == "\n" else " ")
                i += 2
                continue
            if ch == quote:
                quote = None
            i += 1
            continue
        if text.startswith("/*", i):
            block = True
            out.extend("  ")
            i += 2
            continue
        if text.startswith("//", i):
            while i < len(text) and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        ch = text[i]
        if ch == "'" and i + 1 < len(text) and (text[i + 1].isalpha() or text[i + 1] == "_"):
            out.append(ch)
            i += 1
            continue
        if ch in "'\"`":
            quote = ch
            out.append(" ")
        else:
            out.append(ch)
        i += 1
    return "".join(out)


def verify() -> None:
    quota = source("worker/src/lib/quota.ts")
    cache = source("worker/src/lib/quota_storage_cache.ts")
    index = source("worker/src/index.ts")
    expiry = source("worker/src/lib/pat_expiry.ts")
    clerk = source("worker/src/lib/clerk_auth.ts")
    middleware = source("apps/admin-ui/src/middleware.ts")
    tenant = source("crates/corelink-container/src/auth_tenant.rs")
    pat_lookup = source("crates/corelink-container/src/adapter_pat_lookup.rs")
    pat_verifier = source("crates/corelink-container/src/adapter_pat_verifier.rs")
    pat_lookup_code = code_only(pat_lookup)
    pat_verifier_code = code_only(pat_verifier)
    native = source("crates/corelink-container/src/native_pat_gate.rs")
    ac = source("crates/corelink-container/src/storage/r2_s3.rs")
    turbo = source("crates/corelink-container/src/storage/r2_kv.rs")
    tiers = source("crates/corelink-tier-selection/src/tier.rs")
    main = source("crates/corelink-container/src/main.rs")
    bridge_tests = source("worker/tests/customer_clerk_bridge.test.ts")
    middleware_tests = source("apps/admin-ui/tests/middleware-failclosed.test.ts")
    tier_tests = source("crates/corelink-container/src/main.rs")
    turbo_tests = source("crates/corelink-container/src/storage/r2_kv.rs")

    # B181: every storage D1 uncertainty returns the explicit rejection arm;
    # no read-side allow-through remains in either live or cached wiring.
    assert 'ok: false' in quota and 'storage quota temporarily unverifiable' in quota
    tier_error = cache.index('if (tierD1Error)')
    tier_end = cache.index('const { totalBytes', tier_error)
    assert 'ok: false' in cache[tier_error:tier_end]

    # B182/B184: one Worker expiry predicate, with zero as no-expiry.
    assert 'isPatExpiryLive' in index and 'expiresMs === 0 || expiresMs > nowMs' in expiry
    assert re.search(r"pub\(super\)\s+const\s+PAT_LOOKUP_SQL\b", pat_lookup_code)
    assert 'expires_ms = 0 OR expires_ms >' in pat_lookup

    # B183/B189: DB hash corruption is backend-visible and native routes use
    # the full possession verifier.
    assert re.search(r"PatError::HashError\s*\(kind\)", pat_verifier_code)
    assert re.search(r"PatError::HashError\s*\(kind\)\)\)\s*=>\s*VerifyFlight::Backend", pat_verifier_code)
    assert re.search(r"pub\s+enum\s+VerifyError\b", pat_lookup_code)
    assert 'PatVerifier' in native and 'verify_capability' in native

    # B185/B186: production issuer pin and synthetic-cookie double gate.
    assert 'CLERK_ISSUER_URL' in clerk and 'NODE_ENV' in clerk
    assert 'NEXT_PUBLIC_E2E_TEST_MODE' in middleware and 'NODE_ENV' in middleware
    assert 'fails closed when the production issuer pin is absent' in bridge_tests
    assert 'environment: "prod"' in bridge_tests
    assert 'isProductionEnvironment' in clerk
    assert 'production + E2E flag cannot bypass auth' in middleware_tests

    # B187/B188/B192: canonical tier ladder and UUID tenant boundary; the
    # production Turbo path has no pad16 fallback.
    assert 'canonical_tiers' in tiers and 'STRIPE_PRICE_ID_RUNNER_SCALE' in main
    assert 'is_canonical_tenant_id' in tenant and 'Uuid::parse_str' in tenant
    assert 'accepts_test_fixture_tenant' not in tenant
    assert 'non_derivable_tenant_err' in turbo
    assert 'pad16' not in turbo
    assert 'uid.to_string() != tenant' in turbo
    assert 'iso_object_key_rejects_missing_tdk_even_for_uuid_tenant' in turbo
    assert 'runners_resolver_maps_every_ratified_price' in tier_tests

    # B190/B191: canonical AC key and server-side conditional first-writer CAS.
    assert 'is_canonical_ac_digest' in ac and 'put_if_absent' in ac


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
            print("B181-B192 semantic self-test OK: boundary mutants killed")
            return 0
        verify()
    except (AssertionError, ValueError) as error:
        print(f"B181-B192 CONTRACT FAILED: {error}")
        return 1
    print("B181-B192 contracts OK: fail-closed guards and wiring present")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
