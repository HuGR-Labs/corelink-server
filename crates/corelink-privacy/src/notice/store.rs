//! Notice state store trait + in-memory implementation.
//!
//! The canonical durable state is the **published notice version** stored in
//! Cloudflare Pages metadata + R2 audit Object Lock 7y. This crate ships the
//! pure-logic trait + in-memory fake; production wiring is deferred to the
//! Cloudflare Pages + R2 binding at WI-S11-008 per `trait-abstraction-defer`.
//!
//! ## State shape
//!
//! The store holds at most 1 "current published" version per tenant (singleton
//! notice; multi-tenant publishing deferred post-GA per sprint contract).
//! The store is per-instance `Arc<Mutex<>>` (F-001 closure — NEVER `static
//! LazyLock`).

use super::error::NoticeStoreError;
use super::event::NoticeVersion;
use std::sync::{Arc, Mutex};

/// Notice publication state record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoticePublicationState {
    /// The currently published version.
    pub current_version: NoticeVersion,
    /// Wall-clock instant when this version was published (Unix epoch ms).
    pub published_at_ms: u64,
}

impl NoticePublicationState {
    /// Construct a publication state record.
    #[must_use]
    pub fn new(current_version: NoticeVersion, published_at_ms: u64) -> Self {
        Self {
            current_version,
            published_at_ms,
        }
    }
}

/// Trait for reading + writing the canonical notice publication state.
/// Production wiring at WI-S11-008 binds this to Cloudflare Pages metadata
/// + R2 audit Object Lock 7y.
pub trait NoticeStateStore: core::fmt::Debug + Send + Sync {
    /// Look up the current published version. Returns `None` when no version
    /// has been published yet (bootstrap path).
    ///
    /// # Errors
    ///
    /// Returns [`NoticeStoreError`] on infrastructure failure.
    fn current_published(
        &self,
    ) -> Result<Option<NoticePublicationState>, NoticeStoreError>;

    /// Persist the newly published version. Called AFTER audit emit succeeds
    /// (canonical `lookup → emit_audit → mutate_state` ordering per AC-008).
    ///
    /// # Errors
    ///
    /// Returns [`NoticeStoreError`] on infrastructure failure.
    fn set_published(
        &self,
        state: NoticePublicationState,
    ) -> Result<(), NoticeStoreError>;
}

/// In-memory store for tests. Per-instance `Arc<Mutex<>>` (F-001).
#[derive(Clone, Debug, Default)]
pub struct InMemoryNoticeStateStore {
    state: Arc<Mutex<Option<NoticePublicationState>>>,
}

impl InMemoryNoticeStateStore {
    /// Construct a fresh empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(None)),
        }
    }

    /// Construct a store pre-seeded with a known state (for test scenarios).
    #[must_use]
    pub fn with_state(initial: NoticePublicationState) -> Self {
        Self {
            state: Arc::new(Mutex::new(Some(initial))),
        }
    }
}

impl NoticeStateStore for InMemoryNoticeStateStore {
    fn current_published(
        &self,
    ) -> Result<Option<NoticePublicationState>, NoticeStoreError> {
        self.state
            .lock()
            .map(|g| g.clone())
            .map_err(|_| NoticeStoreError::Internal {
                reason: "mutex poisoned".into(),
            })
    }

    fn set_published(
        &self,
        state: NoticePublicationState,
    ) -> Result<(), NoticeStoreError> {
        *self.state.lock().map_err(|_| NoticeStoreError::Internal {
            reason: "mutex poisoned".into(),
        })? = Some(state);
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use super::super::event::NoticeVersion;

    #[test]
    fn empty_store_returns_none() {
        let store = InMemoryNoticeStateStore::new();
        assert!(store.current_published().unwrap().is_none());
    }

    #[test]
    fn set_and_retrieve_published_state() {
        let store = InMemoryNoticeStateStore::new();
        let state = NoticePublicationState::new(NoticeVersion::new(1, 0), 1000);
        store.set_published(state.clone()).unwrap();
        let retrieved = store.current_published().unwrap();
        assert_eq!(retrieved, Some(state));
    }

    #[test]
    fn with_state_seeds_correctly() {
        let state = NoticePublicationState::new(NoticeVersion::new(2, 3), 999);
        let store = InMemoryNoticeStateStore::with_state(state.clone());
        assert_eq!(store.current_published().unwrap(), Some(state));
    }
}
