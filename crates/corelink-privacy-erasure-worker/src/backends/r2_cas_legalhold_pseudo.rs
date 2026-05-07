//! R2 CAS legal_hold partition (pseudonymized backend 3/4 canonical pós
//! Lote 10.11.0-bis).
//!
//! Production wiring at WI-S11-008: governance mode preservation;
//! pseudonymize index references; release post legal_hold expiry. The
//! canonical pseudonymization rule produces a deterministic 64-char
//! hex pseudonym for the index references while the underlying CAS
//! object remains under R2 governance mode lock until the canonical
//! legal_hold expiry pushes it back to the standard CAS partition
//! (where the per-tenant DSR may then erase it via the
//! [`crate::backends::r2_cas`] adapter).

use std::sync::Arc;

use crate::backends::InMemoryBackendErasureAdapter;
use crate::event::BackendKind;

/// Construct a fresh
/// [`crate::event::BackendKind::R2CasLegalHoldPseudo`] adapter.
#[must_use]
pub fn adapter() -> Arc<InMemoryBackendErasureAdapter> {
    Arc::new(InMemoryBackendErasureAdapter::new(
        BackendKind::R2CasLegalHoldPseudo,
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
            BackendKind::R2CasLegalHoldPseudo
        );
    }
}
