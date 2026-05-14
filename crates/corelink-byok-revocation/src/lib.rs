//! `corelink-byok-revocation` — CMK revocation kill switch.
//!
//! # Overview
//!
//! Implements the customer kill switch end-to-end (WI-S14-006):
//!
//! 1. **KMS access check background loop**: [`RevocationDetector`] polls
//!    `KmsProvider::check_access` every 60 seconds per active BYOK tenant.
//!    Configurable via [`RevocationConfig`].
//!
//! 2. **Kill switch path** (triggered on `Revoked` or `NotFound`):
//!    - Atomic DEK cache eviction via `DekCache::evict_all_for_key`.
//!    - Tenant marked `degraded_read_only` in D1 (via pluggable
//!      [`TenantStatusStore`]).
//!    - Audit CloudEvent `corelink.byok.cmk_revoked` emitted atomically
//!      with tenant status update (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
//!    - Customer alerted via multi-channel ([`CustomerAlerter`]).
//!    - SLA tracked: `kill_switch_duration_ms` histogram metric.
//!
//! 3. **Network partition handling**: `Throttled` / `ApiError` do NOT
//!    trigger kill switch. Sustained failure (3 consecutive cycles = 3 min)
//!    degrades tenant to read-only conservatively (false-positive safe).
//!
//! 4. **NO operator override**: there is no function, flag, or code path
//!    that allows an operator to bypass or defer the kill switch.
//!    INV-BYOK-CRYPTO-SOVEREIGNTY CRITICAL — per spec contract §19 waiver
//!    policy this is NOT waivable.
//!
//! 5. **Recovery**: customer re-enables CMK → next 60 s check returns
//!    `KmsAccessStatus::Ok` → tenant restored to `active` via
//!    [`TenantStatusStore::restore_active`].
//!
//! # SLA
//!
//! | Component | Bound |
//! |---|---|
//! | KMS access check cadence | every 60 s per tenant |
//! | Kill switch execution (evict + degrade + audit + alert) | ≤ 30 s |
//! | DEK cache TTL hard ceiling (composed WI-S14-004) | 300 s |
//! | Total p99 customer-perceived kill switch | ≤ 360 s (6 min p99 canonical) |
//!
//! # Example
//!
//! ```rust
//! use std::sync::Arc;
//! use corelink_byok::DekCache;
//! use corelink_byok_revocation::{RevocationDetector, RevocationConfig};
//! use corelink_byok_revocation::testutil::{NoopAlerter, InMemoryTenantStore, StubKmsProvider};
//!
//! # tokio_test::block_on(async {
//! let cache = Arc::new(DekCache::new(300).expect("valid TTL"));
//! let provider = Arc::new(StubKmsProvider::new_revoked());
//! let store = Arc::new(InMemoryTenantStore::default());
//! let alerter = Arc::new(NoopAlerter);
//!
//! let config = RevocationConfig::default();
//! let detector = RevocationDetector::new(
//!     vec![provider],
//!     cache.clone(),
//!     store,
//!     alerter,
//!     config,
//! );
//!
//! // Run one check cycle (non-looping variant for tests)
//! let result = detector.run_one_cycle().await;
//! assert!(result.is_ok());
//! # });
//! ```

#![forbid(unsafe_code)]

pub mod alerter;
pub mod config;
pub mod detector;
pub mod error;
pub mod event;
pub mod store;
pub mod testutil;

pub use alerter::CustomerAlerter;
pub use config::RevocationConfig;
pub use detector::RevocationDetector;
pub use error::RevocationError;
pub use event::RevocationAuditEvent;
pub use store::TenantStatusStore;
