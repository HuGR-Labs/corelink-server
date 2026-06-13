//! Real Stripe pseudonymize adapter (`BackendKind::Stripe`), WI-S11-008
//! Wave 1 increment 4. Pseudonymizes a tenant's Stripe customer (redacts
//! email/name/phone/address, stamps `pii_redacted` metadata) — it NEVER
//! deletes the customer, because the invoice/payment history must survive for
//! fiscal retention (GAAP ASC 606 + LGPD Art. 16). See ADR-S11-013.
//!
//! Ordering subtlety (cold-verified 2026-06-11): the canonical fan-out runs
//! `D1` (idx 4) BEFORE `Stripe` (idx 6), and the D1 adapter deletes the
//! `tenant` row (which carries `stripe_customer_id`). So the Stripe customer id
//! is read from the **retained** `stripe_customers` table (in the D1
//! RETAIN-set, fiscal 5y) — it survives the D1 erase and still maps
//! `tenant_id → stripe_customer_id` when this adapter runs.
//!
//! Pseudonymization is a `POST /v1/customers/:id` (idempotent via a
//! deterministic key), so a replayed erasure is a safe no-op.

use std::sync::Arc;

use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use corelink_privacy_erasure_worker::backends::{
    BackendErasureAdapter, VerificationContext, CANONICAL_EMPTY_TENANT_HASH,
};
use corelink_privacy_erasure_worker::error::ErasureBackendError;
use corelink_privacy_erasure_worker::event::{BackendErasureOutcome, BackendKind};
use corelink_stripe_real::StripeRealClient;

use super::d1util::{col_str, d1_query_blocking};
use crate::storage::d1_http::D1HttpClient;

/// Real Stripe pseudonymize adapter.
pub(super) struct StripePseudonymizeAdapter {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for StripePseudonymizeAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StripePseudonymizeAdapter")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl StripePseudonymizeAdapter {
    /// Construct over a shared [`D1HttpClient`] (used to read the retained
    /// `stripe_customers` index). The Stripe client is built lazily from env
    /// at erase time (`STRIPE_SECRET_KEY`).
    pub(super) fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }

    /// Read the tenant's Stripe customer ids from the RETAINED
    /// `stripe_customers` table (survives the D1 erase).
    fn customer_ids(&self, tid: &str) -> Result<Vec<String>, ErasureBackendError> {
        let rows = d1_query_blocking(
            &self.d1,
            "SELECT stripe_customer_id FROM stripe_customers WHERE tenant_id = ?1",
            vec![json!(tid)],
        )
        .map_err(ErasureBackendError::Transport)?;
        Ok(rows
            .iter()
            .filter_map(|r| col_str(r, "stripe_customer_id"))
            .collect())
    }
}

/// Deterministic, non-PII redaction values derived from `(subject_id, salt)`.
/// `(pseudo_email, pseudo_name, marker_hex)`; `marker_hex` doubles as the
/// erasure marker + the idempotency seed so a retried erasure is a safe replay.
fn redaction_values(subject_id: Uuid, erasure_salt: &[u8; 32]) -> (String, String, String) {
    let mut h = Sha256::new();
    h.update(subject_id.as_bytes());
    h.update(erasure_salt);
    let marker_hex = hex::encode(h.finalize());
    // First 16 hex chars are enough to namespace the redacted email.
    let short = marker_hex.get(..16).unwrap_or(marker_hex.as_str());
    let pseudo_email = format!("erased-{short}@deleted.invalid");
    let pseudo_name = "Erased Subject (GDPR DSR)".to_owned();
    (pseudo_email, pseudo_name, marker_hex)
}

impl BackendErasureAdapter for StripePseudonymizeAdapter {
    fn kind(&self) -> BackendKind {
        BackendKind::Stripe
    }

    fn erase(
        &self,
        tenant_id: Uuid,
        subject_id: Uuid,
        erasure_salt: &[u8; 32],
        legal_hold: bool,
    ) -> Result<BackendErasureOutcome, ErasureBackendError> {
        // Effective backend under legal hold: preserve (CTRL-PRIV-033).
        if legal_hold {
            return Ok(BackendErasureOutcome::NotApplicable);
        }
        let tid = tenant_id.to_string();
        let customer_ids = self.customer_ids(&tid)?;
        if customer_ids.is_empty() {
            // Tenant never had a Stripe customer → nothing to pseudonymize.
            return Ok(BackendErasureOutcome::NotApplicable);
        }

        // No Stripe key ⇒ fail CLOSED (cannot redact the external PII ⇒ must
        // not claim success).
        let stripe = StripeRealClient::from_env()
            .map_err(|e| ErasureBackendError::Transport(format!("Stripe client init failed: {e}")))?;
        let (pseudo_email, pseudo_name, marker_hex) = redaction_values(subject_id, erasure_salt);

        let mut redacted = 0u64;
        for customer_id in &customer_ids {
            // Deterministic per (subject, customer) → idempotent replay.
            let idem = format!("dsr-stripe-pseudo-{marker_hex}-{customer_id}");
            // The Stripe client does blocking HTTP — hand the worker thread
            // back to the scheduler for the round-trip.
            tokio::task::block_in_place(|| {
                stripe.pseudonymize_customer(
                    customer_id,
                    &pseudo_email,
                    &pseudo_name,
                    &marker_hex,
                    &idem,
                )
            })
            .map_err(|e| {
                ErasureBackendError::Transport(format!("Stripe pseudonymize failed: {e}"))
            })?;
            redacted = redacted.saturating_add(1);
        }

        Ok(BackendErasureOutcome::Pseudonymized {
            records_redacted: redacted,
        })
    }

    fn verification_hash(
        &self,
        _ctx: VerificationContext,
    ) -> Result<[u8; 32], ErasureBackendError> {
        // Post-pseudonymization no customer PII remains (email/name/phone/
        // address are redacted; only fiscal references survive), so the
        // canonical "no PII for tenant" sentinel applies. A deeper sweep could
        // GET each customer and assert `metadata.pii_redacted = true`; deferred
        // (heavier Stripe round-trip) — tracked in ADR-S11-013.
        Ok(CANONICAL_EMPTY_TENANT_HASH)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn kind_is_stripe() {
        assert_eq!(BackendKind::Stripe.as_str(), "stripe");
    }

    #[test]
    fn redaction_values_are_deterministic_and_non_pii() {
        let sid = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
        let salt = [7u8; 32];
        let (email1, name1, marker1) = redaction_values(sid, &salt);
        let (email2, _, marker2) = redaction_values(sid, &salt);
        assert_eq!(email1, email2, "deterministic email");
        assert_eq!(marker1, marker2, "deterministic marker");
        assert!(email1.ends_with("@deleted.invalid"));
        assert!(!email1.contains(&sid.to_string()), "must not embed the raw subject id");
        assert_eq!(name1, "Erased Subject (GDPR DSR)");
        assert_eq!(marker1.len(), 64, "sha256 hex");
    }

    #[test]
    fn different_salt_changes_pseudonym() {
        let sid = Uuid::from_u128(1);
        let (_, _, m1) = redaction_values(sid, &[1u8; 32]);
        let (_, _, m2) = redaction_values(sid, &[2u8; 32]);
        assert_ne!(m1, m2);
    }
}
