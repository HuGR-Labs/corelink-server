//! `corelink-config-do` — DO config-singleton per-region: schema-versioned
//! [`ConfigPayload`] (feature flags + rate-limit tunables + retention
//! policies), CAS atomic update, propagation pub-sub model, D1
//! `config_change_log` 90d retention, and rollback API (WI-S13-001,
//! HIGH_RISK lane FF-HR-005).
//!
//! # What this crate ships
//!
//! - [`ConfigPayload`] versioned schema (`schema_version: u32`) with
//!   `#[serde(deny_unknown_fields)]` — schema drift → hard error, never
//!   silent default (risk R-003 mitigated).
//! - [`FeatureFlag`], [`RateLimitKey`], [`RateLimitTunable`],
//!   [`RetentionPolicy`] — typed config primitives.
//! - [`ConfigSingletonStore`] trait — CAS `update`, `current`, `rollback_to`,
//!   `history`.
//! - [`InMemoryConfigSingletonStore`] — in-memory orchestrator for tests
//!   (per-instance `Arc<Mutex<>>` F-001 closure).
//! - [`ConfigAuditSink`] trait + [`InMemoryAuditSink`] + [`FailingAuditSink`]
//!   (for chaos tests).
//! - [`validation::validate_payload`] — domain invariant checks pre-write.
//! - [`hash::compute_payload_hash`] / [`hash::hash_to_hex`] — SHA-256 JCS
//!   canonical hash.
//! - [`metrics`] module — 5 Prometheus metric definitions +
//!   [`MetricsObserver`] trait + [`NoopMetrics`] / [`InMemoryMetrics`].
//! - [`propagation`] module — [`ConfigSnapshot`] in-memory consumer model.
//!
//! # Audit fail-CLOSED ordering (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER)
//!
//! Every mutating path in [`InMemoryConfigSingletonStore`] follows:
//!
//! ```text
//! 1. LOOKUP  — acquire lock, read current state.
//! 2. VALIDATE — CAS version + schema validation (pre-lock for schema).
//! 3. EMIT    — call ConfigAuditSink::emit BEFORE state mutation.
//! 4. MUTATE  — update in-memory state only if emit succeeded.
//! ```
//!
//! If `emit` fails, the mutation is aborted and the error is returned to
//! the caller. The store remains unchanged (fail-CLOSED).
//!
//! # CAS atomic update
//!
//! [`ConfigSingletonStore::update`] takes `expected_version: u64`; if
//! `current_version != expected_version` it returns
//! [`ConfigError::VersionConflict`]. In the Cloudflare DO production
//! implementation this check runs inside a `storage.transaction()` block,
//! making it truly atomic. In the in-memory implementation the mutex
//! guarantees sequential access.
//!
//! # Per-instance `Arc<Mutex<>>` F-001
//!
//! [`InMemoryConfigSingletonStore::new`] creates an independent instance.
//! Cloning the store shares state via the inner [`Arc`]. There is no
//! global singleton (per F-001 closure).
//!
//! # wasm32 compatibility
//!
//! This crate is wasm32-clean: no `tokio` in `src/`, no `std::time`
//! dependency, and uuid uses the `js` feature on wasm32 targets
//! (via `[target.'cfg(target_arch = "wasm32")'.dependencies]`).
//!
//! # Quickstart — CAS update
//!
//! ```
//! use corelink_config_do::{
//!     ConfigPayload, FeatureFlag, AdminActor, InMemoryConfigSingletonStore,
//!     ConfigSingletonStore, store::{InMemoryAuditSink, NoopAuditSink},
//!     metrics::NoopMetrics,
//! };
//! use std::sync::Arc;
//! use uuid::Uuid;
//!
//! # async fn ex() -> Result<(), Box<dyn std::error::Error>> {
//! let store = InMemoryConfigSingletonStore::new(
//!     Arc::new(InMemoryAuditSink::new()),
//!     Arc::new(NoopMetrics),
//! );
//!
//! let (version, _) = store.current().await?;
//! assert_eq!(version, 0);
//!
//! let actor = AdminActor {
//!     user_id: Uuid::nil(),
//!     email_hash: [0u8; 32],
//! };
//! let payload = ConfigPayload::genesis();
//! let new_version = store.update(0, payload, &actor, 1_000_000).await?;
//! assert_eq!(new_version, 1);
//! # Ok(()) }
//! # tokio_test::block_on(ex()).expect("doctest");
//! ```

#![forbid(unsafe_code)]

pub mod error;
pub mod hash;
pub mod metrics;
pub mod propagation;
pub mod store;
pub mod types;
pub mod validation;

pub use error::ConfigError;
pub use metrics::{InMemoryMetrics, MetricsObserver, NoopMetrics};
pub use propagation::{ConfigChangeEvent, ConfigSnapshot};
pub use store::{
    ConfigAuditSink, ConfigSingletonStore, FailingAuditSink, InMemoryAuditSink,
    InMemoryConfigSingletonStore, NoopAuditSink,
};
pub use types::{
    AdminActor, ChangeType, ConfigPayload, ConfigVersionEntry, FeatureFlag, RateLimitKey,
    RateLimitTunable, RetentionPolicy, SUPPORTED_SCHEMA_VERSION,
};
pub use validation::validate_payload;

/// Crate version string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
