//! Propagation consumer model for the DO config-singleton (WI-S13-001).
//!
//! In production, a Cloudflare Queue `cfg-change-region-{region}` receives
//! a change event `{version, payload_hash}` whenever the DO config-singleton
//! is updated. Per-Worker consumers refresh their in-memory snapshot on each
//! queue message and fall back to a 60-second periodic poll to the DO.
//!
//! This module ships the **in-memory snapshot model** used by tests and the
//! conceptual consumer interface. Production CF Worker wiring is deferred
//! per `trait-abstraction-defer` charter pattern.

use std::sync::{Arc, Mutex};

use tracing::info;

use crate::{ConfigError, ConfigPayload};

/// A change event emitted by the DO on each successful update.
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    /// New config version.
    pub version: u64,
    /// SHA-256 payload hash (hex 64 chars) for integrity verification.
    pub payload_hash_hex: String,
}

/// In-memory snapshot of the current config, maintained by each Worker.
///
/// Refreshed on Queue events (fast path ≤ 5s p99) and by 60s safety-net poll.
/// Version-monotone: a Worker only advances its snapshot when `new_version >
/// local_version` (idempotent reconciliation — no flapping on out-of-order
/// Queue deliveries).
#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    inner: Arc<Mutex<SnapshotState>>,
}

#[derive(Debug)]
struct SnapshotState {
    version: u64,
    payload: ConfigPayload,
}

impl ConfigSnapshot {
    /// Create a new snapshot initialized at genesis (version=0).
    #[must_use]
    pub fn new_genesis() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SnapshotState {
                version: 0,
                payload: ConfigPayload::genesis(),
            })),
        }
    }

    /// Attempt to advance the snapshot to `new_version` with `new_payload`.
    ///
    /// Only advances if `new_version > current_version` (idempotent).
    /// Returns `true` if the snapshot was updated, `false` if ignored
    /// (e.g. stale or duplicate delivery).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Backend`] if the mutex is poisoned.
    pub fn try_advance(
        &self,
        new_version: u64,
        new_payload: ConfigPayload,
    ) -> Result<bool, ConfigError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ConfigError::Backend("snapshot mutex poisoned".into()))?;

        if new_version > state.version {
            state.version = new_version;
            state.payload = new_payload;
            info!(version = new_version, "config snapshot advanced");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Return the current `(version, payload)` snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Backend`] if the mutex is poisoned.
    pub fn current(&self) -> Result<(u64, ConfigPayload), ConfigError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ConfigError::Backend("snapshot mutex poisoned".into()))?;
        Ok((state.version, state.payload.clone()))
    }

    /// Return the current version without cloning the payload.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Backend`] if the mutex is poisoned.
    pub fn version(&self) -> Result<u64, ConfigError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ConfigError::Backend("snapshot mutex poisoned".into()))?;
        Ok(state.version)
    }
}
