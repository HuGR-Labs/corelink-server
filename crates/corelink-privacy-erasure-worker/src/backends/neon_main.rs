//! Neon multi-tabela DELETE cascade (effective backend 1/8 canonical
//! pós Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008 binds this to a Neon HTTP client
//! (wasm-bindgen via `worker::Fetch`); the canonical multi-tabela
//! cascade is `DELETE FROM dsr_tickets / account / tenant /
//! user_account / consent_ledger / subscription WHERE
//! subject_user_id = $1` with PG-side CHECK + FK enforcement +
//! tombstone insert in `dsr_erasure_log`.
//!
//! The trait surface ships the canonical
//! [`crate::backends::InMemoryBackendErasureAdapter`] specialised
//! for [`crate::event::BackendKind::NeonMain`]; the in-memory adapter
//! exercises every load-bearing invariant the production binding
//! relies on (`(tenant_id, subject_id)` row scope; effective DELETE
//! cardinality; `legal_hold = true` skip with `NotApplicable`).

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::NeonMain`] adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(BackendKind::NeonMain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_pinned() {
        let a = adapter();
        assert_eq!(
            crate::backends::BackendErasureAdapter::kind(&*a),
            BackendKind::NeonMain
        );
    }
}
