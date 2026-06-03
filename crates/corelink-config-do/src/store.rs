//! [`ConfigSingletonStore`] trait + [`InMemoryConfigSingletonStore`]
//! (WI-S13-001).
//!
//! # Audit fail-CLOSED ordering
//!
//! Every mutating operation follows the canonical ordering:
//! 1. **Lookup** — read current state under the mutex.
//! 2. **Validate** — CAS version + payload schema.
//! 3. **Emit audit** — write to the audit sink BEFORE mutating store.
//! 4. **Mutate** — update in-memory state.
//!
//! If audit emit fails the mutation is aborted and the error is returned
//! (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER herdada from S-03/S-09/S-10).
//!
//! # Per-instance `Arc<Mutex<>>` F-001
//!
//! [`InMemoryConfigSingletonStore`] is `Clone` via inner [`Arc`]; each
//! clone shares the same state. The mutex guards all mutations. There is
//! no global singleton — each [`InMemoryConfigSingletonStore::new`] call
//! creates an independent instance (per F-001 closure).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tracing::{info, warn};

use crate::{
    hash::compute_payload_hash,
    metrics::{MetricsObserver, RollbackOutcome, UpdateOutcome},
    types::{AdminActor, ChangeType, ConfigVersionEntry},
    validation::validate_payload,
    ConfigError, ConfigPayload,
};

/// 90-day retention window in milliseconds.
const RETENTION_90D_MS: u64 = 90 * 24 * 60 * 60 * 1000;

/// Audit sink trait for config-singleton write events.
///
/// Production wiring inserts into D1 `audit_outbox` and
/// `config_change_log` in the same D1 batch (atomic); the in-memory
/// implementation captures events for test assertions.
pub trait ConfigAuditSink: Send + Sync {
    /// Emit a config change audit record. Called BEFORE state mutation.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the audit sink fails; the caller MUST abort the
    /// mutation (fail-CLOSED).
    fn emit(&self, entry: &ConfigVersionEntry) -> Result<(), ConfigError>;
}

/// No-op audit sink (use only in unit tests where audit is not under test).
#[derive(Debug, Default)]
pub struct NoopAuditSink;

impl ConfigAuditSink for NoopAuditSink {
    fn emit(&self, _entry: &ConfigVersionEntry) -> Result<(), ConfigError> {
        Ok(())
    }
}

/// In-memory audit sink that captures all emitted entries.
#[derive(Debug, Default, Clone)]
pub struct InMemoryAuditSink {
    entries: Arc<Mutex<Vec<ConfigVersionEntry>>>,
}

impl InMemoryAuditSink {
    /// Create a new empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot all captured entries.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ConfigVersionEntry> {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl ConfigAuditSink for InMemoryAuditSink {
    fn emit(&self, entry: &ConfigVersionEntry) -> Result<(), ConfigError> {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(entry.clone());
        Ok(())
    }
}

/// Failing audit sink for chaos / fail-CLOSED tests.
#[derive(Debug, Default)]
pub struct FailingAuditSink;

impl ConfigAuditSink for FailingAuditSink {
    fn emit(&self, _entry: &ConfigVersionEntry) -> Result<(), ConfigError> {
        Err(ConfigError::Backend("audit sink injected failure".into()))
    }
}

/// Core trait for the DO config-singleton.
///
/// All methods are async to match the Cloudflare Durable Object I/O model.
/// The in-memory implementation (`InMemoryConfigSingletonStore`) provides
/// a synchronous-equivalent semantic for unit and property tests.
#[async_trait]
pub trait ConfigSingletonStore: Send + Sync {
    /// CAS update: succeeds only if `current_version == expected_version`.
    ///
    /// On success returns the new version number.
    ///
    /// # Errors
    ///
    /// - [`ConfigError::VersionConflict`] if CAS fails.
    /// - [`ConfigError::SchemaInvalid`] if payload fails validation.
    /// - [`ConfigError::Backend`] if storage or audit fails.
    async fn update(
        &self,
        expected_version: u64,
        new_payload: ConfigPayload,
        actor: &AdminActor,
        now_ms: u64,
    ) -> Result<u64, ConfigError>;

