#!/usr/bin/env python3
"""Static B126-H2 guard for the adapter PAT extraction.

This is intentionally a source-level guard, not a substitute for the focal
Rust test. It proves that the stable adapter_pat module still wires every
production submodule, that the security pipeline remains in the verifier
module, and that each extracted file stays bounded. The mutation self-test
proves removal of a module declaration or public reexport is rejected.
"""
from __future__ import annotations

from pathlib import Path
import re
import sys


MAX_LINES = 1000
PRODUCTION = (
    "adapter_pat.rs",
    "adapter_pat_lookup.rs",
    "adapter_pat_gate.rs",
    "adapter_pat_crypto.rs",
    "adapter_pat_verifier.rs",
)
TEST_FRAGMENTS = (
    "adapter_pat_tests_1.rs",
    "adapter_pat_tests_2.rs",
    "adapter_pat_tests_3.rs",
)
EXPECTED = {
    "adapter_pat_lookup.rs": (
        r"pub struct PatRow\b",
        r"pub trait PatRowLookup\b",
        r"pub struct SingleFlightPatLookup\b",
        r"impl PatRowLookup for D1HttpClient",
    ),
    "adapter_pat_gate.rs": (
        r"pub\(super\) struct PerTenantGate\b",
        r"pub\(super\) const ARGON2_PER_TENANT_PERMITS",
        r"pub\(super\) const UNKNOWN_TOKEN_BUCKET",
    ),
    "adapter_pat_crypto.rs": (
        r"pub\(super\) struct SecretMatchMemo\b",
        r"pub\(super\) struct FlightGroup",
        r"pub\(super\) fn secret_match_fingerprint",
        r"pub\(super\) enum VerifyFlight",
        r"pub\(super\) enum BurnFlight",
    ),
    "adapter_pat_verifier.rs": (
        r"pub struct PatVerifier\b",
        r"verify_hmac_only_multi",
        r"self\.lookup\.lookup",
        r"verify_with_hash_multi",
        r"requires_cache_read",
        r"requires_cache_write",
        r"Arc::new\(SingleFlightPatLookup::new\(inner\)\)",
    ),
}


class GuardError(RuntimeError):
    pass


def files(root: Path) -> dict[str, str]:
    src = root / "crates" / "corelink-container" / "src"
    names = PRODUCTION + TEST_FRAGMENTS
    return {name: (src / name).read_text(encoding="utf-8") for name in names}


def validate(data: dict[str, str]) -> None:
    missing = sorted(set(PRODUCTION + TEST_FRAGMENTS) - set(data))
    if missing:
        raise GuardError(f"missing adapter PAT source(s): {', '.join(missing)}")

    for name in PRODUCTION + TEST_FRAGMENTS:
        line_count = len(data[name].splitlines())
        if line_count > MAX_LINES:
            raise GuardError(f"{name} has {line_count} lines (limit {MAX_LINES})")

    parent = data["adapter_pat.rs"]
    required_modules = (
        "mod adapter_pat_crypto;",
        "mod adapter_pat_gate;",
        "mod adapter_pat_lookup;",
        "mod adapter_pat_verifier;",
    )
    for marker in required_modules:
        if parent.count(marker) != 1:
            raise GuardError(f"adapter_pat.rs wiring marker count for {marker!r} is not one")

    required_exports = (
        "pub use adapter_pat_lookup::{PatRow, PatRowLookup, SingleFlightPatLookup};",
        "pub use adapter_pat_verifier::{PatVerifier, VerifyError};",
    )
    for marker in required_exports:
        if parent.count(marker) != 1:
            raise GuardError(f"stable public reanchor missing or duplicated: {marker}")

    for marker in (
        'include!("adapter_pat_tests_1.rs");',
        'include!("adapter_pat_tests_2.rs");',
        'include!("adapter_pat_tests_3.rs");',
    ):
        if parent.count(marker) != 1:
            raise GuardError(f"test fragment wiring missing or duplicated: {marker}")

    for name, markers in EXPECTED.items():
        for marker in markers:
            if not re.search(marker, data[name]):
                raise GuardError(f"{name} missing required symbol/pipeline marker {marker!r}")

    verifier = data["adapter_pat_verifier.rs"]
    ordered = (
        verifier.index("let (_env, token_id) = verify_hmac_only_multi"),
        verifier.index("self.lookup.lookup"),
        verifier.index("let r = verify_with_hash_multi"),
        verifier.index("if !requires_cache_read"),
    )
    if ordered != tuple(sorted(ordered)):
        raise GuardError("verifier pipeline markers are no longer HMAC -> D1 -> Argon2id -> scope")

    # The large implementation must not be silently recomposed into the
    # stable parent. Definitions are allowed only in their dedicated files.
    forbidden_parent_defs = (
        r"pub struct PatVerifier\b",
        r"pub struct PatRow\b",
        r"pub trait PatRowLookup\b",
        r"pub struct SingleFlightPatLookup\b",
    )
    for marker in forbidden_parent_defs:
        if re.search(marker, parent):
            raise GuardError(f"implementation recomposed in adapter_pat.rs: {marker}")


def mutation_self_test(data: dict[str, str]) -> None:
    # A guard that only checks the pristine tree is easy to weaken. Both
    # mutations must turn the same validator red without touching the worktree.
    for marker in (
        "mod adapter_pat_verifier;",
        "pub use adapter_pat_verifier::{PatVerifier, VerifyError};",
        'include!("adapter_pat_tests_2.rs");',
    ):
        mutant = dict(data)
        mutant["adapter_pat.rs"] = mutant["adapter_pat.rs"].replace(marker, "", 1)
        try:
            validate(mutant)
        except GuardError:
            continue
        raise GuardError(f"wiring mutation unexpectedly passed: {marker}")


def main() -> int:
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[1]
    try:
        data = files(root)
        validate(data)
        mutation_self_test(data)
    except (OSError, GuardError) as exc:
        print(f"B126-H2 FAIL: {exc}", file=sys.stderr)
        return 1
    print("B126-H2 PASS: bounded extraction, stable reanchors, pipeline wiring, and mutations")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
