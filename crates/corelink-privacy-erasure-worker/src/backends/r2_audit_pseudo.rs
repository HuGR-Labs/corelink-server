//! R2 audit Object Lock 7y (pseudonymized backend 1/4 canonical pós
//! Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: substitutes `subject_id` in the
//! audit row's secondary index (D1) by `erased_<HMAC(salt, subject_id)>`
//! via HKDF info=`corelink/v1/audit-pseudonym` (security_model.md §7.2
//! + key_management.md §2). The original audit payload is preserved
//! in R2 audit Object Lock 7y per CTRL-AUDIT-IMMUTABILITY +
//! INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116). The canonical
//! pseudonymization rule produces a deterministic 64-char hex
//! pseudonym; query subsequent "audit events where subject_id = S"
//! returns empty (per WI-S11-002 AC-002).

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::R2AuditPseudo`]
/// adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(
        BackendKind::R2AuditPseudo,
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
            BackendKind::R2AuditPseudo
        );
    }
}