    /// Return the current `(version, payload)`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Backend`] on storage error.
    async fn current(&self) -> Result<(u64, ConfigPayload), ConfigError>;

    /// Roll back to a historical version (≤ 90d).
    ///
    /// Creates a new version entry with the payload of `to_version` and
    /// `ChangeType::Rollback`. Returns the new version number.
    ///
    /// # Errors
    ///
    /// - [`ConfigError::VersionUnknown`] if `to_version` is not in history.
    /// - [`ConfigError::VersionExpired`] if `to_version` is outside 90d window.
    /// - [`ConfigError::SchemaInvalid`] if the historical payload fails
    ///   invariant re-check.
    /// - [`ConfigError::Backend`] on storage or audit failure.
    async fn rollback_to(
        &self,
        to_version: u64,
        actor: &AdminActor,
        now_ms: u64,
    ) -> Result<u64, ConfigError>;

    /// Return the last `limit` history entries (newest first).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Backend`] on storage error.
    async fn history(&self, limit: u32) -> Result<Vec<ConfigVersionEntry>, ConfigError>;
}

/// In-memory config singleton store.
///
/// Single source of truth for unit, property, and integration tests.
/// Per-instance [`Arc`]`<`[`Mutex`]`>` F-001 closure; each `new()` call
/// is an independent isolated instance.
#[derive(Clone)]
pub struct InMemoryConfigSingletonStore {
    inner: Arc<Mutex<InMemoryState>>,
    audit: Arc<dyn ConfigAuditSink>,
    metrics: Arc<dyn MetricsObserver>,
}

impl core::fmt::Debug for InMemoryConfigSingletonStore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryConfigSingletonStore")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct InMemoryState {
    current_version: u64,
    current_payload: ConfigPayload,
    /// History ordered by version (ascending).
    history: BTreeMap<u64, ConfigVersionEntry>,
    /// Full payloads for rollback (keyed by version).
    payloads: BTreeMap<u64, ConfigPayload>,
}

impl InMemoryConfigSingletonStore {
    /// Create a new store initialized at version 0 with a genesis payload.
    #[must_use]
    pub fn new(audit: Arc<dyn ConfigAuditSink>, metrics: Arc<dyn MetricsObserver>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(InMemoryState {
                current_version: 0,
                current_payload: ConfigPayload::genesis(),
                history: BTreeMap::new(),
                payloads: BTreeMap::new(),
            })),
            audit,
            metrics,
        }
    }

    /// Convenience constructor with no-op audit and no-op metrics.
    #[must_use]
    pub fn new_noop() -> Self {
        Self::new(
            Arc::new(NoopAuditSink),
            Arc::new(crate::metrics::NoopMetrics),
        )
    }
}

#[async_trait]
impl ConfigSingletonStore for InMemoryConfigSingletonStore {
    async fn update(
        &self,
        expected_version: u64,
        new_payload: ConfigPayload,
        actor: &AdminActor,
        now_ms: u64,
    ) -> Result<u64, ConfigError> {
        // 1. Validate schema before acquiring lock (cheap path first).
        validate_payload(&new_payload)
            .inspect_err(|_| self.metrics.record_update(UpdateOutcome::SchemaInvalid))?;

        // 2. Compute payload hash (before lock, pure computation).
        let payload_hash = compute_payload_hash(&new_payload)?;

        // 3. Lock → CAS check → audit → mutate.
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ConfigError::Backend("mutex poisoned".into()))?;

        if state.current_version != expected_version {
            warn!(
                current = state.current_version,
                expected = expected_version,
                "config CAS conflict"
            );
            self.metrics.record_update(UpdateOutcome::VersionConflict);
            self.metrics.record_cas_conflict("any");
            return Err(ConfigError::VersionConflict {
                expected: expected_version,
                current: state.current_version,
            });
        }

