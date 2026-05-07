//! R2 CAS refcount-aware (effective backend 3/8 canonical pós
//! Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: integrate with the S-07 chunks
//! table + dedup logic. Canonical refcount-aware soft-delete:
//!
//! - `subject_unaffiliated` (refcount > 1 post-decrement; blob shared
//!   with another tenant via S-07 dedup): decrement refcount only;
//!   blob remains; canonical
//!   [`crate::event::BackendErasureOutcome::NotApplicable`] arm.
//! - `subject_dedicated` (refcount = 1 pre-decrement): tombstone +
//!   GC sweep grace 72h; blob is erased; canonical
//!   [`crate::event::BackendErasureOutcome::Erased`] arm.
//!
//! The cross-tenant break (deleting a blob still referenced by another
//! tenant) is CRITICAL per WI-S11-002 §28 R-003; the canonical
//! refcount-aware semantics in [`crate::backends::InMemoryBackendErasureAdapter`]
//! prevent the cross-tenant break by preserving rows with `refcount > 1`.

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::R2Cas`] adapter
/// with refcount-aware S-07 dedup safety semantics.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(BackendKind::R2Cas))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_pinned() {
        let a = adapter();
        assert_eq!(
            crate::backends::BackendErasureAdapter::kind(&*a),
            BackendKind::R2Cas
        );
    }
}
