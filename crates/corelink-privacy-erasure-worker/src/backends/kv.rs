//! KV sessions + cached metadata (effective backend 6/8 canonical pós
//! Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: `DELETE` keys matching `tenant +
//! subject` prefix; eventual consistency tolerable (KV is the canonical
//! degrade-target for the privacy pipeline; verification job 24h
//! sweep accounts for the settle delay).

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::Kv`] adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(BackendKind::Kv))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_pinned() {
        let a = adapter();
        assert_eq!(
            crate::backends::BackendErasureAdapter::kind(&*a),
            BackendKind::Kv
        );
    }
}
