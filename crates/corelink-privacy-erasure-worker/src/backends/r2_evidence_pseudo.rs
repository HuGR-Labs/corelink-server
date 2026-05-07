//! R2 evidence-* buckets 7y (pseudonymized backend 4/4 canonical pós
//! Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: retain per SLA framework
//! (EVT-046 LIA + EVT-049 consent record + EVT-048 DSR evidence);
//! `subject_id` pseudonymized in the secondary index. The original
//! evidence payload is preserved (Object Lock 7y) for forensic
//! re-correlation under court order via the canonical
//! [`crate::pseudonymize::verify_pseudonym`] surface (customer holds
//! the salt under BYOK; ADR-S11-003 interim D1 vault path until
//! S-14 KMS).

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::R2EvidencePseudo`]
/// adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(
        BackendKind::R2EvidencePseudo,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_pinned() {
        let a = adapter();
        assert_eq!(
            crate::backends::BackendErasureAdapter::kind(&*a),
            BackendKind::R2EvidencePseudo
        );
    }
}
