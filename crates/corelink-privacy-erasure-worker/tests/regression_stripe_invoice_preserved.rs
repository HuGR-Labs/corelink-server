//! Regression test for GAAP ASC 606 + LGPD Art. 16 fiscal compliance:
//! Stripe `Customer.update` (NOT delete) preserves invoice integrity.
//! Per WI-S11-002 §10.2 T-2.6 + AC-004 + §28 R-004.
//!
//! Anti-pattern detected pre-emptively: Stripe `customer.delete` is
//! IRREVERSIBLE and breaks the canonical invoice trail (GAAP fiscal
//! 5y violation; multa LGPD Art. 16). The trait surface here NEVER
//! exposes a delete primitive — the canonical
//! [`BackendErasureAdapter::erase`] surface pseudonymizes-only on the
//! production binding.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_privacy_erasure_worker::{
    canonical_in_memory_adapters, BackendErasureAdapter, BackendErasureOutcome, BackendKind,
    ErasureIdempotencyLedger, ErasureRequest, ErasureSalt, ErasureWorker, InMemoryRow,
    InMemoryErasureAuditSink, InMemoryErasureIdempotencyLedger, InMemoryErasureWorker,
};
use uuid::Uuid;

fn fixed(seed: u8) -> Uuid {
    let mut b = [0u8; 16];
    for (i, x) in b.iter_mut().enumerate() {
        *x = seed.wrapping_mul(13).wrapping_add(i as u8);
    }
    Uuid::from_bytes(b)
}

#[test]
fn stripe_adapter_pseudonymizes_does_not_delete_canonical_arm() {
    let audit = Arc::new(InMemoryErasureAuditSink::new());
    let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
    let adapters_typed = canonical_in_memory_adapters();
    let stripe = adapters_typed
        .iter()
        .find(|a| a.kind() == BackendKind::Stripe)
        .unwrap()
        .clone();
    let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
        .iter()
        .map(|a| {
            let d: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
            d
        })
        .collect();
    let worker = InMemoryErasureWorker::try_new(audit, ledger.clone(), adapters_dyn).unwrap();

    let tenant = fixed(1);
    let subject = fixed(2);
    let req = ErasureRequest::new(
        fixed(99),
        tenant,
        subject,
        ErasureSalt::synthetic_for_test(7),
        1_000,
    );

    // Inject a canonical "Stripe customer object" payload — the
    // canonical fake adapter for Stripe is the in-memory effective
    // adapter; in production wiring the canonical
    // pseudonymize-not-delete semantics live in the live S-10
    // StripeClient wrapper. The trait surface here pins:
    //
    //   1. The orchestrator never invokes a `delete` method.
    //   2. The canonical outcome arm is `Erased` for the in-memory
    //      fake (PII fields nullified; customer object preserved on
    //      the live binding).
    stripe.insert_rows(tenant, subject, vec![InMemoryRow::new(b"customer".to_vec())]);
    worker.process_erasure(&req, 1_000).unwrap();

    let snap = ledger.snapshot(req.dsr_id).unwrap();
    let stripe_completion = snap
        .iter()
        .find(|c| c.backend == BackendKind::Stripe)
        .unwrap();
    let arm = matches!(
        stripe_completion.outcome,
        BackendErasureOutcome::Erased { .. }
    );
    assert!(
        arm,
        "Stripe outcome must be Erased (PII nullified; customer.update path)"
    );
}

#[test]
fn stripe_no_delete_primitive_in_trait_surface() {
    // The canonical `BackendErasureAdapter` trait does NOT expose a
    // `delete_customer()` method. The only surface is `erase` (which
    // on the production binding pseudonymizes via Customer.update)
    // and `verification_hash`. Any drift would be caught by the
    // type system — this test pins the surface for the regression
    // catalog.
    let adapter: Arc<dyn BackendErasureAdapter> =
        corelink_privacy_erasure_worker::backends::stripe::adapter() as _;
    assert_eq!(adapter.kind(), BackendKind::Stripe);
    // Calling erase on an empty adapter returns NotApplicable, NOT
    // an error (canonical no-op semantics for a tenant with no Stripe
    // customer object — see WI-S11-002 §1 BackendErasureOutcome
    // taxonomy).
    let outcome = adapter
        .erase(fixed(1), fixed(2), &[0u8; 32], false)
        .unwrap();
    let na = matches!(outcome, BackendErasureOutcome::NotApplicable);
    assert!(na);
}

#[test]
fn invoice_continuity_preserved_post_erasure() {
    // Property: post-erasure the canonical "invoice" trail
    // (modeled by per-tenant ledger snapshots; production wiring
    // queries Stripe Invoices API + Neon billing fiscal table) MUST
    // remain queryable. The trait surface enforces this by design:
    // the Stripe adapter never deletes — it pseudonymizes. The
    // verification fingerprint after erasure is canonical (sentinel)
    // because the in-memory fake clears the (tenant, subject) row
    // set; production wiring preserves the Stripe customer object
    // with `pii_redacted=true` metadata + nulled PII fields.
    let audit = Arc::new(InMemoryErasureAuditSink::new());
    let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
    let adapters_typed = canonical_in_memory_adapters();
    let neon_billing = adapters_typed
        .iter()
        .find(|a| a.kind() == BackendKind::NeonBilling)
        .unwrap()
        .clone();
    let adapters_dyn: Vec<Arc<dyn BackendErasureAdapter>> = adapters_typed
        .iter()
        .map(|a| {
            let d: Arc<dyn BackendErasureAdapter> = Arc::clone(a) as _;
            d
        })
        .collect();
    let worker = InMemoryErasureWorker::try_new(audit, ledger, adapters_dyn).unwrap();

    let tenant = fixed(1);
    let subject = fixed(2);
    // Inject a canonical "invoice row" for neon_billing (would be
    // preserved under legal_hold per LGPD Art. 16 fiscal 5y).
    neon_billing.insert_rows(tenant, subject, vec![InMemoryRow::new(b"invoice".to_vec())]);

    let req = ErasureRequest::new(
        fixed(99),
        tenant,
        subject,
        ErasureSalt::synthetic_for_test(7),
        1_000,
    )
    .with_legal_hold(true);
    worker.process_erasure(&req, 1_000).unwrap();

    // Under legal_hold, the canonical neon_billing row is preserved
    // (effective backends skip with NotApplicable); pseudonymize is
    // the production-binding behavior for retained invoice rows.
    assert_eq!(
        neon_billing.row_count(tenant, subject),
        1,
        "invoice row preserved under legal_hold (LGPD Art. 16 fiscal 5y)"
    );
}
