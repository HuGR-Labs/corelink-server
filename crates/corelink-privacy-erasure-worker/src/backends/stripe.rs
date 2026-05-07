//! Stripe `Customer.update` PII nullify (effective backend 7/8 canonical
//! pós Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: invoke S-10 StripeClient wrapper
//! `Customer.update(C, { email: 'pseudo@redacted.tld', name:
//! 'erased_<sha256_short>', address: null, metadata: { pii_redacted:
//! 'true', erasure_dsr_id: D } })`. Stripe `Customer.delete` is NEVER
//! called (preserves invoice integrity per GAAP ASC 606 + LGPD Art. 16
//! fiscal compliance); the canonical
//! [`crate::event::BackendErasureOutcome::Erased`] arm in the in-memory
//! fake reflects "PII fields nullified, customer object preserved".
//!
//! Per WI AC-004 + §28 R-004: Stripe customer.delete invoked
//! accidentally is a CRITICAL GAAP violation; the trait surface here
//! NEVER exposes a delete primitive — only the canonical
//! [`crate::backends::BackendErasureAdapter::erase`] surface which
//! pseudonymizes-only on the production binding.

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::Stripe`] adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(BackendKind::Stripe))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_pinned() {
        let a = adapter();
        assert_eq!(
            crate::backends::BackendErasureAdapter::kind(&*a),
            BackendKind::Stripe
        );
    }
}
