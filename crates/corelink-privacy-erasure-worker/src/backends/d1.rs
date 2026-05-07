//! D1 `blob_meta` + `ac_meta` row purge (effective backend 5/8 canonical
//! pós Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: subject-scoped row delete from D1
//! `blob_meta` + `ac_meta` tables; refcount sync with R2 CAS via the
//! [`crate::backends::r2_cas`] adapter.

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::D1`] adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(BackendKind::D1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_pinned() {
        let a = adapter();
        assert_eq!(
            crate::backends::BackendErasureAdapter::kind(&*a),
            BackendKind::D1
        );
    }
}
