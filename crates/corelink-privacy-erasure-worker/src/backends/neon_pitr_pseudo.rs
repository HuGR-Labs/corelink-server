//! Neon PITR backup 30d (pseudonymized backend 2/4 canonical pós
//! Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: natural rotation; tombstone replay
//! on any restore; auto-expires after the 30d retention window
//! (no manual delete; the canonical pseudonym is recorded so a future
//! restore is also caught). The tombstone replay invokes the canonical
//! [`crate::backends::BackendErasureAdapter::erase`] surface again on
//! the restored row set.

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh [`crate::event::BackendKind::NeonPitrPseudo`]
/// adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(
        BackendKind::NeonPitrPseudo,
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
            BackendKind::NeonPitrPseudo
        );
    }
}
