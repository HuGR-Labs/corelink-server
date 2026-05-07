//! Loki / Grafana `/loki/api/v1/delete` HTTP API (effective backend
//! 8/8 canonical pós Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: HTTP DELETE
//! `/loki/api/v1/delete?query={subject_id="..."}&start=...&end=...`
//! per privacy_model.md §6.2 step 4f. Loki cold archive settle delay
//! is up to 24h (S-09 R-S09-5); the verification job 24h sweep is the
//! canonical surface that asserts post-settle absence (per WI-S11-002
//! §9.1 DD-005 24h verification window rationale).

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::Loki`] adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(BackendKind::Loki))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_pinned() {
        let a = adapter();
        assert_eq!(
            crate::backends::BackendErasureAdapter::kind(&*a),
            BackendKind::Loki
        );
    }
}
