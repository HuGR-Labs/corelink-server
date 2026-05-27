//! Re-acceptance handler primitive.
//!
//! Mirrors the WI-S19-002 click-through accept handler shape but bumps
//! the tenant's `current_dpa_version` and clears the pending flag /
//! grace deadline atomically.

use std::sync::Arc;

use uuid::Uuid;

use super::error::DpaVersioningError;
use super::schema::{ReAcceptanceReceipt, UnixSeconds};
use super::store::{DpaStore as _, InMemoryDpaStore};
use super::version::SemverVersion;

/// Re-acceptance handler over the in-memory store.
#[derive(Debug)]
pub struct ReAcceptHandler {
    store: Arc<InMemoryDpaStore>,
}

impl ReAcceptHandler {
    /// Construct a handler bound to a store.
    #[must_use]
    pub fn new(store: Arc<InMemoryDpaStore>) -> Self {
        Self { store }
    }

    /// Apply a re-acceptance for the given tenant against the
    /// `presented_version`.
    ///
    /// # Errors
    ///
    /// - [`DpaVersioningError::NoPendingReacceptance`] when the
    ///   tenant has no pending bump (idempotency: repeat call after
    ///   success is a no-op error so callers can detect double-submit).
    /// - [`DpaVersioningError::VersionMismatch`] when the version the
    ///   tenant submits does not match the latest published (replay
    ///   defence + version-skip "latest-wins" rule).
    /// - [`DpaVersioningError::Store`] on persistence failure.
    pub fn re_accept(
        &self,
        tenant_id: Uuid,
        presented_version: SemverVersion,
        now: UnixSeconds,
    ) -> Result<ReAcceptanceReceipt, DpaVersioningError> {
        // 1. Read latest published.
        let latest = self
            .store
            .latest_version()?
            .ok_or(DpaVersioningError::NoPendingReacceptance)?;
        if presented_version != latest.version {
            return Err(DpaVersioningError::VersionMismatch {
                presented: presented_version.render(),
                latest: latest.version.render(),
            });
        }

        // 2. Read tenant. Must have a pending bump.
        let mut state = self
            .store
            .read_tenant(tenant_id)?
            .ok_or(DpaVersioningError::NoPendingReacceptance)?;
        if !state.re_acceptance_pending {
            return Err(DpaVersioningError::NoPendingReacceptance);
        }

        // 3. Atomic update: bump current_dpa_version, clear pending +
        //    grace.
        let from_version = state.current_dpa_version;
        state.current_dpa_version = latest.version;
        state.re_acceptance_pending = false;
        state.grace_expires_at = None;
        self.store.write_tenant(state)?;

        // 4. Emit receipt mirroring WI-S19-002 shape.
        Ok(ReAcceptanceReceipt {
            tenant_id,
            from_version,
            to_version: latest.version,
            accepted_at: now,
            content_hash: latest.content_hash,
        })
    }
}
