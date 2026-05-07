//! Neon billing fiscal exception (effective backend 2/8 canonical pós
//! Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: `DELETE FROM invoice / usage_event
//! WHERE subject_user_id = $1 AND legal_hold = false` (LGPD Art. 16
//! fiscal 5y preserves rows under hold; full DELETE only after the
//! 5y window expires).
//!
//! Canonical pseudonymization rule for retained rows: PII columns
//! (email, name, address) substituted by sha256(subject_id ||
//! erasure_salt) + marker `pii_redacted = true`; production wiring at
//! WI-S11-008 invokes the canonical
//! [`crate::pseudonymize::pseudonymize_subject_id`] helper.

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::NeonBilling`]
/// adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(BackendKind::NeonBilling))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_pinned() {
        let a = adapter();
        assert_eq!(
            crate::backends::BackendErasureAdapter::kind(&*a),
            BackendKind::NeonBilling
        );
    }
}
