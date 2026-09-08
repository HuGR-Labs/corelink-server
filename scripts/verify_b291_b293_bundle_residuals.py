#!/usr/bin/env python3
"""Fail-closed contracts for the second D03 Rust residual set."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTRACTS = (
    ("crates/corelink-container/src/storage.rs", "/// BYOK convergent-encryption helpers for native CAS storage.\npub mod byok_cas;", "pub(crate) mod byok_cas;"),
    ("crates/corelink-container/src/adapter_pat_gate.rs", "pub(super) struct PerTenantEntry", "struct PerTenantEntry"),
    ("tests/e2e-tenant-isolation/tests/adversarial_tail.rs", "use e2e_tenant_isolation::*;", "use super::*;"),
)

class VerificationError(RuntimeError):
    pass

def verify(root: Path = ROOT, *, overrides: dict[str, str] | None = None) -> None:
    overrides = overrides or {}
    for path, required, forbidden in CONTRACTS:
        source = overrides.get(path, (root / path).read_text(encoding="utf-8"))
        if required not in source:
            raise VerificationError(f"{path}: missing {required!r}")
        if forbidden in source.replace(required, ""):
            raise VerificationError(f"{path}: forbidden {forbidden!r}")

if __name__ == "__main__":
    verify()
    print("B-291..B-293 D03 bundle residuals: PASS")
