//! R2 AC mutable (effective backend 4/8 canonical pós Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: `DELETE` Action Cache entries
//! matching `owner_tenant_id`; per-region pinned (no cross-region
//! deletion needed; AC is region-local per data_model.md).

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::R2Ac`] adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(BackendKind::R2Ac))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_pinned() {
        let a = adapter();
        assert_eq!(
            crate::backends::BackendErasureAdapter::kind(&*a),
            BackendKind::R2Ac
        );
    }
}
