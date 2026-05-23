//! Host-side test fake for [`RevocationBroadcast`]. Records every
//! enqueued payload + supports a "queue down" mode for chaos tests.
//!
//! Split from monolith `auth/revocation.rs` (wave-33 stage 2.PRE-A.1).

use core::fmt;

use async_trait::async_trait;
use tokio::sync::Mutex;

use corelink_pat::PatId;

use super::traits::RevocationBroadcast;
use super::types::{PropagationOutcome, RevokedEntry};

// ---------------------------------------------------------------------------
// In-memory broadcast fake
// ---------------------------------------------------------------------------

/// Host-side test fake for [`RevocationBroadcast`]. Records every
/// enqueued payload + supports a "queue down" mode for chaos tests.
pub struct InMemoryBroadcast {
    inner: Mutex<InMemoryBroadcastState>,
}

#[derive(Default)]
struct InMemoryBroadcastState {
    enqueued: Vec<RevokedEntry>,
    queue_down: bool,
}

impl fmt::Debug for InMemoryBroadcast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryBroadcast").finish_non_exhaustive()
    }
}

impl Default for InMemoryBroadcast {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryBroadcast {
    /// Construct a fresh, empty broadcast fake (queue up).
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(InMemoryBroadcastState::default()),
        }
    }

    /// Toggle the "queue down" failure mode. While set, every
    /// enqueue returns [`PropagationOutcome::DlqFallback`].
    pub async fn set_queue_down(&self, down: bool) {
        self.inner.lock().await.queue_down = down;
    }

    /// Snapshot the recorded enqueues.
    pub async fn enqueued(&self) -> Vec<RevokedEntry> {
        self.inner.lock().await.enqueued.clone()
    }

    /// Count the recorded enqueues for a given pat_id.
    pub async fn enqueued_count(&self, pat_id: PatId) -> usize {
        self.inner
            .lock()
            .await
            .enqueued
            .iter()
            .filter(|e| e.pat_id == pat_id)
            .count()
    }
}

#[async_trait]
impl RevocationBroadcast for InMemoryBroadcast {
    async fn enqueue_single(&self, entry: &RevokedEntry) -> PropagationOutcome {
        let mut guard = self.inner.lock().await;
        if guard.queue_down {
            return PropagationOutcome::DlqFallback;
        }
        guard.enqueued.push(entry.clone());
        PropagationOutcome::Enqueued
    }

    async fn enqueue_mass(&self, entries: &[RevokedEntry]) -> PropagationOutcome {
        let mut guard = self.inner.lock().await;
        if guard.queue_down {
            return PropagationOutcome::DlqFallback;
        }
        // Faithfully chunked enqueue (informational; the test fake
        // doesn't model batch semantics — it appends every entry).
        for entry in entries {
            guard.enqueued.push(entry.clone());
        }
        PropagationOutcome::Enqueued
    }
}
