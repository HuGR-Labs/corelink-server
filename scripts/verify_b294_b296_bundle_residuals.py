#!/usr/bin/env python3
"""Fail-closed contracts for the third D03 Rust residual set."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTRACTS = (
    ("tests/e2e-tenant-isolation/tests/adversarial_tail.rs", "use e2e_tenant_isolation::*;", "use super::*;"),
    ("crates/corelink-billing-stripe-materializer/src/handler/tests.rs", 'include_str!("../../container-webhook-events.json")', 'include_str!("../container-webhook-events.json")'),
)

class VerificationError(RuntimeError): pass

def verify(root: Path = ROOT, *, overrides=None):
    overrides = overrides or {}
    main_path = "crates/corelink-container/src/main.rs"
    main = overrides.get(main_path, (root / main_path).read_text(encoding="utf-8"))
    declaration = main.find("let d1_client: Option<Arc<")
    gate = main.find('if let (Ok(secret), Some(_)) = (std::env::var("STRIPE_WEBHOOK_SECRET")')
    if declaration < 0 or gate < 0 or declaration > gate:
        raise VerificationError(main_path)
    for path, required, forbidden in CONTRACTS:
        source = overrides.get(path, (root / path).read_text(encoding="utf-8"))
        if required not in source or forbidden in source:
            raise VerificationError(path)

if __name__ == "__main__":
    verify(); print("B-294..B-296 D03 bundle residuals: PASS")
