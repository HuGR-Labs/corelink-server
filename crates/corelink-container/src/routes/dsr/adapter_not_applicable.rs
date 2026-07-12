//! Reconciliation adapter for canonical backends that are **NOT SHIPPED** in
//! production (WI-S11-008 Wave 1 increment 4, ADR-S11-013).
//!
//! The canonical 12-backend contract assumes a Neon-primary control-plane + a
//! set of WORM stores (audit / evidence / legal-hold / PITR) that were never
//! shipped (cold-verified 2026-06-11: Neon holds only `audit_events_shadow`; no
//! `evidence-*` / `legal-hold` / audit-WORM R2 buckets exist in `wrangler.toml`;
//! KV namespaces are caches; no active Loki sink). For those backends there is
//! **no durable subject PII to erase**, so the contract is honored by returning
//! [`BackendErasureOutcome::NotApplicable`] — the per-backend audit row records
//! `not_applicable`, which is the truthful GDPR record (vs an `InMemory`
//! placeholder that silently returns a no-op "success"). Each instance carries a
//! `reason` documenting *why* it is N/A, surfaced in `Debug`.
//!
//! When one of these stores actually ships, swap its arm in
//! [`super::build_d1_worker`] for a real effective/pseudonymized adapter.

use uuid::Uuid;

use corelink_privacy_erasure_worker::backends::{
    BackendErasureAdapter, VerificationContext, CANONICAL_EMPTY_TENANT_HASH,
};
use corelink_privacy_erasure_worker::error::ErasureBackendError;
use corelink_privacy_erasure_worker::event::{BackendErasureOutcome, BackendKind};

/// Adapter that reports `NotApplicable` for a not-shipped canonical backend.
pub(super) struct NotApplicableAdapter {
    kind: BackendKind,
    /// Why this backend has no durable subject PII in shipped prod.
    reason: &'static str,
}

impl std::fmt::Debug for NotApplicableAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotApplicableAdapter")
            .field("kind", &self.kind.as_str())
            .field("reason", &self.reason)
            .finish()
    }
}

impl NotApplicableAdapter {
    /// Construct for `kind`, documenting the not-shipped `reason`.
    pub(super) fn new(kind: BackendKind, reason: &'static str) -> Self {
        Self { kind, reason }
    }
}

impl BackendErasureAdapter for NotApplicableAdapter {
    fn kind(&self) -> BackendKind {
        self.kind
    }

    fn erase(
        &self,
        _tenant_id: Uuid,
        _subject_id: Uuid,
        _erasure_salt: &[u8; 32],
        _legal_hold: bool,
    ) -> Result<BackendErasureOutcome, ErasureBackendError> {
        // No durable subject PII in shipped prod (see `reason`) → nothing to
        // erase or pseudonymize. NotApplicable regardless of legal_hold.
        Ok(BackendErasureOutcome::NotApplicable)
    }

    fn verification_hash(
        &self,
        _ctx: VerificationContext,
    ) -> Result<[u8; 32], ErasureBackendError> {
        // Nothing stored ⇒ canonical "no rows for tenant" sentinel.
        Ok(CANONICAL_EMPTY_TENANT_HASH)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn reports_kind_and_not_applicable_in_both_hold_states() {
        let a = NotApplicableAdapter::new(BackendKind::Kv, "caches only");
        assert_eq!(a.kind(), BackendKind::Kv);
        let salt = [0u8; 32];
        for legal_hold in [false, true] {
            let out = a
                .erase(Uuid::nil(), Uuid::nil(), &salt, legal_hold)
                .unwrap();
            assert_eq!(out.as_str(), "not_applicable");
        }
    }
}