        let new_version = state.current_version + 1;
        let entry = ConfigVersionEntry {
            version: new_version,
            payload_hash,
            actor: actor.clone(),
            mfa_ts_ms: now_ms, // caller passes MFA ts; handler validates freshness.
            created_at_ms: now_ms,
            change_type: ChangeType::Update,
            previous_version: if state.current_version == 0 {
                None
            } else {
                Some(state.current_version)
            },
        };

        // 4. Emit audit BEFORE mutation (fail-CLOSED: if emit fails, abort).
        self.audit.emit(&entry)?;

        // 5. Mutate.
        state.payloads.insert(new_version, new_payload.clone());
        state.history.insert(new_version, entry);
        state.current_version = new_version;
        state.current_payload = new_payload;

        self.metrics.set_history_size(state.history.len() as u64);
        self.metrics.record_update(UpdateOutcome::Ok);

        info!(version = new_version, "config updated");
        Ok(new_version)
    }

    async fn current(&self) -> Result<(u64, ConfigPayload), ConfigError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ConfigError::Backend("mutex poisoned".into()))?;
        Ok((state.current_version, state.current_payload.clone()))
    }

    async fn rollback_to(
        &self,
        to_version: u64,
        actor: &AdminActor,
        now_ms: u64,
    ) -> Result<u64, ConfigError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| ConfigError::Backend("mutex poisoned".into()))?;

        // 1. Look up target entry in history.
        let target_entry = state.history.get(&to_version).cloned();
        let target_entry = match target_entry {
            Some(e) => e,
            None => {
                self.metrics
                    .record_rollback(RollbackOutcome::VersionUnknown);
                return Err(ConfigError::VersionUnknown(to_version));
            }
        };

        // 2. Check 90d retention window.
        let age_ms = now_ms.saturating_sub(target_entry.created_at_ms);
        if age_ms > RETENTION_90D_MS {
            self.metrics
                .record_rollback(RollbackOutcome::VersionExpired);
            return Err(ConfigError::VersionExpired(to_version));
        }

        // 3. Retrieve historical payload.
        let historical_payload = match state.payloads.get(&to_version).cloned() {
            Some(p) => p,
            None => {
                self.metrics.record_rollback(RollbackOutcome::StateCorrupt);
                return Err(ConfigError::Backend(format!(
                    "payload for version {to_version} missing from store (state corrupt)"
                )));
            }
        };

        // 4. Re-validate historical payload against current invariants.
        validate_payload(&historical_payload)
            .inspect_err(|_| self.metrics.record_rollback(RollbackOutcome::StateCorrupt))?;

        // 5. Compute hash of rolled-back payload.
        let payload_hash = compute_payload_hash(&historical_payload)?;

        let new_version = state.current_version + 1;
        let entry = ConfigVersionEntry {
            version: new_version,
            payload_hash,
            actor: actor.clone(),
            mfa_ts_ms: now_ms,
            created_at_ms: now_ms,
            change_type: ChangeType::Rollback,
            previous_version: Some(state.current_version),
        };

        // 6. Emit audit BEFORE mutation (fail-CLOSED).
        self.audit.emit(&entry)?;

        // 7. Mutate.
        state
            .payloads
            .insert(new_version, historical_payload.clone());
        state.history.insert(new_version, entry);
        state.current_version = new_version;
        state.current_payload = historical_payload;

        self.metrics.set_history_size(state.history.len() as u64);
        self.metrics.record_rollback(RollbackOutcome::Ok);

        info!(new_version, rollback_to = to_version, "config rolled back");
        Ok(new_version)
    }

    async fn history(&self, limit: u32) -> Result<Vec<ConfigVersionEntry>, ConfigError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| ConfigError::Backend("mutex poisoned".into()))?;

        // Return newest first.
        let entries: Vec<ConfigVersionEntry> = state
            .history
            .values()
            .rev()
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(entries)
    }
}
